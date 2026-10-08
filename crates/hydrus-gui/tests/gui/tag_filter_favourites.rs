//! The shared editor's real callbacks: clipboard/file exchange, immediate
//! favourites and a detached Apply/Cancel draft, including stale child handles.
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui::{Clip, TagFilterWindow, headless, set_clipper, set_paster, tag_filter_window};
use hydrus_gui_model::tag_filter_editor::{FavouriteTagFilters, import_favourite};
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn choices(w: &TagFilterWindow) -> Vec<String> {
    let names = w.get_asking_choices();
    (0..names.row_count())
        .map(|i| names.row_data(i).unwrap().to_string())
        .collect()
}

// leaf: audit-shared-tag-apply
// leaf: audit-shared-tag-favourite-save
// leaf: audit-shared-tag-favourite-load
// leaf: audit-shared-tag-favourite-delete
// leaf: audit-shared-tag-favourite-export
// leaf: audit-shared-tag-favourite-import
#[test]
fn shared_favourites_exchange_and_cancel_preserve_the_owner_and_persist_on_reopen() {
    let fixture = hydrus_testkit::fixture_json("tag_filter_favourites.json");
    let dirs = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dirs.path()).unwrap());
    let rendered = headless::init();
    let slot = Rc::new(RefCell::new(None));
    let applied = Rc::new(RefCell::new(Vec::<TagFilter>::new()));
    let caller = TagFilter::new().with_rule("goblin", FilterRule::Blacklist);
    let open = || {
        let output = applied.clone();
        let w = tag_filter_window::open(
            &store,
            &caller,
            false,
            "test filter",
            "",
            &slot,
            Rc::new(move |f| output.borrow_mut().push(f)),
        )
        .unwrap();
        *slot.borrow_mut() = Some(w.clone_strong());
        w
    };
    let clip = Rc::new(RefCell::new(Vec::<Clip>::new()));
    set_clipper({
        let clip = clip.clone();
        move |c| clip.borrow_mut().push(c.clone())
    });
    set_paster({
        let payload = fixture["dirty_payload"].as_str().unwrap().to_owned();
        move || payload.clone()
    });
    let w = open();
    w.invoke_favourite("save".into());
    w.invoke_favourite_named("".into());
    assert!(w.get_favourite_naming());
    assert!(FavouriteTagFilters::load(&store).unwrap().0.is_empty());
    w.invoke_favourite_named("zebra".into());
    assert_eq!(
        FavouriteTagFilters::load(&store).unwrap().get("zebra"),
        Some(caller.clone())
    );
    w.invoke_typed(2, "orc".into());
    w.invoke_favourite("save".into());
    w.invoke_favourite_named("zebra".into());
    assert_eq!(
        w.get_asking_message(),
        "\"zebra\" already exists! Overwrite?"
    );
    w.invoke_chosen(1);
    assert_eq!(
        FavouriteTagFilters::load(&store).unwrap().get("zebra"),
        Some(caller.clone())
    );
    w.invoke_favourite("import".into());
    assert!(w.get_favourite_exchange());
    w.invoke_exchange_action("import".into());
    assert!(w.get_favourite_naming());
    w.invoke_cancelled();
    assert_eq!(FavouriteTagFilters::load(&store).unwrap().0.len(), 1);
    assert!(w.get_current().contains("orc"));
    w.invoke_favourite("import".into());
    w.invoke_exchange_action("import".into());
    w.invoke_favourite_named("apple".into());
    let imported = import_favourite(fixture["payload"].as_str().unwrap()).unwrap();
    assert_eq!(
        FavouriteTagFilters::load(&store).unwrap().get("apple"),
        Some(imported.clone())
    );
    w.invoke_favourite("export".into());
    assert_eq!(choices(&w), ["this tag filter", "zebra", "apple"]);
    w.invoke_chosen(0);
    assert_eq!(clip.borrow().len(), 1);
    let Clip::Text(text) = &clip.borrow()[0] else {
        panic!("text export")
    };
    assert_eq!(import_favourite(text).unwrap(), imported);
    let path = dirs.path().join("filter.json");
    w.set_exchange_path(path.to_string_lossy().as_ref().into());
    w.invoke_exchange_action("save".into());
    assert!(w.get_exchange_error().is_empty());
    assert_eq!(
        import_favourite(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        imported
    );
    w.invoke_cancelled();
    w.invoke_favourite("import".into());
    w.set_exchange_text("[44, 1, [[\"invalid\", 9]]]".into());
    w.invoke_exchange_action("import".into());
    assert!(!w.get_exchange_error().is_empty());
    assert_eq!(FavouriteTagFilters::load(&store).unwrap().0.len(), 2);
    w.set_exchange_path(path.to_string_lossy().as_ref().into());
    w.invoke_exchange_action("open".into());
    w.invoke_favourite_named("from file".into());
    assert_eq!(
        FavouriteTagFilters::load(&store).unwrap().get("from file"),
        Some(imported.clone())
    );
    let pixels = headless::render(&rendered.get(0).unwrap(), 960, 900);
    assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
    w.invoke_cancel();
    assert!(applied.borrow().is_empty());
    assert!(!w.window().is_visible());
    w.invoke_favourite("save".into());
    w.invoke_favourite_named("stale".into());
    assert!(
        FavouriteTagFilters::load(&store)
            .unwrap()
            .get("stale")
            .is_none()
    );
    let reopened = open();
    reopened.invoke_favourite("load".into());
    assert_eq!(choices(&reopened), ["zebra", "apple", "from file"]);
    reopened.invoke_chosen(1);
    reopened.invoke_apply();
    assert_eq!(*applied.borrow(), [imported]);
    assert_eq!(
        caller,
        TagFilter::new().with_rule("goblin", FilterRule::Blacklist)
    );
    let deleted = open();
    deleted.invoke_favourite("delete".into());
    deleted.invoke_chosen(1);
    assert_eq!(deleted.get_asking_message(), "Delete \"apple\"?");
    deleted.invoke_chosen(1);
    assert!(
        FavouriteTagFilters::load(&store)
            .unwrap()
            .get("apple")
            .is_some()
    );
    deleted.invoke_favourite("delete".into());
    deleted.invoke_chosen(1);
    deleted.invoke_chosen(0);
    assert!(
        FavouriteTagFilters::load(&store)
            .unwrap()
            .get("apple")
            .is_none()
    );
    deleted.invoke_cancel();
    // Closing the owner while a name is pending invalidates that child too.
    let closing = open();
    closing.invoke_favourite("import".into());
    closing.invoke_exchange_action("import".into());
    assert!(closing.get_favourite_naming());
    let before = FavouriteTagFilters::load(&store).unwrap();
    closing
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(slot.borrow().is_none());
    closing.invoke_favourite_named("after owner closure".into());
    closing.invoke_apply();
    assert_eq!(FavouriteTagFilters::load(&store).unwrap(), before);
    assert_eq!(applied.borrow().len(), 1);
}

