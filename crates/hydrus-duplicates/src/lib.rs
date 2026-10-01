//! Duplicates auto-resolution: rules that search the potential duplicate
//! pairs, decide which file of a pair is better by comparing them, and act
//! on the pair or queue it for a human, as the reference's auto-resolution
//! does. The rules and their pair queues are stored by
//! `hydrus_store::duplicates::auto`; this crate runs them. It also makes
//! the duplicate filter's comparison statements ([`statements`]).

pub mod content;
pub mod engine;
pub mod potentials;
pub mod selector;
pub mod statements;

pub use engine::{NoShuffle, Orientation, Shuffle, WorkDone, approve, deny, work_rules};
