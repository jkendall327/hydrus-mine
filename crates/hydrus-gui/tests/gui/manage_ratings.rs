//! Files' ratings, managed from the thumbnails' "manage > ratings" (the
//! reference's `DialogManageRatings`): set by clicking, copied and pasted,
//! and written when applied, only those changed. The dialog's steps are
//! tested against the reference's in hydrus-gui-model
//! (`oracle/record_manage_ratings.py`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle, Model as _};

use hydrus_core::{HashId, ServiceId};
use hydrus_gui::{Bound, Clip, MainWindow, ManageRatingsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::media::Rating;

fn ratings(store: &Store, file: HashId) -> HashMap<ServiceId, Rating> {
    let services = store.snapshot().services.clone();
    store
        .read(|c| hydrus_store::media::load(c, &services, None, &[file]))
        .unwrap()
        .results
        .remove(0)
        .ratings
}

fn names(dialog: &ManageRatingsWindow) -> Vec<String> {
    dialog.get_names().iter().map(|n| n.to_string()).collect()
}

/// "manage > ratings" on the thumbnails selected.
fn open(ui: &MainWindow, bound: &Bound, index: i32) -> ManageRatingsWindow {
    ui.invoke_thumbnail_menu_requested(index);
    let manage = ui.get_thumbnail_menu().manage;
    let id = (0..manage.row_count())
        .map(|i| manage.row_data(i).unwrap())
        .find(|r| r.label == "ratings")
        .expect("manage > ratings")
        .id;
    ui.invoke_menu_chosen(id);
    bound
        .manage_ratings
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .expect("the dialog opens")
}

// leaf: audit-media-ratings-like, audit-media-ratings-numerical, audit-media-ratings-clipboard
#[test]
fn ratings_are_set_copied_pasted_and_applied_as_the_reference_does() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let windows = headless::init();
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
    let services = store.snapshot().services.clone();
    let id = |name: &str| services.by_name(name).unwrap().id;
    let key = |name: &str| services.by_name(name).unwrap().key.to_hex();
    let (favourites, stars, counter) = (id("favourites"), id("stars"), id("counter"));
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let results = bound.current.borrow().borrow().results().to_vec();
    let (a, b) = (results[0], results[1]);
    // the two rated alike to start
    store
        .write_content(move |w| {
            w.set_rating(favourites, &[a, b], Some(1.0))?;
            w.set_rating(stars, &[a, b], None)?;
            w.set_incdec(counter, &[a], 3)?;
            w.set_incdec(counter, &[b], 3)
        })
        .unwrap();

    ui.invoke_thumbnail_clicked(0, false, false);
    ui.invoke_thumbnail_clicked(1, true, false);
    let dialog = open(&ui, &bound, 0);
    assert_eq!(dialog.get_window_title(), "manage ratings for 2 files");
    assert_eq!(names(&dialog), ["favourites", "stars", "counter"]);
    // copied: each, as the reference's JSON
    dialog.invoke_copy();
    assert_eq!(
        copied.borrow().last().unwrap(),
        &format!(
            "[[\"{}\", 1], [\"{}\", null], [\"{}\", 3]]",
            key("favourites"),
            key("stars"),
            key("counter")
        )
    );
    assert_eq!(dialog.get_notice(), "Copied 3 ratings!");
    // like clicked off; four of five stars; one more
    dialog.invoke_rating_clicked(0, true, 0.5);
    dialog.invoke_rating_clicked(1, true, 0.8);
    dialog.invoke_rating_clicked(2, true, 0.0);
    let row = |i: usize| dialog.get_ratings().row_data(i).unwrap();
    assert_eq!(row(1).shapes.row_count(), 5);
    assert_eq!(row(2).text, "4");
    // what can't be read says so, and changes nothing
    *pasted.borrow_mut() = "not json".into();
    dialog.invoke_paste();
    assert!(dialog.get_asking());
    assert!(dialog.get_asking_message().starts_with(
        "Sorry, I could not understand what was in the clipboard. I was expecting \"JSON pairs of service keys and rating values\""
    ));
    dialog.invoke_chosen(0);
    assert!(!dialog.get_asking());
    // pasted: the counter back to 3, so it isn't written
    *pasted.borrow_mut() = format!("[[\"{}\", 3]]", key("counter"));
    dialog.invoke_paste();
    assert_eq!(dialog.get_notice(), "Pasted 1 ratings!");
    dialog.invoke_apply();
    assert!(bound.manage_ratings.borrow().is_none());
    for file in [a, b] {
        let rated = ratings(&store, file);
        assert_eq!(rated.get(&favourites), None);
        assert_eq!(rated.get(&stars), Some(&Rating::Fraction(0.8)));
        assert_eq!(rated.get(&counter), Some(&Rating::IncDec(3)));
    }

    // differing: mixed, and left alone unless set
    store
        .write_content(move |w| w.set_incdec(counter, &[b], 7))
        .unwrap();
    let dialog = open(&ui, &bound, 0);
    dialog.invoke_copy();
    assert_eq!(dialog.get_notice(), "Copied 2 ratings!");
    // (the newest window, drawn)
    let last = (0..100)
        .take_while(|&n| windows.get(n).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 320, 200);
    let shot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("manage_ratings.png");
    headless::save_png(&shot, &pixels, 320, 200).unwrap();
    dialog.invoke_rating_clicked(1, false, 0.0);
    dialog.invoke_apply();
    for (file, count) in [(a, 3), (b, 7)] {
        let rated = ratings(&store, file);
        assert_eq!(rated.get(&stars), None);
        assert_eq!(rated.get(&counter), Some(&Rating::IncDec(count)));
    }
}
