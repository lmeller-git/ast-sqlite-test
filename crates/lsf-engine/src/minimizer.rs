use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
};

use lsf_core::{
    IDMAp,
    ast::AST,
    entry::{ID, Meta, RawEntry},
};
use lsf_cov::bitmap::ScoredEdges;
use lsf_feedback::{TestOutcome, TestableEntry};

use crate::{CorpusHandler, Schedule};

pub trait CorpusMinimizer<T>: Send + Sync {
    fn minimize(&mut self, corpus: &mut dyn CorpusHandler<T>, scheduler: &mut dyn Schedule);
    fn on_add(
        &mut self,
        entry: &TestableEntry<RawEntry>,
        meta: &Meta,
        edges_found: Vec<usize>,
    ) -> TestOutcome;
    fn reset(&mut self);
}

const CULL_AFTER: u8 = 3;

pub struct GreedyCoverage {
    best_edges: ScoredEdges,
    diversity: DiversityEscape,
    gc_rotation: IDMAp<u8>,
}

impl GreedyCoverage {
    pub fn new(max_edges: usize) -> Self {
        Self {
            best_edges: ScoredEdges::new(max_edges),
            diversity: DiversityEscape::new(),
            gc_rotation: IDMAp::default(),
        }
    }
}

impl<T> CorpusMinimizer<T> for GreedyCoverage {
    fn minimize(&mut self, corpus: &mut dyn CorpusHandler<T>, scheduler: &mut dyn Schedule) {
        let mut best_entries = self.best_edges.get_best_entries();
        best_entries.extend(corpus.protected_ids());

        let ids = corpus.ids();
        let bad_entries = ids.difference(&best_entries);

        for id in bad_entries.into_iter() {
            let entry = self
                .gc_rotation
                .entry(*id)
                .and_modify(|n| *n += 1)
                .or_default();
            if *entry >= CULL_AFTER {
                self.gc_rotation.remove(id);
                corpus.remove(id);
                scheduler.on_remove(*id);
            }
        }
    }

    fn on_add(
        &mut self,
        entry: &TestableEntry<RawEntry>,
        meta: &Meta,
        edges_found: Vec<usize>,
    ) -> TestOutcome {
        let is_diverse = self.diversity.try_insert(entry.id(), entry.ast());
        if !is_diverse && meta.new_cov_nodes == 0 {
            return TestOutcome::Rejected(lsf_feedback::RejectionReason::Bad);
        }
        if meta.new_cov_nodes > 0 {
            let is_best = self.best_edges.update_if_best(
                entry.id(),
                entry.ast().len(),
                edges_found.len(),
                meta.exec_time,
                edges_found.into_iter(),
            );
            if !is_best && !is_diverse {
                return TestOutcome::Rejected(lsf_feedback::RejectionReason::Bad);
            }
            TestOutcome::Accepted(lsf_feedback::AcceptanceReason::CovIncrease(
                meta.new_cov_nodes,
            ))
        } else {
            TestOutcome::Accepted(lsf_feedback::AcceptanceReason::IsDiverse)
        }
    }

    fn reset(&mut self) {
        self.best_edges
            .best_entries
            .iter_mut()
            .for_each(|item| *item = None);
        self.diversity.entries.clear();
        self.diversity.hashes.clear();
    }
}

const MAX_DIVERSITY_WINDOW: usize = 1024;
const MIN_DIST: u32 = 20;

#[derive(Default)]
pub struct DiversityEscape {
    hashes: VecDeque<u64>,
    pub entries: VecDeque<ID>,
}

impl DiversityEscape {
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
