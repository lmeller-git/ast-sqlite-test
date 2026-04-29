use std::{
    f64,
    fmt::Debug,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use lsf_core::{AtomicF64Ext, entry::RawEntry};
use lsf_feedback::{
    AcceptanceReason,
    AdaptiveStatistics,
    FeedbackHook,
    GenericHook,
    Hookable,
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
    hook: Option<Arc<dyn GenericHook>>,
}

impl Debug for AdaptiveStrategyScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdaptiveStrategyScheduler")
            .field("strategy", &self.strategy)
            .finish()
    }
}

impl AdaptiveStrategyScheduler {
    pub fn new(strategy: Box<dyn MutationStrategy>) -> Self {
        Self {
            strategy,
            stats: Arc::new(StrategySchedulerStats::default()),
            ctx: StrategyContext::default(),
            hook: None,
        }
    }
}

impl MutationStrategy for AdaptiveStrategyScheduler {
    fn breed_inner(
        &self,
        parent: &TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<MutationState, MutationError> {
        self.strategy.breed_inner(parent, parent_gen, rng)
    }

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

        // this should optimally sit in update, as now probability is updated WITHIN one epoch, even though no stats are collected.
        // It is here right now, since we also need to updated this even if we do not insert a hook, i.e. if r != MutationState::Mutated.
        // This would require some kind of NullHook (TODO add later)
        self.stats.attempts.atomic_add_f64(1., Ordering::Relaxed);
        self.stats
            .total_attempts
            .atomic_add_f64(1., Ordering::Relaxed);

        let mut r = self.breed_inner(parent, parent_gen, rng);

        if let Ok(MutationState::Mutated(result)) = &mut r {
            result.attach_hook(self.stats.clone());
            parent.fire_build_hooks(TestOutcome::Mutated);
        } else {
            parent.fire_build_hooks(TestOutcome::NOOP);
        }
        r
    }

    fn init(&mut self, ctx: StrategyContext) {
        self.strategy.init(ctx.clone());
        let stat_ref = Arc::make_mut(&mut self.stats);
        stat_ref.total_attempts = ctx.total_attempts.clone();
        self.ctx = ctx
    }

    fn decay(&self, rate: f64) {
        self.stats
            .attempts
            .atomic_multiply_f64(rate, Ordering::Relaxed);
        self.stats
            .accepted
            .atomic_multiply_f64(rate, Ordering::Relaxed);
        self.stats
            .cov_increases
            .atomic_multiply_f64(rate, Ordering::Relaxed);
        self.stats
            .syntax_err
            .atomic_multiply_f64(rate, Ordering::Relaxed);
        self.stats
            .crash
            .atomic_multiply_f64(rate, Ordering::Relaxed);
    }

    fn snapshot_rule(&self) {
        if let Some(hook) = &self.hook {
            hook.on_snapshot(self.snapshot());
        }
    }
}

fn sigmoid(val: f64) -> f64 {
    1. / (1. + (-val).exp())
}

#[derive(Debug, Default)]
pub struct StrategySchedulerStats {
    attempts: AtomicU64,
    accepted: AtomicU64,
    cov_increases: AtomicU64,
    syntax_err: AtomicU64,
    crash: AtomicU64,
    total_attempts: Arc<AtomicU64>,
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
                    self.syntax_err.atomic_add_f64(1., Ordering::Relaxed);
                }
                RejectionReason::TriggersCrash => {
                    self.crash.atomic_add_f64(1., Ordering::Relaxed);
                }
                RejectionReason::Bad => {}
            },
            TestOutcome::Accepted(s) => match s {
                AcceptanceReason::CovIncrease => {
                    self.accepted.atomic_add_f64(1., Ordering::Relaxed);
                    self.cov_increases.atomic_add_f64(1., Ordering::Relaxed);
                }
                AcceptanceReason::IsDiverse => {
                    self.accepted.atomic_add_f64(1., Ordering::Relaxed);
                }
            },
            _ => {}
        }
    }

    fn calculate_score(&self) -> f64 {
        // ucb1
        // TODO add more relevant terms
        let total_attempts = self.total_attempts.atomic_load_f64(Ordering::Relaxed);
        if total_attempts == 0. {
            return f64::INFINITY;
        }
        let attempts = self.attempts.atomic_load_f64(Ordering::Relaxed);
        if attempts == 0. {
            return f64::INFINITY;
        }

        // we want to
        // increase score for accepted ratio, coverage increase and crashes (likely a bug) and reduce it for syntax errors, as they are somewhat uninteresting
        let cov_inc_rate = (self.cov_increases.atomic_load_f64(Ordering::Relaxed)
            + self.accepted.atomic_load_f64(Ordering::Relaxed) * 0.2
            + self.crash.atomic_load_f64(Ordering::Relaxed) * 2.
            - self.syntax_err.atomic_load_f64(Ordering::Relaxed) * 0.5)
            / attempts;
        let exploration = (2. * (total_attempts).ln() / attempts).sqrt();

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

impl Hookable for AdaptiveStrategyScheduler {
    fn snapshot(&self) -> lsf_feedback::SchedulerStatisticsSnapshot {
        lsf_feedback::SchedulerStatisticsSnapshot {
            epoch: self.ctx.epoch.load(Ordering::Relaxed),
            global_attempts: Some(self.stats.total_attempts.atomic_load_f64(Ordering::Relaxed)),
            name: format!("{:?}", self),
            meta: Vec::new(),
            self_attmepts: vec![self.stats.attempts.atomic_load_f64(Ordering::Relaxed)],
            cov_increases: vec![self.stats.cov_increases.atomic_load_f64(Ordering::Relaxed)],
            accepted: vec![self.stats.accepted.atomic_load_f64(Ordering::Relaxed)],
            synatx_err: vec![self.stats.syntax_err.atomic_load_f64(Ordering::Relaxed)],
            crashes: vec![self.stats.crash.atomic_load_f64(Ordering::Relaxed)],
            rating: vec![self.stats.calculate_score()],
            rating_as_prob: vec![{
                let score = self.stats.calculate_score();
                if score.is_infinite() {
                    // initialize with a smaller probability to allow schedulers to diverege immediately
                    0.3
                } else {
                    sigmoid(score)
                }
            }],
        }
    }

    fn attach_hook(&mut self, hook: Arc<dyn lsf_feedback::GenericHook>) {
        self.hook.replace(hook);
    }
}
