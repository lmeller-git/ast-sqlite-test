use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use lsf_core::entry::{ID, Meta, RawEntry};
use lsf_feedback::{
    AcceptanceReason,
    AdaptiveStatistics,
    FeedbackHook,
    GenericHook,
    Hookable,
    RejectionReason,
    SchedulerStatisticsSnapshot,
    TestOutcome,
    TestableEntry,
};
use rand::{
    Rng,
    distr::{Distribution, weighted::WeightedIndex},
};

use crate::Corpus;

pub trait Schedule: Send + Sync {
    fn next_batch<'a>(
        &mut self,
        from: &'a Corpus,
        size: usize,
        rng: &mut dyn Rng,
    ) -> Vec<TestableEntry<&'a RawEntry>>;

    fn next<'a>(
        &mut self,
        from: &'a Corpus,
        rng: &mut dyn Rng,
    ) -> Option<TestableEntry<&'a RawEntry>> {
        self.next_batch(from, 1, rng).into_iter().next()
    }

    fn init(&mut self, _ctx: SchedulerContext) {}
}

#[derive(Clone, Default)]
pub struct SchedulerContext {
    pub total_attempts: Arc<AtomicU32>,
}

#[derive(Default)]
pub struct WeightedRandomScheduler {}

impl WeightedRandomScheduler {
    pub fn new() -> Self {
        Self {}
    }

    fn calculate_weight(meta: &Meta) -> f64 {
        let mut weight = 1.;
        weight += (meta.new_cov_nodes as f64) * 20.;
        let exec_time_us = meta.exec_time / 1000;
        let mul = if exec_time_us < 10 {
            2.
        } else if exec_time_us < 1000 {
            1.
        } else {
            0.5
        };
        weight *= mul;
        weight
    }
}

impl Schedule for WeightedRandomScheduler {
    fn next_batch<'a>(
        &mut self,
        from: &'a Corpus,
        size: usize,
        rng: &mut dyn Rng,
    ) -> Vec<TestableEntry<&'a RawEntry>> {
        if from.entries.is_empty() {
            return Vec::new();
        }

        let mut ids = Vec::with_capacity(from.entries.len());
        let mut weights = Vec::with_capacity(from.entries.len());

        for (id, entry) in from.entries.iter() {
            ids.push(*id);
            weights.push(Self::calculate_weight(entry.meta()));
        }

        let dist = match WeightedIndex::new(&weights) {
            Ok(dist) => dist,
            Err(_) => WeightedIndex::new(vec![1.; weights.len()]).unwrap(),
        };

        dist.sample_iter(rng)
            .take(size)
            .map(|idx| ids[idx])
            .filter_map(|id| {
                from.entries
                    .get(&id)
                    .map(|entry| TestableEntry::new(entry.raw()))
            })
            .collect()
    }
}

const ZERO_WEIGHT: f64 = 100.;

#[derive(Default)]
pub struct AdaptiveWeightedRandomScheduler {
    stat_mapping: HashMap<ID, Arc<AdaptiveCorpusStats>>,
    ctx: SchedulerContext,
    hook: Option<Arc<dyn GenericHook>>,
    last_snapshot: AtomicU32,
}

#[derive(Default)]
pub struct AdaptiveCorpusStats {
    attempts: AtomicU32,
    accepted: AtomicU32,
    cov_increases: AtomicU32,
    syntax_err: AtomicU32,
    crash: AtomicU32,
    total_attempts: Arc<AtomicU32>,
}

impl AdaptiveCorpusStats {
    fn new(total_attempts: Arc<AtomicU32>) -> Self {
        Self {
            total_attempts,
            ..Default::default()
        }
    }
}

impl AdaptiveStatistics for AdaptiveCorpusStats {
    fn update(&self, test_result: lsf_feedback::TestOutcome) {
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
            return ZERO_WEIGHT;
        }
        let attempts = self.attempts.load(Ordering::Relaxed);
        if attempts == 0 {
            return ZERO_WEIGHT;
        }

        // we want to
        // increase score for accepted ratio, coverage increase and crashes (likely a bug) and reduce it for syntax errors, as they are somewhat uninteresting
        let cov_inc_rate = (self.cov_increases.load(Ordering::Relaxed) as f64 * 2.
            + self.accepted.load(Ordering::Relaxed) as f64 * 0.5
            + self.crash.load(Ordering::Relaxed) as f64
            - self.syntax_err.load(Ordering::Relaxed) as f64)
            / attempts as f64;
        let exploration = (2.0 * (total_attempts as f64).ln() / attempts as f64).sqrt();

