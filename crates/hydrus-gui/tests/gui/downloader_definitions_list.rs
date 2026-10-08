//! The URL class list's actions (`EditURLClassesPanel`): add (a "new url
//! class"), duplicate (a new key and an unused name), the confirmed delete
//! ("Remove all selected?"), the list kept sorted by name, and a cancel
//! after changes asking first and discarding the draft.
use hydrus_core::url::{UrlClass, UrlClassSettings, UrlType};
use hydrus_gui::downloader_definitions_window::{self as windows, Slots};
use hydrus_gui::headless;
use hydrus_store::{Store, settings};
use slint::{ComponentHandle as _, Model as _};

fn names(list: &hydrus_gui::DownloaderDefinitionsWindow) -> Vec<String> {
    list.get_rows()
        .iter()
        .map(|r| r.cells.row_data(0).unwrap().to_string())
        .collect()
}

// leaf: audit-network-definitions-list-actions
#[test]
fn url_class_list_adds_duplicates_deletes_and_cancels_as_the_reference_does() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let class = |name: &str, key: u8| UrlClass {
        name: name.into(),
        key: vec![key],
        url_type: UrlType::Post,
        ..hydrus_gui_model::downloader_definitions::new_class()
    };
    let original = UrlClassSettings {
        url_classes: vec![class("zebra", 1), class("apple", 2)],
        ..UrlClassSettings::default()
    };
    {
        let original = original.clone();
        store
            .write_and_refresh(move |ctx| settings::set(ctx.conn(), &original))
            .unwrap();
    }
    let _windows = headless::init();
    let slots = Slots::default();
    let list = windows::open(&store, &slots, true).unwrap();
    assert_eq!(names(&list), ["apple", "zebra"], "sorted by name");

    // add: the reference's "new url class", sorted into the list
    list.invoke_action("add".into());
    let edit = slots.class_edit.borrow().as_ref().unwrap().clone_strong();
    assert!(names(&list).len() == 2, "not in the list until accepted");
    edit.invoke_action("apply".into());
    assert!(slots.class_edit.borrow().is_none(), "{}", edit.get_error());
    assert_eq!(names(&list), ["apple", "new url class", "zebra"]);

    // duplicate: same rules, a name not in use, a key of its own
    list.invoke_row_clicked(2, false, false);
    list.invoke_action("duplicate".into());
    let edit = slots.class_edit.borrow().as_ref().unwrap().clone_strong();
    edit.invoke_action("apply".into());
    assert!(slots.class_edit.borrow().is_none(), "{}", edit.get_error());
    let shown = names(&list);
    assert_eq!(shown.len(), 4);
    let mut unique = shown.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 4, "{shown:?}");
    assert!(shown.windows(2).all(|w| w[0] <= w[1]), "sorted: {shown:?}");

    // delete asks first; no leaves the list alone
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("delete".into());
    assert_eq!(list.get_question(), "Remove all selected?");
    list.invoke_answered(false);
    assert_eq!(names(&list).len(), 4);
    list.invoke_action("delete".into());
    list.invoke_answered(true);
    assert_eq!(names(&list).len(), 3);
    assert!(!names(&list).contains(&"apple".to_string()));

    // nothing is stored before Apply, and a cancel after changes asks first
    assert_eq!(
        store.read::<UrlClassSettings>(settings::get).unwrap(),
        original
    );
    list.invoke_action("cancel".into());
    assert_eq!(
        list.get_question(),
        "You have made changes. Sure you are ok to cancel?"
    );
    list.invoke_answered(true);
    assert!(slots.classes.borrow().is_none());
    assert_eq!(
        store.read::<UrlClassSettings>(settings::get).unwrap(),
        original
    );

    // Apply stores the duplicate with a key of its own
    let list = windows::open(&store, &slots, true).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("duplicate".into());
    let edit = slots.class_edit.borrow().as_ref().unwrap().clone_strong();
    edit.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let saved: UrlClassSettings = store.read(settings::get).unwrap();
    assert_eq!(saved.url_classes.len(), 3);
    let mut keys: Vec<_> = saved.url_classes.iter().map(|c| c.key.clone()).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 3, "the duplicate has a key of its own");
}
