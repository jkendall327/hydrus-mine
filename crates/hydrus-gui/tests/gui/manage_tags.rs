//! Managing tags (F3), as the reference's dialog does on a local tag
//! service: an entered tag is added, or, if the files all have it,
//! removed; changes wait until applied.

use std::collections::BTreeSet;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

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
#[allow(clippy::float_cmp)] // Whole-pixel authored width.
fn most_used_panels_filter_only_add_broadcast_and_retire_closed_consumers() {
    let (_dirs, store) = crate::subscriptions::store();
    let f = hydrus_testkit::fixture_json("tag_suggestions.json");
    let windows = headless::init();
    let mut page = SearchPage::new(store.clone());
    page.enter();
    let files = page.results().to_vec();
    let mut model = ManageTags::new(store.clone(), files.clone()).unwrap();
    let mine = model
        .service_names()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    model.choose_service(mine).unwrap();
    model
        .add_side_suggestions(&["parity:present".into()])
        .unwrap();
    model.apply().unwrap();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .to_hex();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    assert!(!files.is_empty());
    assert!(
        files
            .iter()
            .all(|file| !tags_of(&store, *file, "my tags").contains("parity:recent"))
    );
    let tags: Vec<String> = serde_json::from_value(f["edited"]["tags"].clone()).unwrap();
    let own = key.clone();
    store
        .write(move |ctx| {
            let mut tabs: hydrus_store::settings::TagAutocompleteTabs =
                hydrus_store::settings::get(ctx.conn())?;
            tabs.most_used.insert(own, tags);
            hydrus_store::settings::set(ctx.conn(), &tabs)?;
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::related_tags::Settings {
                    enabled: false,
                    ..hydrus_store::related_tags::Settings::default()
                },
            )?;
            let recent = hydrus_core::Tag::new("parity:recent").unwrap();
            let tag = hydrus_store::master::intern_tag(ctx.conn(), &recent)?;
            ctx.conn().execute(
                "INSERT INTO recent_tags(service_id,tag_id,used_ms) VALUES(?,?,?) ON CONFLICT(service_id,tag_id) DO UPDATE SET used_ms=excluded.used_ms",
                rusqlite::params![service, tag, hydrus_core::time::TimestampMs::now().0],
            )?;
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::TagSuggestionSettings {
                    width: 240,
                    columns: true,
                    default_page: "recent".into(),
                    ..hydrus_store::settings::TagSuggestionSettings::default()
                },
            )
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_accepted();
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let mine = manage
        .get_service_names()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    manage.invoke_service_chosen(i32::try_from(mine).unwrap());
    assert!(manage.get_suggested_columns());
    assert_eq!(manage.get_suggested_width().to_bits(), 240.0_f32.to_bits());
    assert_eq!(manage.get_suggested_page(), 1);
    let rows = manage.get_most_used_rows();
    assert_eq!(rows.row_count(), 3);
    assert_eq!(
        rows.row_data(0).unwrap().cells.row_data(0).unwrap(),
        "parity:new2"
    );
    assert!(manage.get_most_used_enabled());
    assert!(manage.get_recent_tags_enabled());
    assert!(
        manage
            .get_recent_tag_rows()
            .iter()
            .any(|row| row.cells.row_data(0).unwrap() == "parity:recent")
    );
    assert!(
        !manage
            .get_recent_tag_rows()
            .iter()
            .any(|row| row.cells.row_data(0).unwrap() == "parity:present")
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1100, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("suggested-tags-columns-width240-populated.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    manage.invoke_side_clicked(0, 0, false, false);
    manage.invoke_side_activated(0, 0);
    manage.invoke_side_activated(0, 0);
    assert!(
        files
            .iter()
            .all(|file| !tags_of(&store, *file, "my tags").contains("parity:new2")),
        "side suggestions are staged"
    );
    assert!(
        !manage
            .get_most_used_rows()
            .iter()
            .any(|row| row.cells.row_data(0).unwrap() == "parity:new2")
    );
    let own = key.clone();
    store
        .write(move |ctx| {
            let mut tabs: hydrus_store::settings::TagAutocompleteTabs =
                hydrus_store::settings::get(ctx.conn())?;
            tabs.most_used
                .get_mut(&own)
                .unwrap()
                .push("parity:broadcast".into());
            hydrus_store::settings::set(ctx.conn(), &tabs)
        })
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(220));
    slint::platform::update_timers_and_animations();
    assert!(
        manage
            .get_most_used_rows()
            .iter()
            .any(|row| row.cells.row_data(0).unwrap() == "parity:broadcast")
    );
    manage.invoke_apply();
    assert!(
        files
            .iter()
            .all(|file| tags_of(&store, *file, "my tags").contains("parity:new2"))
    );
    manage.invoke_side_clicked(0, 0, false, false);
    manage.invoke_side_activated(0, 0);
    manage.invoke_apply();
    assert!(
        files
            .iter()
            .all(|file| !tags_of(&store, *file, "my tags").contains("parity:broadcast")),
        "retired side callbacks cannot write"
    );
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<hydrus_store::settings::TagAutocompleteTabs>)
            .unwrap()
            .most_used[&key]
            .len(),
        5
    );
    assert!(bound.manage_tags.borrow().is_none());
    store
        .write(|ctx| {
            let mut prefs: hydrus_store::settings::TagSuggestionSettings =
                hydrus_store::settings::get(ctx.conn())?;
            prefs.columns = false;
            hydrus_store::settings::set(ctx.conn(), &prefs)
        })
        .unwrap();
    // A new owner reads the saved notebook choice; the retired columns owner
    // keeps its opening preferences and cannot change the successor.
    assert!(manage.get_suggested_columns());
    ui.invoke_manage_tags_selected();
    let notebook = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    notebook.invoke_service_chosen(i32::try_from(mine).unwrap());
    assert!(!notebook.get_suggested_columns());
    assert_eq!(
        notebook.get_suggested_width().to_bits(),
        240.0_f32.to_bits()
    );
    assert_eq!(notebook.get_suggested_page(), 1);
    assert!(notebook.get_most_used_enabled());
    assert!(notebook.get_recent_tags_enabled());
    assert!(notebook.get_most_used_rows().row_count() > 0);
    assert!(
        notebook
            .get_recent_tag_rows()
            .iter()
            .any(|row| row.cells.row_data(0).unwrap() == "parity:recent")
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1100, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("suggested-tags-notebook-width240-recent-populated.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    notebook.invoke_cancel();
    assert!(bound.manage_tags.borrow().is_none());
    ui.hide().unwrap();
}

#[test]
fn switching_from_empty_service_restores_the_only_suggestion_page() {
    let (_dirs, store) = crate::subscriptions::store();
    let fixture = hydrus_testkit::fixture_json("tag_suggestions.json");
    let windows = headless::init();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .to_hex();
    let tags: Vec<String> = serde_json::from_value(fixture["edited"]["tags"].clone()).unwrap();
    store
        .write(move |writer| {
            let mut lists: hydrus_store::settings::TagAutocompleteTabs =
                hydrus_store::settings::get(writer.conn())?;
            lists.most_used.clear();
            lists.most_used.insert(key, tags);
            hydrus_store::settings::set(writer.conn(), &lists)?;
            // The recorded most-used/recent notebook deliberately disables related.
            hydrus_store::settings::set(
                writer.conn(),
                &hydrus_store::related_tags::Settings {
                    enabled: false,
                    ..hydrus_store::related_tags::Settings::default()
                },
            )?;
            hydrus_store::settings::set(
                writer.conn(),
                &hydrus_store::settings::TagSuggestionSettings {
                    columns: false,
                    recent_limit: None,
                    default_page: "favourites".into(),
                    ..hydrus_store::settings::TagSuggestionSettings::default()
                },
            )
        })
        .unwrap();
    let mut page = SearchPage::new(store.clone());
    page.enter();
    let files = page.results().to_vec();
    assert!(!files.is_empty());
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(page));
    ui.show().unwrap();
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let service = |name| {
        i32::try_from(
            manage
                .get_service_names()
                .iter()
                .position(|value| value == name)
                .unwrap(),
        )
        .unwrap()
    };
    let mine = service("my tags");
    let empty = service("second tags");
    assert!(!manage.get_suggested_columns());
    assert!(!manage.get_recent_tags_enabled());
    for _ in 0..2 {
        manage.invoke_service_chosen(empty);
        assert!(!manage.get_most_used_enabled());
        assert_eq!(manage.get_most_used_rows().row_count(), 0);
        manage.invoke_service_chosen(mine);
        assert!(manage.get_most_used_enabled());
        assert_eq!(
            manage.get_suggested_page(),
            0,
            "a disabled recent page cannot hide the only list"
        );
        assert!(manage.get_most_used_rows().row_count() > 0);
    }
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1100, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("suggested_tags_service_switch.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    let staged = manage
        .get_most_used_rows()
        .row_data(0)
        .unwrap()
        .cells
        .row_data(0)
        .unwrap()
        .to_string();
    manage.invoke_side_clicked(0, 0, false, false);
    manage.invoke_side_activated(0, 0);
    assert!(
        files
            .iter()
            .all(|file| !tags_of(&store, *file, "my tags").contains(&staged))
    );
    manage.invoke_cancel();
    assert!(bound.manage_tags.borrow().is_none());
    assert!(
        files
            .iter()
            .all(|file| !tags_of(&store, *file, "my tags").contains(&staged)),
        "parent Cancel discards the suggestion activation"
    );
    ui.hide().unwrap();
}

// leaf: audit-media-tags-toggle, audit-media-tags-service
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

    // (typed entry removes a tag the files all have only when the cog's
    // "allow remove/petition result on tag input" is on)
    store
        .write(|ctx| {
            let mut o: hydrus_store::tag_editing::TagEditingSettings =
                hydrus_store::settings::get(ctx.conn())?;
            o.allow_remove_on_input = true;
            hydrus_store::settings::set(ctx.conn(), &o)
        })
        .unwrap();
    let mut manage = ManageTags::new(store.clone(), vec![tagged]).unwrap();
    let enter = |m: &mut ManageTags, typed: &str| {
        m.add_tags(&[typed.to_owned()], false).map(|_| ())
    };
    let names = manage.service_names();
    let mine = names.iter().position(|n| n == "my tags").unwrap();
    manage.choose_service(mine).unwrap();
    let listed = |m: &ManageTags| -> Vec<String> { m.rows().into_iter().map(|(t, _)| t).collect() };
    assert!(listed(&manage).contains(&its_tag));
    // entered, a new tag is added; entered again, it isn't
    enter(&mut manage, "  Brand New ").unwrap();
    assert!(listed(&manage).contains(&"brand new".to_owned()));
    enter(&mut manage, "brand new").unwrap();
    assert!(!listed(&manage).contains(&"brand new".to_owned()));
    assert!(
        manage.has_changes(),
        "adding then removing creates a deleted mapping, as the reference does"
    );
    // an existing tag is removed
    enter(&mut manage, &its_tag).unwrap();
    assert!(!listed(&manage).contains(&its_tag));
    enter(&mut manage, "brand new").unwrap();
    assert!(enter(&mut manage, "").is_err(), "not a tag");
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
    enter(&mut manage, &second).unwrap();
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
    ui.show().unwrap();
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
