use std::{collections::BinaryHeap, sync::Arc};

use lsf_core::entry::ID;
use lsf_feedback::{
    AdaptiveStatistics,
    TestableEntry,
    mab::{MABBody, SchedueldItem},
};

use crate::{CorpusHandler, Schedule};

pub struct MABScheduler {
    queue: BinaryHeap<SchedueldItem<ID>>,
    mab: Arc<MABBody>,
}

impl MABScheduler {
    pub fn new(body: Arc<MABBody>) -> Self {
        Self {
            queue: BinaryHeap::default(),
            mab: body,
        }
    }
}

impl Schedule for MABScheduler {
    fn next_batch(
        &mut self,
        from: &mut dyn CorpusHandler<f64>,
        size: usize,
        _rng: &mut dyn rand::Rng,
    ) -> Vec<lsf_feedback::TestableEntry<lsf_core::entry::RawEntry>> {
        const ACCEPT_UNDER: u32 = 50;

        let current_epoch = self.mab.epoch.load(std::sync::atomic::Ordering::Relaxed);

        let mut parents = Vec::with_capacity(size);
        let mut acc = Vec::with_capacity(size);

        // parents may only be sampled once per generation
        while parents.len() < size
            && let Some(mut top) = self.queue.pop()
        {
            if top.epoch.abs_diff(current_epoch) > ACCEPT_UNDER {
                top.score = top.stats.calculate_score();
                top.epoch = current_epoch;
                self.queue.push(top);
            } else {
                if let Some(parent) = from.get(&top.item) {
                    from.update(&top.item, top.score);
                    parents.push(
                        TestableEntry::new(parent.raw().clone()).with_build_hook(top.stats.clone()),
                    );
                    acc.push(top);
                }
            }
        }

        self.queue.extend(acc.drain(..));

        parents
    }

    fn on_add(&mut self, entry: &lsf_core::entry::CorpusEntry) -> f64 {
        let item = SchedueldItem::new_with_prior(self.mab.clone(), entry.id(), entry.meta());
        let score = item.score;
        self.queue.push(item);
        score
    }

    fn reset(&mut self) {
        self.queue.clear();
        self.mab.reset();
    }
}

impl Default for MABScheduler {
    fn default() -> Self {
        Self::new(MABBody::default().into())
    }
}
