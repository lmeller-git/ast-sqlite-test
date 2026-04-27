use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use lsf_core::{
    AtomicF64Ext,
    entry::{ID, Meta, RawEntry},
};
use lsf_feedback::{
    AcceptanceReason,
    AdaptiveStatistics,
    FeedbackHook,
    RejectionReason,
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

    fn decay(&self, _rate: f64) {}
}

#[derive(Clone, Default)]
pub struct SchedulerContext {
    pub total_attempts: Arc<AtomicU64>,
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
}

#[derive(Default)]
pub struct AdaptiveCorpusStats {
    attempts: AtomicU64,
    accepted: AtomicU64,
    cov_increases: AtomicU64,
    syntax_err: AtomicU64,
    crash: AtomicU64,
    total_attempts: Arc<AtomicU64>,
}

impl AdaptiveCorpusStats {
    fn new(total_attempts: Arc<AtomicU64>) -> Self {
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
                    self.syntax_err.add_f64(1., Ordering::Relaxed);
                }
                RejectionReason::TriggersCrash => {
                    self.crash.add_f64(1., Ordering::Relaxed);
                }
                RejectionReason::Bad => {}
            },
            TestOutcome::Accepted(s) => match s {
                AcceptanceReason::CovIncrease => {
                    self.accepted.add_f64(1., Ordering::Relaxed);
                    self.cov_increases.add_f64(1., Ordering::Relaxed);
                }
                AcceptanceReason::IsDiverse => {
                    self.accepted.add_f64(1., Ordering::Relaxed);
                }
            },
        }
    }

    fn calculate_score(&self) -> f64 {
        // ucb1
        // TODO add more relevant terms
        let total_attempts = self.total_attempts.load_f64(Ordering::Relaxed);
        if total_attempts == 0. {
            return ZERO_WEIGHT;
        }
        let attempts = self.attempts.load_f64(Ordering::Relaxed);
        if attempts == 0. {
            return ZERO_WEIGHT;
        }

        // we want to
        // increase score for accepted ratio, coverage increase and crashes (likely a bug) and reduce it for syntax errors, as they are somewhat uninteresting
        let cov_inc_rate = (self.cov_increases.load_f64(Ordering::Relaxed) * 2.
            + self.accepted.load_f64(Ordering::Relaxed) * 0.5
            + self.crash.load_f64(Ordering::Relaxed)
            - self.syntax_err.load_f64(Ordering::Relaxed))
            / attempts;
        let exploration = (2. * (total_attempts).ln() / attempts).sqrt();

        cov_inc_rate + exploration
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

        dist.sample_iter(rng)
            .take(size)
            .map(|idx| ids[idx])
            .filter_map(|id| {
                from.entries.get(&id).map(|entry| {
                    let stats = self.stat_mapping.get(&id).unwrap();
                    stats.attempts.add_f64(1., Ordering::Relaxed);
                    TestableEntry::new(entry.raw()).with_hook(stats.clone())
                })
            })
            .collect()
    }

    fn init(&mut self, ctx: SchedulerContext) {
        self.ctx = ctx
    }

    fn decay(&self, rate: f64) {
        for stat in self.stat_mapping.values() {
            stat.attempts.multiply_f64(rate, Ordering::Relaxed);
            stat.cov_increases.multiply_f64(rate, Ordering::Relaxed);
            stat.accepted.multiply_f64(rate, Ordering::Relaxed);
            stat.syntax_err.multiply_f64(rate, Ordering::Relaxed);
            stat.crash.multiply_f64(rate, Ordering::Relaxed);
        }
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
