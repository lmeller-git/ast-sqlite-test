use std::{
    cmp::Reverse,
    fs::{File, OpenOptions, create_dir_all, remove_dir_all},
    io::{self, BufWriter, Read, Write},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Sender},
    },
    thread,
};

use bitvec::array::BitArray;
use indexmap::IndexMap;
use lsf_core::{
    IDMAp,
    ast::AST,
    entry::{CorpusEntry, ID, Meta, RawEntry},
};
use ordered_float::OrderedFloat;
use priority_queue::PriorityQueue;
use smallvec::SmallVec;
use sqlparser::{dialect::SQLiteDialect, parser::Parser};

use crate::CorpusHandler;

pub trait StorageBackend: Send + Sync {
    fn retrieve(&mut self, id: ID) -> io::Result<Arc<AST>>;
    fn write(&mut self, id: ID, ast: &Arc<AST>) -> io::Result<()>;
    fn clear(&mut self);
}

enum CacheLocation {
    InFlight(u8),
    Hot,
    Disk,
}

struct EntryData {
    parents: SmallVec<[ID; 2]>,
    meta: Meta,
    score: f64,
    cached: CacheLocation,
}

const CACHE_CAP: usize = 2_usize.pow(17);
const IN_FLIGHT_CAP: usize = 2_usize.pow(11);
const GRACE_PERIOD: u8 = 1;
const INIT_CACHE_CAP: usize = 2_usize.pow(14);
const CACHE_ADJUSTMENT_TICK: usize = 500;
const CACHE_MISS_THRESHHOLD: f64 = 0.15;

pub struct DynamicCorpus {
    index: IDMAp<EntryData>,
    in_flight: IndexMap<ID, Arc<AST>, rustc_hash::FxBuildHasher>,
    hot: IDMAp<Arc<AST>>,
    hot_eviction: PriorityQueue<ID, Reverse<OrderedFloat<f64>>>,
    disk_cache: Box<dyn StorageBackend>,
    hot_cap: usize,
    cache_misses: usize,
    requests: usize,
}

impl CorpusHandler<f64> for DynamicCorpus {
    fn get(&mut self, id: &ID) -> Option<CorpusEntry> {
        let data = self.index.get(id)?;
        self.requests += 1;
        match data.cached {
            CacheLocation::InFlight(_) => self.in_flight.get(id).map(|ast| {
                CorpusEntry::new(
                    RawEntry::from_components(*id, ast.clone(), data.parents.clone()),
                    data.meta,
                )
            }),
            CacheLocation::Hot => self.hot.get(id).map(|ast| {
                CorpusEntry::new(
                    RawEntry::from_components(*id, ast.clone(), data.parents.clone()),
                    data.meta,
                )
            }),
            CacheLocation::Disk => {
                let ast = self.disk_cache.retrieve(*id).ok()?;

                let entry = CorpusEntry::new(
                    RawEntry::from_components(*id, ast.clone(), data.parents.clone()),
                    data.meta,
                );

                if self.hot.len() < self.hot_cap {
                    self.move_to_cache(id, ast);
                } else {
                    // signal that cache may be too small
                    self.cache_misses += 1;
                }

                Some(entry)
            }
        }
    }

    fn update(&mut self, id: &ID, score: f64) {
        if let Some(data) = self.index.get_mut(id) {
            data.score = score;

            if let CacheLocation::InFlight(round) = &mut data.cached {
                // entry got scored, downgrade it if applicable
                *round += 1;
                if *round > GRACE_PERIOD
                    && let Some(ast) = self.in_flight.shift_remove(id)
                {
                    // might want to move to cache only if this is better than some threshhold
                    if self.evict_cached_if(|cached| cached < score) {
                        self.move_to_cache(id, ast);
                    } else {
                        _ = self.move_to_disk(id, ast);
                    }
                }
            // we could already prefetch the item from disk, if the new score is better than the worst score in hot
            } else if let CacheLocation::Disk = data.cached
                && self.evict_cached_if(|cached| cached < score)
                && let Ok(ast) = self.disk_cache.retrieve(*id)
            {
                self.move_to_cache(id, ast);
            } else {
                // hot cache
                self.hot_eviction.change_priority(id, Reverse(score.into()));
            }
        }
    }

