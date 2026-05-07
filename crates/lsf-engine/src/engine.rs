use std::{
    fmt::Debug,
    fs,
    io::{self, Read},
    ops::RangeBounds,
    path::PathBuf,
    sync::Arc,
};

use lsf_core::entry::{CorpusEntry, Meta, RawEntry};
use lsf_cov::{
    bitmap::EdgeMap,
    ipc::{IPCToken, SharedMemHandle},
};
use lsf_feedback::{TestOutcome, TestableEntry, mab::MABBody};
use lsf_mutate::{MutationState, MutationStrategy};
use rand::{SeedableRng, rngs::SmallRng};
use smallvec::smallvec;
use sqlparser::{dialect::SQLiteDialect, parser::Parser};

use crate::{CorpusHandler, CorpusMinimizer, schedule::Schedule};

pub const GRANULARITY: usize = 50;

pub struct Engine {
    corpus: Box<dyn CorpusHandler<f64>>,
    shmem_queue: Arc<SharedMemHandle>,
    scheduler: Box<dyn Schedule>,
    strategies: Vec<Box<dyn MutationStrategy>>,
    minimizer: Box<dyn CorpusMinimizer<f64>>,
    rng: SmallRng,
    mab_bodies: Vec<Arc<MABBody>>,
    edge_map: EdgeMap,
    size_at_last_reset: usize,
}

impl Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fuzing Engine")
            .field("n strategies", &self.strategies.len())
            .finish()
    }
}

impl Engine {
    pub fn new(
        scheduler: Box<dyn Schedule>,
        corpus_handler: Box<dyn CorpusHandler<f64>>,
        minimizer: Box<dyn CorpusMinimizer<f64>>,
        strategies: Vec<Box<dyn MutationStrategy>>,
        shmem_queue: Arc<SharedMemHandle>,
        mab_bodies: Vec<Arc<MABBody>>,
        rng_seed: u64,
    ) -> Self {
        Self {
            scheduler,
            strategies,
            corpus: corpus_handler,
            minimizer,
            edge_map: EdgeMap::new(shmem_queue.shmem_size),
            shmem_queue,
            rng: SmallRng::seed_from_u64(rng_seed),
            mab_bodies,
            size_at_last_reset: 0,
        }
    }

    pub fn with_scheduler(mut self, scheduler: Box<dyn Schedule>) -> Self {
        self.scheduler = scheduler;
        self
    }

    pub fn clear_strategies(&mut self) {
        self.strategies.clear();
    }

    pub fn add_strategy(&mut self, strategy: Box<dyn MutationStrategy>) {
        self.strategies.push(strategy);
    }

    pub fn add_mab_body(&mut self, mab: Arc<MABBody>) {
        self.mab_bodies.push(mab);
    }

    pub fn mutate_batch(&mut self, batch_size: usize) -> Generation {
        if batch_size == 0 {
            return Generation { members: vec![] };
        }
        let next_batch = self
            .scheduler
            .next_batch(self.corpus.as_mut(), batch_size, &mut self.rng);

        let generation: Generation = next_batch
            .iter()
            .filter_map(|entry| {
                let mut state = MutationState::Unchanged;
                let mut child: TestableEntry<RawEntry> =
                    TestableEntry::new(RawEntry::new(entry.ast().clone(), smallvec![entry.id()]));
                child.parent_stats.extend(entry.applied_rule_stats.clone());

                for strategy in &self.strategies {
                    if let Ok(MutationState::Mutated) =
                        strategy.breed(&mut child, &next_batch, &mut self.rng)
                    {
                        state = MutationState::Mutated;
                    }
                }

                state.into_option().map(|_| child)
            })
            .collect();

        for body in &self.mab_bodies {
            body.tick_by(1);
        }

        generation
    }

