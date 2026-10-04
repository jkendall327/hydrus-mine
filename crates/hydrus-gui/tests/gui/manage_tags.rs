//! Managing tags (F3), as the reference's dialog does on a local tag
//! service: an entered tag is added, or, if the files all have it,
//! removed; changes wait until applied.

use std::collections::BTreeSet;
use std::sync::Arc;

use slint::Model as _;

use hydrus_core::HashId;
use hydrus_gui::manage_tags::ManageTags;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

/// A file's current tags on the tag service named.
fn tags_of(store: &Store, file: HashId, service: &str) -> BTreeSet<String> {
    let snapshot = store.snapshot();
    let service = snapshot.services.by_name(service).unwrap().id;
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
        .unwrap();
    batch.results[0]
        .tags
        .get(&service)
        .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
        .into_iter()
        .flatten()
        .map(|id| batch.tags[id].to_string())
        .collect()
}

#[test]
fn tags_are_added_and_removed_as_the_reference_does() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let mut page = SearchPage::new(store.clone());
    page.enter();
    let files = page.results().to_vec();
    // a file with tags on "my tags", and one without one of them
    let (tagged, its_tag) = files
        .iter()
        .find_map(|&f| {
            tags_of(&store, f, "my tags")
                .into_iter()
                .next()
                .map(|t| (f, t))
        })
        .unwrap();
    let other = *files
        .iter()
        .find(|&&f| f != tagged && !tags_of(&store, f, "my tags").contains(&its_tag))
        .unwrap();

    let mut manage = ManageTags::new(store.clone(), vec![tagged]).unwrap();
    let names = manage.service_names();
    let mine = names.iter().position(|n| n == "my tags").unwrap();
    manage.choose_service(mine).unwrap();
    let listed = |m: &ManageTags| -> Vec<String> { m.rows().into_iter().map(|(t, _)| t).collect() };
    assert!(listed(&manage).contains(&its_tag));
    // entered, a new tag is added; entered again, it isn't
    manage.enter("  Brand New ").unwrap();
    assert!(listed(&manage).contains(&"brand new".to_owned()));
    manage.enter("brand new").unwrap();
    assert!(!listed(&manage).contains(&"brand new".to_owned()));
    assert!(
        manage.has_changes(),
        "adding then removing creates a deleted mapping, as the reference does"
    );
    // an existing tag is removed
    manage.enter(&its_tag).unwrap();
    assert!(!listed(&manage).contains(&its_tag));
    manage.enter("brand new").unwrap();
    assert!(manage.enter("").is_err(), "not a tag");
    // suggestions are what was typed, then the service's tags
    manage.set_text("blu");
    assert_eq!(manage.suggestions()[0].0, "blu");
    assert!(
        manage.suggestions()[1..]
            .iter()
            .any(|(tag, _)| tag.starts_with("blu")),
        "{:?}",
        manage.suggestions()
    );
    manage.move_highlight(1);
    let second = manage.suggestions()[1].0.clone();
    manage.enter_input().unwrap();
    assert!(listed(&manage).contains(&second));
    manage.enter(&second).unwrap();
    // nothing changes until applied
    assert!(tags_of(&store, tagged, "my tags").contains(&its_tag));
    manage.apply().unwrap();
    let now = tags_of(&store, tagged, "my tags");
    assert!(now.contains("brand new"));
    assert!(!now.contains(&its_tag));

    // two files: a tag only one has is counted, and entering it adds it to
    // the other
    let mut both = ManageTags::new(store.clone(), vec![tagged, other]).unwrap();
    both.choose_service(mine).unwrap();
    let row = both
        .rows()
        .into_iter()
        .find(|(t, _)| t == "brand new")
        .unwrap();
    assert!(row.1.ends_with(" (1)"), "{row:?}");
    both.enter("brand new").unwrap();
    let row = both
        .rows()
        .into_iter()
        .find(|(t, _)| t == "brand new")
        .unwrap();
    assert!(row.1.ends_with(" (2)"), "{row:?}");
    both.apply().unwrap();
    assert!(tags_of(&store, other, "my tags").contains("brand new"));
    assert!(tags_of(&store, tagged, "my tags").contains("brand new"));

    // the window, from the viewer's F3: entering applies with nothing
    // typed, and the viewer's tags show the change
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let results = bound.current.borrow().borrow().results().to_vec();
    let index = results.iter().position(|&f| f == tagged).unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    viewer.invoke_manage_tags();
    let window = bound
        .manage_tags
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("manage tags opened");
    assert_eq!(window.get_window_title(), "manage tags");
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    assert_eq!(window.get_window_title(), "manage tags");
    let services = window.get_service_names();
    let mine = (0..services.row_count())
        .position(|i| services.row_data(i).unwrap() == "my tags")
        .unwrap();
    window.invoke_service_chosen(i32::try_from(mine).unwrap());
    window.invoke_text_edited("from the window".into());
    window.invoke_entered();
    let rows: Vec<String> = (0..window.get_tags().row_count())
        .map(|i| window.get_tags().row_data(i).unwrap().text.to_string())
        .collect();
    assert!(rows.contains(&"from the window (1)".to_owned()), "{rows:?}");
    assert!(!tags_of(&store, tagged, "my tags").contains("from the window"));
    window.invoke_text_edited("".into());
    window.invoke_entered();
    assert!(bound.manage_tags.borrow().is_none(), "applied and closed");
    assert!(tags_of(&store, tagged, "my tags").contains("from the window"));
    let hover: Vec<String> = (0..viewer.get_tags().row_count())
        .map(|i| viewer.get_tags().row_data(i).unwrap().text.to_string())
        .collect();
    assert!(hover.contains(&"from the window".to_owned()), "{hover:?}");
    // escape forgets
    viewer.invoke_manage_tags();
    let window = bound
        .manage_tags
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    window.invoke_service_chosen(i32::try_from(mine).unwrap());
    window.invoke_text_edited("forgotten".into());
    window.invoke_entered();
    window.invoke_text_edited("blu".into());
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    let drawn = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&drawn, 420, 560);
    headless::save_png(&shots.join("manage_tags.png"), &pixels, 420, 560).unwrap();
    window.invoke_cancel();
    assert!(bound.manage_tags.borrow().is_none());
    assert!(!tags_of(&store, tagged, "my tags").contains("forgotten"));
}
