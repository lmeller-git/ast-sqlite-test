use std::{
    collections::BTreeSet,
    ops::{Deref, DerefMut},
    sync::atomic::AtomicU32,
};

use crate::ast::AST;

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct ID {
    raw: u32,
}

impl ID {
    pub fn next() -> Self {
        static CURRENT_ID: AtomicU32 = AtomicU32::new(0);

        Self {
            raw: CURRENT_ID.fetch_add(1, std::sync::atomic::Ordering::AcqRel),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Meta {}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct RawEntry {
    id: ID,
    parents: BTreeSet<ID>,
    ast: AST,
}

impl RawEntry {
    pub fn new(ast: AST, parents: BTreeSet<ID>) -> Self {
        Self {
            id: ID::next(),
            parents,
            ast,
        }
    }

    pub fn into_corpus_entry(self, meta: Meta) -> CorpusEntry {
        CorpusEntry { raw: self, meta }
    }

    pub fn id(&self) -> ID {
        self.id
    }

    pub fn ast(&self) -> &AST {
        &self.ast
    }

    pub fn parents(&self) -> impl Iterator<Item = &ID> {
        self.parents.iter()
    }

    pub fn parents_mut(&mut self) -> &mut BTreeSet<ID> {
        &mut self.parents
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct CorpusEntry {
    raw: RawEntry,
    meta: Meta,
}

impl CorpusEntry {
    pub fn raw(&self) -> &RawEntry {
        &self.raw
    }
}

impl Deref for CorpusEntry {
    type Target = RawEntry;

    fn deref(&self) -> &Self::Target {
        self.raw()
    }
}

impl DerefMut for CorpusEntry {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.raw
    }
}

impl AsRef<RawEntry> for CorpusEntry {
    fn as_ref(&self) -> &RawEntry {
        self
    }
}
