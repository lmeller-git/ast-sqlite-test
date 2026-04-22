use std::{
    collections::BTreeSet,
    fmt::Debug,
    fs,
    io::{self, Read},
    ops::RangeBounds,
    path::PathBuf,
    sync::{Arc, atomic::AtomicU32},
};

use lsf_core::entry::{CorpusEntry, Meta, RawEntry};
use lsf_cov::ipc::{IPCToken, SharedMemHandle};
use lsf_mutate::{
    AcceptanceReason,
    MutationState,
    MutationStrategy,
    RejectionReason,
    TestOutcome,
    TestableEntry,
};
use rand::{SeedableRng, rngs::SmallRng};
use sqlparser::{dialect::SQLiteDialect, parser::Parser};

use crate::{Corpus, schedule::Schedule};

pub struct Engine {
    corpus: Corpus,
    shmem_queue: Arc<SharedMemHandle>,
    scheduler: Box<dyn Schedule>,
    strategies: Vec<Box<dyn MutationStrategy>>,
    rng: SmallRng,
    total_mutations: Arc<AtomicU32>,
}

impl Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fuzing Engine")
            .field("corpus", &self.corpus)
            .field("n strategies", &self.strategies.len())
            .finish()
    }
}

impl Engine {
    pub fn new(
        scheduler: Box<dyn Schedule>,
        strategies: Vec<Box<dyn MutationStrategy>>,
        shmem_queue: Arc<SharedMemHandle>,
        rng_seed: u64,
    ) -> Self {
        Self {
            scheduler,
            strategies,
            corpus: Corpus::new(shmem_queue.shmem_size),
            shmem_queue,
            rng: SmallRng::seed_from_u64(rng_seed),
            total_mutations: Arc::default(),
        }
    }

    pub fn with_scheduler(mut self, scheduler: Box<dyn Schedule>) -> Self {
        self.scheduler = scheduler;
        self
    }

    pub fn clear_strategies(&mut self) {
        self.strategies.clear();
    }

    pub fn add_strategy(&mut self, mut strategy: Box<dyn MutationStrategy>) {
        strategy.init(lsf_mutate::StrategyContext {
            total_attempts: self.total_mutations.clone(),
        });
        self.strategies.push(strategy);
    }

    pub fn mutate_batch(&mut self, batch_size: usize) -> Generation {
        if batch_size == 0 {
            return Generation { members: vec![] };
        }
        let next_batch = self
            .scheduler
            .next_batch(&self.corpus, batch_size, &mut self.rng);

        let generation: Generation = next_batch
            .iter()
            .filter_map(|entry| {
                if let Some(parent_entry) = self.corpus.entries.get(entry) {
                    let mut state = MutationState::Unchanged;
                    let mut current_parent = parent_entry.raw();

                    for strategy in &self.strategies {
                        if let Ok(MutationState::Mutated(next)) = strategy.breed(
                            current_parent,
                            &next_batch,
                            &self.corpus.entries,
                            &mut self.rng,
                        ) {
                            state = MutationState::Mutated(next);
                            current_parent = if let MutationState::Mutated(next_parent) = &state {
                                next_parent
                            } else {
                                unreachable!()
                            }
                        }
                    }

                    state.into_option()
                } else {
                    None
                }
            })
            .collect();

        self.total_mutations.fetch_add(
            generation.members.len() as u32,
            std::sync::atomic::Ordering::Relaxed,
        );

        generation
    }

