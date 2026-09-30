//! Duplicates auto-resolution: rules that search the potential duplicate
//! pairs, decide which file of a pair is better by comparing them, and act
//! on the pair or queue it for a human, as the reference's auto-resolution
//! does. The rules and their pair queues are stored by
//! `hydrus_store::duplicates::auto`; this crate runs them.

pub mod engine;
pub mod selector;

pub use engine::{NoShuffle, Orientation, Shuffle, WorkDone, approve, deny, work_rules};
