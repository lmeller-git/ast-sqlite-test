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

pub trait MutationStrategy: Send + Sync {
    fn breed_inner(
        &self,
        child: &mut TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<MutationState, MutationError>;

    fn breed(
        &self,
        child: &mut TestableEntry<RawEntry>,
        parent_gen: &[TestableEntry<RawEntry>],
        rng: &mut dyn Rng,
    ) -> Result<MutationState, MutationError> {
        let r = self.breed_inner(child, parent_gen, rng);
        match r {
            Ok(MutationState::Mutated) => {
                child.fire_parent_hooks(lsf_feedback::TestOutcome::Mutated)
            }
            _ => child.fire_parent_hooks(lsf_feedback::TestOutcome::NOOP),
        }

        r
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MutationState {
    Mutated,
    Unchanged,
}

impl MutationState {
    pub fn into_option(self) -> Option<()> {
        match self {
            Self::Mutated => Some(()),
            Self::Unchanged => None,
        }
    }

    pub fn into_bool(self) -> bool {
        self.into_option().is_some()
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
    #[error("Could not acquire mut ref of ast")]
    ASTSHARED,
}

#[cfg(test)]
pub(crate) fn test_single_mutation(sql: &str, expected: &str, strategy: Box<dyn MutationStrategy>) {
    use rand::{SeedableRng, rngs::SmallRng};
    use smallvec::smallvec;
    use sqlparser::{dialect::SQLiteDialect, parser::Parser};

    let parsed = Parser::parse_sql(&SQLiteDialect {}, sql).unwrap();
    let entry = RawEntry::new(parsed, Default::default());
    let mut entry_ = TestableEntry::new(RawEntry::new(entry.ast().clone(), smallvec![entry.id()]));

    let res = strategy
        .breed(
            &mut entry_,
            &[TestableEntry::new(entry)],
            &mut SmallRng::seed_from_u64(42),
        )
        .unwrap();
    let MutationState::Mutated = res else {
        return;
    };

    assert_eq!(
        expected,
        entry_
            .ast()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    )
}
