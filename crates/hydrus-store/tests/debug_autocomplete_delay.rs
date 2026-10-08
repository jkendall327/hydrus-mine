//! Help > debug > gui actions > autocomplete delay mode: every tag
//! autocomplete search takes three seconds at the database level
//! (`ClientDBTagSearch`). The switch is process-wide, so this is tested alone
//! here.
use std::time::{Duration, Instant};

use hydrus_core::ServiceType;
use hydrus_core::debug_flags::Flag;
use hydrus_store::{
    Store,
    autocomplete::{CountDomain, TagDisplayType, TagQuery, TagSearchScope, search_tags},
};

// leaf: audit-options-help-debug-action-autocomplete-delay-mode
#[test]
fn autocomplete_delay_mode_makes_each_search_take_three_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let snap = store.snapshot();
    let all = snap
        .services
        .of_type(ServiceType::CombinedFile)
        .next()
        .unwrap()
        .id;
    let tags = snap.services.tag_services().next().unwrap().id;
    let search = || {
        let started = Instant::now();
        store
            .read(|conn| {
                search_tags(
                    conn,
                    &snap.services,
                    &snap.display,
                    &TagSearchScope {
                        domains: vec![CountDomain {
                            service: all,
                            exact: true,
                        }],
                        tag_service: Some(tags),
                        display: TagDisplayType::Display,
                        include_current: true,
                        include_pending: true,
                    },
                    &TagQuery {
                        text: "blue*".into(),
                        search_namespaces_into_full_tags: false,
                    },
                )
            })
            .unwrap();
        started.elapsed()
    };
    assert!(
        search() < Duration::from_secs(1),
        "quick while the mode is off"
    );
    Flag::AutocompleteDelay.set(true);
    let slow = search();
    Flag::AutocompleteDelay.set(false);
    assert!(slow >= Duration::from_secs(3), "{slow:?}");
}
