use std::{
    f64,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use rand::RngExt;

use crate::{MutationError, MutationState, MutationStrategy, StrategyContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationHook {
    hooked_stats: Arc<StrategySchedulerStats>,
}

impl MutationHook {
    pub fn new(stats: Arc<StrategySchedulerStats>) -> Self {
        Self {
            hooked_stats: stats,
        }
    }

    pub fn fire(&self, with: TestOutcome) {
        match with {
            TestOutcome::Rejected(r) => match r {
                RejectionReason::SyntaxError => {
                    self.hooked_stats.syntax_err.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::TriggersCrash => {
                    self.hooked_stats.crash.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::Bad => {}
            },
            TestOutcome::Accepted(s) => match s {
                AcceptanceReason::CovIncrease => {
                    self.hooked_stats.accepted.fetch_add(1, Ordering::Relaxed);
                    self.hooked_stats
                        .cov_increases
                        .fetch_add(1, Ordering::Relaxed);
                }
                AcceptanceReason::IsDiverse => {
                    self.hooked_stats.accepted.fetch_add(1, Ordering::Relaxed);
                }
            },
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TestOutcome {
    Rejected(RejectionReason),
    Accepted(AcceptanceReason),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RejectionReason {
    SyntaxError,
    TriggersCrash,
    Bad,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AcceptanceReason {
    CovIncrease,
    IsDiverse,
}

pub struct AdaptiveStrategyScheduler {
    strategy: Box<dyn MutationStrategy>,
    stats: Arc<StrategySchedulerStats>,
    ctx: StrategyContext,
}

impl AdaptiveStrategyScheduler {
    pub fn new(strategy: Box<dyn MutationStrategy>) -> Self {
        Self {
            strategy,
            stats: Arc::new(StrategySchedulerStats::default()),
            ctx: StrategyContext::default(),
        }
    }
}

impl MutationStrategy for AdaptiveStrategyScheduler {
    fn breed(
        &self,
        parent: &lsf_core::entry::RawEntry,
        parent_gen: &[lsf_core::entry::ID],
        mapping: &std::collections::HashMap<lsf_core::entry::ID, lsf_core::entry::CorpusEntry>,
        rng: &mut dyn rand::Rng,
    ) -> Result<MutationState, MutationError> {
        let score = self
            .stats
            .calculate_score(self.ctx.total_attempts.load(Ordering::Relaxed));
        let ratio = if score.is_infinite() {
            // initialize with a smaller probability to allow schedulers to diverege immediately
            0.3
        } else {
            sigmoid(score)
        };
        if !rng.random_bool(ratio) {
            return Ok(MutationState::Unchanged);
        }
        self.stats.attempts.fetch_add(1, Ordering::Relaxed);
        self.strategy.breed(parent, parent_gen, mapping, rng)
    }

    fn init(&mut self, ctx: StrategyContext) {
        self.strategy.init(ctx.clone());
        self.ctx = ctx
    }
}

fn sigmoid(val: f64) -> f64 {
    1. / (1. + (-val).exp())
}

#[derive(Debug, Default)]
pub struct StrategySchedulerStats {
    attempts: AtomicU32,
    accepted: AtomicU32,
    cov_increases: AtomicU32,
    syntax_err: AtomicU32,
    crash: AtomicU32,
}

impl StrategySchedulerStats {
    fn calculate_score(&self, total_attempts: u32) -> f64 {
        // ucb1
        // TODO add more relevant terms
        if total_attempts == 0 {
            return f64::INFINITY;
        }
        let attempts = self.attempts.load(Ordering::Relaxed);
        if attempts == 0 {
            return f64::INFINITY;
        }

        let cov_inc_rate = self.cov_increases.load(Ordering::Relaxed) as f64 / attempts as f64;
        let exploration = (2.0 * (total_attempts as f64).ln() / attempts as f64).sqrt();

        cov_inc_rate + exploration
    }
}

impl Clone for StrategySchedulerStats {
    fn clone(&self) -> Self {
        Self {
            attempts: self.attempts.load(Ordering::Relaxed).into(),
            accepted: self.accepted.load(Ordering::Relaxed).into(),
            cov_increases: self.cov_increases.load(Ordering::Relaxed).into(),
            syntax_err: self.syntax_err.load(Ordering::Relaxed).into(),
            crash: self.crash.load(Ordering::Relaxed).into(),
        }
    }
}

impl Eq for StrategySchedulerStats {}

impl PartialEq for StrategySchedulerStats {
    fn eq(&self, other: &Self) -> bool {
        self.attempts
            .load(Ordering::Relaxed)
            .eq(&other.attempts.load(Ordering::Relaxed))
            && self
                .accepted
                .load(Ordering::Relaxed)
                .eq(&other.accepted.load(Ordering::Relaxed))
            && self
                .cov_increases
                .load(Ordering::Relaxed)
                .eq(&other.cov_increases.load(Ordering::Relaxed))
            && self
                .syntax_err
                .load(Ordering::Relaxed)
                .eq(&other.syntax_err.load(Ordering::Relaxed))
            && self
                .crash
                .load(Ordering::Relaxed)
                .eq(&other.crash.load(Ordering::Relaxed))
    }
}