    pub fn commit_test_result(
        &mut self,
        raw_entry: TestableEntry<RawEntry>,
        mut meta: Meta,
        shmem: Box<IPCToken>,
    ) {
        let new_edges = self.edge_map.update(shmem.as_edge_map());
        self.shmem_queue.push(shmem).expect("token was duplicated");
        meta.new_cov_nodes = new_edges.len();

        let accepted = self.minimizer.on_add(&raw_entry, &meta, new_edges);

        raw_entry.fire_rule_hooks(accepted);

        if let TestOutcome::Rejected(_) = accepted {
            return;
        }

        let entry = Into::<RawEntry>::into(raw_entry).into_corpus_entry(meta);
        self.commit_generation(SelectedGeneration {
            members: vec![entry],
        });
    }

    pub fn commit_generation(&mut self, generation: SelectedGeneration) {
        generation.members.into_iter().for_each(|entry| {
            let score = self.scheduler.on_add(&entry);
            self.corpus.insert(entry, score);
        });
    }

    pub fn return_token(&mut self, token: Box<IPCToken>) {
        self.shmem_queue.push(token).expect("token was duplicated");
    }

    pub fn populate(&mut self, seed_gens: Vec<Box<dyn ObtainSeed>>) {
        for generator in seed_gens {
            let seeds = generator.obtain();
            seeds.into_iter().for_each(|seed| {
                self.scheduler.on_add(&seed);
                self.corpus.insert(seed, f64::INFINITY)
            })
        }
    }

    pub fn snapshot(&mut self) -> Vec<CorpusEntry> {
        self.corpus
            .ids()
            .into_iter()
            .filter_map(|id| self.corpus.get(&id))
            .collect()
    }

    pub fn chore(&mut self) {
        const GC_AT: f64 = 0.05;
        const GC_MIN_ABSOLUTE: usize = 100;

        self.corpus.resize();
        self.scheduler.chore();
        let corpus_size = self.corpus_size();

        let new_entries = corpus_size.saturating_sub(self.size_at_last_reset);
        let growth_rate = new_entries as f64 / self.size_at_last_reset as f64;

        if growth_rate >= GC_AT && new_entries >= GC_MIN_ABSOLUTE {
            self.minimizer
                .minimize(self.corpus.as_mut(), self.scheduler.as_mut());
            self.size_at_last_reset = self.corpus_size();
        }
    }

    pub fn corpus_size(&self) -> usize {
        self.corpus.size()
    }

    pub fn clear(&mut self) {
        self.corpus.clear();
        self.minimizer.reset();
        self.scheduler.reset();
    }
}

/// A batch of newly mutated entries, which are yet to be judged
#[derive(Debug, Clone)]
pub struct Generation {
    members: Vec<TestableEntry<RawEntry>>,
}

impl Generation {
    #[allow(dead_code)]
    fn new() -> Self {
        Self::with_capacity(0)
    }

    #[allow(dead_code)]
    fn with_capacity(cap: usize) -> Self {
        Self {
            members: Vec::with_capacity(cap),
        }
    }

    pub fn members(&self) -> &[TestableEntry<RawEntry>] {
        &self.members
    }

    pub fn drain<R>(&mut self, range: R) -> impl Iterator<Item = TestableEntry<RawEntry>>
    where
        R: RangeBounds<usize>,
    {
        self.members.drain(range)
    }
}

impl FromIterator<TestableEntry<RawEntry>> for Generation {
    fn from_iter<T: IntoIterator<Item = TestableEntry<RawEntry>>>(iter: T) -> Self {
        Self {
            members: iter.into_iter().collect(),
        }
    }
}

/// A Generation having undergone selection/fitness screening
#[derive(Debug, Clone)]
pub struct SelectedGeneration {
    members: Vec<CorpusEntry>,
}

impl SelectedGeneration {
    pub fn members(&self) -> &[CorpusEntry] {
        &self.members
    }

    pub fn push(&mut self, entry: CorpusEntry) {
        self.members.push(entry);
    }

    pub fn drain<R>(&mut self, range: R) -> impl Iterator<Item = CorpusEntry>
    where
        R: RangeBounds<usize>,
    {
        self.members.drain(range)
    }
}

