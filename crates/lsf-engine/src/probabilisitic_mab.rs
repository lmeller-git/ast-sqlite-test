use std::sync::Arc;

use indexmap::IndexMap;
use lsf_core::entry::{ID, Meta};
use lsf_feedback::{
    AdaptiveStatistics,
    TestableEntry,
    mab::{MABArm, MABBody},
};
use rand::RngExt;

use crate::{CorpusHandler, Schedule};

const ZERO_WEIGHT: f64 = 0.001;
const MAX_WEIGHT: f64 = 1e20;

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
}

impl BlockSumTable {
    pub fn new(initial_capacity: usize) -> Self {
        let block_size = (initial_capacity as f64).sqrt().ceil() as usize;
        let block_size = align_up(block_size, 2);

        Self {
            leaves: Vec::with_capacity(initial_capacity),
            blocks: Vec::with_capacity(initial_capacity / block_size + 1),
            total_sum: 0.0,
            block_size,
        }
    }

    pub fn push(&mut self, weight: f64) -> usize {
        let weight = weight.clamp(ZERO_WEIGHT, MAX_WEIGHT);
        let idx = self.leaves.len();
        let block_idx = idx / self.block_size;

        if block_idx >= self.blocks.len() {
            self.blocks.push(0.0);
        }

        self.leaves.push(weight);
        self.blocks[block_idx] += weight;
        self.total_sum += weight;
        idx
    }

    pub fn remove(&mut self, idx: usize) {
        let block_idx = idx / self.block_size;

        if let Some(block) = self.blocks.get_mut(block_idx)
            && let Some(leave) = self.leaves.get_mut(idx)
        {
            *block -= *leave;
            self.total_sum -= *leave;
            *leave = 0.;
        }
    }

    pub fn update(&mut self, idx: usize, weight: f64) {
        if let Some(leave) = self.leaves.get_mut(idx)
            && let Some(block) = self.blocks.get_mut(idx / self.block_size)
        {
            let new_weight = weight.clamp(ZERO_WEIGHT, MAX_WEIGHT);
            let delta = new_weight - *leave;
            *leave = new_weight;
            *block += delta;
            self.total_sum += delta
        }
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
    }

    pub fn sample(&self, rng: &mut dyn rand::Rng) -> Option<usize> {
        if self.leaves.is_empty() {
            return None;
        }

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

        Some(self.leaves.len() - 1)
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
                        TestableEntry::new(entry.raw().clone()).with_build_hook(item.stats.clone()),
                    );
                }
            }
        }

        parents
    }

    fn on_add(&mut self, entry: &lsf_core::entry::CorpusEntry) -> f64 {
        let item = SmallSchedueldItem::new_with_prior(self.mab.clone(), (), &entry.meta);
        let score = item.stats.calculate_score();
        let idx = self.queue.push(score);
        let (idx_map, _) = self.id_mapping.insert_full(entry.id(), item);
        debug_assert_eq!(idx, idx_map);
        score
    }

    fn on_remove(&mut self, id: ID) {
        if let Some((idx, _, _entry)) = self.id_mapping.get_full(&id) {
            self.queue.remove(idx);
        }
    }

    fn chore(&mut self) {
        // O(N), maybe not do this very often/at all
        self.queue.resum();
    }
}

impl Default for ProbabilisticMABScheduler {
    fn default() -> Self {
        Self::new(MABBody::default().into())
    }
}
