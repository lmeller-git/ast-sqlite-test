// TODO maybe change this to bitvec/bitslice later to save memory

use std::{collections::HashSet, fmt::Debug};

use lsf_core::entry::ID;

static mut TOTAL_FOUND: usize = 0;

#[derive(Debug)]
pub struct EdgeMap {
    raw_map: Vec<u8>,
}

impl EdgeMap {
    pub fn new(max_edges: usize) -> Self {
        // one byte per edge
        Self {
            raw_map: vec![0; max_edges],
        }
    }

    pub fn update<'a>(&mut self, other: EdgeMapView<'a>) -> Vec<usize> {
        debug_assert_eq!(
            self.raw_map.len(),
            other.raw_view.len(),
            "Map sizes must match"
        );

        let mut new_edges = Vec::new();

        // we can look at 8 bytes == 1 qword at a time
        let their_chunks = other.raw_view.chunks_exact(8);
        let our_chunks = self.raw_map.chunks_exact_mut(8);

        for (chunk_idx, (their_chunk, our_chunk)) in their_chunks.zip(our_chunks).enumerate() {
            let their_val = u64::from_ne_bytes(their_chunk.try_into().unwrap());

            // no edges found here
            if their_val == 0 {
                continue;
            }

            let our_val = u64::from_ne_bytes(our_chunk.try_into().unwrap());

            if (their_val & !our_val) > 0 {
                // new edge
                for i in 0..8 {
                    if their_chunk[i] > 0 && our_chunk[i] == 0 {
                        new_edges.push(chunk_idx * 8 + i);
                        our_chunk[i] = 1;
                    }
                }
            }
        }

        let chunk_len = self.raw_map.len() / 8 * 8;
        for i in chunk_len..self.raw_map.len() {
            if other.raw_view[i] > 0 && self.raw_map[i] == 0 {
                new_edges.push(i);
                self.raw_map[i] = 1;
            }
        }

        unsafe {
            TOTAL_FOUND += new_edges.len();
        }

        if !new_edges.is_empty() {
            println!(
                "Total coverage so far: {:.3}%",
                unsafe { TOTAL_FOUND } as f64 / self.raw_map.len() as f64 * 100.0
            )
        }

        new_edges
    }
}

pub struct EdgeMapView<'a> {
    raw_view: &'a [u8],
}

impl<'a> EdgeMapView<'a> {}

impl<'a> From<&'a [u8]> for EdgeMapView<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self { raw_view: value }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScoredEntry {
    pub id: ID,
    pub score: u32,
}

pub struct ScoredEdges {
    pub best_entries: Vec<Option<ScoredEntry>>,
}

impl ScoredEdges {
    pub fn new(max_edges: usize) -> Self {
        Self {
            best_entries: vec![None; max_edges],
        }
    }

    pub fn get_best_entries(&self) -> HashSet<ID> {
        self.best_entries
            .iter()
            .flatten()
            .map(|entry| entry.id)
            .collect()
    }

    pub fn update_if_best(
        &mut self,
        id: lsf_core::entry::ID,
        query_len: usize,
        exec_time_ns: u32,
        hit_edges: impl Iterator<Item = usize>,
    ) -> bool {
        let score = exec_time_ns.saturating_mul(query_len as u32);
        let mut is_best_anywhere = false;

        for edge_id in hit_edges {
            debug_assert!(edge_id < self.best_entries.len());

            let current_best = self.best_entries[edge_id];

            if current_best.is_none() || score < current_best.unwrap().score {
                self.best_entries[edge_id] = Some(ScoredEntry { id, score });
                is_best_anywhere = true;
            }
        }

        is_best_anywhere
    }
}

impl Debug for ScoredEdges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "ScoredEdges over {} edges", self.best_entries.len())
    }
}
