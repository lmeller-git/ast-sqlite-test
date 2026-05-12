use std::sync::Arc;

use lsf_cov::ipc::{IPCToken, SharedMemHandle};
use lsf_engine::{
    AdaptiveWeightedRandomScheduler,
    BinaryBlob,
    CorpusHandler,
    CorpusMinimizer,
    DynamicCorpus,
    Engine as RawEngine,
    FastProbabilisticMABScheduler,
    Generation as RawGeneration,
    GreedyCoverage,
    InMemory,
    LiteralSeeder,
    MABScheduler,
    ObtainSeed,
    ProbabilisticMABScheduler,
    SQLSaver,
    Schedule,
    SchedulerBatcher,
    SeedDirReader,
    ShardedDiskCache,
    StorageBackend,
    WeightedRandomScheduler,
};
use lsf_mutate::{
    AdaptiveStrategyScheduler,
    ArbitraryGenerator,
    ExprShuffle,
    FieldOperation,
    ForceIdent,
    HoistExpr,
    MutationStrategy,
    NOOP,
    NullInject,
    NumericBounds,
    OperatorFlip,
    RandomMutationSampler,
    Randomly,
    RecursiveExpandExpr,
    RelShuffle,
    Repeat,
    SetOps,
    SpliceIn,
    SpliceOut,
    SubQuery,
    TableGuard,
    TableNameScramble,
    TreeMutator,
    TypeCast,
};
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
use sqlparser::ast::{Expr, Statement};

use crate::{CorpusEntry, TestableEntry};

#[pyclass]
pub struct Engine(RawEngine);

