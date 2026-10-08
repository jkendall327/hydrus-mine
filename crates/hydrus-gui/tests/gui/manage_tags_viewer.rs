//! Manage Tags opened from the media viewer, replaying the recorded
//! `manage_tags_viewer.json` (the reference's real `ManageTagsPanel` with
//! `immediate_commit` and a canvas key) through the real window: entry, the
//! viewer moving, and PageUp/PageDown key events in an empty input (the
//! conditions recorded in `manage_tags_keys.json`).

use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::Tag;
use hydrus_gui::{MainWindow, ManageTagsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::content::MappingAction;

struct Opened {
    _dirs: [tempfile::TempDir; 2],
    store: std::sync::Arc<hydrus_store::Store>,
    files: Vec<hydrus_core::HashId>,
    ui: MainWindow,
    bound: hydrus_gui::Bound,
}

fn open(recorded: &Value) -> Opened {
    let (dirs, store) = crate::subscriptions::store();
    let hashes: Vec<hydrus_core::Sha256> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().parse().unwrap())
        .collect();
    let files = store
        .read(|conn| {
            let ids = hydrus_store::master::hash_ids(conn, &hashes)?;
            Ok(hashes.iter().map(|h| ids[h]).collect::<Vec<_>>())
        })
        .unwrap();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let (corpus, on) = (recorded["corpus"].clone(), files.clone());
    store
        .write_content(move |writer| {
            for row in corpus.as_array().unwrap() {
                let tag = Tag::new(row[0].as_str().unwrap()).unwrap();
                let id = hydrus_store::master::intern_tag(writer.conn(), &tag)?;
                let tagged: Vec<_> = row[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| on[usize::try_from(i.as_u64().unwrap()).unwrap()])
                    .collect();
                writer.update_mappings(service, &MappingAction::Add, id, &tagged)?;
            }
            Ok(())
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "viewer tags",
            None,
            files.clone(),
        )),
    );
    Opened {
        _dirs: dirs,
        store,
        files,
        ui,
        bound,
    }
}

fn set_allow_remove(store: &hydrus_store::Store, allow: bool) {
    store
        .write(move |ctx| {
            let mut o: hydrus_store::tag_editing::TagEditingSettings =
                hydrus_store::settings::get(ctx.conn())?;
            o.allow_remove_on_input = allow;
            o.confirm_remove = true;
            hydrus_store::settings::set(ctx.conn(), &o)
        })
        .unwrap();
}

/// The `v:` tags each of the files has, as the store has them.
fn stored(store: &hydrus_store::Store, files: &[hydrus_core::HashId]) -> Value {
    let snapshot = store.snapshot();
    let service = snapshot.services.by_name("my tags").unwrap().id;
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, files))
        .unwrap();
    serde_json::json!(
        files
            .iter()
            .map(|file| {
                let media = batch.results.iter().find(|m| m.hash_id == *file).unwrap();
                let mut tags: Vec<String> = media
                    .tags
                    .get(&service)
                    .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
                    .into_iter()
                    .flatten()
                    .map(|id| batch.tags[id].to_string())
                    .filter(|t| t.starts_with("v:"))
                    .collect();
                tags.sort();
                tags
            })
            .collect::<Vec<_>>()
    )
}

fn shown(manage: &ManageTagsWindow) -> Value {
    serde_json::json!(
        manage
            .get_tags()
            .iter()
            .map(|row| row.text.to_string())
            .filter(|t| t.starts_with("v:"))
            .collect::<Vec<_>>()
    )
}

fn step<'a>(recorded: &'a Value, name: &str) -> &'a Value {
    recorded["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["step"] == name)
        .unwrap()
}

fn viewer_dialog(o: &Opened) -> (hydrus_gui::MediaViewerWindow, ManageTagsWindow) {
    o.ui.invoke_thumbnail_activated(0);
    let viewer = o.bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_manage_tags();
    let manage = o
        .bound
        .manage_tags
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let mine = manage
        .get_service_names()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    manage.invoke_service_chosen(i32::try_from(mine).unwrap());
    (viewer, manage)
}

fn enter(manage: &ManageTagsWindow, tag: &str) {
    manage.set_text(tag.into());
    manage.invoke_text_edited(tag.into());
    manage.invoke_entered();
}

fn press(window: &ManageTagsWindow, key: slint::platform::Key) {
    let text: slint::SharedString = key.into();
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
}

