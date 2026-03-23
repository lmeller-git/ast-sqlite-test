use lsf_core::entry::RawEntry;
use rand::random_range;

use crate::{MutationState, MutationStrategy};

pub struct RandomMutationSampler {
    choices: Vec<Box<dyn MutationStrategy>>,
    choose_max: usize,
}

impl RandomMutationSampler {
    pub fn new(choose_max: usize, choices: Vec<Box<dyn MutationStrategy>>) -> Self {
        Self {
            choices,
            choose_max,
        }
    }
}

impl MutationStrategy for RandomMutationSampler {
    fn breed(
        &self,
        parent: &lsf_core::entry::RawEntry,
        parent_gen: &[lsf_core::entry::ID],
        mapping: &std::collections::HashMap<lsf_core::entry::ID, lsf_core::entry::CorpusEntry>,
    ) -> Result<MutationState, crate::MutationError> {
        let n_chosen = random_range(..self.choose_max);
        if n_chosen == 0 {
            return Ok(MutationState::Unchanged);
        }

        let mut status = MutationState::Unchanged;
        let mut current_parent: &RawEntry = parent;

        for i in 0..n_chosen {
            let chosen_strategy = random_range(..self.choices.len());
            if let Ok(MutationState::Mutated(mut next)) =
                self.choices[chosen_strategy].breed(current_parent, parent_gen, mapping)
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
            }
        }

        Ok(status)
    }
}
