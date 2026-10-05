//! The manage subscriptions dialog's two lists, as the reference writes
//! them: the subscriptions (`EditSubscriptionsPanel.
//! _ConvertSubscriptionToDisplayTuple`) and, editing one, its queries
//! (`EditSubscriptionPanel._ConvertQueryHeaderToDisplayTuple`). Recorded by
//! `oracle/record_subscriptions_list.py`.

use hydrus_core::numbers::human_int;
use hydrus_core::time::pretty_time_delta;
use hydrus_store::queues::{StatusCounts, file_log_short_status};

/// The subscriptions list's columns (`COLUMN_LIST_SUBSCRIPTIONS`).
pub const SUBSCRIPTION_COLUMNS: [&str; 9] = [
    "name",
    "source",
    "status",
    "last new file time",
    "last checked",
    "error/delay?",
    "items",
    "paused",
    "custom import options",
];

/// A subscription's queries list's columns
/// (`COLUMN_LIST_SUBSCRIPTION_QUERIES`).
pub const QUERY_COLUMNS: [&str; 10] = [
    "name/query",
    "paused",
    "status",
    "last new file time",
    "last check time",
    "next check time",
    "file velocity",
    "recent delays",
    "items",
    "additional tags",
];

/// What a query's row says of it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QueryFacts {
    pub query_text: String,
    pub display_name: Option<String>,
    pub paused: bool,
    pub dead: bool,
    pub check_now: bool,
    /// Seconds; 0 before its first check.
    pub last_check_time: i64,
    pub next_check_time: i64,
    /// Its file log's seeds by status, and when its latest was found (0
    /// for none).
    pub files: StatusCounts,
    pub latest_added: i64,
    /// How fast it finds files ("3 files in previous 1 day",
    /// `GetPrettyCurrentVelocity(no_prefix = True)`).
    pub velocity: String,
    /// What its tag import options add, if anything.
    pub additional_tags: String,
}

/// What a subscription's row says of it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubscriptionFacts {
    pub name: String,
    /// Its downloader's name ("" for none).
    pub gug_name: String,
    pub paused: bool,
    /// No work until this time (seconds), and why.
    pub no_work_until: i64,
    pub no_work_until_reason: String,
    pub queries: Vec<QueryFacts>,
    /// Its own import options, summarised ("" if it has none).
    pub import_options: String,
}

/// Which of a file log's counts its short status shows, as the client's
/// options say (`show_new_on_file_seed_short_summary`,
/// `show_deleted_on_file_seed_short_summary`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShortSummary {
    pub new: bool,
    pub deleted: bool,
}

/// `TimestampToPrettyTimeDelta` with no "now" leeway
/// (`just_now_threshold = 0`).
pub(crate) fn delta_exact(timestamp: i64, now: i64) -> String {
    if timestamp == now {
        return "now".into();
    }
    let span = pretty_time_delta((timestamp - now).abs(), false);
    if now > timestamp {
        format!("{span} ago")
    } else {
        format!("in {span}")
    }
}

pub(crate) fn delta_exact_with_format(
    timestamp: i64,
    now: i64,
    formatting: &hydrus_store::settings::GuiFormatting,
) -> String {
    if formatting.iso {
        crate::gui_format::timestamp(formatting, Some(timestamp), now)
    } else {
        delta_exact(timestamp, now)
    }
}

/// A time as the lists write it: "n/a" for none, else how long ago.
fn ago_or_na(
    timestamp: i64,
    now: i64,
    formatting: &hydrus_store::settings::GuiFormatting,
) -> String {
    if timestamp == 0 {
        "n/a".into()
    } else {
        crate::gui_format::timestamp(formatting, Some(timestamp), now)
    }
}

/// A query's name: its display name with its text, or its text
/// (`GetFullHumanName`).
pub fn full_human_name(query_text: &str, display_name: Option<&str>) -> String {
    match display_name {
        Some(name) => format!("{name} ({query_text})"),
        None => query_text.to_owned(),
    }
}

