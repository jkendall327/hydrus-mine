//! Network data review and detached bandwidth drafts. Live snapshots are
//! consumed when fresh; saved usage remains available while the daemon is off.

use hydrus_core::{
    bandwidth::{BandwidthType, Manager, Rule, Rules, Tracker, default_rules},
    network::{CONTEXT_GLOBAL, NetworkContext},
    numbers::{human_bytes, human_int},
};
use hydrus_store::{Store, bandwidth::BandwidthSettings, network_runtime::Snapshot, settings};

/// The reference's confirmation for reverting one context.
pub const REVERT_QUESTION: &str =
    "Are you sure you want to revert to using the default rules for this context?";
/// The reference's confirmation for resetting the built-in defaults.
pub const RESET_QUESTION: &str = "Reset your 'default' and 'global' bandwidth rules to default?";

/// A complete read, performed off the UI thread.
#[derive(Debug, Clone)]
pub struct Review {
    pub settings: BandwidthSettings,
    pub runtime: Snapshot,
    pub usage: Vec<(NetworkContext, Tracker)>,
    pub live: bool,
}

/// Numeric columns sort by their actual counts, rather than formatted units.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SortKey {
    Text(String),
    Counts(u64, u64),
}
impl Review {
    /// Reference bandwidth sort values for one visible column.
    pub fn sort_key(
        &self,
        context: &NetworkContext,
        column: usize,
        history: Option<u64>,
        now: i64,
    ) -> SortKey {
        let mut tracker = self.tracker(context, now);
        let counts = |tracker: &mut Tracker, span| {
            SortKey::Counts(
                tracker.usage(BandwidthType::Data, span, now),
                tracker.usage(BandwidthType::Requests, span, now),
            )
        };
        match column {
            1 => SortKey::Text(context_kind(context.kind).into()),
            2 => SortKey::Counts(
                if self.live {
                    tracker.usage(BandwidthType::Data, Some(1), now)
                } else {
                    0
                },
                0,
            ),
            3 => counts(&mut tracker, Some(86_400)),
            4 => history.map_or_else(
                || {
                    SortKey::Counts(
                        tracker.all_usage(BandwidthType::Data),
                        tracker.all_usage(BandwidthType::Requests),
                    )
                },
                |span| {
                    let mut tracker = tracker.clone();
                    counts(&mut tracker, Some(span))
                },
            ),
            5 => counts(&mut tracker, None),
            6 => SortKey::Counts(u64::from(!self.inherits(context)), 0),
            7 => SortKey::Counts(self.rules(context).waiting_estimate(&mut tracker, now), 0),
            _ => SortKey::Text(context.to_human_string()),
        }
    }
    /// Current usage and rules, falling back to durable usage when offline.
    pub fn load(store: &Store, now: i64) -> Result<Self, String> {
        store
            .read(|conn| {
                let settings = settings::get::<BandwidthSettings>(conn)?;
                let runtime = settings::get::<Snapshot>(conn)?;
                let live = runtime.fresh(now);
                let usage = if live {
                    runtime.usage.clone()
                } else {
                    hydrus_store::bandwidth::usage(conn, now)?
                };
                Ok(Self {
                    settings,
                    runtime,
                    usage,
                    live,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// Known usage contexts, global usage, and contexts with explicit rules.
    pub fn contexts(&self) -> Vec<NetworkContext> {
        let mut contexts = vec![NetworkContext::global()];
        contexts.extend(self.usage.iter().map(|(c, _)| c.clone()));
        contexts.extend(
            self.settings
                .rules
                .iter()
                .filter(|(c, _)| !c.is_default())
                .map(|(c, _)| c.clone()),
        );
        contexts.sort();
        contexts.dedup();
        contexts
    }

    /// Visible contexts follow the reference's request-age and explicit-rule filters.
    /// All-time includes every retained non-ephemeral context and every specific rule.
    pub fn filtered_contexts(
        &self,
        history: Option<u64>,
        include_rules: bool,
        now: i64,
    ) -> Vec<NetworkContext> {
        let mut contexts = Vec::new();
        if include_rules || history.is_none() {
            contexts.extend(
                self.settings
                    .rules
                    .iter()
                    .filter(|(c, _)| !c.is_default() && c.kind != CONTEXT_GLOBAL)
                    .map(|(c, _)| c.clone()),
            );
        }
        for (context, tracker) in &self.usage {
            if context.is_default() || context.is_ephemeral() {
                continue;
            }
            let mut tracker = tracker.clone();
            if context.kind == CONTEXT_GLOBAL
                || history.is_none()
                || tracker.usage(BandwidthType::Requests, history, now) > 0
            {
                contexts.push(context.clone());
            }
        }
        contexts.sort();
        contexts.dedup();
        contexts
    }

    /// The inherited or specific rules that apply to this context.
    pub fn rules(&self, context: &NetworkContext) -> Rules {
        Manager::new(self.settings.rules.clone())
            .rules_for(context)
            .clone()
    }

    /// Whether a specific context inherits its kind's rules.
    pub fn inherits(&self, context: &NetworkContext) -> bool {
        !context.is_default()
            && context.kind != CONTEXT_GLOBAL
            && !self.settings.rules.iter().any(|(c, _)| c == context)
    }

    /// Empty usage for a context that has not downloaded yet.
    pub fn tracker(&self, context: &NetworkContext, now: i64) -> Tracker {
        self.usage
            .iter()
            .find(|(c, _)| c == context)
            .map_or_else(|| Tracker::new(now), |(_, t)| t.clone())
    }

    /// Native summary matching the Qt review columns (history span is explicit).
    pub fn row(&self, context: &NetworkContext, history: Option<u64>, now: i64) -> [String; 8] {
        let mut tracker = self.tracker(context, now);
        let speed = if self.live {
            tracker.usage(BandwidthType::Data, Some(1), now)
        } else {
            0
        };
        let day = usage_text(&mut tracker, Some(86400), now);
        let month = usage_text(&mut tracker, None, now);
        let search = match history {
            Some(span) => usage_text(&mut tracker, Some(span), now),
            None => all_usage_text(&tracker),
        };
        let wait = self.rules(context).waiting_estimate(&mut tracker, now);
        [
            context.to_human_string(),
            context_kind(context.kind).into(),
            if speed == 0 {
                String::new()
            } else {
                format!("{}/s", human_bytes(speed))
            },
            day,
            search,
            month,
            if context.kind == CONTEXT_GLOBAL {
                "n/a".into()
            } else if self.inherits(context) {
                String::new()
            } else {
                "yes".into()
            },
            if wait == 0 {
                String::new()
            } else {
                duration(wait)
            },
        ]
    }
}

/// The last review age, preserved when reopening the bandwidth browser.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BandwidthReviewPreferences {
    pub history: Option<u64>,
}
impl Default for BandwidthReviewPreferences {
    fn default() -> Self {
        Self {
            history: Some(604_800),
        }
    }
}
impl hydrus_store::settings::Setting for BandwidthReviewPreferences {
    const KEY: &'static str = "bandwidth_review_preferences";
}

/// One UTC calendar-month bar; raw bytes drive the chart independently of formatting.
#[derive(Debug, Clone, PartialEq)]
pub struct MonthlyBar {
    pub month: String,
    pub bytes: u64,
    pub fraction: f32,
}

/// Monthly data totals in chronological order, as the reference tracker supplies them.
pub fn monthly_history(tracker: &Tracker) -> Vec<MonthlyBar> {
    let months = tracker.to_counters()[0].clone();
    let maximum = months
        .iter()
        .map(|(_, bytes)| *bytes)
        .max()
        .unwrap_or(0)
        .max(1);
    months
        .into_iter()
        .filter_map(|(timestamp, bytes)| {
            let date = jiff::Timestamp::from_second(timestamp)
                .ok()?
                .to_zoned(jiff::tz::TimeZone::UTC);
            Some(MonthlyBar {
                month: format!("{:04}-{:02}", date.year(), date.month()),
                bytes,
                fraction: (bytes as f64 / maximum as f64 / 1.2) as f32,
            })
        })
        .collect()
}

/// The confirmation shown before deleting the selected contexts' bandwidth history.
pub const DELETE_HISTORY_QUESTION: &str =
    "Are you sure? This will delete all bandwidth record for the selected network contexts.";

/// The reference's context type names.
pub const fn context_kind(kind: i64) -> &'static str {
    match kind {
        0 => "global",
        1 => "hydrus service",
        2 => "web domain",
        4 => "downloader page",
        5 => "subscription",
        6 => "watcher page",
        _ => "unknown",
    }
}

/// A short duration for usage periods and waits.
pub fn duration(seconds: u64) -> String {
    hydrus_core::time::pretty_time_delta(i64::try_from(seconds).unwrap_or(i64::MAX), false)
}

/// The bandwidth editor's two columns.
pub fn rule_row(rule: Rule) -> [String; 2] {
    [
        match rule.kind {
            BandwidthType::Data => human_bytes(rule.max_allowed),
            BandwidthType::Requests => format!("{} requests", human_int(rule.max_allowed)),
        },
        rule.time_delta.map_or_else(|| "per month".into(), duration),
    ]
}

/// Data and requests over a rolling period, or this calendar month.
pub fn usage_text(tracker: &mut Tracker, span: Option<u64>, now: i64) -> String {
    format!(
        "{} in {} requests",
        human_bytes(tracker.usage(BandwidthType::Data, span, now)),
        human_int(tracker.usage(BandwidthType::Requests, span, now))
    )
}

/// All retained monthly totals.
pub fn all_usage_text(tracker: &Tracker) -> String {
    format!(
        "{} in {} requests",
        human_bytes(tracker.all_usage(BandwidthType::Data)),
        human_int(tracker.all_usage(BandwidthType::Requests))
    )
}

/// Parse numeric values at the edit boundary; zero periods/limits are invalid.
pub fn parse_rule(
    requests: bool,
    amount: &str,
    seconds: &str,
    monthly: bool,
) -> Result<Rule, String> {
    let amount = amount
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or("Enter a positive limit.")?;
    if requests && amount > 1_048_576 {
        return Err("Request limits must be at most 1,048,576.".into());
    }
    let span = if monthly {
        None
    } else {
        Some(
            seconds
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|n| *n > 0 && i64::try_from(*n).is_ok())
                .ok_or("Enter a positive period in seconds.")?,
        )
    };
    Ok(Rule::new(
        if requests {
            BandwidthType::Requests
        } else {
            BandwidthType::Data
        },
        span,
        amount,
    ))
}

/// A detached rules editor; Cancel drops it without writes.
#[derive(Debug, Clone)]
pub struct RulesDraft {
    pub context: NetworkContext,
    original: Option<Rules>,
    inherited: Rules,
    pub rules: Vec<Rule>,
}
impl RulesDraft {
    /// Begin editing this context, using inherited rules as the initial draft.
    pub fn new(review: &Review, context: NetworkContext) -> Self {
        let original = review
            .settings
            .rules
            .iter()
            .find(|(c, _)| c == &context)
            .map(|(_, r)| r.clone());
        let inherited = review.rules(&NetworkContext::default_of_kind(context.kind));
        let rules = review.rules(&context).rules().to_vec();
        Self {
            context,
            original,
            inherited,
            rules,
        }
    }

    /// Apply only this context, preserving unrelated settings and rejecting a
    /// concurrently changed context/default rather than overwriting it.
    pub fn apply(&self, store: &Store, revert: bool) -> Result<(), String> {
        if revert && (self.context.is_default() || self.context.kind == CONTEXT_GLOBAL) {
            return Err("Global and default rules cannot inherit rules.".into());
        }
        let draft = self.clone();
        store
            .write(move |ctx| {
                let mut current = settings::get::<BandwidthSettings>(ctx.conn())?;
                let original = current
                    .rules
                    .iter()
                    .find(|(c, _)| c == &draft.context)
                    .map(|(_, r)| r.clone());
                let defaults = Manager::new(current.rules.clone())
                    .rules_for(&NetworkContext::default_of_kind(draft.context.kind))
                    .clone();
                if original != draft.original || (original.is_none() && defaults != draft.inherited)
                {
                    return Ok(Err(
                        "These rules changed in another editor. Reopen the rules editor.".into(),
                    ));
                }
                current.rules.retain(|(c, _)| c != &draft.context);
                let rules = Rules::new(draft.rules);
                // Reference SetRules removes an empty specific context.
                if !revert
                    && (!rules.is_empty()
                        || draft.context.is_default()
                        || draft.context.kind == CONTEXT_GLOBAL)
                {
                    current.rules.push((draft.context, rules));
                }
                settings::set(ctx.conn(), &current)?;
                Ok(Ok(()))
            })
            .map_err(|e| e.to_string())?
    }
}

/// Restore only built-in defaults/global rules, preserving specific overrides.
pub fn reset_defaults(store: &Store) -> Result<(), String> {
    store
        .write(|ctx| {
            let mut current = settings::get::<BandwidthSettings>(ctx.conn())?;
            current
                .rules
                .retain(|(c, _)| !c.is_default() && c.kind != CONTEXT_GLOBAL);
            current.rules.extend(default_rules());
            settings::set(ctx.conn(), &current)
        })
        .map_err(|e| e.to_string())
}

/// The current-job review columns, with explicit native wait reasons.
pub fn job_row(job: &hydrus_store::network_runtime::NetworkJob) -> [String; 5] {
    [
        job.wait.label().into(),
        job.url.clone(),
        job.status.clone(),
        format!("{}/s", human_bytes(job.speed)),
        job.bytes_total.map_or_else(
            || human_bytes(job.bytes_read),
            |total| format!("{}/{}", human_bytes(job.bytes_read), human_bytes(total)),
        ),
    ]
}

/// Typed transfer counts for sorting the jobs' speed and progress columns.
pub fn job_sort_key(job: &hydrus_store::network_runtime::NetworkJob, column: usize) -> SortKey {
    match column {
        1 => SortKey::Text(job.url.clone()),
        2 => SortKey::Text(job.status.clone()),
        3 => SortKey::Counts(job.speed, 0),
        4 => SortKey::Counts(job.bytes_read, job.bytes_total.unwrap_or(0)),
        _ => SortKey::Text(job.wait.label().into()),
    }
}
