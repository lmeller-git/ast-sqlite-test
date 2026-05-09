use std::{collections::HashMap, marker::PhantomData, mem::Discriminant, ops::ControlFlow};

use lsf_core::{ast::AST, entry::RawEntry};
use lsf_feedback::TestableEntry;
use rand::RngExt;
use serde::{Serialize, de::DeserializeOwned};
use sqlparser::ast::{
    Expr,
    Statement,
    visit_expressions,
    visit_expressions_mut,
    visit_statements,
    visit_statements_mut,
};

use crate::{MutationState, MutationStrategy};

pub trait AstNode: Serialize + DeserializeOwned + 'static {
    fn visit_mut<T>(ast: &mut AST, f: impl FnMut(&mut Self) -> ControlFlow<T>) -> ControlFlow<T>;
    fn visit<T>(ast: &AST, f: impl FnMut(&Self) -> ControlFlow<T>) -> ControlFlow<T>;
    fn discriminant(node: &Self) -> Discriminant<Self>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldOperation {
    ShuffleTwo,
    NullRandom,
    ShuffleSelf,
}

pub struct TreeMutator<T> {
    pub chance_per_node: f64,
    pub chance_per_field: f64,
    pub operation: FieldOperation,
    pub _phantom: PhantomData<T>,
}

impl<T: AstNode + Send + Sync + Clone> MutationStrategy for TreeMutator<T> {
    fn breed_inner(
        &self,
        parent: &mut TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        Ok(match self.operation {
            FieldOperation::ShuffleTwo => {
                let rd_donor = &parent_gen[rng.random_range(..parent_gen.len())];
                shuffle_across::<T>(
                    parent,
                    rd_donor,
                    rng,
                    self.chance_per_node,
                    self.chance_per_field,
                )
            }
            FieldOperation::ShuffleSelf => {
                let parent_clone = parent.clone();
                shuffle_across::<T>(
                    parent,
                    &parent_clone,
                    rng,
                    self.chance_per_node,
                    self.chance_per_field,
                )
            }
            FieldOperation::NullRandom => {
                let Some(child_ast) = parent.ast_mut() else {
                    return Err(crate::MutationError::ASTSHARED);
                };
                let mut is_mutated = false;

                _ = T::visit_mut(child_ast, |node| {
                    if rng.random_bool(self.chance_per_node)
                        && let Ok(mut json_node) = serde_json::to_value(&node)
                    {
                        null_random_fields(&mut json_node, self.chance_per_field, rng);
                        if let Ok(mutated_node) = serde_json::from_value(json_node) {
                            *node = mutated_node;
                            is_mutated = true
                        }
                    }
                    ControlFlow::Continue::<()>(())
                });

                if is_mutated {
                    MutationState::Mutated
                } else {
                    MutationState::Unchanged
                }
            }
        })
    }
}

fn shuffle_across<T: AstNode + Send + Sync + Clone>(
    parent: &mut RawEntry,
    donor: &RawEntry,
    rng: &mut dyn rand::Rng,
    chance_per_node: f64,
    chance_per_field: f64,
) -> MutationState {
    let Some(child_ast) = parent.ast_mut() else {
        return crate::MutationState::Unchanged;
    };
    let mut is_mutated = false;
    let mut donor_nodes: HashMap<std::mem::Discriminant<T>, Vec<T>> = HashMap::new();

    _ = T::visit(donor.ast(), |node| {
        donor_nodes
            .entry(std::mem::discriminant(node))
            .and_modify(|entry| entry.push(node.clone()))
            .or_insert(vec![node.clone()]);
        ControlFlow::Continue::<()>(())
    });

    if donor_nodes.is_empty() {
        return MutationState::Unchanged;
    }

    _ = T::visit_mut(child_ast, |node| {
        if rng.random_bool(chance_per_node)
            && let Some(donor) = donor_nodes
                .get(&T::discriminant(node))
                .map(|donors| &donors[rng.random_range(0..donors.len())])
            && let Ok(mut node_json) = serde_json::to_value(&node)
            && let Ok(donor_json) = serde_json::to_value(donor)
        {
            swap_random_fields(&mut node_json, &donor_json, chance_per_field, rng);
            if let Ok(mutated_node) = serde_json::from_value(node_json) {
                *node = mutated_node;
                is_mutated = true
            }
        }

        ControlFlow::Continue::<()>(())
    });

    if is_mutated {
        parent.parents.push(donor.id());
        MutationState::Mutated
    } else {
        MutationState::Unchanged
    }
}

fn swap_random_fields(
    dst: &mut serde_json::Value,
    src: &serde_json::Value,
    chance: f64,
    rng: &mut dyn rand::Rng,
) {
    match (dst, src) {
        (serde_json::Value::Object(to), serde_json::Value::Object(from)) => {
            for (k, v) in to.iter_mut() {
                if let Some(new) = from.get(k) {
                    if rng.random_bool(chance) {
                        *v = new.clone();
                    } else {
                        swap_random_fields(v, new, chance, rng);
                    }
                }
            }
        }
        (serde_json::Value::Array(to), serde_json::Value::Array(from))
            if from.len() >= to.len() =>
        {
            for to_slot in to {
                let donor = &from[rng.random_range(0..from.len())];
                if rng.random_bool(chance) {
                    *to_slot = donor.clone();
                } else {
                    swap_random_fields(to_slot, donor, chance, rng);
                }
            }
        }
        _ => {}
    }
}

fn null_random_fields(val: &mut serde_json::Value, chance: f64, rng: &mut dyn rand::Rng) {
    match val {
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                if rng.random_bool(chance) {
                    *v = serde_json::Value::Null;
                } else {
                    null_random_fields(v, chance, rng);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                null_random_fields(v, chance, rng);
            }
        }
        _ => {}
    }
}

impl AstNode for Statement {
    fn visit_mut<T>(ast: &mut AST, f: impl FnMut(&mut Self) -> ControlFlow<T>) -> ControlFlow<T> {
        visit_statements_mut(ast, f)
    }

    fn visit<T>(ast: &AST, f: impl FnMut(&Self) -> ControlFlow<T>) -> ControlFlow<T> {
        visit_statements(ast, f)
    }

    fn discriminant(node: &Self) -> Discriminant<Self> {
        std::mem::discriminant(node)
    }
}

impl AstNode for Expr {
    fn visit_mut<T>(ast: &mut AST, f: impl FnMut(&mut Self) -> ControlFlow<T>) -> ControlFlow<T> {
        visit_expressions_mut(ast, f)
    }

    fn visit<T>(ast: &AST, f: impl FnMut(&Self) -> ControlFlow<T>) -> ControlFlow<T> {
        visit_expressions(ast, f)
    }

    fn discriminant(node: &Self) -> Discriminant<Self> {
        std::mem::discriminant(node)
    }
}
