use std::{collections::BinaryHeap, sync::Arc};

use lsf_core::entry::ID;
use lsf_feedback::{
    AdaptiveStatistics,
    TestableEntry,
    mab::{MABBody, SchedueldItem},
};

use crate::Schedule;

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
    fn next_batch<'a>(
        &mut self,
        from: &'a crate::Corpus,
        size: usize,
        _rng: &mut dyn rand::Rng,
    ) -> Vec<lsf_feedback::TestableEntry<&'a lsf_core::entry::RawEntry>> {
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
                if let Some(parent) = from.entries.get(&top.item) {
                    parents
                        .push(TestableEntry::new(parent.raw()).with_build_hook(top.stats.clone()));
                }
                acc.push(top);
            }
        }

        self.queue.extend(acc.drain(..));

        parents
    }

    fn add_entry(&mut self, entry: &lsf_core::entry::CorpusEntry) {
        let item = SchedueldItem::new(self.mab.clone(), entry.id());
        self.queue.push(item);
    }
}

impl Default for MABScheduler {
    fn default() -> Self {
        Self::new(MABBody::default().into())
    }
}
