use std::collections::HashMap;

use lsf_core::entry::{CorpusEntry, ID, RawEntry};
use rand::{Rng, RngExt};
use sqlparser::ast::visit_relations_mut;
use thiserror::Error;

mod afl;
mod ident;
mod random_mutate;
mod recurse;
mod sample;
mod splice;

#[allow(unused_imports)]
pub use afl::*;
pub use ident::*;
#[allow(unused_imports)]
pub use random_mutate::*;
#[allow(unused_imports)]
pub use recurse::*;
pub use sample::*;
pub use splice::*;

pub trait MutationStrategy: Send + Sync {
    fn breed(
        &self,
        parent: &RawEntry,
        parent_gen: &[ID],
        mapping: &HashMap<ID, CorpusEntry>,
        rng: &mut dyn Rng,
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
        rng: &mut dyn Rng,
    ) -> Result<MutationState, MutationError> {
        let other_idx = rng.random_range(..parent_gen.len());
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RandomUpperCase {}

impl RandomUpperCase {
    pub fn new() -> Self {
        Self {}
    }
}

impl MutationStrategy for RandomUpperCase {
    fn breed(
        &self,
        parent: &RawEntry,
        _parent_gen: &[ID],
        _mapping: &HashMap<ID, CorpusEntry>,
        _rng: &mut dyn Rng,
    ) -> Result<MutationState, MutationError> {
        let mut ast = parent.ast().clone();
        let mut was_mutated = false;

        _ = visit_relations_mut(&mut ast, |relation| {
            for id in &mut relation.0 {
                match id {
                    sqlparser::ast::ObjectNamePart::Identifier(id) => {
                        id.value = id.value.to_ascii_uppercase();
                        was_mutated = true;
                    }
                    sqlparser::ast::ObjectNamePart::Function(func) => {
                        func.name.value = func.name.value.to_ascii_uppercase();
                        was_mutated = true;
                    }
                }
            }
            std::ops::ControlFlow::Continue::<()>(())
        });

        if was_mutated {
            Ok(MutationState::Mutated(RawEntry::new(
                ast,
                [parent.id()].into(),
            )))
        } else {
            Ok(MutationState::Unchanged)
        }
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

#[cfg(test)]
pub(crate) fn test_single_mutation(sql: &str, expected: &str, strategy: Box<dyn MutationStrategy>) {
    use rand::{SeedableRng, rngs::SmallRng};
    use sqlparser::{dialect::SQLiteDialect, parser::Parser};

    let parsed = Parser::parse_sql(&SQLiteDialect {}, sql).unwrap();
    let entry = RawEntry::new(parsed, Default::default());

    let res = strategy
        .breed(
            &entry,
            &[entry.id()],
            &([(
                entry.id(),
                entry.clone().into_corpus_entry(lsf_core::entry::Meta {}),
            )]
            .into()),
            &mut SmallRng::seed_from_u64(42),
        )
        .unwrap();
    let MutationState::Mutated(child) = res else {
        panic!()
    };

    assert_eq!(
        expected,
        child
            .ast()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merger() {
        test_single_mutation(
            "SELECT A FROM B",
            "SELECT A FROM B; SELECT A FROM B",
            Box::new(Merger {}),
        );
    }

    #[test]
    fn random_upper() {
        test_single_mutation(
            "CREATE TABLE b (); SELECT a FROM b",
            "CREATE TABLE B (); SELECT a FROM B",
            Box::new(RandomUpperCase::new()),
        );
    }
}
