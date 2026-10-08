//! Manage Tags opened from the media viewer: every change is written at
//! once, the dialog follows the viewer's file, and PageUp/PageDown in an
//! empty input move the viewer (replaying the recorded
//! `manage_tags_viewer.json` of the reference's `immediate_commit` panel).

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::Tag;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::content::MappingAction;

fn tags_of(store: &hydrus_store::Store, file: hydrus_core::HashId) -> Vec<String> {
    let snapshot = store.snapshot();
    let service = snapshot.services.by_name("my tags").unwrap().id;
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
        .unwrap();
    let mut tags: Vec<String> = batch.results[0]
        .tags
        .get(&service)
        .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
        .into_iter()
        .flatten()
        .map(|id| batch.tags[id].to_string())
        .filter(|t| t.starts_with("view:"))
        .collect();
    tags.sort();
    tags
}

fn shown(manage: &hydrus_gui::ManageTagsWindow) -> Vec<String> {
    manage
        .get_tags()
        .iter()
        .map(|row| row.text.to_string())
        .filter(|t| t.starts_with("view:"))
        .collect()
}

// leaf: audit-media-tags-missing-viewer-follow
#[test]
fn dialog_from_the_viewer_commits_at_once_follows_the_file_and_steps_the_viewer() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_viewer.json");
    let first_step = &recorded["steps"][0];
    assert_eq!(first_step["has_changes"], false);
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let mut source = SearchPage::new(store.clone());
    source.enter();
    let files: Vec<_> = source.results().iter().copied().take(3).collect();
    assert_eq!(files.len(), 3);
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let seeded = files.clone();
    store
        .write_content(move |writer| {
            for (i, file) in seeded.iter().enumerate() {
                let id = hydrus_store::master::intern_tag(
                    writer.conn(),
                    &Tag::new(&format!("view:{i}")).unwrap(),
                )?;
                writer.update_mappings(service, &MappingAction::Add, id, &[*file])?;
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
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_manage_tags();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let mine = manage
        .get_service_names()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    manage.invoke_service_chosen(i32::try_from(mine).unwrap());
    assert!(manage.get_immediate());
    assert_eq!(shown(&manage), ["view:0 (1)"]);

    // an entered tag is written at once; there is nothing to apply
    manage.set_text("view:new".into());
    manage.invoke_text_edited("view:new".into());
    manage.invoke_entered();
    assert!(tags_of(&store, files[0]).contains(&"view:new".to_owned()));
    assert_eq!(shown(&manage), ["view:0 (1)", "view:new (1)"]);
    let hover: Vec<String> = viewer
        .get_tags()
        .iter()
        .map(|r| r.text.to_string())
        .collect();
    assert!(
        hover.contains(&"view:new".to_owned()),
        "the viewer's own tag display shows it: {hover:?}"
    );

    // the viewer moves on and the dialog is about the file it shows
    viewer.invoke_next();
    assert_eq!(shown(&manage), ["view:1 (1)"]);
    // an empty input: PageDown / PageUp are the dialog's, asking the viewer
    manage.invoke_show_next();
    assert_eq!(shown(&manage), ["view:2 (1)"]);
    manage.invoke_show_previous();
    assert_eq!(shown(&manage), ["view:1 (1)"]);
    manage.set_text("view:second".into());
    manage.invoke_text_edited("view:second".into());
    manage.invoke_entered();
    assert_eq!(tags_of(&store, files[1]), ["view:1", "view:second"]);
    assert_eq!(tags_of(&store, files[0]), ["view:0", "view:new"]);

    // closing writes nothing more and drops the dialog's hold on the viewer
    manage.invoke_cancel();
    assert!(bound.manage_tags.borrow().is_none());
    viewer.invoke_next();
    assert_eq!(tags_of(&store, files[2]), ["view:2"]);
}
