//! A search page's file and tag domains: new pages search the options'
//! default tag service, as the reference's `NewPageQuery` makes them;
//! and the file and tag domain buttons, against the reference's
//! (`oracle/record_search_domains.py`), changing what the page searches.

use std::sync::Arc;

use hydrus_core::ServiceKey;
use hydrus_core::pages::PageContent;
use hydrus_core::service::builtin_keys;
use serde_json::{Value as Json, json};
use slint::Model as _;

use hydrus_gui::domains::{
    Choice, Domains, Row, location_label, location_menu, multiple_ticks, tag_label, tag_menu,
    ticked_location,
};
use hydrus_gui::page_chooser::NewPage;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::{LocationContext, TagContext};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::settings::{self, SearchDefaults};

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn key(k: &[u8]) -> ServiceKey {
    ServiceKey::new(k.to_vec())
}

/// The page shown's file and tag domains.
fn domains(pages: &Pages) -> (LocationContext, TagContext) {
    match &pages.shown().content {
        PageContent::Search { search, .. } => (search.location.clone(), search.tags.clone()),
        other => panic!("not a search page: {other:?}"),
    }
}

fn set_tag_service(store: &Store, service: ServiceKey) {
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &SearchDefaults {
                    tag_service: service,
                    ..SearchDefaults::default()
                },
            )
        })
        .unwrap();
}

#[test]
fn new_search_pages_search_the_default_tag_service() {
    let (_dirs, store) = store();
    let mut pages = Pages::open(store.clone()).unwrap();
    let my_files = LocationContext::single(key(builtin_keys::MY_FILES));
    // hydrus's default: every tag service
    pages.new_search_page();
    assert_eq!(
        domains(&pages),
        (
            my_files.clone(),
            TagContext::new(key(builtin_keys::COMBINED_TAG), true, true)
        )
    );
    // the options' "my tags": new pages, those the page chooser opens, and
    // files opened in a new page
    set_tag_service(&store, key(builtin_keys::MY_TAGS));
    let my_tags = TagContext::new(key(builtin_keys::MY_TAGS), true, true);
    pages.new_search_page();
    assert_eq!(domains(&pages), (my_files, my_tags.clone()));
    let trash = LocationContext::single(key(builtin_keys::TRASH));
    pages
        .new_page(&NewPage::Search {
            domain: key(builtin_keys::TRASH),
            name: "trash".into(),
        })
        .unwrap();
    assert_eq!(domains(&pages), (trash.clone(), my_tags.clone()));
    pages.open_files(trash.clone(), Vec::new(), None, None);
    assert_eq!(domains(&pages), (trash, my_tags.clone()));
    // all known files, searched with a tag service
    let all_known_files = LocationContext::single(key(builtin_keys::COMBINED_FILE));
    pages.open_search(all_known_files.clone(), Vec::new(), "files");
    assert_eq!(domains(&pages), (all_known_files.clone(), my_tags));
    // but with every tag service, all the files stored here instead
    set_tag_service(&store, key(builtin_keys::COMBINED_TAG));
    pages.open_search(all_known_files, Vec::new(), "files");
    assert_eq!(
        domains(&pages).0,
        LocationContext::single(key(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE))
    );
    // a tag service that is gone: every tag service
    set_tag_service(&store, key(b"a service deleted since"));
    pages.new_search_page();
    assert!(domains(&pages).1.is_all_known_tags());
}

/// A domain menu as the recording writes it.
fn menu_json(rows: &[Option<Row>]) -> Json {
    rows.iter()
        .map(|row| match row {
            None => json!("---"),
            Some(row) => json!({ "text": row.label, "checked": row.checked }),
        })
        .collect()
}

/// The recording's name for a file domain, as a service.
fn named(store: &Store, name: &str) -> ServiceKey {
    let builtin = match name {
        "my files" => builtin_keys::MY_FILES,
        "trash" => builtin_keys::TRASH,
        "all my files" => builtin_keys::COMBINED_LOCAL_FILE_DOMAINS,
        "all local files" => builtin_keys::HYDRUS_LOCAL_FILE_STORAGE,
        "all known files" => builtin_keys::COMBINED_FILE,
        other => {
            return store
                .snapshot()
                .services
                .by_name(other)
                .unwrap_or_else(|| panic!("no service {other}"))
                .key
                .clone();
        }
    };
    key(builtin)
}