impl FromIterator<CorpusEntry> for SelectedGeneration {
    fn from_iter<T: IntoIterator<Item = CorpusEntry>>(iter: T) -> Self {
        Self {
            members: iter.into_iter().collect(),
        }
    }
}
pub trait ObtainSeed: Send + Sync {
    fn obtain(&self) -> Vec<CorpusEntry>;
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SeedDirReader {
    dir: PathBuf,
}

impl SeedDirReader {
    pub fn new(path: PathBuf) -> Self {
        Self { dir: path }
    }

    fn collect_contents(&self) -> Result<Vec<CorpusEntry>, io::Error> {
        let entries = fs::read_dir(&self.dir)?;
        let mut contents = Vec::with_capacity(entries.size_hint().0);
        let mut buffer = String::new();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if let Some(b"sql") = path.extension().map(|ext| ext.as_encoded_bytes()) {
                let f = fs::File::open(&path)?;
                let mut reader = io::BufReader::new(f);

                _ = reader.read_to_string(&mut buffer)?;
                if let Ok(ast) = Parser::parse_sql(&SQLiteDialect {}, &buffer).inspect_err(|e| {
                    eprintln!(
                        "could not parse sql\n{}in file {}, due to {e:?}\n",
                        &buffer,
                        path.display()
                    )
                }) {
                    contents.push(
                        RawEntry::new(ast, smallvec![])
                            .into_corpus_entry(lsf_core::entry::Meta::default()),
                    );
                }
                buffer.clear();
            }
        }
        println!(
            "parsed {} testaces from {}",
            contents.len(),
            self.dir.display()
        );
        Ok(contents)
    }
}

impl ObtainSeed for SeedDirReader {
    fn obtain(&self) -> Vec<CorpusEntry> {
        self.collect_contents()
            .inspect_err(|e| eprintln!("could not read dir {}, due to {e:?}", self.dir.display()))
            .unwrap_or_default()
    }
}

#[derive(Debug)]
pub struct LiteralSeeder {
    lit: String,
}

impl LiteralSeeder {
    pub fn new(lit: String) -> Self {
        Self { lit }
    }
}

impl ObtainSeed for LiteralSeeder {
    fn obtain(&self) -> Vec<CorpusEntry> {
        let mut v = Vec::new();
        if let Ok(ast) = Parser::parse_sql(&SQLiteDialect {}, &self.lit)
            .inspect_err(|e| eprintln!("could not parse sql \n{}\n due to {:?}\n", self.lit, e))
        {
            v.push(
                RawEntry::new(ast, Default::default())
                    .into_corpus_entry(lsf_core::entry::Meta::default()),
            );
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use lsf_mutate::SpliceIn;

    use super::*;
    use crate::{GreedyCoverage, InMemory, WeightedRandomScheduler};

    #[test]
    fn engine_functionality() {
        let mut engine = Engine::new(
            Box::new(WeightedRandomScheduler {}),
            Box::new(InMemory::new()),
            Box::new(GreedyCoverage::new(1)),
            vec![Box::new(SpliceIn {})],
            Arc::new(SharedMemHandle::new(1, 1)),
            vec![],
            42,
        );
        engine.clear_strategies();
        assert!(engine.strategies.is_empty());
        engine.add_strategy(Box::new(SpliceIn {}));

        assert!(engine.mutate_batch(16).members().is_empty());

        engine.populate(vec![Box::new(LiteralSeeder::new(
            "SELECT a FROM b".to_string(),
        ))]);

        assert!(engine.mutate_batch(0).members().is_empty());
        let mut children = engine.mutate_batch(1);
        assert_eq!(children.members().len(), 1);

        engine.commit_generation(
            children
                .drain(..)
                .map(|raw| {
                    Into::<RawEntry>::into(raw).into_corpus_entry(lsf_core::entry::Meta::default())
                })
                .collect(),
        );
        engine.clear_strategies();
        assert!(engine.mutate_batch(1).members().is_empty());

        engine.populate(vec![Box::new(LiteralSeeder::new(
            "SELECT a FROM b".to_string(),
        ))]);

        engine.add_strategy(Box::new(SpliceIn {}));
        assert!(!engine.mutate_batch(1).members().is_empty());
    }
}
