use lsf_core::entry::{ID, Meta};
use rand::{
    Rng,
    distr::{Distribution, weighted::WeightedIndex},
};

use crate::Corpus;

pub trait Schedule: Send + Sync {
    fn next_batch(&self, from: &Corpus, size: usize, rng: &mut dyn Rng) -> Vec<ID>;

    fn next(&self, from: &Corpus, rng: &mut dyn Rng) -> Option<ID> {
        self.next_batch(from, 1, rng).into_iter().next()
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
        weight *= 10000. / (meta.exec_time as f64 + 1.);
        weight
    }
}

impl Schedule for WeightedRandomScheduler {
    fn next_batch(&self, from: &Corpus, size: usize, rng: &mut dyn Rng) -> Vec<ID> {
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
            .collect()
    }
}

#[cfg(test)]
mod tests {}
