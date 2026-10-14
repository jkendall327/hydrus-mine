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
    // (the reference's order: local tag services, then repositories, each by lowercase name)
    let mut services: Vec<_> = snapshot.services.tag_services().collect();
    services.sort_by_key(|service| {
        (
            service.service_type() != hydrus_core::ServiceType::LocalTag,
            service.name.to_lowercase(),
        )
    });
    let tabs: Vec<Tab> = services
        .into_iter()
        .map(|service| Tab {
            key: service.key.clone(),
            name: service.name.clone(),
            progress: progress(snapshot.display.get(service.id).rule_count()),
        })
        .collect();
    let start = tabs.iter().position(|t| t.key == default).unwrap_or(0);
    Ok((tabs, start))
}

/// The warning heading of the sibling/parent application editor
/// (`EditTagDisplayApplication`).
pub const APPLICATION_WARNING: &str = "THIS IS COMPLICATED, THINK CAREFULLY";
/// Its explanation.
pub const APPLICATION_MESSAGE: &str = "While a tag service normally only applies its own siblings and parents to itself, it does not have to. You can have other services' rules apply (e.g. putting the PTR's siblings on your \"my tags\"), or no siblings/parents at all.\n\nIf you apply multiple services and there are conflicts (e.g. disagreements on where siblings go, or loops), the services at the top of the list have precedence. If you want to overwrite some PTR rules, then make what you want on a local service and then put it above the PTR here. Also, siblings apply first, then parents.\n\nIf you make big changes here, it will take a long time for the client to recalculate everything. Sibling and parent chains will be broken apart and rearranged live, and for a brief period, some sibling or parent suggestions or presentation may be unusual. Check the sync progress panel under _tags->sync->review current sibling/parent sync_ to see how it is going. If your client gets too laggy doing the recalc, turn it off during \"normal time\".";

/// The application editor's sync status line, by the background sync switches;
/// `true` when it is the valid (rather than warning) colour.
pub fn application_status(during_idle: bool, during_active: bool) -> (&'static str, bool) {
    if during_active {
        (
            "Siblings and parents are set to sync all the time. Changes will start applying as soon as you ok this dialog.",
            true,
        )
    } else if during_idle {
        (
            "Siblings and parents are only set to sync during idle time. Changes here will only start to apply when you are not using the client.",
            false,
        )
    } else {
        (
            "Siblings and parents are not set to sync in the background at any time. If there is sync work to do, you will have to force it to run using the 'review' window under _tags->sync_.",
            false,
        )
    }
}
