use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use lsf_core::{
    ast::AST,
    entry::{CorpusEntry, ID},
};
use lsf_cov::bitmap::{EdgeMap, ScoredEdges};

#[derive(Debug)]
pub struct Corpus {
    pub entries: HashMap<ID, CorpusEntry>,
    pub edge_map: EdgeMap,
    pub entry_rating: ScoredEdges,
    pub diversity: DiversityEnsurance,
}

impl Corpus {
    pub fn new(max_edges: usize) -> Self {
        Self {
            entries: HashMap::default(),
            edge_map: EdgeMap::new(max_edges),
            entry_rating: ScoredEdges::new(max_edges),
            diversity: DiversityEnsurance::new(),
        }
    }
}

const MIN_DIST: u32 = 15;

#[derive(Debug, Default)]
pub struct DiversityEnsurance {
    hashes: Vec<u64>,
    pub entries: Vec<ID>,
}

impl DiversityEnsurance {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_insert(&mut self, id: ID, ast: &AST) -> bool {
        let str = ast.iter().map(ToString::to_string).collect::<Vec<_>>();
        let ast_hash = simhash_stream(str.iter().map(|s| s.as_str()));
        if !self
            .hashes
            .iter()
            .any(|hash| hamming_distance(*hash, ast_hash) < MIN_DIST)
        {
            self.hashes.push(ast_hash);
            self.entries.push(id);
            true
        } else {
            false
        }
    }
}

/* Adapted from https://github.com/bartolsthoorn/simhash-rs, MIT License */

use siphasher::sip::SipHasher;

fn hash_feature<T: Hash>(t: &T) -> u64 {
    let mut s = SipHasher::default();
    t.hash(&mut s);
    s.finish()
}

/// Calculate `u64` simhash from stream of `&str` words
fn simhash_stream<'w, W>(words: W) -> u64
where
    W: Iterator<Item = &'w str>,
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

// /// Calculate `u64` simhash from `&str` split by whitespace
// fn simhash(text: &str) -> u64 {
//     simhash_stream(text.split_whitespace())
// }

/// Bitwise hamming distance of two `u64` hashes
fn hamming_distance(x: u64, y: u64) -> u32 {
    (x ^ y).count_ones()
}

// /// Calculate similarity as `f64` of two hashes
// /// 0.0 means no similarity, 1.0 means identical
// fn hash_similarity(hash1: u64, hash2: u64) -> f64 {
//     let distance: f64 = hamming_distance(hash1, hash2) as f64;
//     1.0 - (distance / 64.0)
// }
