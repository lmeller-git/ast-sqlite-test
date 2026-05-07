use std::sync::{Arc, atomic::Ordering};

use indexmap::IndexMap;
use lsf_core::entry::{CorpusEntry, ID, Meta, RawEntry};
use lsf_feedback::{
    AdaptiveStatistics,
    TestableEntry,
    mab::{MABArm, MABBody},
};
use rand::{
    Rng,
    RngExt,
    distr::{Distribution, weighted::WeightedIndex},
};

use crate::{CorpusHandler, GRANULARITY};

pub trait Schedule: Send + Sync {
    fn next_batch(
        &mut self,
        from: &mut dyn CorpusHandler<f64>,
        size: usize,
        rng: &mut dyn Rng,
    ) -> Vec<TestableEntry<RawEntry>>;

    fn next(
        &mut self,
        from: &mut dyn CorpusHandler<f64>,
        rng: &mut dyn Rng,
    ) -> Option<TestableEntry<RawEntry>> {
        self.next_batch(from, 1, rng).into_iter().next()
    }

    fn on_add(&mut self, _entry: &CorpusEntry) -> f64 {
        f64::INFINITY
    }

    fn on_remove(&mut self, _id: ID) {}

    fn chore(&mut self) {}
    fn reset(&mut self) {}
}

const MAX_REFILL: usize = 128;
const BATCH_MULTIPLICATOR: usize = 4;
const MIN_REFILL: usize = 32;
const BASE_PERSISTENCE: usize = 5;
const MAX_WEIGHT: f64 = 1e20;

pub struct SchedulerBatcher {
    inner_scheduler: Box<dyn Schedule>,
    batch: Vec<(TestableEntry<RawEntry>, usize)>,
}

impl SchedulerBatcher {
    pub fn new(schedule: Box<dyn Schedule>) -> Self {
        Self {
            inner_scheduler: schedule,
            batch: Vec::with_capacity(MIN_REFILL),
        }
    }

    fn refill(&mut self, from: &mut dyn CorpusHandler<f64>, size: usize, rng: &mut dyn Rng) {
        let size = size.clamp(MIN_REFILL, MAX_REFILL);

        let batch = self.inner_scheduler.next_batch(from, size, rng);
        self.batch.extend(batch.into_iter().map(|item| {
            let score = Self::calculate_persistence(&item);
            (item, score)
        }));
    }

    fn calculate_persistence(entry: &TestableEntry<RawEntry>) -> usize {
        let mut base = BASE_PERSISTENCE;

        // Hack: if the entry was scheduled by a "smart" scheduler, it will contain hooks pointing to the score
        if let Some(hook) = entry.parent_stats.first() {
            let score = hook.calculate_score().min(MAX_WEIGHT);
            base *= (1. + score).log10().ceil() as usize;
        }

        base
    }
}

impl Schedule for SchedulerBatcher {
    fn next_batch(
        &mut self,
        from: &mut dyn CorpusHandler<f64>,
        size: usize,
        rng: &mut dyn Rng,
    ) -> Vec<TestableEntry<RawEntry>> {
        if self.batch.len() < size {
            self.refill(from, size * BATCH_MULTIPLICATOR, rng);
        }

        let mut batch = Vec::with_capacity(size);

        while batch.len() < size {
            let window_size = self.batch.len().min(size);
            let window_start = self.batch.len() - window_size;

            let rd_index = rng.random_range(window_start..self.batch.len());

            if let Some((entry, energy)) = self.batch.get_mut(rd_index) {
                *energy -= 1;
                batch.push(entry.clone());
                if *energy == 0 {
                    self.batch.swap_remove(rd_index);
                }
            }
        }

        batch
    }

    fn on_add(&mut self, entry: &CorpusEntry) -> f64 {
        self.inner_scheduler.on_add(entry)
    }

    fn on_remove(&mut self, id: ID) {
        self.inner_scheduler.on_remove(id);
    }

    fn chore(&mut self) {
        self.inner_scheduler.chore();
    }

    fn reset(&mut self) {
        self.batch.clear();
        self.inner_scheduler.reset();
    }
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

// DO NOT USE. VERY INEFFICIENT TODO IMPROVE
impl Schedule for WeightedRandomScheduler {
    fn next_batch(
        &mut self,
        from: &mut dyn CorpusHandler<f64>,
        size: usize,
        rng: &mut dyn Rng,
    ) -> Vec<TestableEntry<RawEntry>> {
        if from.size() == 0 {
            return Vec::new();
        }

        let mut ids = Vec::with_capacity(from.size());
        let mut weights = Vec::with_capacity(from.size());

        for id in from.ids() {
            if let Some(entry) = from.get(&id) {
                ids.push(id);
                weights.push(Self::calculate_weight(entry.meta()));
            }
        }

        let dist = match WeightedIndex::new(&weights) {
            Ok(dist) => dist,
            Err(_) => WeightedIndex::new(vec![1.; weights.len()]).unwrap(),
        };

        dist.sample_iter(rng)
            .take(size)
            .map(|idx| ids[idx])
            .filter_map(|id| {
                from.get(&id)
                    .map(|entry| TestableEntry::new(entry.raw().clone()))
            })
            .collect()
    }
}

#[derive(Default)]
pub struct AdaptiveWeightedRandomScheduler {
    stat_mapping: IndexMap<ID, Arc<MABArm>, rustc_hash::FxBuildHasher>,
    dist: Option<WeightedIndex<f64>>,
    body: Arc<MABBody>,
}

impl AdaptiveWeightedRandomScheduler {
    pub fn new(body: Arc<MABBody>) -> Self {
        Self {
            stat_mapping: IndexMap::with_hasher(rustc_hash::FxBuildHasher),
            dist: None,
            body,
        }
    }
}

impl Schedule for AdaptiveWeightedRandomScheduler {
    fn next_batch(
        &mut self,
        from: &mut dyn CorpusHandler<f64>,
        size: usize,
        rng: &mut dyn Rng,
    ) -> Vec<TestableEntry<RawEntry>> {
        if from.size() == 0 {
            return Vec::new();
        }

        if self.dist.is_none()
            || self
                .body
                .epoch
                .load(Ordering::Relaxed)
                .is_multiple_of(GRANULARITY as u32)
        {
            let weights: Vec<_> = from
                .ids()
                .into_iter()
                .map(|id| {
                    let stats = self
                        .stat_mapping
                        .entry(id)
                        .or_insert(Arc::new(MABArm::new(self.body.clone())));

                    let mut stats = stats.calculate_score();
                    if stats.is_infinite() {
                        stats = 0.5;
                    }
                    stats
                })
                .collect();

            let dist = match WeightedIndex::new(&weights) {
                Ok(dist) => dist,
                Err(_) => WeightedIndex::new(vec![1.; weights.len()]).unwrap(),
            };
            self.dist.replace(dist);
        }

        self.dist
            .as_ref()
            .unwrap()
            .sample_iter(rng)
            .take(size)
            .filter_map(|idx| {
                self.stat_mapping.get_index(idx).and_then(|(id, _)| {
                    from.get(id).and_then(|entry| {
                        self.stat_mapping
                            .get_index(idx)
                            .map(|s| TestableEntry::new(entry.raw().clone()).with_hook(s.1.clone()))
                    })
                })
            })
            .collect()
    }

    fn reset(&mut self) {
        self.stat_mapping.clear();
        self.dist = None
    }
}
