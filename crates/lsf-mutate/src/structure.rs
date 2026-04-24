use std::{
    collections::{HashMap, HashSet},
    ops::ControlFlow,
};

use lsf_core::entry::RawEntry;
use lsf_feedback::TestableEntry;
use rand::RngExt;
use sqlparser::ast::{
    Expr,
    ObjectName,
    visit_expressions,
    visit_expressions_mut,
    visit_relations,
    visit_relations_mut,
};

use crate::MutationStrategy;

#[derive(Debug)]
pub struct ExprShuffle {
    pub chance_per_node: f64,
}

impl MutationStrategy for ExprShuffle {
    fn breed(
        &self,
        parent: &TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let mut parent_exprs: HashMap<std::mem::Discriminant<Expr>, Vec<Expr>> = HashMap::new();

        let rd_donor = &parent_gen[rng.random_range(..parent_gen.len())];

        _ = visit_expressions(rd_donor.ast(), |expr| {
            parent_exprs
                .entry(std::mem::discriminant(expr))
                .and_modify(|entry| entry.push(expr.clone()))
                .or_insert(vec![expr.clone()]);
            ControlFlow::Continue::<()>(())
        });

        _ = visit_expressions(parent.ast(), |expr| {
            parent_exprs
                .entry(std::mem::discriminant(expr))
                .and_modify(|entry| entry.push(expr.clone()))
                .or_insert(vec![expr.clone()]);
            ControlFlow::Continue::<()>(())
        });

        if parent_exprs.is_empty() {
            return Ok(crate::MutationState::Unchanged);
        }

        let mut child_ast = parent.ast().clone();
        let mut is_mutated = false;

        _ = visit_expressions_mut(&mut child_ast, |expr| {
            if let Some(targets) = parent_exprs.get(&std::mem::discriminant(expr))
                && rng.random_bool(self.chance_per_node)
            {
                let random_target = &targets[rng.random_range(..targets.len())];
                if expr != random_target {
                    *expr = random_target.clone();
                    is_mutated = true;
                }
            }
            ControlFlow::Continue::<()>(())
        });

        if is_mutated {
            Ok(crate::MutationState::Mutated(
                RawEntry::new(child_ast, [parent.id(), rd_donor.id()].into()).into(),
            ))
        } else {
            Ok(crate::MutationState::Unchanged)
        }
    }
}

#[derive(Debug)]
pub struct RelShuffle {
    pub chance_per_node: f64,
}

impl MutationStrategy for RelShuffle {
    fn breed(
        &self,
        parent: &TestableEntry<RawEntry>,
        _parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn rand::Rng,
    ) -> Result<crate::MutationState, crate::MutationError> {
        let mut parent_rels: HashSet<ObjectName> = HashSet::new();

        _ = visit_relations(parent.ast(), |rel| {
            parent_rels.insert(rel.clone());
            ControlFlow::Continue::<()>(())
        });

        if parent_rels.is_empty() {
            return Ok(crate::MutationState::Unchanged);
        }

        let mut child_ast = parent.ast().clone();
        let mut is_mutated = false;
        let n_targets = parent_rels.len();

        _ = visit_relations_mut(&mut child_ast, |rel| {
            if rng.random_bool(self.chance_per_node) {
                let random_target = parent_rels
                    .iter()
                    .nth(rng.random_range(..n_targets))
                    .unwrap();
                if rel != random_target {
                    *rel = random_target.clone();
                    is_mutated = true;
                }
            }
            ControlFlow::Continue::<()>(())
        });

        if is_mutated {
            Ok(crate::MutationState::Mutated(
                RawEntry::new(child_ast, [parent.id()].into()).into(),
            ))
        } else {
            Ok(crate::MutationState::Unchanged)
        }
    }
}
