use std::{
    fmt::Debug,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64},
    },
};

use lsf_core::entry::{ID, RawEntry};
use lsf_feedback::TestableEntry;
use rand::Rng;
use thiserror::Error;

mod afl;
mod ident;
mod json_tree;
mod random_mutate;
mod recurse;
mod sample;
mod schedule;
mod splice;
mod structure;
mod values;

#[allow(unused_imports)]
pub use afl::*;
pub use ident::*;
pub use json_tree::*;
#[allow(unused_imports)]
pub use random_mutate::*;
#[allow(unused_imports)]
pub use recurse::*;
pub use sample::*;
pub use schedule::*;
pub use splice::*;
pub use structure::*;
pub use values::*;

pub trait MutationStrategy: Send + Sync + Debug {
    fn breed(
        &self,
        parent: &TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<&RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<MutationState, MutationError>;

    fn init(&mut self, _ctx: StrategyContext) {}
    fn decay(&self, _rate: f64) {}
}

#[derive(Clone, Default)]
pub struct StrategyContext {
    pub total_attempts: Arc<AtomicU64>,
    pub epoch: Arc<AtomicU32>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MutationState {
    Mutated(TestableEntry<RawEntry>),
    Unchanged,
}

impl MutationState {
    pub fn into_option(self) -> Option<TestableEntry<RawEntry>> {
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
    let entry_ = TestableEntry::new(entry.clone());

    let res = strategy
        .breed(
            &entry_,
            &[TestableEntry::new(&entry)],
            &mut SmallRng::seed_from_u64(42),
        )
        .unwrap();
    let MutationState::Mutated(child) = res else {
        return;
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
