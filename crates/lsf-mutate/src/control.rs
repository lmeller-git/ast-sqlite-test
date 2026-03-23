use lsf_core::entry::RawEntry;
use rand::random_range;

use crate::{MutationState, MutationStrategy};

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
    fn breed(
        &self,
        parent: &lsf_core::entry::RawEntry,
        parent_gen: &[lsf_core::entry::ID],
        mapping: &std::collections::HashMap<lsf_core::entry::ID, lsf_core::entry::CorpusEntry>,
    ) -> Result<MutationState, crate::MutationError> {
        let n_chosen = random_range(self.choose_min..=self.choose_max);
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

#[cfg(test)]
mod tests {
    use sqlparser::{dialect::SQLiteDialect, parser::Parser};

    use super::*;
    use crate::{RandomUpperCase, SliceIn, test_single_mutation};

    #[test]
    fn random_sampler() {
        let sql = "SELECT a FROM b";
        let expected1 = "SELECT a FROM B";
        let expected2 = "SELECT a FROM b; SELECT a FROM b";

        let parsed = Parser::parse_sql(&SQLiteDialect {}, sql).unwrap();
        let entry = RawEntry::new(parsed, Default::default());

        let res = RandomMutationSampler::new(
            1,
            1,
            vec![Box::new(RandomUpperCase::new(1.)), Box::new(SliceIn {})],
        )
        .breed(
            &entry,
            &[entry.id()],
            &([(
                entry.id(),
                entry.clone().into_corpus_entry(lsf_core::entry::Meta {}),
            )]
            .into()),
        )
        .unwrap();
        let MutationState::Mutated(child) = res else {
            panic!()
        };

        let res = child
            .ast()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("; ");

        if res != expected1 && res != expected2 {
            panic!()
        }

        let expected3 = "SELECT a FROM b; SELECT a FROM b; SELECT a FROM b";
        test_single_mutation(
            sql,
            expected3,
            Box::new(RandomMutationSampler::new(2, 2, vec![Box::new(SliceIn {})])),
        );
    }
}
