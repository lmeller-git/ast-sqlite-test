use std::collections::VecDeque;

use lsf_core::entry::ID;

pub(crate) type Queue<T> = VecDeque<T>;

pub trait Schedule: Send + Sync {
    fn next_batch(&self, from: &mut Queue<ID>, size: usize) -> Vec<ID>;

    fn next(&mut self, from: &mut Queue<ID>) -> Option<ID> {
        self.next_batch(from, 1).into_iter().next()
    }
}

pub struct FIFOScheduler {}

impl Schedule for FIFOScheduler {
    fn next_batch(&self, from: &mut Queue<ID>, size: usize) -> Vec<ID> {
        from.drain(..(size.min(from.len()))).collect()
    }
}
