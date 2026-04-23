use std::{
    f64,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use lsf_core::entry::RawEntry;
use lsf_feedback::{
    AcceptanceReason,
    AdaptiveStatistics,
    FeedbackHook,
    RejectionReason,
    TestOutcome,
    TestableEntry,
};
use rand::RngExt;

use crate::{MutationError, MutationState, MutationStrategy, StrategyContext};

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
        parent: &TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<MutationState, MutationError> {
        let score = self.stats.calculate_score();
        let ratio = if score.is_infinite() {
            // initialize with a smaller probability to allow schedulers to diverege immediately
            0.3
        } else {
            sigmoid(score)
        };
        if !rng.random_bool(ratio) {
            return Ok(MutationState::Unchanged);
        }

        let mut r = self.strategy.breed(parent, parent_gen, rng);

        if let Ok(MutationState::Mutated(result)) = &mut r {
            self.stats.attempts.fetch_add(1, Ordering::Relaxed);
            result.attach_hook(self.stats.clone());
        }

        r
    }

    fn init(&mut self, ctx: StrategyContext) {
        self.strategy.init(ctx.clone());
        let stat_ref = Arc::make_mut(&mut self.stats);
        stat_ref.total_attempts = ctx.total_attempts.clone();
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
    total_attempts: Arc<AtomicU32>,
}

impl FeedbackHook for StrategySchedulerStats {
    fn fire(&self, test_outcome: lsf_feedback::TestOutcome) {
        self.update(test_outcome);
    }
}

impl AdaptiveStatistics for StrategySchedulerStats {
    fn update(&self, test_result: TestOutcome) {
        match test_result {
            TestOutcome::Rejected(r) => match r {
                RejectionReason::SyntaxError => {
                    self.syntax_err.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::TriggersCrash => {
                    self.crash.fetch_add(1, Ordering::Relaxed);
                }
                RejectionReason::Bad => {}
            },
            TestOutcome::Accepted(s) => match s {
                AcceptanceReason::CovIncrease => {
                    self.accepted.fetch_add(1, Ordering::Relaxed);
                    self.cov_increases.fetch_add(1, Ordering::Relaxed);
                }
                AcceptanceReason::IsDiverse => {
                    self.accepted.fetch_add(1, Ordering::Relaxed);
                }
            },
        }
    }

    fn calculate_score(&self) -> f64 {
        // ucb1
        // TODO add more relevant terms
        let total_attempts = self.total_attempts.load(Ordering::Relaxed);
        if total_attempts == 0 {
            return f64::INFINITY;
        }
        let attempts = self.attempts.load(Ordering::Relaxed);
        if attempts == 0 {
            return f64::INFINITY;
        }

        // we want to
        // increase score for accepted ratio, coverage increase and crashes (likely a bug) and reduce it for syntax errors, as they are somewhat uninteresting
        let cov_inc_rate = (self.cov_increases.load(Ordering::Relaxed) as f64
            + self.accepted.load(Ordering::Relaxed) as f64 * 0.2
            + self.crash.load(Ordering::Relaxed) as f64 * 2.
            - self.syntax_err.load(Ordering::Relaxed) as f64 * 0.5)
            / attempts as f64;
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
            total_attempts: self.total_attempts.clone(),
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