#[pymethods]
impl Engine {
    #[new]
    #[pyo3(signature = (scheduler, corpus_handler, corpus_minimzer, strategies, shmem_queue, mab_bodies = Vec::new(), rng_seed = 42))]
    pub fn new(
        mut scheduler: PyRefMut<SchedulerBuilder>,
        mut corpus_handler: PyRefMut<CorpusManagerBuilder>,
        mut corpus_minimzer: PyRefMut<CorpusMinimizerBuilder>,
        mut strategies: Vec<PyRefMut<StrategyBuilder>>,
        shmem_queue: PyRef<IPCTokenQueue>,
        mab_bodies: Vec<PyRef<MABBody>>,
        rng_seed: u64,
    ) -> Self {
        Self(RawEngine::new(
            scheduler.0.take().unwrap(),
            corpus_handler.0.take().unwrap(),
            corpus_minimzer.0.take().unwrap(),
            strategies.iter_mut().map(|s| s.0.take().unwrap()).collect(),
            shmem_queue.0.clone(),
            mab_bodies.iter().map(|b| b.0.clone()).collect(),
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
        let raw = raw.0.take().unwrap();
        let meta = lsf_core::entry::Meta {
            triggers_bug: data.triggers_bug,
            is_valid_syntax: data.is_valid_syntax,
            new_cov_nodes: 0,
            exec_time: data.exec_time,
            query_size: data.query_size,
        };
        let token = data.token.take().unwrap();
        self.0.commit_test_result(raw, meta, token);
    }

    pub fn return_token(&mut self, mut token: PyRefMut<IPCTokenHandle>) {
        let token = token.0.take().unwrap();
        self.0.return_token(token);
    }

    pub fn snapshot(&mut self) -> Vec<CorpusEntry> {
        self.0.snapshot().into_iter().map(CorpusEntry).collect()
    }

    pub fn clear_strategies(&mut self) {
        self.0.clear_strategies();
    }

    pub fn add_strategy(&mut self, mut strategy: PyRefMut<StrategyBuilder>) {
        self.0.add_strategy(strategy.0.take().unwrap());
    }

    pub fn chore(&mut self) {
        self.0.chore();
    }

    pub fn corpus_size(&self) -> usize {
        self.0.corpus_size()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[pyclass]
pub struct CorpusMinimizerBuilder(Option<Box<dyn CorpusMinimizer<f64>>>);

#[pymethods]
impl CorpusMinimizerBuilder {
    #[staticmethod]
    pub fn greedy_coverage(max_edges: usize) -> Self {
        Self(Some(Box::new(GreedyCoverage::new(max_edges))))
    }
}

#[pyclass]
pub struct CorpusManagerBuilder(Option<Box<dyn CorpusHandler<f64>>>);

#[pymethods]
impl CorpusManagerBuilder {
    #[staticmethod]
    pub fn dynamic_cache(mut disk_cache: PyRefMut<DiskCacheBuilder>) -> Self {
        Self(Some(Box::new(DynamicCorpus::new(
            disk_cache.0.take().unwrap(),
        ))))
    }

    #[staticmethod]
    pub fn in_memory() -> Self {
        Self(Some(Box::new(InMemory::new())))
    }
}

#[pyclass]
pub struct DiskCacheBuilder(Option<Box<dyn StorageBackend>>);

#[pymethods]
impl DiskCacheBuilder {
    #[staticmethod]
    pub fn sharded(cache_dir: String) -> Self {
        Self(Some(Box::new(ShardedDiskCache::new(cache_dir.into()))))
    }

    #[staticmethod]
    pub fn blob(cache_dir: String) -> Self {
        Self(Some(Box::new(BinaryBlob::new(cache_dir.into()))))
    }

    #[staticmethod]
    pub fn sql_saver(mut backend: PyRefMut<DiskCacheBuilder>, save_dir: String) -> Self {
        Self(Some(Box::new(SQLSaver::new(
            backend.0.take().unwrap(),
            save_dir.into(),
        ))))
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
    #[pyo3(get, set)]
    pub query_size: usize,
    token: Option<Box<IPCToken>>,
}

#[pymethods]
impl TestResult {
    #[new]
    #[pyo3(signature = (exec_time, query_size, token, is_valid_syntax = false, triggers_bug = false))]
    pub fn new(
        exec_time: u32,
        query_size: usize,
        mut token: PyRefMut<IPCTokenHandle>,
        is_valid_syntax: bool,
        triggers_bug: bool,
    ) -> Self {
        Self {
            triggers_bug,
            is_valid_syntax,
            exec_time,
            token: token.0.take(),
            query_size,
        }
    }

    #[getter]
    pub fn token(&mut self) -> IPCTokenHandle {
        self.token.take().map(|t| IPCTokenHandle(Some(t))).unwrap()
    }
}
#[pyclass(from_py_object)]
#[derive(Debug, Clone)]
pub struct MABConfig {
    // threshhold for syntax penalty
    #[pyo3(get, set)]
    pub exploration_constant: f64,
    #[pyo3(get, set)]
    pub max_accepted_syntax_err: f64,
    /// accepted + cov_inc should be 1. ideally
    #[pyo3(get, set)]
    pub weight_accepted: f64,
    #[pyo3(get, set)]
    pub weight_cov_inc: f64,
    /// sum(malus) shopuld be 1. ideallyu
    #[pyo3(get, set)]
    pub weight_timeout: f64,
    #[pyo3(get, set)]
    pub weight_syntax_penalty: f64,
    #[pyo3(get, set)]
    pub weight_size_penalty: f64,
    #[pyo3(get, set)]
    pub weight_time_penalty: f64,
    /// baseline size
    #[pyo3(get, set)]
    pub scale_size: f64,
}

#[pymethods]
impl MABConfig {
    #[staticmethod]
    pub fn new_default() -> Self {
        Self::default()
    }
}

impl Default for MABConfig {
    fn default() -> Self {
        Self {
            exploration_constant: 4.0,
            max_accepted_syntax_err: 0.5,
            weight_accepted: 0.1,
            weight_cov_inc: 0.9,
            weight_timeout: 0.33,
            weight_syntax_penalty: 0.33,
            weight_size_penalty: 0.16,
            weight_time_penalty: 0.16,
            scale_size: 100.0,
        }
    }
}

#[allow(clippy::from_over_into)]
impl Into<lsf_feedback::mab::MABConfig> for MABConfig {
    fn into(self) -> lsf_feedback::mab::MABConfig {
        lsf_feedback::mab::MABConfig {
            exploration_constant: self.exploration_constant,
            max_accepted_syntax_err: self.max_accepted_syntax_err,
            weight_accepted: self.weight_accepted,
            weight_cov_inc: self.weight_cov_inc,
            weight_timeout: self.weight_timeout,
            weight_syntax_penalty: self.weight_syntax_penalty,
            weight_size_penalty: self.weight_size_penalty,
            weight_time_penalty: self.weight_time_penalty,
            scale_size: self.scale_size,
        }
    }
}

#[pyclass]
pub struct MABBody(Arc<lsf_feedback::mab::MABBody>);

#[pymethods]
impl MABBody {
    #[new]
    pub fn new() -> Self {
        Self(Arc::new(lsf_feedback::mab::MABBody::new()))
    }

    pub fn with_config(&mut self, config: MABConfig) {
        let mut_body = Arc::get_mut(&mut self.0).unwrap();
        mut_body.config = config.into();
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
    pub fn ucb1(body: PyRef<MABBody>) -> Self {
        Self(Some(Box::new(MABScheduler::new(body.0.clone()))))
    }

    #[staticmethod]
    pub fn weighted_ucb1(body: PyRef<MABBody>) -> Self {
        Self(Some(Box::new(ProbabilisticMABScheduler::new(
            body.0.clone(),
        ))))
    }

    #[staticmethod]
    pub fn batched(mut scheduler: PyRefMut<SchedulerBuilder>) -> Self {
        Self(Some(Box::new(SchedulerBatcher::new(
            scheduler.0.take().unwrap(),
        ))))
    }

    #[staticmethod]
    pub fn fast_weigthed_ucb1(body: PyRef<MABBody>) -> Self {
        Self(Some(Box::new(FastProbabilisticMABScheduler::new(
            body.0.clone(),
        ))))
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
    #[pyo3(signature = (p_extend = 0.5))]
    pub fn splice_in(p_extend: f64) -> Self {
        Self(Some(Box::new(SpliceIn::new(p_extend))))
    }

    #[staticmethod]
    #[pyo3(signature = (p_extend = 0.5))]
    pub fn splice_out(p_extend: f64) -> Self {
        Self(Some(Box::new(SpliceOut::new(p_extend))))
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
    pub fn scheduled(mut strategy: PyRefMut<StrategyBuilder>, body: PyRef<MABBody>) -> Self {
        Self(Some(Box::new(AdaptiveStrategyScheduler::new(
            strategy.0.take().unwrap(),
            body.0.clone(),
        ))))
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
    #[pyo3(signature = (max_depth = 3, chance_per_node = 0.2, chance_per_level = 0.5))]
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

    #[staticmethod]
    #[pyo3(signature = (chance_per_node = 0.2))]
    pub fn hoist_expr(chance_per_node: f64) -> Self {
        Self(Some(Box::new(HoistExpr { chance_per_node })))
    }

    #[staticmethod]
    #[pyo3(signature = (body, strategies, choose = 1))]
    pub fn ucb1(
        body: PyRef<MABBody>,
        mut strategies: Vec<PyRefMut<StrategyBuilder>>,
        choose: usize,
    ) -> Self {
        Self(Some(Box::new(lsf_mutate::MABScheduler::new(
            body.0.clone(),
            strategies.iter_mut().map(|s| s.0.take().unwrap()),
            choose,
        ))))
    }

    #[staticmethod]
    pub fn arbitrary_stmt_generator() -> Self {
        Self(Some(Box::new(ArbitraryGenerator::<Statement>::new())))
    }

    #[staticmethod]
    pub fn arbitrary_expr_generator() -> Self {
        Self(Some(Box::new(ArbitraryGenerator::<Expr>::new())))
    }

    #[staticmethod]
    pub fn repeat(mut rule: PyRefMut<StrategyBuilder>, up_to: usize) -> Self {
        Self(Some(Box::new(Repeat {
            up_to,
            inner: rule.0.take().unwrap(),
        })))
    }

    #[staticmethod]
    pub fn noop() -> Self {
        Self(Some(Box::new(NOOP)))
    }

    #[staticmethod]
    pub fn force_ident() -> Self {
        Self(Some(Box::new(ForceIdent)))
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

#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct IPCTokenQueue(SharedMemHandle);

#[pymethods]
impl IPCTokenQueue {
    #[new]
    pub fn new(n_workers: usize, max_edges: usize) -> Self {
        Self(SharedMemHandle::new(n_workers, max_edges))
    }

    pub fn send(&self, py: Python, mut token: PyRefMut<IPCTokenHandle>) {
        let token = token.0.take().unwrap();
        py.detach(|| self.0.send(token))
    }

    pub fn recv<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let rx_clone = self.0.rx().clone();

        future_into_py(py, async move {
            let token = rx_clone.recv_async().await;
            if let Ok(t) = token {
                Ok(Some(IPCTokenHandle(Some(t))))
            } else {
                Ok(None)
            }
        })
    }
}
