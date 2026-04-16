use lsf_core::entry::RawEntry;
use rand::{Rng, RngExt};
use sqlparser::ast::{BinaryOperator, Expr, Value, ValueWithSpan, visit_expressions_mut};

use crate::MutationStrategy;

const NUM_BOUNDS: &[&str] = &[
    "0",
    "1",
    "-1",
    "2147483647",
    "-2147483648",
    "9223372036854775807",
    "-9223372036854775808",
];

pub struct NumericBounds {
    pub mutate_chance: f64,
}

impl MutationStrategy for NumericBounds {
    fn breed(
        &self,
        parent: &lsf_core::entry::RawEntry,
        _parent_gen: &[lsf_core::entry::ID],
        _mapping: &std::collections::HashMap<lsf_core::entry::ID, lsf_core::entry::CorpusEntry>,
        rng: &mut dyn Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let mut child_ast = parent.ast().clone();
        let mut child_is_mutated = false;

        for stmt in &mut child_ast {
            _ = visit_expressions_mut(stmt, |expr| {
                if let Expr::Value(ValueWithSpan {
                    value: Value::Number(n, _),
                    span: _,
                }) = expr
                    && rng.random_bool(self.mutate_chance)
                {
                    let choice = rng.random_range(..NUM_BOUNDS.len());
                    *n = NUM_BOUNDS[choice].to_string();
                    child_is_mutated = true;
                }

                std::ops::ControlFlow::Continue::<()>(())
            });
        }

        if child_is_mutated {
            Ok(crate::MutationState::Mutated(RawEntry::new(
                child_ast,
                [parent.id()].into(),
            )))
        } else {
            Ok(crate::MutationState::Unchanged)
        }
    }
}

pub struct OperatorFlip {
    pub flip_chance: f64,
}

impl MutationStrategy for OperatorFlip {
    fn breed(
        &self,
        parent: &lsf_core::entry::RawEntry,
        _parent_gen: &[lsf_core::entry::ID],
        _mapping: &std::collections::HashMap<lsf_core::entry::ID, lsf_core::entry::CorpusEntry>,
        rng: &mut dyn Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let mut child_ast = parent.ast().clone();
        let mut child_is_mutated = false;

        for stmt in &mut child_ast {
            _ = visit_expressions_mut(stmt, |expr| {
                if let Expr::BinaryOp { op, .. } = expr
                    && rng.random_bool(self.flip_chance)
                {
                    let new_op = match op {
                        BinaryOperator::Eq => BinaryOperator::NotEq,
                        BinaryOperator::NotEq => BinaryOperator::Eq,
                        BinaryOperator::Lt => BinaryOperator::LtEq,
                        BinaryOperator::LtEq => BinaryOperator::Lt,
                        BinaryOperator::Gt => BinaryOperator::GtEq,
                        BinaryOperator::GtEq => BinaryOperator::Gt,
                        BinaryOperator::And => BinaryOperator::Or,
                        BinaryOperator::Or => BinaryOperator::And,
                        BinaryOperator::Plus => BinaryOperator::Minus,
                        BinaryOperator::Minus => BinaryOperator::Plus,
                        _ => op.clone(),
                    };

                    if *op != new_op {
                        *op = new_op;
                        child_is_mutated = true;
                    }
                }
                std::ops::ControlFlow::Continue::<()>(())
            });
        }

        if child_is_mutated {
            Ok(crate::MutationState::Mutated(RawEntry::new(
                child_ast,
                [parent.id()].into(),
            )))
        } else {
            Ok(crate::MutationState::Unchanged)
        }
    }
}
