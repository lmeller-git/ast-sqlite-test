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
        _parent: &TestableEntry<RawEntry>,
        _parent_gen: &[TestableEntry<&RawEntry>],
        _rng: &mut dyn Rng,
    ) -> Result<MutationState, crate::MutationError> {
        Err(crate::MutationError::NOOP)
    }

    fn breed(
        &self,
        parent: &TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<MutationState, crate::MutationError> {
        let n_chosen = rng.random_range(self.choose_min..=self.choose_max);
        if n_chosen == 0 {
            return Ok(MutationState::Unchanged);
        }

        let mut status = MutationState::Unchanged;
        let mut current_parent: &TestableEntry<RawEntry> = parent;

        for i in 0..n_chosen {
            let chosen_strategy = rng.random_range(..self.choices.len());
            if let Ok(MutationState::Mutated(mut next)) =
                self.choices[chosen_strategy].breed_inner(current_parent, parent_gen, rng)
            {
                if i > 0 && status != MutationState::Unchanged {
                    next.parents_mut().extend(current_parent.parents());
                }
                status = MutationState::Mutated(next);
                current_parent = if let MutationState::Mutated(next) = &status {
                    next
                } else {
                    unreachable!()
                };
                parent.fire_build_hooks(lsf_feedback::TestOutcome::Mutated);
            } else {
                parent.fire_build_hooks(lsf_feedback::TestOutcome::NOOP);
            }
        }

        Ok(status)
    }

    fn init(&mut self, ctx: crate::StrategyContext) {
        for s in &mut self.choices {
            s.init(ctx.clone());
        }
    }

    fn decay(&self, rate: f64) {
        for s in &self.choices {
            s.decay(rate);
        }
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
        parent: &TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<MutationState, crate::MutationError> {
        self.over.breed(parent, parent_gen, rng)
    }

    fn breed(
        &self,
        parent: &TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<MutationState, crate::MutationError> {
        if rng.random_bool(self.prob) {
            self.breed_inner(parent, parent_gen, rng)
        } else {
            Ok(MutationState::Unchanged)
        }
    }

    fn init(&mut self, ctx: crate::StrategyContext) {
        self.over.init(ctx);
    }

    fn decay(&self, rate: f64) {
        self.over.decay(rate);
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
