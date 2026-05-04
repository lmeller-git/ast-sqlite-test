use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use portable_atomic::AtomicF64;

use crate::{AcceptanceReason, AdaptiveStatistics, FeedbackHook, RejectionReason, TestOutcome};

const DECAY_RATE: f64 = 0.999;
const RESCALE_FACTOR: f64 = 1e15_f64;

#[derive(Debug, Default)]
pub struct MABBody {
    pub total_attempts: AtomicF64,
    pub current_inflation: AtomicF64,
    pub epoch: AtomicU32,
    pub normalization_epoch: AtomicU32,
}

impl MABBody {
    pub fn new() -> Self {
        Self {
            current_inflation: AtomicF64::new(1.),
            ..Default::default()
        }
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
}

#[derive(Debug, Default)]
pub struct MABArm {
    attempts: AtomicF64,
    accepted: AtomicF64,
    cov_increases: AtomicF64,
    syntax_err: AtomicF64,
    crash: AtomicF64,
    local_epoch: AtomicU32,
    ctx: Arc<MABBody>,
}

impl MABArm {
    pub fn new(body: Arc<MABBody>) -> Self {
        Self {
            ctx: body,
            ..Default::default()
        }
    }

    fn normalize(&self) {
        let norm_epoch = self.ctx.normalization_epoch.load(Ordering::Relaxed);
        let local_epoch = self.local_epoch.load(Ordering::Relaxed);
        let diff = norm_epoch - local_epoch;

        if diff != 0 {
            _ = self
                .attempts
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff as i32))
                });
            _ = self
                .cov_increases
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff as i32))
                });
            _ = self
                .accepted
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff as i32))
                });
            _ = self
                .syntax_err
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff as i32))
                });
            _ = self
                .crash
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                    Some(old / RESCALE_FACTOR.powi(diff as i32))
                });

            self.local_epoch.store(norm_epoch, Ordering::Relaxed);
        }
    }
}

impl FeedbackHook for MABArm {
    fn on_exec(&self, test_outcome: crate::TestOutcome) {
        let inflation = self.ctx.current_inflation.load(Ordering::Relaxed);

        match test_outcome {
            TestOutcome::Rejected(r) => match r {
                RejectionReason::SyntaxError => {
                    self.syntax_err.fetch_add(1. * inflation, Ordering::Relaxed);
                }
                RejectionReason::TriggersCrash => {
                    self.crash.fetch_add(1. * inflation, Ordering::Relaxed);
                }
                RejectionReason::Bad => {}
            },
            TestOutcome::Accepted(s) => match s {
                AcceptanceReason::CovIncrease(n_found) => {
                    self.accepted.fetch_add(1. * inflation, Ordering::Relaxed);
                    self.cov_increases
                        .fetch_add(n_found as f64 * inflation, Ordering::Relaxed);
                }
                AcceptanceReason::IsDiverse => {
                    self.accepted.fetch_add(1. * inflation, Ordering::Relaxed);
                }
            },
            _ => {}
        }
    }

    fn on_mutate(&self, _mutation_outcome: TestOutcome) {
        let inflation = self.ctx.current_inflation.load(Ordering::Relaxed);

        match _mutation_outcome {
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
            return f64::INFINITY;
        }

        let inflation = self.ctx.current_inflation.load(Ordering::Relaxed);

        // we want to
        // increase score for accepted ratio, coverage increase and crashes (likely a bug) and reduce it for syntax errors, as they are somewhat uninteresting
        let cov_inc_rate = (self.cov_increases.load(Ordering::Relaxed)) / (inflated_attempts);

        let effective_attempts = inflated_attempts / inflation;
        let effective_total_attempts = self.ctx.total_attempts.load(Ordering::Relaxed) / inflation;
        let exploration = (4. * (effective_total_attempts).ln() / (effective_attempts)).sqrt();

        cov_inc_rate + exploration
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
