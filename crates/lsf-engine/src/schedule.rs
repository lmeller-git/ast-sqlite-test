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
    distr::{Distribution, weighted::WeightedIndex},
};

use crate::{Corpus, CorpusHandler, GRANULARITY};

pub trait Schedule: Send + Sync {
    fn next_batch(
        &mut self,
        from: &mut Corpus,
        size: usize,
        rng: &mut dyn Rng,
    ) -> Vec<TestableEntry<RawEntry>>;

    fn next(&mut self, from: &mut Corpus, rng: &mut dyn Rng) -> Option<TestableEntry<RawEntry>> {
        self.next_batch(from, 1, rng).into_iter().next()
    }

    fn add_entry(&mut self, _entry: &CorpusEntry) {}
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
        from: &mut Corpus,
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
    stat_mapping: IndexMap<ID, Arc<MABArm>>,
    dist: Option<WeightedIndex<f64>>,
    body: Arc<MABBody>,
}

impl Schedule for AdaptiveWeightedRandomScheduler {
    fn next_batch(
        &mut self,
        from: &mut Corpus,
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
}