// leaf: audit-media-tags-missing-viewer-follow
#[test]
fn dialog_from_the_viewer_replays_the_recorded_immediate_commit_follow_and_page_keys() {
    use slint::platform::Key;
    let recorded = hydrus_testkit::fixture_json("manage_tags_viewer.json");
    let keys = hydrus_testkit::fixture_json("manage_tags_keys.json");
    let _windows = headless::init();
    let o = open(&recorded);
    set_allow_remove(&o.store, false);
    let (viewer, manage) = viewer_dialog(&o);
    assert!(manage.get_immediate());
    let check = |name: &str, manage: &ManageTagsWindow| {
        let expected = step(&recorded, name);
        assert_eq!(shown(manage), expected["rows"], "{name}: rows");
        assert_eq!(
            stored(&o.store, &o.files),
            expected["stored"],
            "{name}: store"
        );
    };
    check("opened_on_first", &manage);

    enter(&manage, "v:new");
    check("typed_add_commits_at_once", &manage);
    let hover: Vec<String> = viewer
        .get_tags()
        .iter()
        .map(|r| r.text.to_string())
        .collect();
    assert!(hover.contains(&"v:new".to_owned()), "{hover:?}");

    viewer.invoke_next();
    check("follows_to_second", &manage);
    enter(&manage, "v:shared");
    check("typed_existing_tag_does_nothing_by_default", &manage);
    set_allow_remove(&o.store, true);
    enter(&manage, "v:shared");
    check("typed_existing_tag_removes_when_allowed", &manage);

    // remove: asks (the recorded question), writes only on yes
    let row = manage
        .get_tags()
        .iter()
        .position(|r| r.text.starts_with("v:b"))
        .unwrap();
    manage.invoke_tag_clicked(i32::try_from(row).unwrap(), false, false);
    manage.invoke_remove_pressed();
    assert_eq!(
        manage.get_tag_menu_question().as_str(),
        step(&recorded, "remove_confirms_then_commits")["asked"][0]
            .as_str()
            .unwrap()
    );
    assert_eq!(stored(&o.store, &o.files)[1], serde_json::json!(["v:b"]));
    manage.invoke_tag_menu_answered(true);
    check("remove_confirms_then_commits", &manage);

    // PageUp / PageDown in an empty input move the viewer, as the recorded
    // media_previous / media_next do; they do nothing with text typed
    manage.invoke_focus_input();
    assert!(manage.get_input_focused());
    for r in keys["results"].as_array().unwrap() {
        if r["command"].as_str().unwrap().starts_with("media_") {
            assert_eq!(
                r["matched"],
                r["input"] == "" && r["list_filled"] == false,
                "(the conditions recorded)"
            );
        }
    }
    manage.set_text("blu".into());
    manage.invoke_text_edited("blu".into());
    press(&manage, Key::PageDown);
    check("remove_confirms_then_commits", &manage);
    manage.set_text("".into());
    manage.invoke_text_edited("".into());
    press(&manage, Key::PageDown);
    check("follows_to_third", &manage);
    press(&manage, Key::PageUp);
    assert_eq!(
        shown(&manage),
        step(&recorded, "remove_confirms_then_commits")["rows"]
    );
    press(&manage, Key::PageDown);
    check("follows_to_third", &manage);

    // closing asks nothing and writes nothing more
    let before = stored(&o.store, &o.files);
    manage.invoke_cancel();
    assert!(o.bound.manage_tags.borrow().is_none());
    viewer.invoke_previous();
    assert_eq!(stored(&o.store, &o.files), before);
    assert_eq!(
        step(&recorded, "ok_to_cancel_asks_nothing")["asked"],
        serde_json::json!([])
    );
}

#[test]
fn a_question_about_the_file_left_behind_is_dropped_when_the_viewer_moves() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_viewer.json");
    let _windows = headless::init();
    let o = open(&recorded);
    set_allow_remove(&o.store, true);
    let (viewer, manage) = viewer_dialog(&o);
    let before = stored(&o.store, &o.files);
    manage.invoke_remove_pressed();
    assert!(manage.get_tag_menu_question().starts_with("Are you sure"));
    viewer.invoke_next();
    assert!(
        manage.get_tag_menu_question().is_empty(),
        "the question went with the file"
    );
    manage.invoke_tag_menu_answered(true);
    assert_eq!(
        stored(&o.store, &o.files),
        before,
        "nothing was removed from either file"
    );
}

#[test]
fn a_dialog_opened_from_a_page_does_not_follow_the_viewer() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_viewer.json");
    let _windows = headless::init();
    let o = open(&recorded);
    o.ui.invoke_select_all();
    o.ui.invoke_thumbnail_clicked(0, false, false);
    o.ui.invoke_manage_tags_selected();
    let manage = o
        .bound
        .manage_tags
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(!manage.get_immediate());
    let rows = shown(&manage);
    o.ui.invoke_thumbnail_activated(1);
    let viewer = o.bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_next();
    assert_eq!(shown(&manage), rows);
}
