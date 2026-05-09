use std::ops::ControlFlow;

use lsf_core::entry::RawEntry;
use lsf_feedback::TestableEntry;
use rand::RngExt;
use sqlparser::ast::{Expr, visit_expressions_mut};

use crate::{MutationState, MutationStrategy};

pub struct RecursiveExpandExpr {
    pub max_depth: usize,
    pub chance_per_node: f64,
    pub chance_per_level: f64,
}

impl MutationStrategy for RecursiveExpandExpr {
    fn breed_inner(
        &self,
        parent: &mut TestableEntry<RawEntry>,
        _parent_gen: &[TestableEntry<RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<MutationState, crate::MutationError> {
        let Some(child_ast) = parent.ast_mut() else {
            return Err(crate::MutationError::ASTSHARED);
        };
        let mut is_mutated = false;

        _ = visit_expressions_mut(child_ast, |expr| {
            if rng.random_bool(self.chance_per_node)
                && let Ok(mut json_node) = serde_json::to_value(&expr)
            {
                let root_node = json_node.clone();

                let serde_json::Value::Object(json_object) = &mut json_node else {
                    return ControlFlow::Continue::<()>(());
                };

                let expandable_fields: Vec<String> = json_object
                    .keys()
                    .filter(|k| {
                        let mut test = json_object.clone();
                        test.insert((*k).clone(), root_node.clone());
                        serde_json::from_value::<Expr>(serde_json::Value::Object(test)).is_ok()
                    })
                    .cloned()
                    .collect();

                if expandable_fields.is_empty() {
                    return ControlFlow::Continue::<()>(());
                }

                expand_recursive(
                    &root_node,
                    json_object,
                    &expandable_fields,
                    self.max_depth,
                    self.chance_per_level,
                    rng,
                );

                if let Ok(mutated_expr) = serde_json::from_value(json_node) {
                    is_mutated = true;
                    *expr = mutated_expr;
                }
            }

            ControlFlow::Continue::<()>(())
        });

        if is_mutated {
            Ok(MutationState::Mutated)
        } else {
            Ok(MutationState::Unchanged)
        }
    }
}

fn expand_recursive(
    root: &serde_json::Value,
    node: &mut serde_json::Map<String, serde_json::Value>,
    expandable_fields: &[String],
    depth: usize,
    chance_per_level: f64,
    rng: &mut dyn rand::Rng,
) {
    if depth == 0 || !rng.random_bool(chance_per_level) {
        return;
    }
    let key = &expandable_fields[rng.random_range(..expandable_fields.len())];
    if let Some(slot) = node.get_mut(key) {
        *slot = root.clone();
        if let serde_json::Value::Object(inner_map) = slot {
            expand_recursive(
                root,
                inner_map,
                expandable_fields,
                depth - 1,
                chance_per_level,
                rng,
            );
        }
    }
}
