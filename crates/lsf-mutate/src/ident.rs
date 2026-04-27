use lsf_core::entry::RawEntry;
use lsf_feedback::TestableEntry;
use rand::{Rng, RngExt};
use sqlparser::ast::{
    CreateTable,
    ObjectName,
    ObjectNamePart,
    Statement,
    visit_relations_mut,
    visit_statements,
    visit_statements_mut,
};

use crate::MutationStrategy;

#[derive(Debug)]
pub struct TableNameScramble {}

impl TableNameScramble {}

impl MutationStrategy for TableNameScramble {
    fn breed_inner(
        &self,
        parent: &TestableEntry<RawEntry>,
        _parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let mut tables = Vec::new();

        _ = visit_statements(parent.ast(), |stmt| {
            if let Statement::CreateTable(CreateTable { name, .. })
            | Statement::CreateVirtualTable { name, .. } = stmt
                && let Some(ident) = name.0[0].as_ident()
            {
                tables.push(ident.value.clone())
            }
            std::ops::ControlFlow::Continue::<()>(())
        });

        if tables.is_empty() {
            return Ok(crate::MutationState::Unchanged);
        }

        let mut child_ast = parent.ast().clone();
        let mut child_is_mutated = false;

        for stmt in &mut child_ast {
            if matches!(
                stmt,
                Statement::CreateTable { .. } | Statement::CreateVirtualTable { .. }
            ) {
                continue;
            }

            _ = visit_relations_mut(stmt, |relation| {
                let ObjectName(name_parts) = relation;
                for part in name_parts {
                    if let ObjectNamePart::Identifier(ident) = part {
                        let random_choice = rng.random_range(..tables.len());
                        ident.value = tables[random_choice].clone();
                        child_is_mutated = true;
                    }
                }
                std::ops::ControlFlow::Continue::<()>(())
            });
        }

        if child_is_mutated {
            Ok(crate::MutationState::Mutated(
                RawEntry::new(child_ast, [parent.id()].into()).into(),
            ))
        } else {
            Ok(crate::MutationState::Unchanged)
        }
    }
}

#[derive(Debug)]
pub struct TableGuard {}

impl MutationStrategy for TableGuard {
    fn breed_inner(
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
    fn table_name_scrambler() {
        test_single_mutation(
            "CREATE TABLE A (); SELECT BAR FROM C; SELECT FOO FROM A",
            "CREATE TABLE A (); SELECT BAR FROM A; SELECT FOO FROM A",
            Box::new(TableNameScramble {}),
        );
    }

    #[test]
    fn table_guard() {
        test_single_mutation(
            "CREATE TABLE A (); CREATE TABLE A ()",
            "CREATE TABLE IF NOT EXISTS A (); CREATE TABLE IF NOT EXISTS A ()",
            Box::new(TableGuard {}),
        );
    }
}
