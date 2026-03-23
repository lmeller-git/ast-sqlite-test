use std::collections::HashMap;

use lsf_core::entry::{CorpusEntry, ID, RawEntry};
use rand::random_range;
use sqlparser::ast::visit_relations_mut;
use thiserror::Error;

mod afl;
mod control;
mod ident;
mod random_mutate;
mod recurse;
mod slice;

pub use afl::*;
pub use control::*;
pub use ident::*;
pub use random_mutate::*;
pub use recurse::*;
pub use slice::*;

pub trait MutationStrategy: Send + Sync {
    fn breed(
        &self,
        parent: &RawEntry,
        parent_gen: &[ID],
        mapping: &HashMap<ID, CorpusEntry>,
    ) -> Result<MutationState, MutationError>;
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Merger;

impl Merger {
    pub fn new() -> Self {
        Self
    }
}

impl MutationStrategy for Merger {
    fn breed(
        &self,
        parent: &RawEntry,
        parent_gen: &[ID],
        mapping: &HashMap<ID, CorpusEntry>,
    ) -> Result<MutationState, MutationError> {
        let other_idx = random_range(..parent_gen.len());
        if let Some(other) = mapping.get(&parent_gen[other_idx]) {
            let mut new = parent.ast().clone();
            new.extend(other.ast().iter().cloned());
            Ok(MutationState::Mutated(RawEntry::new(
                new,
                [parent.id(), other.id()].into(),
            )))
        } else {
            Err(MutationError::NOPARENT(parent_gen[other_idx]))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RandomUpperCase {
    threshhold: f32,
}

impl RandomUpperCase {
    pub fn new(threshhold: f32) -> Self {
        Self { threshhold }
    }
}

impl MutationStrategy for RandomUpperCase {
    fn breed(
        &self,
        parent: &RawEntry,
        _parent_gen: &[ID],
        _mapping: &HashMap<ID, CorpusEntry>,
    ) -> Result<MutationState, MutationError> {
        let mut ast = parent.ast().clone();

        _ = visit_relations_mut(&mut ast, |relation| {
            let rd = random_range(0.0..=1.);
            match &mut relation.0[0] {
                sqlparser::ast::ObjectNamePart::Identifier(id) => {
                    if rd <= self.threshhold {
                        id.value = id.value.to_ascii_uppercase();
                    }
                    std::ops::ControlFlow::Continue::<()>(())
                }
                sqlparser::ast::ObjectNamePart::Function(func) => {
                    if rd <= self.threshhold {
                        func.name.value = func.name.value.to_ascii_uppercase();
                    }
                    std::ops::ControlFlow::Continue::<()>(())
                }
            }
        });

        Ok(MutationState::Mutated(RawEntry::new(
            ast,
            [parent.id()].into(),
        )))
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MutationState {
    Mutated(RawEntry),
    Unchanged,
}

impl MutationState {
    pub fn into_option(self) -> Option<RawEntry> {
        match self {
            Self::Mutated(some) => Some(some),
            Self::Unchanged => None,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum MutationError {
    #[error("No entry with id {0:?} exists in mapping.")]
    NOPARENT(ID),
    #[error("The AST of node {0:?} is invalid for the purpose of this mutation")]
    INVALIDAST(ID),
    #[error("No mutation was done")]
    NOOP,
}
