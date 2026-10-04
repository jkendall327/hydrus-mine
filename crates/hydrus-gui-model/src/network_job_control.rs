//! The reference's per-control cog and retained error state, independent of widgets.
use crate::network_data::{Review, duration};
use hydrus_core::network::NetworkContext;
use hydrus_store::network_runtime::{
    Command, JobAction, JobControl, NetworkJob, Snapshot, WaitReason,
};

pub const AUTO_LABEL: &str = "auto-override bandwidth rules for all jobs here after five seconds";
pub const CONNECTION_LABEL: &str = "reattempt connection now";
pub const SERVER_LABEL: &str = "reattempt request now (server reports low bandwidth)";
pub const DOMAIN_LABEL: &str = "scrub domain errors";
pub const BANDWIDTH_LABEL: &str = "override bandwidth rules for this job";
pub const GALLERY_LABEL: &str = "override forced gallery wait times for this job";

/// The downloader control's owner, independent of URL (two jobs can share a URL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    pub queue: i64,
    pub gallery: bool,
}
impl Target {
    pub fn matches(self, contexts: &[NetworkContext], gallery: bool) -> bool {
        gallery == self.gallery
            && contexts
                .iter()
                .any(|c| c.is_ephemeral() && c.data == format!("{:016x}", self.queue))
    }
    pub fn job(self, snapshot: &Snapshot, now: i64) -> Option<(&NetworkJob, &JobControl)> {
        if !snapshot.fresh(now) {
            return None;
        }
        snapshot.jobs.iter().find_map(|job| {
            let meta = snapshot.controls.iter().find(|c| c.id == job.id)?;
            self.matches(&job.contexts, meta.gallery)
                .then_some((job, meta))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    CopyUrl(String),
    Rules(NetworkContext),
    Job(JobAction),
    AutoOverride,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub action: Action,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cog {
    pub url: Option<Entry>,
    pub rules: Vec<Entry>,
    pub actions: Vec<Entry>,
    pub auto_override: bool,
}

/// Default contexts occur once; ephemeral instances never get an edit entry.
pub fn cog(
    review: &Review,
    job: Option<(&NetworkJob, &JobControl)>,
    auto_override: bool,
    now: i64,
) -> Cog {
    let mut menu = Cog {
        auto_override,
        ..Cog::default()
    };
    let Some((job, meta)) = job else {
        return menu;
    };
    menu.url = Some(Entry {
        label: hydrus_core::url::pyurl::unquote(&job.url),
        action: Action::CopyUrl(job.url.clone()),
    });
    let mut defaults = Vec::new();
    for context in &job.contexts {
        let inherited = review.inherits(context);
        let mut contexts = Vec::new();
        if inherited {
            let default = NetworkContext::default_of_kind(context.kind);
            if !defaults.contains(&default) {
                defaults.push(default.clone());
                contexts.push((false, default));
            }
        }
        if !context.is_ephemeral() {
            contexts.push((inherited, context.clone()));
        }
        for (inherited, edit) in contexts {
            let mut label = edit.to_human_string();
            if inherited {
                label = format!("set rules for {label}");
            } else {
                let wait = review
                    .rules(context)
                    .waiting_estimate(&mut review.tracker(context, now), now);
                if wait > 0 {
                    label = format!("{label} ({})", duration(wait));
                }
            }
            menu.rules.push(Entry {
                label,
                action: Action::Rules(edit),
            });
        }
    }
    for (include, label, action) in [
        (
            job.wait == WaitReason::Connection,
            CONNECTION_LABEL,
            JobAction::OverrideConnectionWait,
        ),
        (!meta.domain_ok, DOMAIN_LABEL, JobAction::ScrubDomainErrors),
        (
            job.wait == WaitReason::ServerBandwidth,
            SERVER_LABEL,
            JobAction::OverrideServerBandwidthWait,
        ),
        (
            job.obeys_bandwidth,
            BANDWIDTH_LABEL,
            JobAction::OverrideBandwidth,
        ),
        (
            !meta.tokens_ok,
            GALLERY_LABEL,
            JobAction::OverrideGalleryWait,
        ),
    ] {
        if include {
            menu.actions.push(Entry {
                label: label.into(),
                action: Action::Job(action),
            });
        }
    }
    menu
}

/// An owner clears errors explicitly; removing a job does not clear its last error.
#[derive(Debug, Clone, Default)]
pub struct Control {
    pub auto_override: bool,
    error: Option<String>,
    seen_error: Option<(String, u64)>,
    sent_auto: Option<(String, u64, bool)>,
}
impl Control {
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn set_error(&mut self, text: String) {
        self.error = Some(text);
    }
    pub fn clear_error(&mut self) {
        self.error = None;
    }
    /// Copy a newly completed failure once, retaining it after the live job leaves.
    pub fn sync(&mut self, target: Target, snapshot: &Snapshot, now: i64) -> Option<Command> {
        if !snapshot.fresh(now) {
            return None;
        }
        if let Some(error) = snapshot
            .errors
            .iter()
            .rev()
            .find(|e| target.matches(&e.contexts, e.gallery))
        {
            let token = (snapshot.epoch.clone(), error.id);
            if self.seen_error.as_ref() != Some(&token) {
                self.error = Some(error.text.clone());
                self.seen_error = Some(token);
            }
        }
        let (job, _) = target.job(snapshot, now)?;
        let token = (snapshot.epoch.clone(), job.id, self.auto_override);
        if self.sent_auto.as_ref() == Some(&token) {
            return None;
        }
        self.sent_auto = Some(token);
        Some(Command {
            epoch: snapshot.epoch.clone(),
            job: job.id,
            action: JobAction::AutoOverrideBandwidth(self.auto_override),
        })
    }
    pub fn flip_auto_override(&mut self) {
        self.auto_override = !self.auto_override;
    }
}
