use lsf_core::entry::{CorpusEntry, ID};

pub trait CorpusHandler<T>: Sync + Send {
    fn get(&mut self, id: &ID) -> Option<CorpusEntry>;
    fn update(&mut self, id: &ID, s: T);
    fn insert(&mut self, entry: CorpusEntry, s: T);
    fn remove(&mut self, id: &ID);
    fn resize(&mut self);
    fn ids(&self) -> rustc_hash::FxHashSet<ID>;
    fn protected_ids(&self) -> rustc_hash::FxHashSet<ID>;
    fn clear(&mut self);
    fn size(&self) -> usize;
}