/// When a query checks next (`GetNextCheckStatusString`).
pub fn next_check_status(query: &QueryFacts, now: i64) -> String {
    next_check_status_with_format(
        query,
        now,
        &hydrus_store::settings::GuiFormatting::default(),
    )
}
pub fn next_check_status_with_format(
    query: &QueryFacts,
    now: i64,
    formatting: &hydrus_store::settings::GuiFormatting,
) -> String {
    if query.check_now {
        return "checking on dialog ok".into();
    }
    if query.dead {
        return "dead, so not checking".into();
    }
    // (`TimeHasPassed`: strictly after)
    let next = if now > query.next_check_time {
        "imminent".to_owned()
    } else {
        crate::gui_format::timestamp(formatting, Some(query.next_check_time), now)
    };
    if query.paused {
        format!("paused, but would be {next}")
    } else {
        next
    }
}

/// A query's row in its subscription's list.
pub fn query_row(query: &QueryFacts, now: i64, short: ShortSummary) -> Vec<String> {
    query_row_with_format(
        query,
        now,
        short,
        &hydrus_store::settings::GuiFormatting::default(),
    )
}
pub fn query_row_with_format(
    query: &QueryFacts,
    now: i64,
    short: ShortSummary,
    formatting: &hydrus_store::settings::GuiFormatting,
) -> Vec<String> {
    vec![
        full_human_name(&query.query_text, query.display_name.as_deref()),
        if query.paused { "yes" } else { "" }.into(),
        if query.dead { "dead" } else { "ok" }.into(),
        ago_or_na(query.latest_added, now, formatting),
        if query.last_check_time == 0 {
            "(initial check has not yet occurred)".into()
        } else {
            crate::gui_format::timestamp(formatting, Some(query.last_check_time), now)
        },
        next_check_status_with_format(query, now, formatting),
        query.velocity.clone(),
        // (the bandwidth it waits on: not reckoned here)
        String::new(),
        file_log_short_status(&query.files, short.new, short.deleted),
        query.additional_tags.clone(),
    ]
}

/// "2 working, 1 paused, 1 dead", or "no queries": a dead query counts as
/// dead whether paused or not.
pub fn subscription_status(queries: &[QueryFacts]) -> String {
    if queries.is_empty() {
        return "no queries".into();
    }
    let dead = queries.iter().filter(|q| q.dead).count();
    let paused = queries.iter().filter(|q| !q.dead && q.paused).count();
    let mut parts = vec![format!(
        "{} working",
        human_int((queries.len() - dead - paused) as u64)
    )];
    if paused > 0 {
        parts.push(format!("{} paused", human_int(paused as u64)));
    }
    if dead > 0 {
        parts.push(format!("{} dead", human_int(dead as u64)));
    }
    parts.join(", ")
}

/// A subscription's row in the subscriptions list.
pub fn subscription_row(
    subscription: &SubscriptionFacts,
    now: i64,
    short: ShortSummary,
) -> Vec<String> {
    subscription_row_with_format(
        subscription,
        now,
        short,
        &hydrus_store::settings::GuiFormatting::default(),
    )
}
pub fn subscription_row_with_format(
    subscription: &SubscriptionFacts,
    now: i64,
    short: ShortSummary,
    formatting: &hydrus_store::settings::GuiFormatting,
) -> Vec<String> {
    let queries = &subscription.queries;
    let latest_added = queries.iter().map(|q| q.latest_added).max().unwrap_or(0);
    let last_checked = queries.iter().map(|q| q.last_check_time).max().unwrap_or(0);
    let delay = if now > subscription.no_work_until {
        // (the bandwidth its queries wait on: not reckoned here)
        String::new()
    } else {
        format!(
            "delayed--retrying {} - because: {}",
            delta_exact_with_format(subscription.no_work_until, now, formatting),
            subscription.no_work_until_reason
        )
    };
    let mut files = StatusCounts::new();
    for query in queries {
        for (&status, &n) in &query.files {
            *files.entry(status).or_default() += n;
        }
    }
    vec![
        subscription.name.clone(),
        if subscription.gug_name.is_empty() {
            "no downloader set!".into()
        } else {
            subscription.gug_name.clone()
        },
        subscription_status(queries),
        ago_or_na(latest_added, now, formatting),
        ago_or_na(last_checked, now, formatting),
        delay,
        file_log_short_status(&files, short.new, short.deleted),
        if subscription.paused { "yes" } else { "" }.into(),
        subscription.import_options.clone(),
    ]
}
