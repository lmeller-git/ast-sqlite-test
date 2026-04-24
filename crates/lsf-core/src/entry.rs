use std::{
    collections::BTreeSet,
    fmt::Display,
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

impl Display for ID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Meta {
    pub triggers_bug: bool,
    pub is_valid_syntax: bool,
    pub new_cov_nodes: usize,
    pub exec_time: u32,
}

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

    pub fn meta(&self) -> &Meta {
        &self.meta
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_unique() {
        let id1 = ID::next();
        let id2 = ID::next();
        assert!(id2 > id1);

        let raw = RawEntry::new(vec![], [].into());
        let raw2 = RawEntry::new(vec![], [].into());
        assert!(raw.id() > id2);
        assert!(raw2.id() > raw.id());
    }

    #[test]
    fn entry() {
        let raw = RawEntry::new(vec![], [].into());
        let raw2 = RawEntry::new(vec![], [raw.id()].into());
        assert_ne!(raw, raw2);
        assert!(raw.parents().next().is_none());
        assert_eq!(*raw2.parents().next().unwrap(), raw.id());

        let entry = raw.clone().into_corpus_entry(Meta::default());
        assert_eq!(*entry, raw);
    }
}
