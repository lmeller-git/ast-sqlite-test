use lsf_core::entry::RawEntry;
use lsf_feedback::TestableEntry;
use rand::{Rng, RngExt};
use sqlparser::ast::{
    Expr,
    Query,
    SetExpr,
    SetOperator,
    SetQuantifier,
    Statement,
    visit_expressions,
    visit_expressions_mut,
};

use crate::MutationStrategy;

pub struct SpliceIn {}

impl SpliceIn {}

impl MutationStrategy for SpliceIn {
    fn breed_inner(
        &self,
        parent: &mut TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let Some(child_ast) = parent.ast_mut() else {
            return Err(crate::MutationError::ASTSHARED);
        };
        let other_idx = rng.random_range(..parent_gen.len());
        let other = &parent_gen[other_idx];
        let random_start = rng.random_range(..other.ast().len());
        let random_end = rng.random_range(random_start + 1..=other.ast().len());
        let random_insert = rng.random_range(..child_ast.len());

        _ = child_ast.splice(
            random_insert..random_insert,
            other.ast()[random_start..random_end].iter().cloned(),
        );

        parent.parents.push(other.id());

        Ok(crate::MutationState::Mutated)
    }
}

pub struct SubQuery {
    pub mutation_chance: f64,
}

impl MutationStrategy for SubQuery {
    fn breed_inner(
        &self,
        parent: &mut TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let Some(child_ast) = parent.ast_mut() else {
            return Err(crate::MutationError::ASTSHARED);
        };
        let mut child_is_mutated = false;

        _ = visit_expressions_mut(child_ast, |expr| {
            if let Expr::BinaryOp { right, .. } = expr
                && matches!(**right, Expr::Value(_))
                && rng.random_bool(self.mutation_chance)
            {
                let other_idx = rng.random_range(..parent_gen.len());
                let other = &parent_gen[other_idx];
                _ = visit_expressions(other.ast(), |expr| {
                    if let Expr::Subquery(query) = expr {
                        **right = Expr::Subquery(query.clone());
                        child_is_mutated = true;
                    }
                    std::ops::ControlFlow::<()>::Break(())
                });
            }
            std::ops::ControlFlow::Continue::<()>(())
        });

        if child_is_mutated {
            Ok(crate::MutationState::Mutated)
        } else {
            Ok(crate::MutationState::Unchanged)
        }
    }
}

pub struct SetOps {}

impl MutationStrategy for SetOps {
    fn breed_inner(
        &self,
        parent: &mut TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let Some(child_ast) = parent.ast_mut() else {
            return Err(crate::MutationError::ASTSHARED);
        };
        let other_idx = rng.random_range(..parent_gen.len());
        let other = &parent_gen[other_idx];

        let left = child_ast
            .iter()
            .enumerate()
            .filter_map(|(i, stmt)| {
                if matches!(stmt, Statement::Query(_)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if left.is_empty() {
            return Ok(crate::MutationState::Unchanged);
        }

        let right = other
            .ast()
            .iter()
            .enumerate()
            .filter_map(|(i, stmt)| {
                if matches!(stmt, Statement::Query(_)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if right.is_empty() {
            return Ok(crate::MutationState::Unchanged);
        }

        let ops = [
            SetOperator::Union,
            SetOperator::Intersect,
            SetOperator::Except,
        ];
        let op = ops[rng.random_range(..ops.len())];
        let all = rng.random_bool(0.5);

        let right = other
            .ast()
            .get(*right.get(rng.random_range(..right.len())).unwrap())
            .unwrap();
        let left = child_ast
            .get(*left.get(rng.random_range(..left.len())).unwrap())
            .unwrap();

        let (Statement::Query(lq), Statement::Query(rq)) = (left, right) else {
            return Ok(crate::MutationState::Unchanged);
        };

        let combined = Statement::Query(Box::new(Query {
            body: Box::new(SetExpr::SetOperation {
                op,
                set_quantifier: if all {
                    SetQuantifier::All
                } else {
                    SetQuantifier::Distinct
                },
                left: lq.body.clone(),
                right: rq.body.clone(),
            }),
            with: None,
            order_by: None,
            limit_clause: None,
            fetch: None,
            locks: vec![],
            for_clause: None,
            settings: None,
            format_clause: None,
            pipe_operators: vec![],
        }));

        let rd = rng.random_range(..child_ast.len());
        child_ast[rd] = combined;

        parent.parents.push(other.id());

        Ok(crate::MutationState::Mutated)
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
            Box::new(SpliceIn {}),
        );
    }
}
