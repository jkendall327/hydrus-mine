//! Tags > sibling/parent sync > "review current sibling/parent sync" (the
//! reference's `ReviewTagDisplayMaintenancePanel`): a tab per tag service
//! saying how its siblings and parents are applied. hydrus-rs applies them
//! as it writes, so every service reads as the reference's synced state and
//! "work hard now!" never shows.

use hydrus_core::numbers::human_int;

pub const TITLE: &str = "tag display sync";
pub const MESSAGE: &str = "Figuring out how tags should appear according to sibling and parent application rules takes time. When you set new rules, the changes in presentation and suggestion do not happen immediately--the client catches up to your settings in the background. This work takes a lot of math and can cause lag.\n\nIf there is a lot of work still to do, your tag suggestions and presentation during the interim may be unusual.";
pub const SYNCED: &str = "All synced!";
pub const REFRESHING: &str = "refreshing\u{2026}";

/// The sync status line, by the background sync switches; `true` when it
/// is the valid (rather than warning) colour.
pub fn status(during_idle: bool, during_active: bool) -> (&'static str, bool) {
    if during_active {
        (
            "Siblings and parents are set to sync all the time. If there is work to do here, it should be cleared out in real time as you watch.",
            true,
        )
    } else if during_idle {
        (
            "Siblings and parents are only set to sync during idle time. If there is work to do here, it should be cleared out when you are not using the client.",
            false,
        )
    } else {
        (
            "Siblings and parents are not set to sync in the background at any time. If there is work to do here, you can force it now by clicking 'work now!' button.",
            false,
        )
    }
}

/// A synced service's progress line, for its `rules` applied.
pub fn progress(rules: usize) -> String {
    if rules == 0 {
        "No siblings/parents applying to this service.".to_owned()
    } else {
        format!("{} rules, all synced!", human_int(rules as u64))
    }
}

/// One service's tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub key: hydrus_core::ServiceKey,
    pub name: String,
    pub progress: String,
}

/// Every real tag service's tab, in the registry's order, and the tab
/// to open on (the default tag service tab's, else the first).
pub fn tabs(store: &hydrus_store::Store) -> hydrus_store::Result<(Vec<Tab>, usize)> {
    let snapshot = store.snapshot();
    let default = store
        .read(hydrus_store::settings::get::<hydrus_store::tag_editing::TagEditingSettings>)?
        .default_service;
    let tabs: Vec<Tab> = snapshot
        .services
        .tag_services()
        .map(|service| Tab {
            key: service.key.clone(),
            name: service.name.clone(),
            progress: progress(snapshot.display.get(service.id).rule_count()),
        })
        .collect();
    let start = tabs.iter().position(|t| t.key == default).unwrap_or(0);
    Ok((tabs, start))
}
