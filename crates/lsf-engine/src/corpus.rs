use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
};

use lsf_core::{
    IDMAp,
    ast::AST,
    entry::{CorpusEntry, ID},
};
use lsf_cov::bitmap::{EdgeMap, ScoredEdges};

pub trait CorpusHandler<T>: Sync + Send {
    fn get(&mut self, id: &ID) -> Option<CorpusEntry>;
    fn update(&mut self, id: &ID, s: T);
    fn insert(&mut self, entry: CorpusEntry, s: T);
    fn resize(&mut self);
    fn ids(&self) -> Vec<ID>;
    fn clear(&mut self);
    fn size(&self) -> usize;
}

pub struct Corpus {
    pub edge_map: EdgeMap,
    pub entry_rating: ScoredEdges,
    pub diversity: DiversityEnsurance,
    handler: Box<dyn CorpusHandler<f64>>,
}

impl Corpus {
    pub fn new(max_edges: usize, handler: Box<dyn CorpusHandler<f64>>) -> Self {
        Self {
            edge_map: EdgeMap::new(max_edges),
            entry_rating: ScoredEdges::new(max_edges),
            diversity: DiversityEnsurance::new(),
            handler,
        }
    }
}

impl CorpusHandler<f64> for Corpus {
    fn get(&mut self, id: &ID) -> Option<CorpusEntry> {
        self.handler.get(id)
    }

    fn update(&mut self, id: &ID, s: f64) {
        self.handler.update(id, s);
    }

    fn insert(&mut self, entry: CorpusEntry, s: f64) {
        self.handler.insert(entry, s);
    }

    fn clear(&mut self) {
        self.handler.clear();
        self.entry_rating
            .best_entries
            .iter_mut()
            .for_each(|item| *item = None);
        self.diversity.entries.clear();
        self.diversity.hashes.clear();
    }

    fn resize(&mut self) {
        self.handler.resize();
    }

    fn size(&self) -> usize {
        self.handler.size()
    }

    fn ids(&self) -> Vec<ID> {
        self.handler.ids()
    }
}

#[derive(Default)]
pub struct InMemoryCorpus {
    inner: IDMAp<CorpusEntry>,
}

impl InMemoryCorpus {
    pub fn new() -> Self {
        Self {
            inner: IDMAp::default(),
        }
    }
}

impl<T> CorpusHandler<T> for InMemoryCorpus {
    fn get(&mut self, id: &ID) -> Option<CorpusEntry> {
        self.inner.get(id).cloned()
    }

    fn update(&mut self, _id: &ID, _s: T) {}

    fn insert(&mut self, entry: CorpusEntry, _s: T) {
        self.inner.insert(entry.id(), entry);
    }

    fn resize(&mut self) {}

    fn clear(&mut self) {
        self.inner.clear();
    }

    fn size(&self) -> usize {
        self.inner.len()
    }

    fn ids(&self) -> Vec<ID> {
        self.inner.keys().copied().collect()
    }
}

const MAX_DIVERSITY_WINDOW: usize = 2048;
const MIN_DIST: u32 = 5;

#[derive(Default)]
pub struct DiversityEnsurance {
    hashes: VecDeque<u64>,
    pub entries: VecDeque<ID>,
}

impl DiversityEnsurance {
    pub fn new() -> Self {
        Self {
            hashes: VecDeque::with_capacity(MAX_DIVERSITY_WINDOW),
            entries: VecDeque::with_capacity(MAX_DIVERSITY_WINDOW),
        }
    }

    pub fn try_insert(&mut self, id: ID, ast: &AST) -> bool {
        let ast_hash = simhash_stream(ast.iter());

        let is_too_similar = self
            .hashes
            .iter()
            .any(|&h| hamming_distance(h, ast_hash) < MIN_DIST);

        if !is_too_similar {
            if self.hashes.len() >= MAX_DIVERSITY_WINDOW {
                self.hashes.pop_front();
                self.entries.pop_front();
            }
            self.hashes.push_back(ast_hash);
            self.entries.push_back(id);
            true
        } else {
            false
        }
    }
}

/* Adapted from https://github.com/bartolsthoorn/simhash-rs, MIT License */
#[inline(always)]
fn hash_feature<T: Hash>(t: &T) -> u64 {
    let mut s = rustc_hash::FxHasher::default();
    t.hash(&mut s);
    s.finish()
}

/// Calculate `u64` simhash from stream of hashable words
fn simhash_stream<W, T: Hash>(words: W) -> u64
where
    W: Iterator<Item = T>,
{
    let mut v = [0i32; 64];
    let mut simhash: u64 = 0;

    for feature in words {
        let feature_hash: u64 = hash_feature(&feature);

        for (i, weight) in v.iter_mut().enumerate() {
            let bit = (feature_hash >> i) & 1;
            if bit == 1 {
                *weight = weight.saturating_add(1);
            } else {
                *weight = weight.saturating_sub(1);
            }
        }
    }

    for (i, weight) in v.iter().enumerate() {
        if *weight > 0 {
            simhash |= 1 << i;
        }
    }
    simhash
}

/// Bitwise hamming distance of two `u64` hashes
#[inline(always)]
fn hamming_distance(x: u64, y: u64) -> u32 {
    (x ^ y).count_ones()
}
