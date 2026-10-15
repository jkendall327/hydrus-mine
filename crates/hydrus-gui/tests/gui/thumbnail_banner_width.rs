//! A thumbnail whose bottom-right banner is far wider than the thumbnail:
//! the grid draws it, at several scale factors.

use hydrus_core::Tag;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{Store, import::import_legacy};
use slint::platform::WindowAdapter as _;
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

/// A user's inbox crashed the release build (2026-10-15) in the software
/// renderer, drawing the bottom-right banner of a file whose page tags
/// joined to "...-zoosyn_119-zoosyn_120": the banner's box was not finite.
/// The fonts here measure that text fine, so this does not reproduce the
/// crash; it keeps such a banner drawing now that its box is bounded.
#[test]
fn a_very_long_bottom_banner_is_drawn() {
    let (_dirs, store) = store();
    let file = {
        let mut page = SearchPage::new(store.clone());
        page.add_predicate("system:everything");
        page.results()[0]
    };
    let own = store.snapshot().services.by_name("my tags").unwrap().id;
    let long: Vec<String> = (1..=120).map(|i| format!("zoosyn_{i}")).collect();
    let tag = format!("page:{}", long.join("-"));
    store
        .write_content(move |w| {
            let tag = hydrus_store::master::intern_tag(w.conn(), &Tag::new(&tag).unwrap())?;
            w.update_mappings(
                own,
                &hydrus_store::content::MappingAction::Add,
                tag,
                &[file],
            )?;
            Ok(())
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let native = windows.get(0).unwrap();
    headless::render(&native, 1100, 700);
    let first = || {
        bound
            .rows
            .row_data(0)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
    };
    assert!(
        first().bottom.contains("zoosyn_100"),
        "{:?}",
        first().bottom
    );
    for _ in 0..50 {
        if first().paint.bottom == first().bottom {
            break;
        }
        bound.rows.paint_tick(true, 0, 10);
        bound.rows.wait();
        slint::platform::update_timers_and_animations();
    }
    assert_eq!(first().paint.bottom, first().bottom);
    for scale_factor in [1.0, 1.25, 1.5, 2.0, 3.0] {
        native
            .window()
            .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor });
        let pixels = headless::render(&native, 1920, 1080);
        assert!(pixels.iter().any(|&p| p != 0), "{scale_factor}");
    }
}
