use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use lsf_core::entry::Meta;
use portable_atomic::AtomicF64;

use crate::{AcceptanceReason, AdaptiveStatistics, FeedbackHook, RejectionReason, TestOutcome};

const DECAY_RATE: f64 = 0.999;
const RESCALE_FACTOR: f64 = 1e15_f64;
pub const MIN_WEIGHT: f64 = 0.001;
pub const MAX_WEIGHT: f64 = 2e2;

#[derive(Debug, Clone)]
pub struct MABConfig {
    // threshhold for syntax penalty
    pub exploration_constant: f64,
    pub max_accepted_syntax_err: f64,
    /// accepted + cov_inc should be 1. ideally
    pub weight_accepted: f64,
    pub weight_cov_inc: f64,
    /// sum(malus) shopuld be 1. ideallyu
    pub weight_timeout: f64,
    pub weight_syntax_penalty: f64,
    pub weight_size_penalty: f64,
    pub weight_time_penalty: f64,
    /// baseline size
    pub scale_size: f64,
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

#[derive(Debug, Default)]
pub struct MABBody {
    pub total_attempts: AtomicF64,
    pub current_inflation: AtomicF64,
    pub epoch: AtomicU32,
    pub normalization_epoch: AtomicU32,
    pub config: MABConfig,
}

impl MABBody {
    pub fn new() -> Self {
        Self {
            current_inflation: AtomicF64::new(1.),
            ..Default::default()
        }
    }

    pub fn with_config(mut self, config: MABConfig) -> Self {
        self.config = config;
        self
    }

    pub fn tick_by(&self, by: u32) {
        self.epoch.fetch_add(by, Ordering::Relaxed);
        // TODO if inflation gets too high, we should do a global reset for stability
        _ = self
            .current_inflation
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                Some(old * (1. / DECAY_RATE).powi(by as i32))
            });
        self.normalize();
    }

    fn normalize(&self) {
        let inflation = self.current_inflation.load(Ordering::Relaxed);

        if inflation >= RESCALE_FACTOR {
            _ = self
                .total_attempts
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR)
                });
            _ = self
                .current_inflation
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR)
                });
            self.normalization_epoch.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn reset(&self) {
        self.total_attempts.store(0., Ordering::Relaxed);
        self.epoch.store(0, Ordering::Relaxed);
        self.normalization_epoch.store(0, Ordering::Relaxed);
        self.current_inflation.store(1., Ordering::Relaxed);
    }
}

#[derive(Debug, Default)]
pub struct MABArm {
    attempts: AtomicF64,
    accepted: AtomicF64,
    cov_increases: AtomicF64,
    syntax_err: AtomicF64,
    crash: AtomicF64,
    child_timeout: AtomicF64,
    total_exec_ns: AtomicF64,
    total_query_size: AtomicF64,
    pub local_epoch: AtomicU32,
    ctx: Arc<MABBody>,
}

impl MABArm {
    pub fn new(body: Arc<MABBody>) -> Self {
        let norm_epoch = body.normalization_epoch.load(Ordering::Relaxed);
        Self {
            ctx: body,
            local_epoch: AtomicU32::new(norm_epoch),
            ..Default::default()
        }
    }

    pub fn new_with_prior(body: Arc<MABBody>, meta: &Meta) -> Self {
        let inflation = body.current_inflation.load(Ordering::Relaxed);
        let norm_epoch = body.normalization_epoch.load(Ordering::Relaxed);

        let base_attempts = 1. * inflation;
        body.total_attempts
            .fetch_add(base_attempts, Ordering::Relaxed);

        let base_cov = meta.new_cov_nodes as f64 * inflation;

        Self {
            attempts: AtomicF64::new(base_attempts),
            cov_increases: AtomicF64::new(base_cov),
            accepted: AtomicF64::new(if meta.is_valid_syntax {
                1. * inflation
            } else {
                0.
            }),
            syntax_err: AtomicF64::new(if meta.is_valid_syntax {
                0.
            } else {
                1. * inflation
            }),
            crash: AtomicF64::new(if meta.triggers_bug {
                1. * inflation
            } else {
                0.
            }),
            total_exec_ns: AtomicF64::new(meta.exec_time as f64 * inflation),
            total_query_size: AtomicF64::new(meta.query_size as f64 * inflation),
            local_epoch: AtomicU32::new(norm_epoch),
            child_timeout: AtomicF64::new(0.),
            ctx: body,
        }
    }

