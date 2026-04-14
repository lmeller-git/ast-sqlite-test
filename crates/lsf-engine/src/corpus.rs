use std::collections::HashMap;

use lsf_core::entry::{CorpusEntry, ID};
use lsf_cov::bitmap::EdgeMap;

#[derive(Debug)]
pub struct Corpus {
    pub entries: HashMap<ID, CorpusEntry>,
    pub edge_map: EdgeMap,
}

impl Corpus {
    pub fn new(max_edges: usize) -> Self {
        Self {
            entries: HashMap::default(),
            edge_map: EdgeMap::new(max_edges),
        }
    }
}
