use std::{
    collections::{BTreeSet, HashMap},
    fmt::Debug,
    ops::RangeBounds,
    path::PathBuf,
};

use lsf_core::entry::{CorpusEntry, ID, RawEntry};
use lsf_mutate::{MutationState, MutationStrategy};
use sqlparser::{dialect::SQLiteDialect, parser::Parser};

use crate::schedule::{Queue, Schedule};

pub struct Engine {
    corpus: HashMap<ID, CorpusEntry>,
    scheduler: Box<dyn Schedule>,
    active: Queue<ID>,
    strategies: Vec<Box<dyn MutationStrategy>>,
}

impl Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fuzing Engine")
            .field("corpus", &self.corpus)
            .field("breeding population", &self.active)
            .field("n strategies", &self.strategies.len())
            .finish()
    }
}

impl Engine {
    pub fn new(scheduler: Box<dyn Schedule>, strategies: Vec<Box<dyn MutationStrategy>>) -> Self {
        Self {
            scheduler,
            strategies,
            corpus: Default::default(),
            active: Default::default(),
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

    pub fn mutate_batch(&mut self, batch_size: usize) -> Generation {
        let next_batch = self.scheduler.next_batch(&mut self.active, batch_size);
        next_batch
            .iter()
            .filter_map(|entry| {
                if let Some(parent_entry) = self.corpus.get(entry) {
                    let mut state = MutationState::Unchanged;
                    let mut current_parent = parent_entry.raw();

                    for strategy in &self.strategies {
                        if let Ok(MutationState::Mutated(next)) =
                            strategy.breed(current_parent, &next_batch, &self.corpus)
                        {
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
            .collect()
    }

    pub fn commit_generation(&mut self, generation: SelectedGeneration) {
        let ids = generation
            .members()
            .iter()
            .map(|entry| entry.id())
            .collect::<Vec<_>>();
        self.corpus.extend(
            generation
                .members
                .into_iter()
                .map(|entry| (entry.id(), entry)),
        );
        self.active.extend(ids);
    }

    pub fn populate(&mut self, seed_gens: Vec<Box<dyn ObtainSeed>>) {
        for generator in seed_gens {
            let seeds = generator.obtain();
            let ids = seeds.iter().map(|seed| seed.id()).collect::<Vec<_>>();
            self.corpus
                .extend(seeds.into_iter().map(|seed| (seed.id(), seed)));
            self.active.extend(ids);
        }
    }

    pub fn snapshot(&self) -> Vec<CorpusEntry> {
        self.corpus.values().cloned().collect()
    }
}

/// A batch of newly mutated entries, which are yet to be judged
#[derive(Debug, Clone)]
pub struct Generation {
    members: Vec<RawEntry>,
}

impl Generation {
    fn new() -> Self {
        Self::with_capacity(0)
    }

    fn with_capacity(cap: usize) -> Self {
        Self {
            members: Vec::with_capacity(cap),
        }
    }

    pub fn members(&self) -> &[RawEntry] {
        &self.members
    }

    pub fn drain<R>(&mut self, range: R) -> impl Iterator<Item = RawEntry>
    where
        R: RangeBounds<usize>,
    {
        self.members.drain(range)
    }
}

impl FromIterator<RawEntry> for Generation {
    fn from_iter<T: IntoIterator<Item = RawEntry>>(iter: T) -> Self {
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

#[derive(Debug, Clone)]
pub struct SeedDirReader {
    dir: PathBuf,
}

impl ObtainSeed for SeedDirReader {
    fn obtain(&self) -> Vec<CorpusEntry> {
        todo!()
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
            v.push(RawEntry::new(ast, BTreeSet::new()).into_corpus_entry(lsf_core::entry::Meta {}));
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use lsf_mutate::{RandomUpperCase, SliceIn};

    use super::*;
    use crate::FIFOScheduler;

    #[test]
    fn engine_functionality() {
        let mut engine = Engine::new(Box::new(FIFOScheduler {}), vec![Box::new(SliceIn {})]);
        engine.clear_strategies();
        assert!(engine.strategies.is_empty());
        engine.add_strategy(Box::new(RandomUpperCase::new(1.)));

        assert!(engine.mutate_batch(16).members().is_empty());

        engine.populate(vec![Box::new(LiteralSeeder::new(
            "SELECT a FROM b".to_string(),
        ))]);

        assert!(engine.mutate_batch(0).members().is_empty());
        let mut children = engine.mutate_batch(1);
        assert!(!children.members().is_empty());
        assert!(engine.mutate_batch(1).members().is_empty());

        engine.commit_generation(
            children
                .drain(..)
                .map(|raw| raw.into_corpus_entry(lsf_core::entry::Meta {}))
                .collect(),
        );
        engine.clear_strategies();
        assert!(engine.mutate_batch(1).members().is_empty());

        engine.populate(vec![Box::new(LiteralSeeder::new(
            "SELECT a FROM b".to_string(),
        ))]);

        engine.add_strategy(Box::new(SliceIn {}));
        assert!(!engine.mutate_batch(1).members().is_empty());
    }
}