// leaf: audit-shared-tag-paste
// leaf: audit-shared-tag-extra-panels
#[test]
fn blacklist_extra_panels_and_all_four_paste_controls_follow_the_recording() {
    let fixture = hydrus_testkit::fixture_json("tag_filter_favourites.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    let _headless_windows = headless::init();
    store
        .write(|ctx| {
            hydrus_store::settings::set(ctx.conn(), &hydrus_store::settings::AdvancedMode(true))
        })
        .unwrap();
    let slot = Rc::new(RefCell::new(None));
    let w = tag_filter_window::open(
        &store,
        &TagFilter::new().with_rule("goblin", FilterRule::Blacklist),
        true,
        "blacklist",
        "",
        &slot,
        Rc::new(|_| {}),
    )
    .unwrap();
    assert!(w.get_show_other_panels());
    w.invoke_show_panels();
    assert!(!w.get_show_other_panels());
    let tabs = w.get_tabs();
    assert_eq!(
        (0..tabs.row_count())
            .map(|i| tabs.row_data(i).unwrap().to_string())
            .collect::<Vec<_>>(),
        ["blacklist", "whitelist", "advanced"]
    );
    w.set_test_input("creator:goblin".into());
    w.invoke_test_edited();
    assert_eq!(
        w.get_test_result().as_str(),
        fixture["extra_panels"][1]["test"].as_str().unwrap()
    );
    w.invoke_cancel();
    for (list, case) in fixture["paste"].as_array().unwrap().iter().enumerate() {
        let payload = case["text"].as_str().unwrap().to_owned();
        set_paster(move || payload.clone());
        let value = Rc::new(RefCell::new(None));
        let result = value.clone();
        let w = tag_filter_window::open(
            &store,
            &TagFilter::new().with_rule(":", FilterRule::Blacklist),
            false,
            "filter",
            "",
            &slot,
            Rc::new(move |f| *result.borrow_mut() = Some(f)),
        )
        .unwrap();
        w.invoke_paste(i32::try_from(list).unwrap());
        w.invoke_apply();
        let filter = value.borrow().clone().unwrap();
        let rules = serde_json::json!(
            filter
                .rules()
                .map(|(s, r)| (s, i32::from(r == FilterRule::Blacklist)))
                .collect::<Vec<_>>()
        );
        assert_eq!(case["rules"], rules, "paste list {list}");
    }
}
