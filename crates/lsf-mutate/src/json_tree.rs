use std::{
    collections::HashMap,
    fmt::Debug,
    marker::PhantomData,
    mem::Discriminant,
    ops::ControlFlow,
};

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
    fn dbg_ty() -> &'static str;
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
    fn breed(
        &self,
        parent: &TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<&RawEntry>],
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
            FieldOperation::ShuffleSelf => shuffle_across::<T>(
                parent,
                parent,
                rng,
                self.chance_per_node,
                self.chance_per_field,
            ),
            FieldOperation::NullRandom => {
                let mut is_mutated = false;
                let mut child_ast = parent.ast().clone();

                _ = T::visit_mut(&mut child_ast, |node| {
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
                    MutationState::Mutated(RawEntry::new(child_ast, [parent.id()].into()).into())
                } else {
                    MutationState::Unchanged
                }
            }
        })
    }
}

impl<T: AstNode> Debug for TreeMutator<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeMutator")
            .field("node_chance", &self.chance_per_node)
            .field("field_chance", &self.chance_per_field)
            .field("op", &self.operation)
            .field("ty", &T::dbg_ty())
            .finish()
    }
}

#[derive(Debug)]
pub struct RecursiveExpandExpr {
    pub max_depth: usize,
    pub chance_per_node: f64,
    pub chance_per_level: f64,
}

impl MutationStrategy for RecursiveExpandExpr {
    fn breed(
        &self,
        parent: &TestableEntry<RawEntry>,
        _parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<MutationState, crate::MutationError> {
        let mut is_mutated = false;
        let mut child_ast = parent.ast().clone();

        _ = visit_expressions_mut(&mut child_ast, |expr| {
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
            Ok(MutationState::Mutated(
                RawEntry::new(child_ast, [parent.id()].into()).into(),
            ))
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

fn shuffle_across<T: AstNode + Send + Sync + Clone>(
    parent: &RawEntry,
    donor: &RawEntry,
    rng: &mut dyn rand::Rng,
    chance_per_node: f64,
    chance_per_field: f64,
) -> MutationState {
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

    let mut child_ast = parent.ast().clone();

    _ = T::visit_mut(&mut child_ast, |node| {
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
        MutationState::Mutated(RawEntry::new(child_ast, [donor.id(), parent.id()].into()).into())
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

    fn dbg_ty() -> &'static str {
        "Statement"
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

    fn dbg_ty() -> &'static str {
        "Expr"
    }
}