    pub fn reset(&self) {
        self.attempts.store(0., Ordering::Relaxed);
        self.accepted.store(0., Ordering::Relaxed);
        self.cov_increases.store(0., Ordering::Relaxed);
        self.syntax_err.store(0., Ordering::Relaxed);
        self.crash.store(0., Ordering::Relaxed);
        self.child_timeout.store(0., Ordering::Relaxed);
        self.local_epoch.store(0, Ordering::Relaxed);
        self.total_exec_ns.store(0., Ordering::Relaxed);
        self.total_query_size.store(0., Ordering::Relaxed);
    }

    fn normalize(&self) {
        let norm_epoch = self.ctx.normalization_epoch.load(Ordering::Relaxed);
        let local_epoch = self.local_epoch.load(Ordering::Relaxed);
        let diff = norm_epoch as i32 - local_epoch as i32;

        if diff != 0 {
            _ = self
                .attempts
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff))
                });
            _ = self
                .cov_increases
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff))
                });
            _ = self
                .accepted
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff))
                });
            _ = self
                .syntax_err
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff))
                });
            _ = self
                .crash
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff))
                });
            _ = self
                .total_exec_ns
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff))
                });
            _ = self
                .child_timeout
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff))
                });
            _ = self
                .total_query_size
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff))
                });

            self.local_epoch.store(norm_epoch, Ordering::Relaxed);
        }
    }
}

impl FeedbackHook for MABArm {
    fn on_exec(&self, test_outcome: crate::TestOutcome, meta: &Meta) {
        let inflation = self.ctx.current_inflation.load(Ordering::Relaxed);

        self.total_exec_ns
            .fetch_add(meta.exec_time as f64 * inflation, Ordering::Relaxed);
        self.total_query_size
            .fetch_add(meta.query_size as f64 * inflation, Ordering::Relaxed);

        match test_outcome {
            TestOutcome::Rejected(r) => match r {
                RejectionReason::SyntaxError => {
                    self.syntax_err.fetch_add(1. * inflation, Ordering::Relaxed);
                }
                RejectionReason::TriggersCrash => {
                    self.crash.fetch_add(1. * inflation, Ordering::Relaxed);
                    // implicitly reward crashes
                    self.accepted.fetch_add(1. * inflation, Ordering::Relaxed);
                }
                RejectionReason::TimeOut => {
                    // at least it has valid syntax
                    self.accepted.fetch_add(0.5 * inflation, Ordering::Relaxed);
                    self.child_timeout
                        .fetch_add(1. * inflation, Ordering::Relaxed);
                }
                // implicitly discouraged
                RejectionReason::Bad => {}
            },
            TestOutcome::Accepted(s) => match s {
                AcceptanceReason::CovIncrease(n_found) => {
                    self.accepted.fetch_add(1. * inflation, Ordering::Relaxed);
                    self.cov_increases
                        .fetch_add(n_found as f64 * inflation, Ordering::Relaxed);
                }
                AcceptanceReason::IsDiverse | AcceptanceReason::Unspecified => {
                    self.accepted.fetch_add(1. * inflation, Ordering::Relaxed);
                }
            },
            _ => {}
        }
    }

