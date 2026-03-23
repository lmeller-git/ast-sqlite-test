use lsf_engine::{
    Engine as RawEngine,
    FIFOScheduler as RawFIFOScheduler,
    Generation as RawGeneration,
    LiteralSeeder,
    ObtainSeed,
    Schedule,
    SelectedGeneration as RawSelectedGeneration,
};
use lsf_mutate::{
    Merger,
    MutationStrategy,
    RandomMutationSampler,
    RandomUpperCase,
    SliceIn,
    TableGuard,
    TableNameScramble,
};
use pyo3::prelude::*;

use crate::{CorpusEntry, RawEntry};

#[pyclass]
pub struct Engine(RawEngine);

#[pymethods]
impl Engine {
    #[new]
    pub fn new(
        mut scheduler: PyRefMut<SchedulerBuilder>,
        mut strategies: Vec<PyRefMut<StrategyBuilder>>,
    ) -> Self {
        Self(RawEngine::new(
            scheduler.0.take().unwrap(),
            strategies.iter_mut().map(|s| s.0.take().unwrap()).collect(),
        ))
    }

    pub fn populate(&mut self, mut seeders: Vec<PyRefMut<SeedGeneratorBuilder>>) {
        self.0
            .populate(seeders.iter_mut().map(|s| s.0.take().unwrap()).collect());
    }

    pub fn mutate_batch(&mut self, batch_size: usize) -> Generation {
        Generation(self.0.mutate_batch(batch_size))
    }

    pub fn commit_generation(&mut self, generation: SelectedGeneration) {
        self.0.commit_generation(generation.0);
    }

    pub fn snapshot(&self) -> Vec<CorpusEntry> {
        self.0.snapshot().into_iter().map(CorpusEntry).collect()
    }

    pub fn clear_strategies(&mut self) {
        self.0.clear_strategies();
    }

    pub fn add_strategy(&mut self, mut strategy: PyRefMut<StrategyBuilder>) {
        self.0.add_strategy(strategy.0.take().unwrap());
    }
}

#[pyclass]
pub struct Generation(RawGeneration);

#[pymethods]
impl Generation {
    #[allow(clippy::wrong_self_convention)]
    pub fn into_members(&mut self) -> Vec<RawEntry> {
        self.0
            .drain(..)
            .map(|rawest| RawEntry(Some(rawest)))
            .collect()
    }
}

#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct SelectedGeneration(RawSelectedGeneration);

#[pymethods]
impl SelectedGeneration {
    #[new]
    pub fn new(members: Vec<CorpusEntry>) -> Self {
        Self(members.into_iter().map(|py_corpus| py_corpus.0).collect())
    }

    pub fn push(&mut self, member: CorpusEntry) {
        self.0.push(member.0);
    }
}

#[pyclass]
pub struct SchedulerBuilder(Option<Box<dyn Schedule>>);

#[pymethods]
impl SchedulerBuilder {
    #[staticmethod]
    pub fn fifo() -> Self {
        Self(Some(Box::new(RawFIFOScheduler {}) as Box<dyn Schedule>))
    }
}

#[pyclass]
pub struct StrategyBuilder(Option<Box<dyn MutationStrategy>>);

#[pymethods]
impl StrategyBuilder {
    #[staticmethod]
    pub fn random_uppercase(threshhold: f32) -> Self {
        Self(Some(
            Box::new(RandomUpperCase::new(threshhold)) as Box<dyn MutationStrategy>
        ))
    }

    #[staticmethod]
    pub fn merger() -> Self {
        Self(Some(Box::new(Merger) as Box<dyn MutationStrategy>))
    }

    #[staticmethod]
    pub fn random_sampler(
        max_choices: usize,
        min_choices: usize,
        mut choices: Vec<PyRefMut<StrategyBuilder>>,
    ) -> Self {
        Self(Some(Box::new(RandomMutationSampler::new(
            max_choices,
            min_choices,
            choices
                .iter_mut()
                .map(|strat| strat.0.take().unwrap())
                .collect(),
        ))))
    }

    #[staticmethod]
    pub fn slice_in() -> Self {
        Self(Some(Box::new(SliceIn {}) as Box<dyn MutationStrategy>))
    }

    #[staticmethod]
    pub fn table_scrambler() -> Self {
        Self(Some(
            Box::new(TableNameScramble {}) as Box<dyn MutationStrategy>
        ))
    }

    #[staticmethod]
    pub fn table_guard() -> Self {
        Self(Some(Box::new(TableGuard {}) as Box<dyn MutationStrategy>))
    }
}

#[pyclass]
pub struct SeedGeneratorBuilder(Option<Box<dyn ObtainSeed>>);

#[pymethods]
impl SeedGeneratorBuilder {
    #[staticmethod]
    pub fn literal(lit: &str) -> Self {
        Self(Some(
            Box::new(LiteralSeeder::new(lit.to_string())) as Box<dyn ObtainSeed>
        ))
    }
}