/// The file and tag domain buttons' labels and menus, what choosing from
/// each does to both, the "multiple/deleted locations" list, the labels
/// of mixed domains and what the list keeps of ticked sets, against the
/// reference's (`oracle/record_search_domains.py`), with advanced mode
/// off and on.
// leaf: audit-shared-location-current
// leaf: audit-shared-location-normalize
#[test]
fn the_domain_buttons_are_the_references() {
    let (_dirs, store) = store();
    let recorded = hydrus_testkit::fixture_json("search_domains.json");
    let snapshot = store.snapshot();
    let services = &snapshot.services;
    let my_files = LocationContext::single(key(builtin_keys::MY_FILES));
    let start = Domains {
        location: my_files.clone(),
        tags: TagContext::default(),
    };
    let labels = |d: &Domains| {
        json!({
            "location": location_label(services, &d.location),
            "tags": tag_label(services, &d.tags),
        })
    };
    for (mode, advanced) in [("normal", false), ("advanced", true)] {
        let theirs = &recorded[mode];
        assert_eq!(labels(&start), theirs["start"], "{mode}");
        let files_menu = location_menu(services, advanced, &start.location);
        assert_eq!(menu_json(&files_menu), theirs["location_menu"], "{mode}");
        let tags_menu = tag_menu(services, &start.tags);
        assert_eq!(menu_json(&tags_menu), theirs["tag_menu"], "{mode}");
        // each entry chosen from the start
        let chosen: Vec<Json> = files_menu
            .iter()
            .flatten()
            .filter_map(|row| match &row.choice {
                Choice::Location(location) => {
                    let mut d = start.clone();
                    d.choose_location(services, location.clone());
                    let mut out = labels(&d);
                    out["chosen"] = json!(row.label);
                    Some(out)
                }
                _ => None,
            })
            .collect();
        assert_eq!(json!(chosen), theirs["location_choices"], "{mode}");
        let chosen: Vec<Json> = tags_menu
            .iter()
            .flatten()
            .map(|row| {
                let Choice::Tags(service) = &row.choice else {
                    panic!("a tag service");
                };
                let mut d = start.clone();
                d.choose_tags(service.clone(), &my_files);
                let mut out = labels(&d);
                out["chosen"] = json!(row.label);
                out
            })
            .collect();
        assert_eq!(json!(chosen), theirs["tag_choices"], "{mode}");
        // the list, on "my files"
        let ticks: Vec<Json> = multiple_ticks(services, advanced)
            .iter()
            .map(|t| {
                json!({
                    "text": t.label,
                    "checked": !t.deleted && my_files.current().contains(&t.service),
                })
            })
            .collect();
        assert_eq!(json!(ticks), theirs["multiple"], "{mode}");
        // all known files, then every tag service
        if advanced {
            let mut d = start.clone();
            d.choose_location(
                services,
                LocationContext::single(key(builtin_keys::COMBINED_FILE)),
            );
            let after_files = labels(&d);
            d.choose_tags(key(builtin_keys::COMBINED_TAG), &my_files);
            assert_eq!(
                json!({ "after_files": after_files, "after_tags": labels(&d) }),
                theirs["all_known_files_then_all_known_tags"]
            );
        }
    }
    let names = |list: &Json| -> Vec<ServiceKey> {
        list.as_array()
            .unwrap()
            .iter()
            .map(|n| named(&store, n.as_str().unwrap()))
            .collect()
    };
    for mix in recorded["labels"].as_array().unwrap() {
        let location = LocationContext::new(names(&mix["current"]), names(&mix["deleted"]));
        assert_eq!(location_label(services, &location), mix["label"], "{mix}");
    }
    for case in recorded["surplus"].as_array().unwrap() {
        let ticked: Vec<(bool, ServiceKey)> = names(&case["checked"])
            .into_iter()
            .map(|k| (false, k))
            .chain(names(&case["deleted"]).into_iter().map(|k| (true, k)))
            .collect();
        let kept = ticked_location(services, &ticked);
        let sorted_names = |keys: &std::collections::BTreeSet<ServiceKey>| {
            let mut names: Vec<String> = keys
                .iter()
                .map(|k| services.by_key(k).unwrap().name.clone())
                .collect();
            names.sort();
            names
        };
        assert_eq!(
            json!({ "current": sorted_names(kept.current()), "deleted": sorted_names(kept.deleted()) }),
            case["kept"],
            "{case}"
        );
    }
}

/// The open menu's lines' labels, and the line labelled `label`.
fn menu_lines(ui: &MainWindow) -> Vec<(String, bool)> {
    let panes = ui.get_menu_panes();
    let lines = panes.row_data(0).expect("a menu open").lines;
    (0..lines.row_count())
        .map(|i| {
            let line = lines.row_data(i).unwrap();
            (line.label.to_string(), line.checked)
        })
        .collect()
}

fn choose(ui: &MainWindow, label: &str) {
    let at = menu_lines(ui)
        .iter()
        .position(|(l, _)| l == label)
        .unwrap_or_else(|| panic!("no {label} in {:?}", menu_lines(ui)));
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
}

