//! Saved neighbourhood policy reaches actual viewer/archive cache consumers without SetMedia.
use hydrus_core::HashId;
use hydrus_gui::{Bound, MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_store::{
    Store,
    image_cache::Policy,
    settings,
    viewer_prefetch::{self, Preferences},
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
const PREVIOUS: &str = "Num previous to prefetch in Media Viewer:";
const NEXT: &str = "Num next to prefetch in Media Viewer:";
const PERCENT: &str = "Maximum % of cache that will be prefetched per media viewer:";
fn row(window: &OptionsWindow, label: &str) -> i32 {
    i32::try_from(
        window
            .get_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap(),
    )
    .unwrap()
}
fn options(ui: &MainWindow, bound: &Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let at = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|row| row.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let child = bound.options.borrow().as_ref().unwrap().clone_strong();
    let at = child
        .get_pages()
        .iter()
        .position(|page| page.text == "speed and memory")
        .unwrap();
    child.invoke_page_chosen(i32::try_from(at).unwrap());
    child
}
fn pump_until(done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(
            Instant::now() < deadline,
            "owned warm worker did not finish expected cache admission"
        );
        slint::platform::update_timers_and_animations();
        std::thread::yield_now();
    }
}
fn import(store: &Arc<Store>, name: &str) -> HashId {
    let result = FileImporter::new(store.clone(), hydrus_media::MediaTools::new())
        .import_path(
            &hydrus_testkit::fixture_path(format!("image_cache/{name}")),
            &FileImportOptions::default(),
        )
        .unwrap();
    store
        .read(|conn| hydrus_store::master::hash_id(conn, &result.hash.unwrap()))
        .unwrap()
        .unwrap()
}
#[test]
fn options_cancel_hidden_modal_save_reopen_and_live_hidden_viewer_warming_leave_current_pixels_unchanged()
 {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let imported = [
        import(&store, "a.png"),
        import(&store, "b.png"),
        import(&store, "c.png"),
    ];
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Policy {
                    bytes: 400,
                    timeout: 300,
                    percentage: 50,
                },
            )?;
            settings::set(
                ctx.conn(),
                &Preferences {
                    previous: 0,
                    next: 0,
                    percentage: 50,
                    ..Preferences::default()
                },
            )?;
            let mut layout = hydrus_store::page_layout::load(ctx.conn())?;
            layout.hide_preview = true;
            settings::set(ctx.conn(), &layout)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let adapter = windows.get(0).unwrap();
    headless::render(&adapter, 1400, 1000);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    assert_eq!(files.len(), 3);
    assert!(imported.iter().all(|file| files.contains(file)));
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(viewer.get_media().size().width, 10);
    assert_eq!(bound.image_cache.keys(), vec![files[0]]);
    let before = viewer.get_media();
    let before_pixels = before.to_rgba8().unwrap().as_bytes().to_vec();
    let cancelled = options(&ui, &bound);
    cancelled.invoke_number_edited(row(&cancelled, NEXT), 1);
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert_eq!(store.read(viewer_prefetch::load).unwrap().next, 0);
    let hidden = options(&ui, &bound);
    hidden.hide().unwrap();
    hidden.invoke_number_edited(row(&hidden, NEXT), 2);
    hidden.invoke_number_edited(row(&hidden, PREVIOUS), 1);
    hidden.invoke_number_edited(row(&hidden, PERCENT), 10);
    hidden.show().unwrap();
    hidden.invoke_apply();
    assert_eq!(
        store.read(viewer_prefetch::load).unwrap(),
        Preferences {
            previous: 0,
            next: 0,
            percentage: 50,
            ..Preferences::default()
        }
    );
    let modal = options(&ui, &bound);
    let suggested = modal
        .get_pages()
        .iter()
        .position(|page| page.text == "tag suggestions")
        .unwrap();
    modal.invoke_page_chosen(i32::try_from(suggested).unwrap());
    modal.invoke_related_weights_clicked();
    let weights = bound
        .options_suggested_tags_slot
        .weights
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let speed = modal
        .get_pages()
        .iter()
        .position(|page| page.text == "speed and memory")
        .unwrap();
    modal.invoke_page_chosen(i32::try_from(speed).unwrap());
    modal.invoke_number_edited(row(&modal, NEXT), 2);
    modal.invoke_number_edited(row(&modal, PREVIOUS), 2);
    modal.invoke_number_edited(row(&modal, PERCENT), 10);
    weights.invoke_action("cancel".into());
    modal.invoke_apply();
    assert_eq!(store.read(viewer_prefetch::load).unwrap().next, 0);
    let saved = options(&ui, &bound);
    saved.invoke_number_edited(row(&saved, NEXT), 1);
    saved.invoke_apply();
    assert_eq!(store.read(viewer_prefetch::load).unwrap().next, 1);
    viewer.hide().unwrap();
    pump_until(|| bound.image_cache.keys().contains(&files[1]));
    assert!(
        !viewer.window().is_visible(),
        "Qt allows admitted warm work while hidden"
    );
    viewer.show().unwrap();
    assert_eq!(
        viewer.get_media().to_rgba8().unwrap().as_bytes(),
        before_pixels.as_slice(),
        "warm readiness and policy saves never SetMedia"
    );
    let reopened = options(&ui, &bound);
    assert_eq!(
        reopened
            .get_rows()
            .row_data(row(&reopened, NEXT) as usize)
            .unwrap()
            .number,
        1
    );
    reopened.invoke_number_edited(row(&reopened, PREVIOUS), 1);
    reopened.invoke_apply();
    viewer.invoke_next();
    pump_until(|| bound.image_cache.keys().contains(&files[2]));
    assert!(
        files
            .iter()
            .all(|file| bound.image_cache.keys().contains(file))
    );
    viewer.invoke_close_requested();
    assert!(bound.viewer.borrow().is_none());
    viewer.show().unwrap();
    viewer.invoke_next();
    let cache = bound.image_cache.clone();
    let clone = bound.clone();
    drop(bound);
    assert!(!cache.keys().is_empty());
    drop(clone);
    assert!(cache.keys().is_empty());
    viewer.invoke_next();
    assert!(
        cache.keys().is_empty(),
        "retained/re-shown viewer cannot revive final binding cache admission"
    );
    ui.hide().unwrap();
    let ui2 = MainWindow::new().unwrap();
    ui2.show().unwrap();
    let successor = bind(
        &ui2,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let stale = saved;
    saved_policy_unchanged(&store, &stale);
    let visible = options(&ui2, &successor);
    let native = windows.get(windows.count() - 1).unwrap();
    let pixels = headless::render(&native, 1100, 650);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("viewer-prefetch-options.png"),
        &pixels,
        1100,
        650,
    )
    .unwrap();
    visible.invoke_cancel();
    successor.preview.close();
}
fn saved_policy_unchanged(store: &Store, stale: &OptionsWindow) {
    let before = store.read(viewer_prefetch::load).unwrap();
    stale.show().unwrap();
    stale.invoke_number_edited(row(stale, NEXT), 50);
    stale.invoke_apply();
    assert_eq!(store.read(viewer_prefetch::load).unwrap(), before);
}
#[test]
fn archive_delete_uses_saved_neighbour_counts_and_retains_current_media_after_warm_readiness() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let imported = [
        import(&store, "a.png"),
        import(&store, "b.png"),
        import(&store, "c.png"),
    ];
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Policy {
                    bytes: 400,
                    timeout: 300,
                    percentage: 50,
                },
            )?;
            settings::set(
                ctx.conn(),
                &Preferences {
                    previous: 0,
                    next: 1,
                    percentage: 50,
                    ..Preferences::default()
                },
            )?;
            let mut layout = hydrus_store::page_layout::load(ctx.conn())?;
            layout.hide_preview = true;
            settings::set(ctx.conn(), &layout)
        })
        .unwrap();
    let _headless_windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    assert_eq!(files.len(), 3);
    assert!(imported.iter().all(|file| files.contains(file)));
    ui.invoke_archive_delete_filter();
    let archive = bound
        .archive_delete
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let current = archive.get_media().to_rgba8().unwrap().as_bytes().to_vec();
    assert_eq!(archive.get_media().size().width, 10);
    pump_until(|| bound.image_cache.keys().contains(&files[1]));
    assert_eq!(
        archive.get_media().to_rgba8().unwrap().as_bytes(),
        current.as_slice()
    );
    archive.invoke_close_requested();
    assert!(bound.archive_delete.borrow().is_none());
    let cached = bound.image_cache.keys();
    archive.show().unwrap();
    archive.invoke_skip();
    assert_eq!(bound.image_cache.keys(), cached);
    bound.preview.close();
}
#[test]
fn actual_duplicate_filter_uses_the_same_saved_percentage_for_future_images_without_replacing_current()
 {
    use hydrus_core::{
        duplicates::DuplicatesSearch,
        pages::{DuplicatesPage, Page, PageContent, PageKey, Session},
    };
    use hydrus_search::{FileSearchContext, LocationContext};
    use hydrus_store::{
        duplicates::{PairSearchKind, PixelDuplicates},
        sessions::{self, LAST_SESSION},
    };
    let (_dir, store) = super::duplicate_filter::store_with_pairs();
    let (_, key) = super::duplicate_filter::my_files(&store);
    let search = FileSearchContext {
        location: LocationContext::single(key),
        ..Default::default()
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates: DuplicatesPage::new(DuplicatesSearch {
                    search_1: search.clone(),
                    search_2: search,
                    kind: PairSearchKind::OneFileMatchesOneSearch,
                    pixel_duplicates: PixelDuplicates::Allowed,
                    max_hamming_distance: 4,
                }),
                sort: None,
            },
        }],
    };
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 0)?;
            settings::set(
                ctx.conn(),
                &Preferences {
                    percentage: 50,
                    ..Preferences::default()
                },
            )
        })
        .unwrap();
    let _headless_windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_launch_filter();
    let filter = bound.filter.borrow().as_ref().unwrap().clone_strong();
    let current = filter.get_media().to_rgba8().unwrap().as_bytes().to_vec();
    assert!(!current.is_empty());
    pump_until(|| bound.image_cache.keys().len() > 2);
    assert_eq!(
        filter.get_media().to_rgba8().unwrap().as_bytes(),
        current.as_slice()
    );
    let cache = bound.image_cache.clone();
    drop(bound);
    assert!(cache.keys().is_empty());
    filter.show().unwrap();
    filter.invoke_switch_media();
    assert!(
        cache.keys().is_empty(),
        "retained duplicate owner cannot admit through the retired binding cache"
    );
    filter.invoke_close_requested();
}
