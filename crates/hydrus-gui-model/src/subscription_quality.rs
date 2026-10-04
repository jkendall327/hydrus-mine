//! The reference quality-information menu and its exact report/clipboard output.
use hydrus_core::numbers::{float_to_percentage, human_int};
use hydrus_store::subscription_quality::QueryQuality;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Show,
    CopyCsv,
}
pub const MENU: [(&str, Action); 2] = [
    ("show", Action::Show),
    ("copy csv data to clipboard", Action::CopyCsv),
];

fn percent(row: &QueryQuality) -> String {
    let count = row.archived + row.deleted;
    if count == 0 {
        "0.0%".into()
    } else {
        float_to_percentage(row.archived as f64 / count as f64)
    }
}
pub fn information(rows: &[QueryQuality]) -> String {
    rows.iter()
        .map(|row| {
            let mut text = format!(
                "{}: inbox {} | archive {} | deleted {}",
                row.name,
                human_int(row.inbox),
                human_int(row.archived),
                human_int(row.deleted)
            );
            if row.archived + row.deleted > 0 {
                text.push_str(&format!(" | good {}", percent(row)));
            }
            text
        })
        .collect::<Vec<_>>()
        .join("\n")
}
/// Raw comma-joined rows, without quoting or a header, as in `_CopyQualityInfo`.
pub fn csv(rows: &[QueryQuality]) -> String {
    rows.iter()
        .map(|row| {
            format!(
                "{},{},{},{},{}",
                row.name,
                row.inbox,
                row.archived,
                row.deleted,
                percent(row)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Only saved queues that already belonged to this editor at opening qualify.
/// Draft display names are used, but query logs and file states are durable.
pub fn selected(
    dialog: &crate::edit_subscription::EditSubscription,
    original: &std::collections::BTreeSet<i64>,
    now: i64,
) -> Vec<(i64, String)> {
    dialog
        .selected(now)
        .into_iter()
        .filter_map(|key| {
            let q = &dialog.get(key)?.query;
            let queue = q.queue.filter(|queue| original.contains(queue))?;
            Some((
                queue,
                q.state
                    .display_name
                    .clone()
                    .unwrap_or_else(|| q.state.query_text.clone()),
            ))
        })
        .collect()
}
