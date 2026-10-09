//! Native manage-tags defaults and immediate tab memory coexist with cancelled tag drafts.
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{Store, settings, tag_editing::TagEditingSettings};
use slint::{ComponentHandle as _, Model as _};

fn selected(w: &hydrus_gui::ManageTagsWindow) -> String {
    w.get_service_names()
        .row_data(usize::try_from(w.get_service_index()).unwrap())
        .unwrap()
        .to_string()
}
fn preferences(store: &Store) -> (bool, String) {
    let p: TagEditingSettings = store.read(settings::get).unwrap();
    (
        p.remember_service,
        store
            .snapshot()
            .services
            .by_key(&p.default_service)
            .unwrap()
            .name
            .clone(),
    )
}

// leaf: audit-options-tag-editing-tag-dialogs-remember-last-used-default-tag-service-in-manage-tag-dialogs
#[test]
fn remembered_tag_service_survives_native_cancel_without_saving_staged_tags() {
    let f = hydrus_testkit::fixture_json("tag_dialog_defaults.json");
    let (_dirs, store) = crate::subscriptions::store();
    let initial: TagEditingSettings = store.read(settings::get).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_select_all();
    let selected_count = bound.current.borrow().borrow().selected_files().len();
    ui.invoke_manage_tags_selected();
    let w = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        selected(&w),
        f["initial_preferences"]["service"].as_str().unwrap()
    );
    let downloader = w
        .get_service_names()
        .iter()
        .position(|n| n == "downloader tags")
        .unwrap();
    w.invoke_service_chosen(i32::try_from(downloader).unwrap());
    assert_eq!(preferences(&store), (true, "downloader tags".into()));
    w.invoke_text_edited("cancelled default preference".into());
    w.invoke_entered();
    assert!(
        w.get_tags()
            .iter()
            .any(|r| r.text == format!("cancelled default preference ({selected_count})"))
    );
    w.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(bound.manage_tags.borrow().is_none());
    let stale = w;
    ui.invoke_manage_tags_selected();
    let w = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let stale_mine = stale
        .get_service_names()
        .iter()
        .position(|n| n == "my tags")
        .unwrap();
    stale.invoke_service_chosen(i32::try_from(stale_mine).unwrap());
    stale.invoke_apply();
    stale.invoke_cancel();
    assert!(bound.manage_tags.borrow().is_some());
    assert!(w.window().is_visible());
    assert_eq!(preferences(&store), (true, "downloader tags".into()));
    assert_eq!(selected(&w), "downloader tags");
    assert!(
        !w.get_tags()
            .iter()
            .any(|r| r.text.starts_with("cancelled default preference ("))
    );
    // Applying the option while this consumer is open must stop preference memory.
    store
        .write(move |ctx| {
            let mut options: TagEditingSettings = settings::get(ctx.conn())?;
            options.remember_service = false;
            settings::set(ctx.conn(), &options)
        })
        .unwrap();
    let mine = w
        .get_service_names()
        .iter()
        .position(|n| n == "my tags")
        .unwrap();
    w.invoke_service_chosen(i32::try_from(mine).unwrap());
    assert_eq!(selected(&w), "my tags");
    assert_eq!(preferences(&store), (false, "downloader tags".into()));
    w.invoke_cancel();
    ui.invoke_manage_tags_selected();
    let w = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(selected(&w), "downloader tags");
    w.invoke_cancel();
    // A configured fixed service is consumed when the next dialog opens.
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &TagEditingSettings {
                    remember_service: false,
                    ..initial
                },
            )
        })
        .unwrap();
    ui.invoke_manage_tags_selected();
    let w = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(selected(&w), "my tags");
    w.invoke_cancel();
}
