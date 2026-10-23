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
    Login,
    /// Waiting for a free request slot, overall or on its domain.
    Slot,
}

/// The reference engine's job positions (`job_status_str_lookup`), in its order.
pub const POSITIONS: [&str; 5] = [
    "waiting for validation",
    "waiting for bandwidth",
    "waiting for login",
    "waiting for free work slot",
    "running",
];

impl WaitReason {
    /// Where the reference's engine would hold a job at this point, as an
    /// index into [`POSITIONS`]: a new job and one whose custom header awaits
    /// approval are being validated; paused traffic waits in the bandwidth
    /// queue; the wake, domain-error, gallery-token and slot checks are the
    /// ready queue's; connection and serverside-bandwidth retries happen
    /// while the job runs.
    pub const fn position(self) -> usize {
        match self {
            Self::Engine | Self::Headers => 0,
            Self::Paused | Self::Bandwidth => 1,
            Self::Login => 2,
            Self::Wake | Self::Domain | Self::Gallery | Self::Slot => 3,
            Self::Connection | Self::ServerBandwidth | Self::Downloading => 4,
        }
    }

    /// The position shown in the current-jobs list (the reference's words).
    pub const fn label(self) -> &'static str {
        POSITIONS[self.position()]
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

/// Per-request control state, kept separate from the displayed transfer row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobControl {
    pub id: u64,
    pub created: i64,
    pub gallery: bool,
    pub domain_ok: bool,
    pub tokens_ok: bool,
    pub auto_override: bool,
    /// Its connection-error retry wait has not passed
    /// (`CurrentlyWaitingOnConnectionError`).
    #[serde(default)]
    pub waiting_on_connection_error: bool,
    /// Its serverside-bandwidth retry wait has not passed
    /// (`CurrentlyWaitingOnServersideBandwidth`).
    #[serde(default)]
    pub waiting_on_serverside_bandwidth: bool,
    /// Seconds each of the job's contexts (in the job's order) would make it
    /// wait for bandwidth now (`GetWaitingEstimateAndContext` of each alone).
    #[serde(default)]
    pub context_waits: Vec<u64>,
}
/// A recent failed request, retained even when a short request leaves the live list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobError {
    pub id: u64,
    pub url: String,
    pub contexts: Vec<NetworkContext>,
    pub gallery: bool,
    pub text: String,
}

/// One engine-owned domain login, retained across its individual HTTP steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginProcess {
    pub id: u64,
    pub domain: String,
    pub script: String,
    pub epoch: String,
    pub status: String,
}

/// Current daemon snapshot. It expires after five seconds without a heartbeat.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub epoch: String,
    pub at: i64,
    pub jobs: Vec<NetworkJob>,
    pub usage: Vec<(NetworkContext, Tracker)>,
    #[serde(default)]
    pub controls: Vec<JobControl>,
    #[serde(default)]
    pub errors: Vec<JobError>,
    #[serde(default)]
    pub login: Option<LoginProcess>,
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
    /// Cancel the global login process, independently of a triggering downloader.
    CancelLogin,
    OverrideBandwidth,
    OverrideConnectionWait,
    OverrideServerBandwidthWait,
    OverrideGalleryWait,
    ScrubDomainErrors,
    /// Applies to this request only; the GUI control owns future-request policy.
    AutoOverrideBandwidth(bool),
    /// Independent control owners may watch the same request.
    AutoOverrideBandwidthFor {
        owner: u64,
        enabled: bool,
    },
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
