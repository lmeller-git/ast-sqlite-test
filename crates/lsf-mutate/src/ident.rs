use lsf_core::entry::RawEntry;
use lsf_feedback::TestableEntry;
use rand::Rng;
use sqlparser::ast::{CreateTable, Statement, visit_statements_mut};

use crate::MutationStrategy;

pub struct TableGuard {}

impl MutationStrategy for TableGuard {
    fn breed(
        &self,
        parent: &TestableEntry<RawEntry>,
        _parent_gen: &[TestableEntry<&RawEntry>],
        _rng: &mut dyn Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let mut child_ast = parent.ast().clone();
        let mut mutation_occured = false;

        _ = visit_statements_mut(&mut child_ast, |stmt| {
            if let Statement::CreateTable(CreateTable { if_not_exists, .. })
            | Statement::CreateVirtualTable { if_not_exists, .. } = stmt
            {
                mutation_occured = true;
                *if_not_exists = true
            }
            std::ops::ControlFlow::Continue::<()>(())
        });

        if mutation_occured {
            Ok(crate::MutationState::Mutated(
                RawEntry::new(child_ast, [parent.id()].into()).into(),
            ))
        } else {
            Ok(crate::MutationState::Unchanged)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_single_mutation;

    #[test]
    fn table_guard() {
        test_single_mutation(
            "CREATE TABLE A (); CREATE TABLE A ()",
            "CREATE TABLE IF NOT EXISTS A (); CREATE TABLE IF NOT EXISTS A ()",
            Box::new(TableGuard {}),
        );
    }
}
