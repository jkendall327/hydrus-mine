//! Files' URLs, managed from the thumbnails' "urls > manage" (the
//! reference's `EditURLsPanel`): URLs typed, pasted (asking before ones
//! that don't parse), removed and edited are written to the files when
//! applied, asking first with text left in the box. The dialog's steps are
//! tested against the reference's in hydrus-gui-model
//! (`oracle/record_manage_urls.py`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle, Model as _};

use hydrus_core::HashId;
use hydrus_gui::{Bound, Clip, MainWindow, ManageUrlsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn urls(store: &Store, file: HashId) -> Vec<String> {
    let services = store.snapshot().services.clone();
    let mut urls = store
        .read(|c| hydrus_store::media::load(c, &services, None, &[file]))
        .unwrap()
        .results
        .remove(0)
        .urls;
    urls.sort();
    urls
}

fn rows(dialog: &ManageUrlsWindow) -> Vec<String> {
    let rows = dialog.get_rows();
    (0..rows.row_count())
        .map(|i| {
            rows.row_data(i)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect()
}

fn selected(dialog: &ManageUrlsWindow) -> Vec<bool> {
    let rows = dialog.get_rows();
    (0..rows.row_count())
        .map(|i| rows.row_data(i).unwrap().selected)
        .collect()
}

/// "urls > manage" on the thumbnails selected.
fn open(ui: &MainWindow, bound: &Bound, index: i32) -> ManageUrlsWindow {
    ui.invoke_thumbnail_menu_requested(index);
    let menu = ui.get_thumbnail_menu();
    assert!(menu.has_urls);
    ui.invoke_menu_chosen(menu.urls_manage);
    bound
        .manage_urls
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .expect("the dialog opens")
}

fn enter(dialog: &ManageUrlsWindow, text: &str) {
    dialog.set_input(text.into());
    dialog.invoke_entered();
}

#[test]
fn urls_are_added_removed_and_edited_as_the_reference_does() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    let pasted: Rc<RefCell<String>> = Rc::default();
    hydrus_gui::set_paster({
        let pasted = pasted.clone();
        move || pasted.borrow().clone()
    });
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let results = bound.current.borrow().borrow().results().to_vec();
    // a file with URLs, and another
    let a = results
        .iter()
        .position(|&f| !urls(&store, f).is_empty())
        .unwrap();
    let b = usize::from(a == 0);
    let (file_a, file_b) = (results[a], results[b]);
    let (before_a, before_b) = (urls(&store, file_a), urls(&store, file_b));

    // one file: its URLs, no warning
    ui.invoke_thumbnail_clicked(i32::try_from(a).unwrap(), false, false);
    let dialog = open(&ui, &bound, i32::try_from(a).unwrap());
    assert_eq!(dialog.get_window_title(), "manage urls for 1 files");
    assert_eq!(dialog.get_warning(), "");
    assert_eq!(rows(&dialog), before_a);
    // copy with none selected: all of them
    dialog.invoke_copy();
    assert_eq!(copied.borrow().last().unwrap(), &before_a.join("\n"));
    dialog.invoke_cancel();
    assert!(bound.manage_urls.borrow().is_none());

    // both: the warning, and counts
    ui.invoke_thumbnail_clicked(i32::try_from(b).unwrap(), true, false);
    let dialog = open(&ui, &bound, i32::try_from(a).unwrap());
    assert_eq!(dialog.get_window_title(), "manage urls for 2 files");
    assert!(
        dialog
            .get_warning()
            .starts_with("Warning: you are editing urls for multiple files!")
    );
    let first = &before_a[0];
    let in_both = before_b.contains(first);
    assert_eq!(
        rows(&dialog)[0],
        format!("{first} ({})", if in_both { 2 } else { 1 })
    );
    // typed: normalised, added to both
    enter(&dialog, "https://example.com/a b#frag");
    assert!(rows(&dialog).contains(&"https://example.com/a%20b (2)".to_owned()));
    assert_eq!(dialog.get_input(), "");
    // pasted: what doesn't parse asks first; "no" adds nothing
    *pasted.borrow_mut() = "not a url\n\nhttps://z.org/x\n".into();
    dialog.invoke_paste();
    assert!(dialog.get_asking());
    assert!(
        dialog
            .get_asking_message()
            .starts_with("The URLs:\n\nnot a url\n\n--did not parse.")
    );
    dialog.invoke_chosen(1);
    assert!(!rows(&dialog).iter().any(|r| r.contains("z.org")));
    // "yes" adds both
    dialog.invoke_paste();
    dialog.invoke_chosen(0);
    let shown = rows(&dialog);
    assert!(
        shown.contains(&"https://z.org/x (2)".to_owned()),
        "{shown:?}"
    );
    assert!(shown.contains(&"not a url (2)".to_owned()));
    // selected and deleted: from both
    let row = shown.iter().position(|r| r == "not a url (2)").unwrap();
    dialog.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
    assert!(selected(&dialog)[row]);
    dialog.invoke_copy();
    assert_eq!(copied.borrow().last().unwrap(), "not a url");
    dialog.invoke_delete_pressed();
    assert!(!rows(&dialog).contains(&"not a url (2)".to_owned()));
    // double-clicked: out of the list, into the box
    let row = rows(&dialog)
        .iter()
        .position(|r| r == "https://z.org/x (2)")
        .unwrap();
    dialog.invoke_row_activated(i32::try_from(row).unwrap());
    assert_eq!(dialog.get_input(), "https://z.org/x");
    assert!(!rows(&dialog).iter().any(|r| r.contains("z.org")));
    // applying with text in the box asks; "no" keeps the dialog
    dialog.invoke_apply();
    assert_eq!(
        dialog.get_asking_message(),
        "You have text still in the input! Sure you are ok to apply?"
    );
    dialog.invoke_chosen(1);
    assert!(bound.manage_urls.borrow().is_some());
    // the original's first removed too, then applied
    let row = rows(&dialog)
        .iter()
        .position(|r| r.starts_with(&format!("{first} (")))
        .unwrap();
    dialog.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
    dialog.invoke_delete_pressed();
    dialog.invoke_apply();
    dialog.invoke_chosen(0);
    assert!(bound.manage_urls.borrow().is_none());
    let expect = |before: &[String]| {
        let mut after: Vec<String> = before.iter().filter(|u| *u != first).cloned().collect();
        after.push("https://example.com/a%20b".to_owned());
        after.sort();
        after.dedup();
        after
    };
    assert_eq!(urls(&store, file_a), expect(&before_a));
    assert_eq!(urls(&store, file_b), expect(&before_b));
}
