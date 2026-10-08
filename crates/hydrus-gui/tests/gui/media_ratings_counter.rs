//! An inc/dec counter in "manage > ratings": a left click adds one, a right
//! click takes one away, and Apply writes the file's count
//! (`ClientGUIRatings.RatingIncDecDialog`, `DialogManageRatings`).

use hydrus_store::media::Rating;
use slint::Model as _;

use super::media_support::*;

// leaf: audit-media-ratings-incdec
#[test]
fn left_and_right_clicks_step_a_counter_and_apply_writes_it() {
    let fixture = start();
    let results = fixture.results();
    let file = results[0];
    let counter = fixture
        .store
        .snapshot()
        .services
        .by_name("counter")
        .unwrap()
        .id;
    fixture
        .store
        .write_content(move |w| w.set_incdec(counter, &[file], 3))
        .unwrap();
    let read = || {
        let services = fixture.store.snapshot().services.clone();
        fixture
            .store
            .read(|c| hydrus_store::media::load(c, &services, None, &[file]))
            .unwrap()
            .results
            .remove(0)
            .ratings
            .get(&counter)
            .copied()
    };
    assert_eq!(read(), Some(Rating::IncDec(3)));
    fixture.select(0, &[]);
    let menu = fixture.menu(0);
    fixture
        .ui
        .invoke_menu_chosen(find(&rows(&menu.manage), "ratings"));
    let dialog = fixture
        .bound
        .manage_ratings
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the dialog opens");
    let row = dialog
        .get_names()
        .iter()
        .position(|n| n == "counter")
        .unwrap();
    let shown = || dialog.get_ratings().row_data(row).unwrap().text.to_string();
    assert_eq!(shown(), "3");
    // left: one more; right: one fewer; nothing is written until Apply
    dialog.invoke_rating_clicked(i32::try_from(row).unwrap(), true, 0.0);
    dialog.invoke_rating_clicked(i32::try_from(row).unwrap(), true, 0.0);
    assert_eq!(shown(), "5");
    for _ in 0..3 {
        dialog.invoke_rating_clicked(i32::try_from(row).unwrap(), false, 0.0);
    }
    assert_eq!(shown(), "2");
    assert_eq!(read(), Some(Rating::IncDec(3)));
    dialog.invoke_apply();
    assert!(fixture.bound.manage_ratings.borrow().is_none());
    assert_eq!(read(), Some(Rating::IncDec(2)));
    // cancelled, a count is left as it was
    fixture.select(0, &[]);
    let menu = fixture.menu(0);
    fixture
        .ui
        .invoke_menu_chosen(find(&rows(&menu.manage), "ratings"));
    let dialog = fixture
        .bound
        .manage_ratings
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    dialog.invoke_rating_clicked(i32::try_from(row).unwrap(), true, 0.0);
    dialog.invoke_cancel();
    assert_eq!(read(), Some(Rating::IncDec(2)));
}
