use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    sync::{Arc, atomic::AtomicU32},
};

use smallvec::SmallVec;

use crate::ast::AST;

// TODO
// May want to use twox-hash to create ID from a hash of AST. This would allow using nohash_hasher for Hashmaps and wautomatic deduplication based on AST.
// However profiling shows that hashing the whole AST is quite slow evenb with twox-hash. May be worth it only if dedup is a bigger win

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

    pub fn as_raw(&self) -> u32 {
        self.raw
    }
}

impl Display for ID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Meta {
    pub triggers_bug: bool,
    pub is_valid_syntax: bool,
    pub exec_time: u32,
    pub new_cov_nodes: usize,
    pub query_size: usize,
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct RawEntry {
    pub id: ID,
    pub parents: SmallVec<[ID; 2]>,
    pub ast: Arc<AST>,
}

impl RawEntry {
    pub fn new(ast: AST, parents: SmallVec<[ID; 2]>) -> Self {
        Self {
            id: ID::next(),
            parents,
            ast: ast.into(),
        }
    }

    pub fn from_components(id: ID, ast: Arc<AST>, parents: SmallVec<[ID; 2]>) -> Self {
        Self { id, parents, ast }
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

    pub fn ast_mut(&mut self) -> Option<&mut AST> {
        Arc::get_mut(&mut self.ast)
    }

    pub fn make_mut_ast(&mut self) -> &mut AST {
        Arc::make_mut(&mut self.ast)
    }

    pub fn parents(&self) -> impl Iterator<Item = &ID> {
        self.parents.iter()
    }

    pub fn parents_mut(&mut self) -> &mut SmallVec<[ID; 2]> {
        &mut self.parents
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct CorpusEntry {
    pub raw: RawEntry,
    pub meta: Meta,
}

impl CorpusEntry {
    pub fn new(raw: RawEntry, meta: Meta) -> Self {
        Self { raw, meta }
    }

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

        let raw = RawEntry::new(vec![], Default::default());
        let raw2 = RawEntry::new(vec![], Default::default());
        assert!(raw.id() > id2);
        assert!(raw2.id() > raw.id());
    }

    #[test]
    fn entry() {
        let raw = RawEntry::new(vec![], Default::default());
        let raw2 = RawEntry::new(vec![], vec![raw.id()].into());
        assert_ne!(raw, raw2);
        assert!(raw.parents().next().is_none());
        assert_eq!(*raw2.parents().next().unwrap(), raw.id());

        let entry = raw.clone().into_corpus_entry(Meta::default());
        assert_eq!(*entry, raw);
    }
}
