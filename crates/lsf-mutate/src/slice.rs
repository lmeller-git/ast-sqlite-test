use lsf_core::entry::RawEntry;
use rand::random_range;

use crate::MutationStrategy;

pub struct SliceIn {}

impl SliceIn {}

impl MutationStrategy for SliceIn {
    fn breed(
        &self,
        parent: &lsf_core::entry::RawEntry,
        parent_gen: &[lsf_core::entry::ID],
        mapping: &std::collections::HashMap<lsf_core::entry::ID, lsf_core::entry::CorpusEntry>,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let other_idx = random_range(..parent_gen.len());
        if let Some(other) = mapping.get(&parent_gen[other_idx]) {
            let random_start = random_range(..other.ast().len());
            let random_end = random_range(random_start + 1..=other.ast().len());
            let random_insert = random_range(..parent.ast().len());

            let mut child_ast = parent.ast().clone();
            _ = child_ast.splice(
                random_insert..random_insert,
                other.ast()[random_start..random_end].iter().cloned(),
            );
            Ok(crate::MutationState::Mutated(RawEntry::new(
                child_ast,
                [parent.id(), other.id()].into(),
            )))
        } else {
            Err(crate::MutationError::NOPARENT(parent_gen[other_idx]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_single_mutation;

    #[test]
    fn slice_basic() {
        test_single_mutation(
            "SELECT A FROM B",
            "SELECT A FROM B; SELECT A FROM B",
            Box::new(SliceIn {}),
        );
    }
}
