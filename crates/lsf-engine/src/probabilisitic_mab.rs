use std::sync::Arc;

use indexmap::{IndexMap, map::MutableKeys};
use lsf_core::entry::{ID, Meta};
use lsf_feedback::{
    AdaptiveStatistics,
    TestableEntry,
    mab::{MABArm, MABBody, MAX_WEIGHT, MIN_WEIGHT},
};
use rand::RngExt;

use crate::{CorpusHandler, Schedule};

pub fn align_up(n: usize, alignment: usize) -> usize {
    (n + alignment - 1) & !(alignment - 1)
}

pub struct SmallSchedueldItem<T> {
    pub epoch: u32,
    pub stats: Arc<MABArm>,
    pub item: T,
}

impl<T> SmallSchedueldItem<T> {
    pub fn new(body: Arc<MABBody>, item: T) -> Self {
        let epoch = body.epoch.load(std::sync::atomic::Ordering::Relaxed);
        let stats = MABArm::new(body);
        Self {
            epoch,
            stats: stats.into(),
            item,
        }
    }

    pub fn new_with_prior(body: Arc<MABBody>, item: T, meta: &Meta) -> Self {
        let epoch = body.epoch.load(std::sync::atomic::Ordering::Relaxed);
        let stats = MABArm::new_with_prior(body, meta);
        Self {
            epoch,
            stats: stats.into(),
            item,
        }
    }
}

pub struct BlockSumTable {
    leaves: Vec<f64>,
    blocks: Vec<f64>,
    total_sum: f64,
    block_size: usize,
    free_slots: Vec<usize>,
    requests: usize,
    fall_through: usize,
}

impl BlockSumTable {
    pub fn new(initial_capacity: usize) -> Self {
        let block_size = (initial_capacity as f64).sqrt().ceil() as usize;
        let block_size = align_up(block_size, 2);

        Self {
            leaves: Vec::with_capacity(initial_capacity),
            blocks: Vec::with_capacity(initial_capacity / block_size + 1),
            total_sum: 0.0,
            free_slots: Vec::new(),
            block_size,
            requests: 0,
            fall_through: 0,
        }
    }

    pub fn insert(&mut self, weight: f64) -> usize {
        if let Some(free) = self.free_slots.pop() {
            self.set_weigth(free, weight);
            free
        } else {
            self.push(weight)
        }
    }

    pub fn push(&mut self, weight: f64) -> usize {
        let weight = weight.clamp(MIN_WEIGHT, MAX_WEIGHT);

        let idx = self.leaves.len();
        self.leaves.push(0.);
        self.set_weigth(idx, weight);

        idx
    }

    fn set_weigth(&mut self, slot: usize, weight: f64) {
        let block_idx = slot / self.block_size;

        if block_idx >= self.blocks.len() {
            self.blocks.push(0.0);
        }

        let old = self.leaves[slot];
        self.leaves[slot] = weight;
        self.blocks[block_idx] += weight - old;
        self.total_sum += weight - old;
    }

    pub fn remove(&mut self, idx: usize) {
        self.set_weigth(idx, 0.);
        self.free_slots.push(idx);
    }

    pub fn update(&mut self, idx: usize, weight: f64) {
        let new_weight = weight.clamp(MIN_WEIGHT, MAX_WEIGHT);
        self.set_weigth(idx, new_weight);
    }

    pub fn resum(&mut self) {
        self.total_sum = 0.;
        for block_sum in self.blocks.iter_mut() {
            *block_sum = 0.
        }

        for (leave_idx, leave) in self.leaves.iter().enumerate() {
            if let Some(block) = self.blocks.get_mut(leave_idx / self.block_size) {
                *block += leave
            }
        }

        self.total_sum = self.blocks.iter().sum();
        self.requests = 0;
        self.fall_through = 0;
    }

    pub fn sample(&mut self, rng: &mut dyn rand::Rng) -> Option<usize> {
        if self.leaves.is_empty() {
            return None;
        }

        self.requests += 1;

        let mut target = rng.random_range(0.0..self.total_sum);

        let mut block_idx = 0;
        for (i, &sum) in self.blocks.iter().enumerate() {
            if target < sum {
                block_idx = i;
                break;
            }
            target -= sum;
        }

        let start = block_idx * self.block_size;
        let end = (start + self.block_size).min(self.leaves.len());

        for (i, &weight) in self.leaves[start..end].iter().enumerate() {
            if target < weight {
                return Some(start + i);
            }
            target -= weight;
        }

        self.fall_through += 1;
        Some(self.leaves.len() - 1)
    }

    pub fn fall_through_rate(&self) -> f64 {
        self.fall_through as f64 / self.requests as f64
    }
}

pub struct ProbabilisticMABScheduler {
    queue: BlockSumTable,
    id_mapping: IndexMap<ID, SmallSchedueldItem<()>, rustc_hash::FxBuildHasher>,
    mab: Arc<MABBody>,
}

const DEFAULT_CAP: usize = 10000;

impl ProbabilisticMABScheduler {
    pub fn new(body: Arc<MABBody>) -> Self {
        Self {
            queue: BlockSumTable::new(DEFAULT_CAP),
            id_mapping: IndexMap::with_capacity_and_hasher(DEFAULT_CAP, rustc_hash::FxBuildHasher),
            mab: body,
        }
    }
}

