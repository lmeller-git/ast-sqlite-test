use std::collections::VecDeque;

use lsf_core::entry::ID;
use rand::Rng;

pub(crate) type Queue<T> = VecDeque<T>;

pub trait Schedule: Send + Sync {
    fn next_batch(&self, from: &mut Queue<ID>, size: usize, rng: &mut dyn Rng) -> Vec<ID>;

    fn next(&mut self, from: &mut Queue<ID>, rng: &mut dyn Rng) -> Option<ID> {
        self.next_batch(from, 1, rng).into_iter().next()
    }
}

pub struct FIFOScheduler {}

impl Schedule for FIFOScheduler {
    fn next_batch(&self, from: &mut Queue<ID>, size: usize, _rng: &mut dyn Rng) -> Vec<ID> {
        from.drain(..(size.min(from.len()))).collect()
    }
}
