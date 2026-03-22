use std::collections::HashMap;

use lsf_core::entry::{CorpusEntry, ID, RawEntry};
use rand::random_range;
use sqlparser::ast::visit_relations_mut;
use thiserror::Error;

pub trait MutationStrategy: Send + Sync {
    fn breed(
        &self,
        parent: &RawEntry,
        parent_gen: &[ID],
        mapping: &HashMap<ID, CorpusEntry>,
    ) -> Result<RawEntry, MutationError>;
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
    ) -> Result<RawEntry, MutationError> {
        let other_idx = random_range(..parent_gen.len());
        if let Some(other) = mapping.get(&parent_gen[other_idx]) {
            let mut new = parent.ast().clone();
            new.extend(other.ast().iter().cloned());
            Ok(RawEntry::new(new, vec![parent.id(), other.id()]))
        } else {
            Err(MutationError::TODO)
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
    ) -> Result<RawEntry, MutationError> {
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

        Ok(RawEntry::new(ast, vec![parent.id()]))
    }
}

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum MutationError {
    #[error("TODO")]
    TODO,
}

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
