use std::marker::PhantomData;

use arbitrary::{Arbitrary, Unstructured};

use crate::{AstNode, MutationState, MutationStrategy};

#[derive(Default)]
pub struct ArbitraryGenerator<T> {
    _phantom: PhantomData<T>,
}

impl<T> ArbitraryGenerator<T> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T: Send + Sync + for<'a> Arbitrary<'a> + AstNode> MutationStrategy for ArbitraryGenerator<T> {
    fn breed_inner(
        &self,
        child: &mut lsf_feedback::TestableEntry<lsf_core::entry::RawEntry>,
        _parent_gen: &[lsf_feedback::TestableEntry<lsf_core::entry::RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let Some(child_ast) = child.ast_mut() else {
            return Err(crate::MutationError::ASTSHARED);
        };

        let mut is_mutated = MutationState::Unchanged;

        let mut data = vec![0u8; size_of::<T>() * 2];

        _ = T::visit_mut(child_ast, |node| {
            rng.fill_bytes(&mut data);

            let mut unstructured = Unstructured::new(&data);

            if let Ok(new_node) = unstructured.arbitrary() {
                *node = new_node;
                is_mutated = MutationState::Mutated;
            }

            std::ops::ControlFlow::Continue::<()>(())
        });

        Ok(is_mutated)
    }
}