    // TODO use MutationState here and remove Mutation arms from TestOutcome -> requires moving MutationState into lsf-feedback
    fn on_mutate(&self, mutation_outcome: TestOutcome) {
        let inflation = self.ctx.current_inflation.load(Ordering::Relaxed);

        match mutation_outcome {
            TestOutcome::Mutated | TestOutcome::NOOP => {
                self.ctx
                    .total_attempts
                    .fetch_add(1. * inflation, Ordering::Relaxed);
                self.attempts.fetch_add(1. * inflation, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

impl AdaptiveStatistics for MABArm {
    fn update(&self, _test_result: TestOutcome) {}

    fn calculate_score(&self) -> f64 {
        self.normalize();

        let inflated_attempts = self.attempts.load(Ordering::Relaxed);

        if inflated_attempts < 1. {
            return MAX_WEIGHT;
        }

        let config = &self.ctx.config;

        let inflation = self.ctx.current_inflation.load(Ordering::Relaxed);

        // we want to
        // increase score for accepted ratio, coverage increase and crashes (likely a bug) and reduce it for syntax errors, as they are somewhat uninteresting
        let cov_inc_rate = self.cov_increases.load(Ordering::Relaxed) / inflated_attempts;
        let accepted_rate = self.accepted.load(Ordering::Relaxed) / inflated_attempts;

        let pros = accepted_rate * config.weight_accepted + cov_inc_rate * config.weight_cov_inc;

        // if we feed syntax errs, hangs, ... back we need to discourage them. We migth even want to discourage them anyways

        let time_out_rate = self.child_timeout.load(Ordering::Relaxed) / inflated_attempts;
        let syntax_err_rate = self.syntax_err.load(Ordering::Relaxed) / inflated_attempts;
        let avg_t_exec = self.total_exec_ns.load(Ordering::Relaxed) / inflated_attempts;
        let avg_size = self.total_query_size.load(Ordering::Relaxed) / inflated_attempts;

        let syntax_penalty = if syntax_err_rate > config.max_accepted_syntax_err {
            syntax_err_rate - config.max_accepted_syntax_err
        } else {
            0.
        };

        // 1ms, 100 chars
        let time_penalty_ratio = (avg_t_exec / 1_000_000.0).ln_1p();
        let size_penalty_ratio = (avg_size / config.scale_size).ln_1p();

        let penalty = time_out_rate * config.weight_timeout
            + syntax_penalty * config.weight_syntax_penalty
            + size_penalty_ratio * config.weight_syntax_penalty
            + time_penalty_ratio * config.weight_time_penalty;
        let cons = (1. - penalty).max(0.001);

        let exploitation = pros * cons;

        let effective_attempts = inflated_attempts / inflation;
        let effective_total_attempts =
            (self.ctx.total_attempts.load(Ordering::Relaxed) / inflation).max(1.);
        let exploration = (config.exploration_constant * (effective_total_attempts).ln()
            / (effective_attempts))
            .sqrt();

        let final_score = exploitation + exploration;

        if final_score.is_nan() {
            return MIN_WEIGHT;
        }

        final_score.clamp(MIN_WEIGHT, MAX_WEIGHT)
    }
}

pub struct SchedueldItem<T> {
    pub score: f64,
    pub epoch: u32,
    pub stats: Arc<MABArm>,
    pub item: T,
}

impl<T> SchedueldItem<T> {
    pub fn new(body: Arc<MABBody>, item: T) -> Self {
        let epoch = body.epoch.load(std::sync::atomic::Ordering::Relaxed);
        let stats = MABArm::new(body);
        Self {
            score: stats.calculate_score(),
            epoch,
            stats: stats.into(),
            item,
        }
    }

    pub fn new_with_prior(body: Arc<MABBody>, item: T, meta: &Meta) -> Self {
        let epoch = body.epoch.load(std::sync::atomic::Ordering::Relaxed);
        let stats = MABArm::new_with_prior(body, meta);
        Self {
            score: stats.calculate_score(),
            epoch,
            stats: stats.into(),
            item,
        }
    }
}

impl<T> Ord for SchedueldItem<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score
            .partial_cmp(&other.score)
            // older targets should be recalculated first on draw
            .unwrap_or_else(|| other.epoch.cmp(&self.epoch))
    }
}

impl<T> PartialOrd for SchedueldItem<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Eq for SchedueldItem<T> {}

impl<T> PartialEq for SchedueldItem<T> {
    fn eq(&self, other: &Self) -> bool {
        self.score.eq(&other.score) || self.epoch.eq(&other.epoch)
    }
}