impl Schedule for ProbabilisticMABScheduler {
    fn next_batch(
        &mut self,
        from: &mut dyn CorpusHandler<f64>,
        size: usize,
        rng: &mut dyn rand::Rng,
    ) -> Vec<lsf_feedback::TestableEntry<lsf_core::entry::RawEntry>> {
        const ACCEPT_UNDER: u32 = 50;

        let current_epoch = self.mab.epoch.load(std::sync::atomic::Ordering::Relaxed);

        let mut parents = Vec::with_capacity(size);

        while parents.len() < size
            && let Some(top) = self.queue.sample(rng)
        {
            if let Some((id, item)) = self.id_mapping.get_index_mut(top) {
                if item.epoch.abs_diff(current_epoch) > ACCEPT_UNDER {
                    item.epoch = current_epoch;
                    self.queue.update(top, item.stats.calculate_score());
                } else if let Some(entry) = from.get(id) {
                    parents.push(
                        TestableEntry::new(entry.raw().clone())
                            .with_build_hook(item.stats.clone())
                            .with_hook(item.stats.clone()),
                    );
                }
            }
        }

        parents
    }

    fn on_add(&mut self, entry: &lsf_core::entry::CorpusEntry) -> f64 {
        let item = SmallSchedueldItem::new_with_prior(self.mab.clone(), (), &entry.meta);
        let score = item.stats.calculate_score();
        let idx = self.queue.insert(score);
        if idx >= self.id_mapping.len() {
            // really this is idx == self.id_mapping.len()
            self.id_mapping.insert(entry.id(), item);
        } else if let Some((id, value)) = self.id_mapping.get_index_mut2(idx) {
            *id = entry.id();
            *value = item;
        }
        score
    }

    fn on_remove(&mut self, id: ID) {
        if let Some((idx, _, _entry)) = self.id_mapping.get_full(&id) {
            self.queue.remove(idx);
        }
    }

    fn chore(&mut self) {
        // O(N), maybe not do this very often/at all
        const DO_RESUM_OVER: f64 = 0.1;
        let fall_through_rate = self.queue.fall_through_rate();
        if fall_through_rate >= DO_RESUM_OVER {
            self.queue.resum();
        }
    }

    fn reset(&mut self) {
        self.queue = BlockSumTable::new(DEFAULT_CAP);
        self.id_mapping.clear();
        self.mab.reset();
    }
}

impl Default for ProbabilisticMABScheduler {
    fn default() -> Self {
        Self::new(MABBody::default().into())
    }
}

// Roulette-wheel selection via stochastic acceptance
// we can set max_weight to MAX_WEIGHT, which still yields a decent acceptance rate for bad entries, as the score dist is roughly 0. - 6.5 - 100
// if we set max_weigth to the max_weigth in corpus:
// max_weight may be stale and is not necessarily the true max_weight. However by updating max_weight in the sample loop, we guarantee that no fractions are > 1, which should presevre integrity
pub struct FastProbabilisticMABScheduler {
    body: Arc<MABBody>,
    max_weight: f64,
    entries: IndexMap<ID, SmallSchedueldItem<f64>, rustc_hash::FxBuildHasher>,
}

impl FastProbabilisticMABScheduler {
    pub fn new(body: Arc<MABBody>) -> Self {
        Self {
            body,
            max_weight: MAX_WEIGHT,
            entries: IndexMap::with_hasher(rustc_hash::FxBuildHasher),
        }
    }
}

impl Schedule for FastProbabilisticMABScheduler {
    fn next_batch(
        &mut self,
        from: &mut dyn CorpusHandler<f64>,
        size: usize,
        rng: &mut dyn rand::Rng,
    ) -> Vec<TestableEntry<lsf_core::entry::RawEntry>> {
        // it is not clear wether cachign is actually faster here, but its just a copy paste from above so whatever
        const ACCEPT_UNDER: u32 = 10;

        let current_epoch = self.body.epoch.load(std::sync::atomic::Ordering::Relaxed);
        let mut generation = Vec::with_capacity(size);

        while generation.len() < size {
            let rd_idx = rng.random_range(0..self.entries.len());
            if let Some((id, arm)) = self.entries.get_index_mut(rd_idx)
                && let Some(entry) = from.get(id)
            {
                if arm.epoch.abs_diff(current_epoch) >= ACCEPT_UNDER {
                    arm.item = arm.stats.calculate_score();
                    arm.epoch = current_epoch;
                }

                if rng.random_bool(arm.item / self.max_weight) {
                    generation.push(
                        TestableEntry::new(entry.raw().clone())
                            .with_hook(arm.stats.clone())
                            .with_build_hook(arm.stats.clone()),
                    );
                }
            }
        }

        generation
    }

    fn on_add(&mut self, entry: &lsf_core::entry::CorpusEntry) -> f64 {
        let mut item = SmallSchedueldItem::new_with_prior(self.body.clone(), 0., entry.meta());
        let score = item.stats.calculate_score();
        item.epoch = self.body.epoch.load(std::sync::atomic::Ordering::Relaxed);
        item.item = score;

        self.entries.insert(entry.id(), item);

        score
    }

    fn on_remove(&mut self, id: ID) {
        // could check if this was max and do chore if so, but this is very unlikely, as top fliers are guarded by corpus
        self.entries.swap_remove(&id);
    }

    fn chore(&mut self) {}

    fn reset(&mut self) {
        self.body.reset();
        self.entries.clear();
    }
}
