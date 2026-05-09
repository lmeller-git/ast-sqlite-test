use std::ops::ControlFlow;

use rand::RngExt;
use sqlparser::ast::{Expr, visit_expressions_mut};

use crate::MutationStrategy;

pub struct SpliceOut {
    pub p_extend: f64,
}

impl SpliceOut {
    pub fn new(p_extend: f64) -> Self {
        Self { p_extend }
    }
}

impl Default for SpliceOut {
    fn default() -> Self {
        Self { p_extend: 0.5 }
    }
}

impl MutationStrategy for SpliceOut {
    fn breed_inner(
        &self,
        child: &mut lsf_feedback::TestableEntry<lsf_core::entry::RawEntry>,
        _parent_gen: &[lsf_feedback::TestableEntry<lsf_core::entry::RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let Some(child_ast) = child.ast_mut() else {
            return Err(crate::MutationError::ASTSHARED);
        };

        // extremely short queries are useless, as we need to create at least some state
        if child_ast.len() <= 4 {
            return Ok(crate::MutationState::Unchanged);
        }

        let random_start = rng.random_range(..child_ast.len());

        let mut splice_len = 1;
        while rng.random_bool(self.p_extend) {
            splice_len += 1;
        }

        let random_end = child_ast.len().min(random_start + splice_len);

        if random_end == random_start {
            return Ok(crate::MutationState::Unchanged);
        }

        _ = child_ast.splice(random_start..random_end, []);

        Ok(crate::MutationState::Mutated)
    }
}

pub struct HoistExpr {
    pub chance_per_node: f64,
}

impl MutationStrategy for HoistExpr {
    fn breed_inner(
        &self,
        parent: &mut lsf_feedback::TestableEntry<lsf_core::entry::RawEntry>,
        _parent_gen: &[lsf_feedback::TestableEntry<lsf_core::entry::RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let Some(child_ast) = parent.ast_mut() else {
            return Err(crate::MutationError::ASTSHARED);
        };
        let mut is_mutated = false;

        _ = visit_expressions_mut(child_ast, |expr| {
            if rng.random_bool(self.chance_per_node)
                && let Ok(json_node) = serde_json::to_value(&expr)
            {
                let candidates = extract_child_exprs(&json_node);

                if !candidates.is_empty() {
                    let chosen_child = &candidates[rng.random_range(..candidates.len())];
                    *expr = chosen_child.clone();
                    is_mutated = true;
                }
            }

            ControlFlow::Continue::<()>(())
        });

        if is_mutated {
            Ok(crate::MutationState::Mutated)
        } else {
            Ok(crate::MutationState::Unchanged)
        }
    }
}

fn extract_child_exprs(root: &serde_json::Value) -> Vec<Expr> {
    fn dig(val: &serde_json::Value, results: &mut Vec<Expr>) {
        if let Ok(expr) = serde_json::from_value::<Expr>(val.clone()) {
            results.push(expr);
        }

        match val {
            serde_json::Value::Object(map) => {
                for v in map.values() {
                    dig(v, results);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    dig(v, results);
                }
            }
            _ => {}
        }
    }

    let mut results = Vec::new();

    dig(root, &mut results);
    results
}
