//! The daemon's current network requests and usage, with epoch-scoped commands.
//!
//! This local IPC is independent of the optional Client API listener. The daemon
//! owns the snapshot; commands are appended and drained in writer transactions.

use hydrus_core::{bandwidth::Tracker, network::NetworkContext};
use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    settings::{self, Setting},
};

/// What a request is waiting for, or its active transfer phase.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitReason {
    #[default]
    Engine,
    Paused,
    Headers,
    Wake,
    Bandwidth,
    Domain,
    Gallery,
    Connection,
    ServerBandwidth,
    Downloading,
}

impl WaitReason {
    /// The phase shown in the current-jobs list.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Engine => "waiting for engine",
            Self::Paused => "network paused",
            Self::Headers => "header approval",
            Self::Wake => "computer waking",
            Self::Bandwidth => "bandwidth",
            Self::Domain => "domain errors",
            Self::Gallery => "gallery token",
            Self::Connection => "connection retry",
            Self::ServerBandwidth => "server bandwidth",
            Self::Downloading => "downloading",
        }
    }
}

/// A stable identifier within one daemon epoch, with transfer/debug details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkJob {
    pub id: u64,
    pub url: String,
    pub status: String,
    pub wait: WaitReason,
    pub bytes_read: u64,
    pub bytes_total: Option<u64>,
    pub speed: u64,
    pub contexts: Vec<NetworkContext>,
    pub obeys_bandwidth: bool,
}

/// Current daemon snapshot. It expires after five seconds without a heartbeat.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub epoch: String,
    pub at: i64,
    pub jobs: Vec<NetworkJob>,
    pub usage: Vec<(NetworkContext, Tracker)>,
}
impl Setting for Snapshot {
    const KEY: &'static str = "network_runtime";
}
impl Snapshot {
    /// Whether this snapshot describes a currently running daemon.
    pub fn fresh(&self, now: i64) -> bool {
        !self.epoch.is_empty() && (0..=5).contains(&now.saturating_sub(self.at))
    }
}

/// A current-job action sent to the daemon that owns the identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobAction {
    Cancel,
    OverrideBandwidth,
}

/// Commands cannot affect a replacement daemon or a later request on one job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub epoch: String,
    pub job: u64,
    pub action: JobAction,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Pending(Vec<Command>);
impl Setting for Pending {
    const KEY: &'static str = "network_runtime_commands";
}

/// Append a command atomically within the store's writer transaction.
pub fn send(conn: &rusqlite::Connection, command: Command) -> Result<()> {
    let mut pending = settings::get::<Pending>(conn)?;
    if !pending.0.contains(&command) {
        pending.0.push(command);
    }
    settings::set(conn, &pending)
}

/// Consume pending commands once, within the store's writer transaction.
pub fn take_commands(conn: &rusqlite::Connection) -> Result<Vec<Command>> {
    let pending = settings::get::<Pending>(conn)?;
    if !pending.0.is_empty() {
        settings::set(conn, &Pending::default())?;
    }
    Ok(pending.0)
}
