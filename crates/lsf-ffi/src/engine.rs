use std::sync::{Arc, Mutex, atomic::AtomicBool};

use lsf_cov::ipc::{IPCToken, SharedMemHandle};
use lsf_engine::{
    AdaptiveWeightedRandomScheduler,
    Engine as RawEngine,
    Generation as RawGeneration,
    LiteralSeeder,
    ObtainSeed,
    Schedule,
    SeedDirReader,
    WeightedRandomScheduler,
};
use lsf_feedback::{GenericHook, Hookable, SchedulerStatisticsSnapshot};
use lsf_mutate::{
    AdaptiveStrategyScheduler,
    ExprShuffle,
    FieldOperation,
    MutationStrategy,
    NullInject,
    NumericBounds,
    OperatorFlip,
    RandomMutationSampler,
    Randomly,
    RecursiveExpandExpr,
    RelShuffle,
    SetOps,
    SpliceIn,
    SubQuery,
    TableGuard,
    TableNameScramble,
    TreeMutator,
    TypeCast,
};
use pyo3::{prelude::*, sync::MutexExt};
use sqlparser::ast::{Expr, Statement};

use crate::{CorpusEntry, TestableEntry};

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
        mut raw: PyRefMut<TestableEntry>,
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

    pub fn return_token(&mut self, mut token: PyRefMut<IPCTokenHandle>) {
        self.0.return_token(token.0.take().unwrap());
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

    pub fn gc(&mut self) {
        self.0.gc();
    }

    pub fn corpus_size(&self) -> usize {
        self.0.corpus_size()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[pyclass]
pub struct Generation(RawGeneration);

#[pymethods]
impl Generation {
    #[allow(clippy::wrong_self_convention)]
    pub fn into_members(&mut self) -> Vec<TestableEntry> {
        self.0
            .drain(..)
            .map(|rawest| TestableEntry(Some(rawest)))
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

pub struct SchedulerHook__ {
    stats: Mutex<Vec<SchedulerStatisticsSnapshot>>,
    dirty: AtomicBool,
}

impl GenericHook for SchedulerHook__ {
    fn on_snapshot(&self, snapshot: SchedulerStatisticsSnapshot) {
        self.stats.lock().unwrap().push(snapshot);
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[pyclass(skip_from_py_object)]
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SchedulerSnapshot {
    #[pyo3(get)]
    pub epoch: u32,
    #[pyo3(get)]
    pub global_attempts: Option<f64>,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub meta: Vec<String>,
    #[pyo3(get)]
    pub self_attempts: Vec<f64>,
    #[pyo3(get)]
    pub cov_increases: Vec<f64>,
    #[pyo3(get)]
    pub accepted: Vec<f64>,
    #[pyo3(get)]
    pub syntax_err: Vec<f64>,
    #[pyo3(get)]
    pub crashes: Vec<f64>,
    #[pyo3(get)]
    pub rating: Vec<f64>,
    #[pyo3(get)]
    pub rating_as_prob: Vec<f64>,
}

impl From<SchedulerStatisticsSnapshot> for SchedulerSnapshot {
    fn from(value: SchedulerStatisticsSnapshot) -> Self {
        Self {
            epoch: value.epoch,
            global_attempts: value.global_attempts,
            name: value.name,
            meta: value.meta,
            self_attempts: value.self_attmepts,
            cov_increases: value.cov_increases,
            accepted: value.accepted,
            syntax_err: value.synatx_err,
            crashes: value.crashes,
            rating: value.rating,
            rating_as_prob: value.rating_as_prob,
        }
    }
}

#[pyclass]
pub struct SchedulerHook(Arc<SchedulerHook__>);

#[pymethods]
impl SchedulerHook {
    #[new]
    pub fn new() -> Self {
        Self(
            SchedulerHook__ {
                stats: Mutex::default(),
                dirty: false.into(),
            }
            .into(),
        )
    }

    pub fn drain(&self, py: Python<'_>) -> Vec<SchedulerSnapshot> {
        let drain = self
            .0
            .stats
            .lock_py_attached(py)
            .map(|mut s| s.drain(..).map(|s| s.into()).collect())
            .unwrap_or_default();
        self.0
            .dirty
            .store(false, std::sync::atomic::Ordering::Relaxed);
        drain
    }

    pub fn dirty(&self) -> bool {
        self.0.dirty.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[pyclass]
pub struct SchedulerBuilder(Option<Box<dyn Schedule>>);

#[pymethods]
impl SchedulerBuilder {
    #[staticmethod]
    pub fn weighted_random() -> Self {
        Self(Some(Box::new(WeightedRandomScheduler {})))
    }

    #[staticmethod]
    pub fn adaptive_weighted_random() -> Self {
        Self(Some(Box::new(AdaptiveWeightedRandomScheduler::default())))
    }

    #[staticmethod]
    pub fn hooked_adaptive_weighted_random(hook: PyRef<SchedulerHook>) -> Self {
        let mut scheduler = AdaptiveWeightedRandomScheduler::default();
        scheduler.attach_hook(hook.0.clone());
        Self(Some(Box::new(scheduler)))
    }
}

#[pyclass]
pub struct StrategyBuilder(Option<Box<dyn MutationStrategy>>);

#[pymethods]
impl StrategyBuilder {
    #[staticmethod]
    pub fn random_sampler(
        min_choices: usize,
        max_choices: usize,
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
    pub fn table_guard() -> Self {
        Self(Some(Box::new(TableGuard {})))
    }

    #[staticmethod]
    #[pyo3(signature = (flip_chance = 0.3))]
    pub fn op_flip(flip_chance: f64) -> Self {
        Self(Some(Box::new(OperatorFlip { flip_chance })))
    }

    #[staticmethod]
    #[pyo3(signature = (mutate_chance = 0.3))]
    pub fn num_bounds(mutate_chance: f64) -> Self {
        Self(Some(Box::new(NumericBounds { mutate_chance })))
    }

    #[staticmethod]
    #[pyo3(signature = (mutation_chance = 0.3))]
    pub fn null_inject(mutation_chance: f64) -> Self {
        Self(Some(Box::new(NullInject { mutation_chance })))
    }

    #[staticmethod]
    #[pyo3(signature = (mutation_chance = 0.3))]
    pub fn type_cast(mutation_chance: f64) -> Self {
        Self(Some(Box::new(TypeCast { mutation_chance })))
    }

    #[staticmethod]
    pub fn set_ops() -> Self {
        Self(Some(Box::new(SetOps {})))
    }

    #[staticmethod]
    #[pyo3(signature = (mutation_chance = 0.3))]
    pub fn sub_query(mutation_chance: f64) -> Self {
        Self(Some(Box::new(SubQuery { mutation_chance })))
    }

    #[staticmethod]
    #[pyo3(signature = (mutation_chance = 0.3))]
    pub fn expr_shuffle(mutation_chance: f64) -> Self {
        Self(Some(Box::new(ExprShuffle {
            chance_per_node: mutation_chance,
        })))
    }

    #[staticmethod]
    #[pyo3(signature = (mutation_chance = 0.3))]
    pub fn relation_shuffle(mutation_chance: f64) -> Self {
        Self(Some(Box::new(RelShuffle {
            chance_per_node: mutation_chance,
        })))
    }

    #[staticmethod]
    pub fn scheduled(mut strategy: PyRefMut<StrategyBuilder>) -> Self {
        Self(Some(Box::new(AdaptiveStrategyScheduler::new(
            strategy.0.take().unwrap(),
        ))))
    }

    #[staticmethod]
    pub fn hooked_scheduled(
        mut strategy: PyRefMut<StrategyBuilder>,
        hook: PyRef<SchedulerHook>,
    ) -> Self {
        let mut scheduler = AdaptiveStrategyScheduler::new(strategy.0.take().unwrap());
        scheduler.attach_hook(hook.0.clone());
        Self(Some(Box::new(scheduler)))
    }

    #[staticmethod]
    #[pyo3(signature = (operator, chance_per_node = 0.3, chance_per_field = 0.1))]
    pub fn tree_mutate_stmt(
        operator: TreeMutatorOperation,
        chance_per_node: f64,
        chance_per_field: f64,
    ) -> Self {
        Self(Some(Box::new(TreeMutator {
            chance_per_node,
            chance_per_field,
            operation: operator.0,
            _phantom: std::marker::PhantomData::<Statement>,
        })))
    }

    #[staticmethod]
    #[pyo3(signature = (operator, chance_per_node = 0.3, chance_per_field = 0.1))]
    pub fn tree_mutate_expr(
        operator: TreeMutatorOperation,
        chance_per_node: f64,
        chance_per_field: f64,
    ) -> Self {
        Self(Some(Box::new(TreeMutator {
            chance_per_node,
            chance_per_field,
            operation: operator.0,
            _phantom: std::marker::PhantomData::<Expr>,
        })))
    }

    #[staticmethod]
    pub fn table_name_guard() -> Self {
        Self(Some(Box::new(TableNameScramble {})))
    }

    #[staticmethod]
    #[pyo3(signature = (max_depth = 3, chance_per_node = 0.1, chance_per_level = 0.5))]
    pub fn recursive_expand_expr(
        max_depth: usize,
        chance_per_node: f64,
        chance_per_level: f64,
    ) -> Self {
        Self(Some(Box::new(RecursiveExpandExpr {
            max_depth,
            chance_per_node,
            chance_per_level,
        })))
    }
}

#[pyclass(from_py_object)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeMutatorOperation(FieldOperation);

#[pymethods]
impl TreeMutatorOperation {
    #[staticmethod]
    pub fn shuffle_two() -> Self {
        Self(FieldOperation::ShuffleTwo)
    }

    #[staticmethod]
    pub fn shuffle_self() -> Self {
        Self(FieldOperation::ShuffleSelf)
    }

    #[staticmethod]
    pub fn null_random() -> Self {
        Self(FieldOperation::NullRandom)
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

    pub fn id(&self) -> usize {
        self.0.as_ref().map(|t| t.id()).unwrap()
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
