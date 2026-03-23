use lsf_core::entry::ID;
use rand::{Rng, RngExt};

pub(crate) type Queue<T> = Vec<T>;

pub trait Schedule: Send + Sync {
    fn next_batch(&self, from: &mut Queue<ID>, size: usize, rng: &mut dyn Rng) -> Vec<ID>;

    fn next(&self, from: &mut Queue<ID>, rng: &mut dyn Rng) -> Option<ID> {
        self.next_batch(from, 1, rng).into_iter().next()
    }
}

pub struct FIFOScheduler {}

impl Schedule for FIFOScheduler {
    fn next_batch(&self, from: &mut Queue<ID>, size: usize, _rng: &mut dyn Rng) -> Vec<ID> {
        from.split_off(from.len() - size.min(from.len()))
    }
}

pub struct RandomScheduler {}

impl Schedule for RandomScheduler {
    fn next_batch(&self, from: &mut Queue<ID>, size: usize, rng: &mut dyn Rng) -> Vec<ID> {
        (0..size.min(from.len()))
            .map(|_| {
                let chosen_idx = rng.random_range(..from.len());
                from.swap_remove(chosen_idx)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::SmallRng};

    use super::*;

    #[test]
    fn fifo() {
        let scheduler = FIFOScheduler {};
        let mut queue: Vec<ID> = (0..10).map(|_| ID::next()).collect();
        let queue_clone = queue.clone();
        let mut rng = SmallRng::seed_from_u64(42);

        assert_eq!(
            &scheduler.next_batch(&mut queue, 3, &mut rng),
            &queue_clone[7..]
        );
        assert_eq!(
            &scheduler.next_batch(&mut queue, 3, &mut rng),
            &queue_clone[4..7]
        );
        assert_eq!(
            &scheduler.next_batch(&mut queue, 10, &mut rng),
            &queue_clone[..4]
        );
        assert!(scheduler.next(&mut queue, &mut rng).is_none());
    }

    #[test]
    fn radom() {
        let scheduler = RandomScheduler {};
        let mut queue: Vec<ID> = (0..10).map(|_| ID::next()).collect();
        let queue_clone = queue.clone();
        let mut rng = SmallRng::seed_from_u64(42);

        let batch = scheduler.next_batch(&mut queue, 3, &mut rng);
        assert!(batch.len() == 3);
        assert!(batch.iter().all(|ele| queue_clone.contains(ele)));

        let batch = scheduler.next_batch(&mut queue, 10, &mut rng);
        assert!(batch.len() == 7);
        assert!(batch.iter().all(|ele| queue_clone.contains(ele)));

        assert!(scheduler.next(&mut queue, &mut rng).is_none());
    }
}
