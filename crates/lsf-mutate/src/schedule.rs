use std::{
    collections::BinaryHeap,
    f64,
    sync::{Arc, Mutex},
};

use lsf_core::entry::RawEntry;
use lsf_feedback::{
    AdaptiveStatistics,
    FeedbackHook,
    TestOutcome,
    TestableEntry,
    mab::{MABArm, MABBody, SchedueldItem},
};
use rand::RngExt;

use crate::{MutationError, MutationState, MutationStrategy};

pub struct AdaptiveStrategyScheduler {
    strategy: Box<dyn MutationStrategy>,
    arm: Arc<MABArm>,
}

impl AdaptiveStrategyScheduler {
    pub fn new(strategy: Box<dyn MutationStrategy>, body: Arc<MABBody>) -> Self {
        Self {
            strategy,
            arm: MABArm::new(body).into(),
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
        self.strategy.breed(parent, parent_gen, rng)
    }

    fn breed(
        &self,
        parent: &TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<MutationState, MutationError> {
        let score = self.arm.calculate_score();
        let ratio = sigmoid(score);

        if !rng.random_bool(ratio) {
            return Ok(MutationState::Unchanged);
        }

        let mut r = self.breed_inner(parent, parent_gen, rng);

        if let Ok(MutationState::Mutated(result)) = &mut r {
            result.attach_hook(self.arm.clone());
            // this should optimally sit in update, as now probability is updated WITHIN one epoch, even though no stats are collected.
            // It is here right now, since we also need to updated this even if we do not insert a hook, i.e. if r != MutationState::Mutated.
            // This would require some kind of NullHook (TODO add later)
            self.arm.on_mutate(TestOutcome::Mutated);
        } else {
            self.arm.on_mutate(TestOutcome::NOOP);
        }

        r
    }
}

fn sigmoid(val: f64) -> f64 {
    const SCALER: f64 = 0.5;
    1. / (1. + (-SCALER * val).exp())
}

pub struct MABScheduler {
    queue: Mutex<BinaryHeap<SchedueldItem<Box<dyn MutationStrategy>>>>,
    body: Arc<MABBody>,
    choose: usize,
}

impl MABScheduler {
    pub fn new(
        body: Arc<MABBody>,
        strategies: impl Iterator<Item = Box<dyn MutationStrategy>>,
        choose: usize,
    ) -> Self {
        let queue: Mutex<BinaryHeap<SchedueldItem<Box<dyn MutationStrategy>>>> = Mutex::new(
            strategies
                .map(|s| SchedueldItem::new(body.clone(), s))
                .collect(),
        );
        debug_assert!(queue.lock().unwrap().len() >= choose);
        Self {
            queue,
            body,
            choose,
        }
    }
}

impl MutationStrategy for MABScheduler {
    fn breed_inner(
        &self,
        _parent: &TestableEntry<RawEntry>,
        _parent_gen: &[TestableEntry<&RawEntry>],
        _rng: &mut dyn rand::Rng,
    ) -> Result<MutationState, MutationError> {
        Err(MutationError::NOOP)
    }

    fn breed(
        &self,
        parent: &TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<MutationState, MutationError> {
        const ACCEPT_UNDER: u32 = 50;
        let current_epoch = self.body.epoch.load(std::sync::atomic::Ordering::Relaxed);
        let mut acc = Vec::new();

        let mut lock = self.queue.lock().unwrap();

        while acc.len() < self.choose
            && let Some(mut top) = lock.pop()
        {
            if top.epoch.abs_diff(current_epoch) > ACCEPT_UNDER {
                top.score = top.stats.calculate_score();
                top.epoch = current_epoch;
                lock.push(top);
            } else {
                acc.push(top);
            }
        }

        if let Some(chosen) = acc.first() {
            let mut r = chosen.item.breed(parent, parent_gen, rng);
            if let Ok(MutationState::Mutated(result)) = &mut r {
                result.attach_hook(chosen.stats.clone());
                // this should optimally sit in update, as now probability is updated WITHIN one epoch, even though no stats are collected.
                // It is here right now, since we also need to updated this even if we do not insert a hook, i.e. if r != MutationState::Mutated.
                // This would require some kind of NullHook (TODO add later)
                chosen.stats.on_mutate(TestOutcome::Mutated);
            } else {
                chosen.stats.on_mutate(TestOutcome::NOOP);
            }

            lock.extend(acc.drain(..));

            r
        } else {
            Err(MutationError::NOOP)
        }
    }
}