    fn insert(&mut self, entry: CorpusEntry, score: f64) {
        _ = self.disk_cache.write(entry.id(), &entry.ast);
        if self.in_flight.len() >= IN_FLIGHT_CAP {
            self.evict_in_flight();
        }
        self.in_flight.insert(entry.id(), entry.raw.ast.clone());

        self.index.insert(
            entry.id(),
            EntryData {
                parents: entry.parents.clone(),
                meta: entry.meta,
                score,
                cached: CacheLocation::InFlight(0),
            },
        );
    }

    fn remove(&mut self, id: &ID) {
        if let Some(data) = self.index.remove(id) {
            match data.cached {
                CacheLocation::InFlight(_) => {}
                CacheLocation::Hot => _ = self.hot.remove(id),
                _ => {}
            }
        }
    }

    fn resize(&mut self) {
        if self.requests < CACHE_ADJUSTMENT_TICK {
            return;
        }

        let miss_rate = self.cache_misses as f64 / self.requests as f64;
        if miss_rate > CACHE_MISS_THRESHHOLD && self.hot_cap < CACHE_CAP {
            let new_cap = (self.hot_cap * 3 / 2).min(CACHE_CAP);
            println!(
                "Cache misses exceeded {}%: missed {}% of requests\n Resizing cache from {} to {}.",
                CACHE_MISS_THRESHHOLD * 100.,
                miss_rate * 100.,
                self.hot_cap,
                new_cap
            );
            self.hot_cap = new_cap
        }
        self.requests = 0;
        self.cache_misses = 0;
    }

    fn ids(&self) -> rustc_hash::FxHashSet<ID> {
        self.index.keys().copied().collect()
    }

    fn protected_ids(&self) -> rustc_hash::FxHashSet<ID> {
        const PROTECTED_RATIO: f64 = 0.15;

        let in_flight_count = self.in_flight.len();
        let target =
            ((self.size() as f64 * PROTECTED_RATIO) as usize).saturating_sub(in_flight_count);

        let mut protected = rustc_hash::FxHashSet::with_capacity_and_hasher(
            in_flight_count + target,
            Default::default(),
        );
        protected.extend(self.in_flight.keys().copied());

        if target == 0 {
            return protected;
        }

        let mut index: Vec<_> = self.index.iter().collect();
        let index_len = index.len();

        if target < index_len {
            index
                .select_nth_unstable_by(index_len - target, |a, b| a.1.score.total_cmp(&b.1.score));
            protected.extend(index[index_len - target..].iter().map(|(id, _)| **id));
        } else {
            protected.extend(index.iter().map(|(id, _)| **id));
        }

        protected
    }

    fn clear(&mut self) {
        self.index.clear();
        self.in_flight.clear();
        self.hot.clear();
        self.disk_cache.clear();
    }

    fn size(&self) -> usize {
        self.index.len()
    }
}

impl DynamicCorpus {
    pub fn new(disk_cache: Box<dyn StorageBackend>) -> Self {
        Self {
            index: IDMAp::default(),
            in_flight: IndexMap::with_hasher(rustc_hash::FxBuildHasher),
            hot: IDMAp::with_capacity_and_hasher(INIT_CACHE_CAP, rustc_hash::FxBuildHasher),
            hot_eviction: PriorityQueue::with_capacity(INIT_CACHE_CAP),
            // ast_cleanup: CleanUp::new(),
            hot_cap: INIT_CACHE_CAP,
            requests: 0,
            cache_misses: 0,
            disk_cache,
        }
    }

    pub fn restore(_cache_dir: PathBuf) -> Self {
        todo!()
    }

    fn move_to_cache(&mut self, id: &ID, ast: Arc<AST>) {
        if self.hot.len() >= self.hot_cap {
            self.evict_cached_if(|_| true);
        }

        self.hot.insert(*id, ast);

        if let Some(data) = self.index.get_mut(id) {
            data.cached = CacheLocation::Hot;
            self.hot_eviction.push(*id, Reverse(data.score.into()));
        }
    }

    fn move_to_disk(&mut self, id: &ID, ast: Arc<AST>) -> Result<(), io::Error> {
        self.disk_cache.write(*id, &ast)?;
        if let Some(data) = self.index.get_mut(id) {
            data.cached = CacheLocation::Disk;
        }

        // dropping ASTs is suprisingly expensive at scale, migth want to move that to a background worker
        // self.ast_cleanup.do_drop(ast);

        Ok(())
    }

