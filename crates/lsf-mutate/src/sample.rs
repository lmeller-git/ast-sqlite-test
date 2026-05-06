use lsf_core::entry::RawEntry;
use lsf_feedback::TestableEntry;
use rand::{Rng, RngExt};

use crate::{MutationState, MutationStrategy};

/// applies a random sample with replacement of size choose_max..=choose_min from choices
pub struct RandomMutationSampler {
    choices: Vec<Box<dyn MutationStrategy>>,
    choose_max: usize,
    choose_min: usize,
}

impl RandomMutationSampler {
    pub fn new(
        choose_max: usize,
        choose_min: usize,
        choices: Vec<Box<dyn MutationStrategy>>,
    ) -> Self {
        debug_assert!(choose_min <= choose_max);
        Self {
            choices,
            choose_max,
            choose_min,
        }
    }
}

impl MutationStrategy for RandomMutationSampler {
    fn breed_inner(
        &self,
        _parent: &mut TestableEntry<RawEntry>,
        _parent_gen: &[TestableEntry<RawEntry>],
        _rng: &mut dyn Rng,
    ) -> Result<MutationState, crate::MutationError> {
        Err(crate::MutationError::NOOP)
    }

    fn breed(
        &self,
        parent: &mut TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<MutationState, crate::MutationError> {
        let n_chosen = rng.random_range(self.choose_min..=self.choose_max);
        if n_chosen == 0 {
            return Ok(MutationState::Unchanged);
        }

        let mut status = MutationState::Unchanged;

        for _ in 0..n_chosen {
            let chosen_strategy = rng.random_range(..self.choices.len());
            let r = self.choices[chosen_strategy].breed(parent, parent_gen, rng);

            if let Ok(MutationState::Mutated) = r {
                status = MutationState::Mutated;
            }
        }

        Ok(status)
    }
}

/// applies the strategy over with probability prob
pub struct Randomly {
    over: Box<dyn MutationStrategy>,
    prob: f64,
}

impl Randomly {
    pub fn new(over: Box<dyn MutationStrategy>, probability: f64) -> Self {
        Self {
            over,
            prob: probability,
        }
    }
}

impl MutationStrategy for Randomly {
    fn breed_inner(
        &self,
        parent: &mut TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<MutationState, crate::MutationError> {
        self.over.breed(parent, parent_gen, rng)
    }

    fn breed(
        &self,
        parent: &mut TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<MutationState, crate::MutationError> {
        if rng.random_bool(self.prob) {
            self.breed_inner(parent, parent_gen, rng)
        } else {
            Ok(MutationState::Unchanged)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SpliceIn, test_single_mutation};

    #[test]
    fn random_sampler() {
        let sql = "SELECT a FROM b";
        let expected2 = "SELECT a FROM b; SELECT a FROM b";

        test_single_mutation(
            sql,
            expected2,
            Box::new(RandomMutationSampler::new(
                1,
                1,
                vec![Box::new(SpliceIn {})],
            )),
        );

        let expected3 = "SELECT a FROM b; SELECT a FROM b; SELECT a FROM b";
        test_single_mutation(
            sql,
            expected3,
            Box::new(RandomMutationSampler::new(
                2,
                2,
                vec![Box::new(SpliceIn {})],
            )),
        );
    }

    #[test]
    fn randomly() {
        test_single_mutation(
            "SELECT a FROM b",
            "SELECT a FROM b",
            Box::new(Randomly::new(Box::new(SpliceIn {}), 0.)),
        );
    }
}