    pub fn commit_test_result(
        &mut self,
        raw_entry: TestableEntry,
        mut meta: Meta,
        shmem: Box<IPCToken>,
    ) {
        let new_edges = self.corpus.edge_map.update(shmem.as_edge_map());
        self.shmem_queue.push(shmem).expect("token was duplicated");

        let is_diverse = self
            .corpus
            .diversity
            .try_insert(raw_entry.id(), raw_entry.ast());

        if !is_diverse && new_edges.is_empty() {
            raw_entry.fire_hooks(TestOutcome::Rejected(RejectionReason::Bad));
            return;
        }

        meta.new_cov_nodes = new_edges.len();

        if !new_edges.is_empty() {
            let is_best = self.corpus.entry_rating.update_if_best(
                raw_entry.id(),
                raw_entry.ast().len(),
                meta.exec_time,
                new_edges.into_iter(),
            );

            if !is_best && !is_diverse {
                raw_entry.fire_hooks(TestOutcome::Rejected(RejectionReason::Bad));
                return;
            }
            raw_entry.fire_hooks(TestOutcome::Accepted(AcceptanceReason::CovIncrease));
        } else {
            raw_entry.fire_hooks(TestOutcome::Accepted(AcceptanceReason::IsDiverse));
        }

        let entry = Into::<RawEntry>::into(raw_entry).into_corpus_entry(meta);
        self.commit_generation(SelectedGeneration {
            members: vec![entry],
        });
    }

    pub fn commit_generation(&mut self, generation: SelectedGeneration) {
        self.corpus.entries.extend(
            generation
                .members
                .into_iter()
                .map(|entry| (entry.id(), entry)),
        );
    }

    pub fn return_token(&mut self, token: Box<IPCToken>) {
        self.shmem_queue.push(token).expect("token was duplicated");
    }

    pub fn populate(&mut self, seed_gens: Vec<Box<dyn ObtainSeed>>) {
        for generator in seed_gens {
            let seeds = generator.obtain();
            self.corpus
                .entries
                .extend(seeds.into_iter().map(|seed| (seed.id(), seed)));
        }
    }

    pub fn snapshot(&self) -> Vec<CorpusEntry> {
        let mut snapshot: Vec<CorpusEntry> = self.corpus.entries.values().cloned().collect();
        // sort snapshot, to ensure same output across runs/snapshots, as this is created from std::collections::HashMap.
        // This may actually be necessary if a snapshot could at some point be fed back into the engine
        snapshot.sort_by_key(|item| item.id());
        snapshot
    }

    pub fn gc(&mut self) {
        let mut should_keep = self.corpus.entry_rating.get_best_entries();
        should_keep.extend(&self.corpus.diversity.entries);
        println!(
            "keeping {} out of {} entries",
            should_keep.len(),
            self.corpus.entries.len(),
        );
        self.corpus.entries.retain(|id, _| should_keep.contains(id));
    }

    pub fn corpus_size(&self) -> usize {
        self.corpus.entries.len()
    }

    pub fn clear(&mut self) {
        self.corpus.clear();
    }
}

/// A batch of newly mutated entries, which are yet to be judged
#[derive(Debug, Clone)]
pub struct Generation {
    members: Vec<TestableEntry>,
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

    pub fn members(&self) -> &[TestableEntry] {
        &self.members
    }

    pub fn drain<R>(&mut self, range: R) -> impl Iterator<Item = TestableEntry>
    where
        R: RangeBounds<usize>,
    {
        self.members.drain(range)
    }
}

impl FromIterator<TestableEntry> for Generation {
    fn from_iter<T: IntoIterator<Item = TestableEntry>>(iter: T) -> Self {
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
                        RawEntry::new(ast, [].into())
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
                RawEntry::new(ast, BTreeSet::new())
                    .into_corpus_entry(lsf_core::entry::Meta::default()),
            );
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use lsf_mutate::{RandomUpperCase, SpliceIn};

    use super::*;
    use crate::WeightedRandomScheduler;

    #[test]
    fn engine_functionality() {
        let mut engine = Engine::new(
            Box::new(WeightedRandomScheduler {}),
            vec![Box::new(SpliceIn {})],
            Arc::new(SharedMemHandle::new(1, 1)),
            42,
        );
        engine.clear_strategies();
        assert!(engine.strategies.is_empty());
        engine.add_strategy(Box::new(RandomUpperCase::new()));

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