    fn evict_in_flight(&mut self) {
        if let Some((id, ast)) = self.in_flight.shift_remove_index(0) {
            if let Some(data) = self.index.get(&id)
                && {
                    let score = data.score;
                    // might want to move to cache only if this is better than some threshhold
                    self.evict_cached_if(|cached| cached < score)
                }
            {
                self.move_to_cache(&id, ast);
            } else {
                _ = self.move_to_disk(&id, ast);
            }
        }
    }

    fn evict_cached_if(&mut self, cond: impl Fn(f64) -> bool) -> bool {
        if self.hot.len() < self.hot_cap {
            return true;
        }

        if let Some((worst_id, worst_score)) = self.hot_eviction.pop() {
            if cond(worst_score.0.into())
                && let Some(ast) = self.hot.remove(&worst_id)
            {
                _ = self.move_to_disk(&worst_id, ast);
                return true;
            }
            self.hot_eviction.push_increase(worst_id, worst_score);
            return false;
        }
        true
    }
}

#[derive(Default)]
pub struct InMemory<T> {
    inner: IDMAp<T>,
}

impl<T> InMemory<T> {
    pub fn new() -> Self {
        Self {
            inner: IDMAp::default(),
        }
    }
}

impl<T> CorpusHandler<T> for InMemory<CorpusEntry> {
    fn get(&mut self, id: &ID) -> Option<CorpusEntry> {
        self.inner.get(id).cloned()
    }

    fn update(&mut self, _id: &ID, _s: T) {}

    fn insert(&mut self, entry: CorpusEntry, _s: T) {
        self.inner.insert(entry.id(), entry);
    }

    fn remove(&mut self, id: &ID) {
        _ = self.inner.remove(id)
    }

    fn resize(&mut self) {}

    fn clear(&mut self) {
        self.inner.clear();
    }

    fn size(&self) -> usize {
        self.inner.len()
    }

    fn ids(&self) -> rustc_hash::FxHashSet<ID> {
        self.inner.keys().copied().collect()
    }

    fn protected_ids(&self) -> rustc_hash::FxHashSet<ID> {
        Default::default()
    }
}

impl StorageBackend for InMemory<Arc<AST>> {
    fn retrieve(&mut self, id: ID) -> io::Result<Arc<AST>> {
        self.inner
            .get(&id)
            .cloned()
            .ok_or(io::Error::new(io::ErrorKind::NotFound, "item not found"))
    }

    fn write(&mut self, id: ID, ast: &Arc<AST>) -> io::Result<()> {
        self.inner.insert(id, ast.clone());
        Ok(())
    }

    fn clear(&mut self) {
        self.inner.clear();
    }
}

pub struct ShardedDiskCache {
    cache_dir: PathBuf,
    created_shards: BitArray<[u64; 1024]>, // 256 * 256 / 64
}

impl StorageBackend for ShardedDiskCache {
    fn retrieve(&mut self, id: ID) -> io::Result<Arc<AST>> {
        let path = self.path_from_id(id);

        let mut file = OpenOptions::new().read(true).open(&path)?;

        let mut sql_string = String::new();
        let _n_read = file.read_to_string(&mut sql_string)?;

        let ast = Parser::parse_sql(&SQLiteDialect {}, &sql_string)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Could not parse sql"))?;

        Ok(Arc::new(ast))
    }

    fn write(&mut self, id: ID, ast: &Arc<AST>) -> io::Result<()> {
        let path = self.path_from_id(id);
        if let Some(parent) = path.parent() {
            self.ensure_dir_exists(id, parent)?;
        }

        let file = OpenOptions::new()
            .create_new(true)
            .truncate(true)
            .write(true)
            .open(&path)?;

        let mut writer = io::BufWriter::new(file);

        for (i, stmt) in ast.iter().enumerate() {
            if i > 0 {
                write!(writer, ";")?;
            }
            write!(writer, "{}", stmt)?;
        }
        writer.flush()?;

        Ok(())
    }

    fn clear(&mut self) {
        remove_dir_all(&self.cache_dir).unwrap()
    }
}

impl ShardedDiskCache {
    pub fn new(cache_dir: PathBuf) -> Self {
        create_dir_all(&cache_dir).unwrap();
        Self {
            cache_dir,
            created_shards: BitArray::ZERO,
        }
    }

    fn ensure_dir_exists(&mut self, id: ID, path: &Path) -> io::Result<()> {
        let shards = self.id_to_shards(id);
        let index = ((shards.0 as usize) << 8) | (shards.1 as usize);
        if !self.created_shards[index] {
            std::fs::create_dir_all(path)?;
            self.created_shards.set(index, true);
        }
        Ok(())
    }

