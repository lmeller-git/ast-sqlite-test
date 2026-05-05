use std::{
    cmp::Reverse,
    collections::HashMap,
    fs::{OpenOptions, create_dir_all, remove_dir_all},
    io::{self, Read, Write},
    path::PathBuf,
    sync::Arc,
};

use indexmap::IndexMap;
use lsf_core::{
    ast::AST,
    entry::{CorpusEntry, ID, Meta, RawEntry},
};
use ordered_float::OrderedFloat;
use priority_queue::PriorityQueue;
use smallvec::SmallVec;
use sqlparser::{dialect::SQLiteDialect, parser::Parser};

use crate::CorpusHandler;

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

const CACHE_CAP: usize = 2_usize.pow(14);
const IN_FLIGHT_CAP: usize = 2_usize.pow(11);
const GRACE_PERIOD: u8 = 3;

pub struct DynamicCorpus {
    index: HashMap<ID, EntryData>,
    in_flight: IndexMap<ID, Arc<AST>>,
    hot: HashMap<ID, Arc<AST>>,
    hot_eviction: PriorityQueue<ID, Reverse<OrderedFloat<f64>>>,
    cache_dir: PathBuf,
}

impl CorpusHandler<f64> for DynamicCorpus {
    fn get(&mut self, id: &ID) -> Option<CorpusEntry> {
        let data = self.index.get(id)?;
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
            CacheLocation::Disk => self.retrieve_from_disk(id).map(|ast| {
                CorpusEntry::new(
                    RawEntry::from_components(*id, ast, data.parents.clone()),
                    data.meta,
                )
            }),
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
                && let Some(ast) = self.retrieve_from_disk(id)
            {
                self.move_to_cache(id, ast);
            } else {
                // hot cache
                self.hot_eviction.change_priority(id, Reverse(score.into()));
            }
        }
    }

    fn insert(&mut self, entry: CorpusEntry, score: f64) {
        _ = self.write_to_disk(&entry.id(), &entry.ast);
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

    fn ids(&self) -> Vec<ID> {
        self.index.keys().copied().collect()
    }

    fn clear(&mut self) {
        self.index.clear();
        self.in_flight.clear();
        self.hot.clear();
        _ = remove_dir_all(&self.cache_dir);
    }

    fn size(&self) -> usize {
        self.index.len()
    }
}

impl DynamicCorpus {
    pub fn new(cache_dir: PathBuf) -> Self {
        create_dir_all(&cache_dir).unwrap();

        Self {
            index: HashMap::new(),
            in_flight: IndexMap::new(),
            hot: HashMap::with_capacity(CACHE_CAP),
            hot_eviction: PriorityQueue::with_capacity(CACHE_CAP),
            cache_dir,
        }
    }

    pub fn restore(_cache_dir: PathBuf) -> Self {
        todo!()
    }

    fn retrieve_from_disk(&self, id: &ID) -> Option<Arc<AST>> {
        let path = self.path_from_id(id);

        let mut file = OpenOptions::new().read(true).open(&path).ok()?;

        let mut sql_string = String::new();
        let _n_read = file.read_to_string(&mut sql_string).ok()?;

        let ast = Parser::parse_sql(&SQLiteDialect {}, &sql_string).ok()?;

        Some(Arc::new(ast))
    }

    fn move_to_cache(&mut self, id: &ID, ast: Arc<AST>) {
        if self.hot.len() >= CACHE_CAP {
            self.evict_cached_if(|_| true);
        }

        self.hot.insert(*id, ast);

        if let Some(data) = self.index.get_mut(id) {
            data.cached = CacheLocation::Hot;
            self.hot_eviction.push(*id, Reverse(data.score.into()));
        }
    }

    fn move_to_disk(&mut self, id: &ID, ast: Arc<AST>) -> Result<(), io::Error> {
        self.write_to_disk(id, &ast)?;
        if let Some(data) = self.index.get_mut(id) {
            data.cached = CacheLocation::Disk;
        }

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
        if self.hot.len() < CACHE_CAP {
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

    fn write_to_disk(&self, id: &ID, ast: &Arc<AST>) -> Result<(), io::Error> {
        let path = self.path_from_id(id);
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create_new(true)
            .truncate(true)
            .write(true)
            .open(&path)?;

        for (i, stmt) in ast.iter().enumerate() {
            if i > 0 {
                write!(file, ";")?;
            }
            write!(file, "{}", stmt)?;
        }

        Ok(())
    }

    fn path_from_id(&self, id: &ID) -> PathBuf {
        let val = id.as_raw();

        let s1 = format!("{:02x}", val & 0xFF);
        let s2 = format!("{:02x}", (val >> 8) & 0xFF);

        self.cache_dir
            .join(s1)
            .join(s2)
            .join(format!("{}.sql", val))
    }
}
