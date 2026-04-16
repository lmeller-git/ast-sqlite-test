use std::sync::Arc;

use lsf_cov::ipc::{IPCToken, SharedMemHandle};
use lsf_engine::{
    Engine as RawEngine,
    FIFOScheduler as RawFIFOScheduler,
    Generation as RawGeneration,
    LiteralSeeder,
    ObtainSeed,
    Schedule,
    SeedDirReader,
};
use lsf_mutate::{
    Merger,
    MutationStrategy,
    RandomMutationSampler,
    RandomUpperCase,
    Randomly,
    SpliceIn,
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
    #[pyo3(signature = (scheduler, strategies, shmem_queue, rng_seed = 42))]
    pub fn new(
        mut scheduler: PyRefMut<SchedulerBuilder>,
        mut strategies: Vec<PyRefMut<StrategyBuilder>>,
        shmem_queue: PyRef<IPCTokenQueue>,
        rng_seed: u64,
    ) -> Self {
        Self(RawEngine::new(
            scheduler.0.take().unwrap(),
            strategies.iter_mut().map(|s| s.0.take().unwrap()).collect(),
            shmem_queue.0.clone(),
            rng_seed,
        ))
    }

    pub fn populate(&mut self, mut seeders: Vec<PyRefMut<SeedGeneratorBuilder>>) {
        self.0
            .populate(seeders.iter_mut().map(|s| s.0.take().unwrap()).collect());
    }

    pub fn mutate_batch(&mut self, batch_size: usize) -> Generation {
        Generation(self.0.mutate_batch(batch_size))
    }

    pub fn commit_test_result(
        &mut self,
        mut raw: PyRefMut<RawEntry>,
        mut data: PyRefMut<TestResult>,
    ) {
        self.0.commit_test_result(
            raw.0.take().unwrap(),
            lsf_core::entry::Meta {
                triggers_bug: data.triggers_bug,
                is_valid_syntax: data.is_valid_syntax,
                new_cov_nodes: 0,
                exec_time: data.exec_time,
            },
            data.token.take().unwrap(),
        );
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

#[pyclass]
pub struct TestResult {
    #[pyo3(get, set)]
    pub triggers_bug: bool,
    #[pyo3(get, set)]
    pub is_valid_syntax: bool,
    #[pyo3(get, set)]
    pub exec_time: u32,
    token: Option<Box<IPCToken>>,
}

#[pymethods]
impl TestResult {
    #[new]
    #[pyo3(signature = (exec_time, token, is_valid_syntax = false, triggers_bug = false))]
    pub fn new(
        exec_time: u32,
        mut token: PyRefMut<IPCTokenHandle>,
        is_valid_syntax: bool,
        triggers_bug: bool,
    ) -> Self {
        Self {
            triggers_bug,
            is_valid_syntax,
            exec_time,
            token: token.0.take(),
        }
    }
}

#[pyclass]
pub struct SchedulerBuilder(Option<Box<dyn Schedule>>);

#[pymethods]
impl SchedulerBuilder {
    #[staticmethod]
    pub fn fifo() -> Self {
        Self(Some(Box::new(RawFIFOScheduler {})))
    }
}

#[pyclass]
pub struct StrategyBuilder(Option<Box<dyn MutationStrategy>>);

#[pymethods]
impl StrategyBuilder {
    #[staticmethod]
    pub fn uppercase() -> Self {
        Self(Some(Box::new(RandomUpperCase::new())))
    }

    #[staticmethod]
    pub fn merger() -> Self {
        Self(Some(Box::new(Merger)))
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
    pub fn randomize(mut strategy: PyRefMut<StrategyBuilder>, probability: f64) -> Self {
        Self(Some(Box::new(Randomly::new(
            strategy.0.take().unwrap(),
            probability,
        ))))
    }

    #[staticmethod]
    pub fn splice_in() -> Self {
        Self(Some(Box::new(SpliceIn {})))
    }

    #[staticmethod]
    pub fn table_scrambler() -> Self {
        Self(Some(Box::new(TableNameScramble {})))
    }

    #[staticmethod]
    pub fn table_guard() -> Self {
        Self(Some(Box::new(TableGuard {})))
    }
}

#[pyclass]
pub struct SeedGeneratorBuilder(Option<Box<dyn ObtainSeed>>);

#[pymethods]
impl SeedGeneratorBuilder {
    #[staticmethod]
    pub fn literal(lit: &str) -> Self {
        Self(Some(Box::new(LiteralSeeder::new(lit.to_string()))))
    }

    #[staticmethod]
    pub fn dir_reader(dir: &str) -> Self {
        Self(Some(Box::new(SeedDirReader::new(dir.into()))))
    }
}

#[pyclass]
pub struct IPCTokenHandle(Option<Box<IPCToken>>);

#[pymethods]
impl IPCTokenHandle {
    pub fn as_env(&self) -> String {
        self.0.as_ref().map(|t| t.get_path().to_string()).unwrap()
    }
}

#[pyclass]
pub struct IPCTokenQueue(Arc<SharedMemHandle>);

#[pymethods]
impl IPCTokenQueue {
    #[new]
    pub fn new(n_workers: usize, max_edges: usize) -> Self {
        Self(Arc::new(SharedMemHandle::new(n_workers, max_edges)))
    }

    pub fn pop(&self) -> Option<IPCTokenHandle> {
        self.0.pop().map(|token| IPCTokenHandle(Some(token)))
    }

    pub fn push(&self, mut token: PyRefMut<IPCTokenHandle>) -> Option<IPCTokenHandle> {
        self.0
            .push(token.0.take().unwrap())
            .map_or_else(|tok| Some(IPCTokenHandle(Some(tok))), |_| None)
    }
}