    fn id_to_shards(&self, id: ID) -> (u32, u32) {
        let val = id.as_raw();
        (val & 0xFF, (val >> 8) & 0xFF)
    }

    fn path_from_id(&self, id: ID) -> PathBuf {
        let (shard1, shard2) = self.id_to_shards(id);

        let s1 = format!("{:02x}", shard1);
        let s2 = format!("{:02x}", shard2);

        self.cache_dir.join(s1).join(s2).join(format!("{}.sql", id))
    }
}

struct FileSlot {
    offset: u64,
    size: usize,
}

pub struct BinaryBlob {
    index: IDMAp<FileSlot>,
    f_handle: File,
    current_offset: u64,
}

impl BinaryBlob {
    pub fn new(path: PathBuf) -> Self {
        let blob_path = path.join("blob").with_extension("bin");
        if let Some(parent) = blob_path.parent() {
            _ = create_dir_all(parent);
        }
        let f_handle = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&blob_path)
            .unwrap();

        Self {
            index: IDMAp::default(),
            f_handle,
            current_offset: 0,
        }
    }
}

impl StorageBackend for BinaryBlob {
    fn retrieve(&mut self, id: ID) -> io::Result<Arc<AST>> {
        let slot = self
            .index
            .get(&id)
            .ok_or(io::Error::new(io::ErrorKind::NotFound, "Entry not indexed"))?;

        let mut buffer = vec![0; slot.size];
        self.f_handle.read_exact_at(&mut buffer, slot.offset)?;

        let ast = postcard::from_bytes(&buffer)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Arc::new(ast))
    }

    fn write(&mut self, id: ID, ast: &Arc<AST>) -> io::Result<()> {
        if self.index.contains_key(&id) {
            return Ok(());
        }

        let bytes = postcard::to_allocvec(ast.as_ref())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let size = bytes.len();

        self.f_handle.write_all(&bytes)?;

        self.index.insert(
            id,
            FileSlot {
                offset: self.current_offset,
                size,
            },
        );
        self.current_offset += size as u64;

        Ok(())
    }

    fn clear(&mut self) {
        self.f_handle.set_len(0).unwrap();
        self.current_offset = 0;
        self.index.clear();
    }
}

enum SqlSaverCommand {
    Write(Arc<AST>),
    Clear,
}

pub struct SQLSaver {
    backend: Box<dyn StorageBackend>,
    tx: Sender<SqlSaverCommand>,
}

impl SQLSaver {
    pub fn new(backend: Box<dyn StorageBackend>, save_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel::<SqlSaverCommand>();

        let file_path = save_dir.join("queries").with_extension("sql");

        thread::spawn(move || {
            if let Some(parent) = file_path.parent() {
                _ = create_dir_all(parent);
            }
            let save_file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(file_path)
                .unwrap();

            let mut writer = BufWriter::new(save_file);

            while let Ok(command) = rx.recv() {
                match command {
                    SqlSaverCommand::Write(ast) => {
                        for (i, stmt) in ast.iter().enumerate() {
                            if i > 0 {
                                _ = write!(writer, ";");
                            }
                            _ = write!(writer, "{}", stmt);
                        }
                        _ = writeln!(writer, ";");
                    }
                    SqlSaverCommand::Clear => {
                        let _ = writer.flush();
                        _ = writer.get_mut().set_len(0);
                    }
                }
            }
            _ = writer.flush();
        });

        Self { backend, tx }
    }
}

impl StorageBackend for SQLSaver {
    fn retrieve(&mut self, id: ID) -> io::Result<Arc<AST>> {
        self.backend.retrieve(id)
    }

    fn write(&mut self, id: ID, ast: &Arc<AST>) -> io::Result<()> {
        _ = self.tx.send(SqlSaverCommand::Write(Arc::clone(ast)));
        self.backend.write(id, ast)?;
        Ok(())
    }

    fn clear(&mut self) {
        self.backend.clear();
        _ = self.tx.send(SqlSaverCommand::Clear);
    }
}

pub struct CleanUp<T> {
    tx: Sender<T>,
}

impl<T: Send + Sync + 'static> CleanUp<T> {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            while let Ok(item) = rx.recv() {
                drop(item);
            }
        });

        Self { tx }
    }

    pub fn do_drop(&self, item: T) {
        _ = self.tx.send(item);
    }
}

impl<T: Send + Sync + 'static> Default for CleanUp<T> {
    fn default() -> Self {
        Self::new()
    }
}