        cov_inc_rate + exploration
    }
}

impl Hookable for AdaptiveWeightedRandomScheduler {
    fn snapshot(&self) -> SchedulerStatisticsSnapshot {
        if self.stat_mapping.is_empty() {
            return SchedulerStatisticsSnapshot::default();
        }

        #[allow(clippy::type_complexity)]
        let ((((((ids, attempts), accepted), cov_inc), syntax_err), crashes), ratings): (
            (
                ((((Vec<ID>, Vec<u32>), Vec<u32>), Vec<u32>), Vec<u32>),
                Vec<u32>,
            ),
            Vec<f64>,
        ) = self
            .stat_mapping
            .iter()
            .map(|(id, stats)| {
                (
                    (
                        (
                            (
                                (
                                    (*id, stats.attempts.load(Ordering::Relaxed)),
                                    stats.accepted.load(Ordering::Relaxed),
                                ),
                                stats.cov_increases.load(Ordering::Relaxed),
                            ),
                            stats.syntax_err.load(Ordering::Relaxed),
                        ),
                        stats.crash.load(Ordering::Relaxed),
                    ),
                    stats.calculate_score(),
                )
            })
            .unzip();

        SchedulerStatisticsSnapshot {
            global_attempts: Some(self.ctx.total_attempts.load(Ordering::Relaxed)),
            name: "AdaptiveWeightedRandomScheduler".into(),
            meta: ids.into_iter().map(|id| format!("Entry_{}", id)).collect(),
            self_attmepts: attempts,
            cov_increases: cov_inc,
            accepted,
            synatx_err: syntax_err,
            crashes,
            rating: ratings,
            rating_as_prob: Vec::new(),
        }
    }

    fn attach_hook(&mut self, hook: Arc<dyn lsf_feedback::GenericHook>) {
        self.hook.replace(hook);
    }
}

impl FeedbackHook for AdaptiveCorpusStats {
    fn fire(&self, test_outcome: lsf_feedback::TestOutcome) {
        self.update(test_outcome);
    }
}

impl Schedule for AdaptiveWeightedRandomScheduler {
    fn next_batch<'a>(
        &mut self,
        from: &'a Corpus,
        size: usize,
        rng: &mut dyn Rng,
    ) -> Vec<TestableEntry<&'a RawEntry>> {
        if from.entries.is_empty() {
            return Vec::new();
        }

        let (ids, weights): (Vec<_>, Vec<_>) = from
            .entries
            .keys()
            .map(|id| {
                let stats =
                    self.stat_mapping
                        .entry(*id)
                        .or_insert(Arc::new(AdaptiveCorpusStats::new(
                            self.ctx.total_attempts.clone(),
                        )));

                let score = stats.calculate_score();
                (*id, score)
            })
            .unzip();

        let dist = match WeightedIndex::new(&weights) {
            Ok(dist) => dist,
            Err(_) => WeightedIndex::new(vec![1.; weights.len()]).unwrap(),
        };

        let batch = dist
            .sample_iter(rng)
            .take(size)
            .map(|idx| ids[idx])
            .filter_map(|id| {
                from.entries.get(&id).map(|entry| {
                    let stats = self.stat_mapping.get(&id).unwrap();
                    stats.attempts.fetch_add(1, Ordering::Relaxed);
                    TestableEntry::new(entry.raw()).with_hook(stats.clone())
                })
            })
            .collect();

        let total = self.ctx.total_attempts.load(Ordering::Relaxed);
        let last = self.last_snapshot.load(Ordering::Relaxed);
        if let Some(hook) = &self.hook
            && total.saturating_sub(last) >= 500
        {
            self.last_snapshot.store(total, Ordering::Relaxed);
            hook.on_snapshot(self.snapshot());
        }

        batch
    }

    fn init(&mut self, ctx: SchedulerContext) {
        self.ctx = ctx
    }
}

impl Clone for AdaptiveCorpusStats {
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

impl Eq for AdaptiveCorpusStats {}

impl PartialEq for AdaptiveCorpusStats {
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
