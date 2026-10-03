//! The downloader's HTTP engine.
//!
//! A request goes out as the reference's network jobs send it: with the
//! custom headers of every network context it falls under (everything, its
//! domain and parent domains, the downloader it is for), its URL class's
//! header overrides and referral URL, and the cookies of its domain's
//! session, which also keeps every cookie the responses set (redirects
//! included). Failures are retried as the reference retries them: busy
//! servers after a growing wait (or as long as they ask), broken
//! connections after a wait that grows with each attempt, file downloads
//! resumed with ranged requests.

pub mod cookies;
mod engine;
mod error;
pub mod text;

pub use engine::{BandwidthScope, Job, JobState, Method, NetEngine, NetOptions, Request, Response};
pub use error::{NetError, StatusKind};
