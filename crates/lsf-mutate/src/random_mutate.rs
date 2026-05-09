use std::marker::PhantomData;

use arbitrary::{Arbitrary, Unstructured};
use rand::RngExt;

use crate::{AstNode, MutationStrategy};

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

        let mut node_count = 0;
        _ = T::visit(child_ast, |_| {
            node_count += 1;
            std::ops::ControlFlow::Continue::<()>(())
        });

        if node_count == 0 {
            return Ok(crate::MutationState::Unchanged);
        }

        let target_index = rng.random_range(0..node_count);

        let mut current_index = 0;
        let mut is_mutated = crate::MutationState::Unchanged;
        let mut data = vec![0u8; 256];

        _ = T::visit_mut(child_ast, |node| {
            if current_index == target_index {
                rng.fill_bytes(&mut data);
                let mut unstructured = Unstructured::new(&data);

                if let Ok(new_node) = unstructured.arbitrary() {
                    *node = new_node;
                    is_mutated = crate::MutationState::Mutated;
                }

                return std::ops::ControlFlow::Break::<()>(());
            }

            current_index += 1;
            std::ops::ControlFlow::Continue::<()>(())
        });

        Ok(is_mutated)
    }
}
