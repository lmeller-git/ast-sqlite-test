pub mod ast;
pub mod entry;

pub type IDMAp<V> = rustc_hash::FxHashMap<crate::entry::ID, V>;