/// The files in a domain, as the store searches it.
fn searched(store: &Store, location: LocationContext, tags: TagContext) -> usize {
    use hydrus_search::{FileSearchContext, Predicate, SystemPredicate};
    let search = FileSearchContext {
        location,
        tags,
        predicates: vec![Predicate::System(SystemPredicate::Everything)],
    };
    let snapshot = store.snapshot();
    let sort = hydrus_search::FileSort {
        by: hydrus_search::SortBy::ImportTime,
        order: hydrus_search::SortOrder::Descending,
    };
    store
        .read(|conn| {
            Ok(hydrus_search::search_files(
                conn,
                &snapshot,
                &search,
                sort,
                &hydrus_search::Clock::system(),
            )
            .unwrap()
            .len())
        })
        .unwrap()
}

#[test]
fn the_domain_buttons_change_what_the_page_searches() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let page = || bound.current.borrow().clone();
    // a page on "my files" searching everything
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let my_files = LocationContext::single(key(builtin_keys::MY_FILES));
    assert_eq!(page().borrow().location(), &my_files);
    assert_eq!(ui.get_location_label(), "my files");
    assert_eq!(ui.get_tags_label(), "all known tags");
    assert_eq!(
        page().borrow().results().len(),
        searched(&store, my_files.clone(), TagContext::default())
    );

    // the file domain button's menu, "my files" ticked; the trash chosen
    ui.invoke_domain_menu_requested(0, 10.0, 200.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    assert_eq!((pane.x, pane.y), (10.0, 200.0), "under the button");
    assert_eq!(ui.get_menu_open(), -2, "no title of the bar open");
    assert!(menu_lines(&ui).contains(&("my files".to_owned(), true)));
    assert!(menu_lines(&ui).contains(&("trash".to_owned(), false)));
    choose(&ui, "trash");
    assert_eq!(ui.get_menu_panes().row_count(), 0, "closed");
    let trash = LocationContext::single(key(builtin_keys::TRASH));
    assert_eq!(page().borrow().location(), &trash);
    assert_eq!(ui.get_location_label(), "trash");
    let in_trash = searched(&store, trash.clone(), TagContext::default());
    assert!(in_trash > 0);
    assert_eq!(page().borrow().results().len(), in_trash, "searched again");

    // the tag domain button: "my tags"
    ui.invoke_domain_menu_requested(1, 200.0, 200.0);
    assert!(menu_lines(&ui).contains(&("all known tags".to_owned(), true)));
    choose(&ui, "my tags");
    assert_eq!(ui.get_tags_label(), "my tags");
    assert_eq!(
        page().borrow().tag_context().service,
        key(builtin_keys::MY_TAGS)
    );

    // excluding current tags
    assert!(ui.get_include_current());
    ui.invoke_include_flipped(0);
    assert!(!ui.get_include_current());
    assert!(!page().borrow().tag_context().include_current);
    assert!(page().borrow().tag_context().include_pending);
    ui.invoke_include_flipped(1);
    assert!(!page().borrow().tag_context().include_pending);

    // the list: my files and the trash ticked, applied
    ui.invoke_domain_menu_requested(0, 10.0, 200.0);
    choose(&ui, "multiple/deleted locations");
    let list = || {
        bound
            .locations
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the list is open")
    };
    let ticks = || -> Vec<(String, bool)> {
        let ticks = list().get_ticks();
        (0..ticks.row_count())
            .map(|i| {
                let t = ticks.row_data(i).unwrap();
                (t.label.to_string(), t.checked)
            })
            .collect()
    };
    let at =
        |label: &str| i32::try_from(ticks().iter().position(|(l, _)| l == label).unwrap()).unwrap();
    assert!(ticks().contains(&("trash".to_owned(), true)), "as searched");
    list().invoke_toggled(at("my files"), true);
    list().invoke_apply();
    assert!(bound.locations.borrow().is_none(), "closed");
    let both = LocationContext::new([key(builtin_keys::MY_FILES), key(builtin_keys::TRASH)], []);
    assert_eq!(page().borrow().location(), &both);
    assert_eq!(ui.get_location_label(), "my files, trash");
    // (a mix is none of the menu's own: "multiple/deleted locations" is
    // ticked)
    ui.invoke_domain_menu_requested(0, 10.0, 200.0);
    assert!(menu_lines(&ui).contains(&("multiple/deleted locations".to_owned(), true)));
    assert!(menu_lines(&ui).contains(&("trash".to_owned(), false)));
    // ticking all my files unticks my files
    choose(&ui, "multiple/deleted locations");
    list().invoke_toggled(at("combined local file domains"), true);
    assert!(ticks().contains(&("my files".to_owned(), false)));
    list().invoke_cancel();
    assert_eq!(page().borrow().location(), &both, "cancelled");

    // kept as the page is saved
    let content = page()
        .borrow()
        .content(&bound.pages.borrow().shown().content);
    let PageContent::Search { search, .. } = content else {
        panic!("a search page");
    };
    assert_eq!(search.location, both);
    assert_eq!(search.tags.service, key(builtin_keys::MY_TAGS));
    assert!(!search.tags.include_current);
}
