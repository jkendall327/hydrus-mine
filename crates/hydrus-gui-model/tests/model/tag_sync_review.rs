//! Tags > sibling/parent sync > review current sibling/parent sync: what
//! it says (`ReviewTagDisplayMaintenancePanel`).
use hydrus_gui_model::tag_sync_review::{progress, status, tabs};
use hydrus_store::Store;

#[test]
fn the_status_follows_the_sync_switches() {
    assert!(status(true, true).1);
    assert!(
        status(false, true)
            .0
            .starts_with("Siblings and parents are set to sync all the time.")
    );
    assert!(!status(true, false).1);
    assert!(
        status(true, false)
            .0
            .starts_with("Siblings and parents are only set to sync during idle time.")
    );
    assert!(
        status(false, false)
            .0
            .ends_with("by clicking 'work now!' button.")
    );
}

#[test]
fn a_service_counts_its_rules() {
    assert_eq!(progress(0), "No siblings/parents applying to this service.");
    assert_eq!(progress(1_234), "1,234 rules, all synced!");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (tabs, start) = tabs(&store).unwrap();
    assert!(!tabs.is_empty());
    assert!(start < tabs.len());
    assert!(tabs.iter().all(|t| t.progress == progress(0)));
}
