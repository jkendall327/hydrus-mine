//! Owned per-page accepted previews, bounded snapshots and terminal lifetimes.
use hydrus_core::{CanvasType, HashId, Sha256};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::{Store, page_layout, settings};
use slint::ComponentHandle as _;
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};
// The last decoder Arc belongs to its in-flight Request after set_decoder
// replaces the owner copy. Drop occurs after Workers publishes that reply.
struct Published(crossbeam_channel::Sender<()>);
impl Drop for Published {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}
fn files(store: &Store) -> [HashId; 2] {
    let fixture = hydrus_testkit::fixture_json("hidden_page_preview.json");
    std::array::from_fn(|i| {
        let hash: Sha256 = fixture["hashes"][i].as_str().unwrap().parse().unwrap();
        store
            .read(|c| hydrus_store::master::hash_id(c, &hash))
            .unwrap()
            .unwrap()
    })
}
fn preferences(store: &Store, hidden: bool) {
    store
        .write_and_refresh(move |c| {
            let mut options = page_layout::load(c.conn())?;
            options.hide_preview = hidden;
            settings::set(c.conn(), &options)
        })
        .unwrap();
}
fn query(ui: &MainWindow) {
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
}
// A saved same-key page already has Everything. Entering it again removes
// it, as the reference's active-predicate list does. Use the real F5 route.
fn refresh_saved_query(ui: &MainWindow, bound: &hydrus_gui::Bound, required: HashId) {
    let current = bound.current.borrow().clone();
    let predicates = current.borrow().predicates();
    assert_eq!(predicates, vec!["system:everything".to_owned()]);
    let location = current.borrow().location().clone();
    let mut files = current.borrow().results().to_vec();
    files.sort();
    assert!(
        files.contains(&required),
        "saved page retains requested file"
    );
    ui.invoke_refresh_page();
    assert_eq!(current.borrow().predicates(), predicates);
    assert_eq!(current.borrow().location(), &location);
    let mut refreshed = current.borrow().results().to_vec();
    refreshed.sort();
    assert_eq!(refreshed, files, "Refresh preserves saved query membership");
}
// The headless adapter starts unmeasured. An owned visible preview requires
// an actual sidebar/preview viewport before its first SetMedia or sash reveal.
fn settle_viewport(ui: &MainWindow, bound: &hydrus_gui::Bound, windows: &headless::Windows) {
    assert!(ui.window().is_visible());
    let native = windows.get(0).unwrap();
    for _ in 0..3 {
        headless::render(&native, 1400, 1000);
    }
    bound.preview.refresh();
    assert!(ui.get_layout_available_width() > 0.0);
    assert!(ui.get_sidebar_actual_width() > 0.0);
    assert!(ui.get_preview_actual_height() > 0.0);
    assert!(!ui.get_preview_splitter_hidden());
}
fn select(ui: &MainWindow, bound: &hydrus_gui::Bound, file: HashId) {
    let index = bound
        .current
        .borrow()
        .borrow()
        .results()
        .iter()
        .position(|id| *id == file)
        .unwrap();
    ui.invoke_thumbnail_clicked(i32::try_from(index).unwrap(), false, false);
    bound.preview.refresh();
}
fn wait(ui: &MainWindow, bound: &hydrus_gui::Bound) {
    let started = std::time::Instant::now();
    while ui.get_preview_loading() {
        bound.preview.refresh();
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn tab(ui: &MainWindow, bound: &hydrus_gui::Bound, index: i32) {
    ui.invoke_tab_chosen(0, index);
    bound.preview.refresh();
}
fn totals(store: &Store, file: HashId) -> (u64, u64) {
    store
        .read(|c| hydrus_store::media::viewing_stats(c, &[file]))
        .unwrap()
        .into_iter()
        .find(|r| r.canvas == CanvasType::Preview)
        .map_or((0, 0), |r| (r.views, r.viewtime_ms))
}
fn setup() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let (dirs, store) = crate::subscriptions::store();
    store
        .write(|c| {
            c.conn()
                .execute("DELETE FROM file_viewing_stats WHERE canvas_type=1", [])?;
            let mut value: settings::FileViewingStatistics = settings::get(c.conn())?;
            value.active = true;
            value.preview_min_ms = None;
            value.preview_max_ms = None;
            settings::set(c.conn(), &value)
        })
        .unwrap();
    preferences(&store, false);
    (dirs, store)
}
fn clock(bound: &hydrus_gui::Bound, now: &Rc<Cell<i64>>) {
    bound.preview.set_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
}
#[test]
fn actual_qt_owned_hide_roundtrip_preserves_both_page_intervals_and_normal_restore() {
    let (_dirs, store) = setup();
    let [first, second] = files(&store);
    let fixture = hydrus_testkit::fixture_json("hidden_page_preview.json");
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let now = Rc::new(Cell::new(1000));
    clock(&bound, &now);
    settle_viewport(&ui, &bound, &windows);
    // Render distinct synthetic frame sizes while the real selected files own
    // identity, admission and persisted statistics.
    bound.preview.set_decoder(Arc::new(move |_, file| {
        let (w, h) = if file == first { (3, 2) } else { (5, 4) };
        Some(hydrus_media::Raster::new(w, h, 3, vec![80; (w * h * 3) as usize]).unwrap())
    }));
    query(&ui);
    select(&ui, &bound, first);
    wait(&ui, &bound);
    assert_eq!(bound.preview.displayed_file(), Some(first));
    let first_frame = ui.get_preview_media();
    now.set(1500);
    preferences(&store, true);
    bound.preview.refresh();
    now.set(2000);
    bound.pages.borrow_mut().new_search_page();
    let (_, i) = bound.pages.borrow().shown_position();
    tab(&ui, &bound, i32::try_from(i).unwrap());
    query(&ui);
    select(&ui, &bound, second);
    assert_eq!(bound.preview.displayed_file(), None);
    assert!(!ui.get_preview_has_media());
    now.set(3000);
    tab(&ui, &bound, 0);
    assert_eq!(bound.preview.displayed_file(), Some(first));
    assert_eq!(ui.get_preview_media().size(), first_frame.size());
    assert!(!ui.get_preview_loading());
    assert_eq!(totals(&store, first), (0, 0));
    ui.invoke_select_none();
    bound.preview.refresh();
    assert_eq!(bound.preview.displayed_file(), Some(first));
    now.set(4000);
    tab(&ui, &bound, 1);
    preferences(&store, false);
    // Live disable alone does not resend B's already rejected focus request.
    bound.preview.refresh();
    assert!(!ui.get_preview_has_media());
    // Real restore makes B's newly hidden preview visible; focus must change.
    let mut layout = store.read(page_layout::load).unwrap();
    layout.hpos = 400;
    layout.vpos = -240;
    store
        .write(move |c| settings::set(c.conn(), &layout))
        .unwrap();
    // New page's initial layout was globally hidden. Re-show through the actual
    // Pages menu consumer rather than changing the preview property directly.
    super::sidebar_layout::restore(&ui);
    settle_viewport(&ui, &bound, &windows);
    ui.invoke_select_none();
    select(&ui, &bound, second);
    wait(&ui, &bound);
    assert_eq!(bound.preview.displayed_file(), Some(second));
    preferences(&store, true);
    now.set(5000);
    tab(&ui, &bound, 0);
    assert_eq!(bound.preview.displayed_file(), Some(first));
    assert_eq!(totals(&store, first), (0, 0));
    assert_eq!(totals(&store, second), (0, 0));
    now.set(5500);
    ui.hide().unwrap();
    bound.preview.refresh();
    assert_eq!(bound.preview.displayed_file(), Some(first));
    assert_eq!(totals(&store, first), (0, 0));
    assert_eq!(totals(&store, second), (0, 0));
    now.set(5600);
    ui.show().unwrap();
    bound.preview.refresh();
    assert_eq!(bound.preview.displayed_file(), Some(first));
    now.set(6000);
    ui.invoke_sidebar_collapsed(ui.get_layout_page_key(), ui.get_layout_epoch(), true);
    bound.preview.refresh();
    assert_eq!(bound.preview.displayed_file(), Some(first));
    now.set(7000);
    preferences(&store, false);
    bound.preview.refresh();
    assert_eq!(
        bound.preview.displayed_file(),
        Some(first),
        "disabling global hide while still collapsed does not replay ClearMedia"
    );
    assert_eq!(totals(&store, first), (0, 0));
    bound.pages.borrow_mut().new_search_page();
    tab(&ui, &bound, 2);
    tab(&ui, &bound, 0);
    assert_eq!(
        bound.preview.displayed_file(),
        Some(first),
        "collapsed preview rejects PageHidden clear too"
    );
    assert_eq!(totals(&store, first), (0, 0));
    super::sidebar_layout::restore(&ui);
    settle_viewport(&ui, &bound, &windows);
    bound.preview.refresh();
    assert_eq!(
        bound.preview.displayed_file(),
        Some(first),
        "revealing alone preserves the accepted canvas"
    );
    select(&ui, &bound, first);
    ui.invoke_select_none();
    bound.preview.refresh();
    assert_eq!(totals(&store, first), (1, 6000));
    assert_eq!(
        fixture["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|step| step["action"] == "accepted clear ends A interval")
            .unwrap()["intervals"][0]["elapsed"],
        6000
    );
    now.set(8000);
    tab(&ui, &bound, 1);
    assert_eq!(bound.preview.displayed_file(), Some(second));
    now.set(9000);
    tab(&ui, &bound, 0);
    assert_eq!(totals(&store, second), (1, 5000));
    now.set(10_000);
    tab(&ui, &bound, 1);
    assert_eq!(bound.preview.displayed_file(), Some(second));
    let native = windows.get(0).unwrap();
    let pixels = headless::render(&native, 1400, 1000);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("hidden-page-preview-native.png"),
        &pixels,
        1400,
        1000,
    )
    .unwrap();
    now.set(11_000);
    bound.preview.close();
    assert_eq!(totals(&store, second), (2, 6000));
    assert_eq!(bound.preview.resident_snapshot_bytes(), 0);
    bound.preview.refresh();
    bound.preview.close();
    assert_eq!(totals(&store, second), (2, 6000));
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(totals(&reopened, first), (1, 6000));
    assert_eq!(totals(&reopened, second), (2, 6000));
}

#[test]
fn snapshot_eviction_redecodes_owned_identity_without_restarting_hidden_intervals() {
    let (_dirs, store) = setup();
    let [first, second] = files(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let now = Rc::new(Cell::new(1000));
    clock(&bound, &now);
    settle_viewport(&ui, &bound, &windows);
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    bound.preview.set_decoder(Arc::new({
        let calls = calls.clone();
        move |_, _| {
            if calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 3 {
                return None;
            }
            Some(hydrus_media::Raster::new(4096, 3072, 3, vec![100; 4096 * 3072 * 3]).unwrap())
        }
    }));
    query(&ui);
    select(&ui, &bound, first);
    wait(&ui, &bound);
    preferences(&store, true);
    now.set(2000);
    bound.pages.borrow_mut().new_search_page();
    let (_, i) = bound.pages.borrow().shown_position();
    tab(&ui, &bound, i32::try_from(i).unwrap());
    query(&ui);
    preferences(&store, false);
    super::sidebar_layout::restore(&ui);
    settle_viewport(&ui, &bound, &windows);
    select(&ui, &bound, second);
    wait(&ui, &bound);
    preferences(&store, true);
    assert!(bound.preview.resident_snapshot_bytes() <= 64 * 1024 * 1024);
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 2);
    now.set(3000);
    tab(&ui, &bound, 0);
    wait(&ui, &bound);
    assert_eq!(bound.preview.displayed_file(), Some(first));
    let size = ui.get_preview_media().size();
    assert_eq!((size.width, size.height), (4096, 3072));
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::Relaxed),
        3,
        "evicted frame restored from accepted identity"
    );
    assert!(bound.preview.resident_snapshot_bytes() <= 64 * 1024 * 1024);
    assert_eq!(totals(&store, first), (0, 0));
    assert_eq!(totals(&store, second), (0, 0));
    tab(&ui, &bound, 1);
    wait(&ui, &bound);
    assert!(
        !ui.get_preview_has_media(),
        "failed evicted raster restoration remains blank"
    );
    for _ in 0..10 {
        bound.preview.refresh();
    }
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::Relaxed),
        4,
        "failed restoration does not create an endless decoder retry"
    );
    assert_eq!(
        bound.preview.displayed_file(),
        Some(second),
        "accepted identity and timer survive rendering-cache failure"
    );
    now.set(4000);
    bound.preview.close();
    assert_eq!(totals(&store, first), (1, 3000));
    assert_eq!(totals(&store, second), (1, 2000));
    assert_eq!(bound.preview.resident_snapshot_bytes(), 0);
}

#[test]
fn pending_hidden_decode_is_page_owned_and_same_key_successor_retires_snapshots_and_replies() {
    let (_dirs, store) = setup();
    let [first, second] = files(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let now = Rc::new(Cell::new(1000));
    clock(&bound, &now);
    settle_viewport(&ui, &bound, &windows);
    let (entered, entries) = crossbeam_channel::bounded(2);
    let (release, released) = crossbeam_channel::bounded(2);
    let (finished, finishes) = crossbeam_channel::bounded(2);
    bound.preview.set_decoder(Arc::new(move |_, _| {
        entered.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(5)).unwrap();
        finished.send(()).unwrap();
        Some(hydrus_media::Raster::new(3, 2, 3, vec![110; 18]).unwrap())
    }));
    query(&ui);
    select(&ui, &bound, first);
    entries.recv_timeout(Duration::from_secs(5)).unwrap();
    preferences(&store, true);
    now.set(2000);
    bound.pages.borrow_mut().new_search_page();
    let (_, i) = bound.pages.borrow().shown_position();
    tab(&ui, &bound, i32::try_from(i).unwrap());
    assert!(!ui.get_preview_has_media());
    assert!(!ui.get_preview_loading());
    release.send(()).unwrap();
    let started = std::time::Instant::now();
    loop {
        bound.preview.refresh();
        tab(&ui, &bound, 0);
        if ui.get_preview_has_media() {
            break;
        }
        tab(&ui, &bound, 1);
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(bound.preview.displayed_file(), Some(first));
    assert_eq!(totals(&store, first), (0, 0));
    finishes.recv_timeout(Duration::from_secs(5)).unwrap();
    now.set(2500);
    preferences(&store, false);
    tab(&ui, &bound, 1);
    super::sidebar_layout::restore(&ui);
    settle_viewport(&ui, &bound, &windows);
    query(&ui);
    select(&ui, &bound, second);
    entries.recv_timeout(Duration::from_secs(5)).unwrap();
    preferences(&store, true);
    assert!(ui.get_preview_loading());
    assert_eq!(totals(&store, first), (1, 1500));
    // Persist the real page key then rebind the same MainWindow with fresh live
    // SearchPage identities. The successor cannot inherit accepted state.
    bound.pages.borrow_mut().sync(now.get() / 1000).unwrap();
    let old_key = bound.pages.borrow().shown().key;
    now.set(3000);
    let retired = bound.preview.clone();
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    clock(&successor, &now);
    assert_eq!(successor.pages.borrow().shown().key, old_key);
    successor.preview.refresh();
    assert_eq!(successor.preview.displayed_file(), None);
    assert!(!ui.get_preview_has_media());
    assert_eq!(totals(&store, first), (1, 1500));
    assert_eq!(retired.resident_snapshot_bytes(), 0);
    preferences(&store, false);
    super::sidebar_layout::restore(&ui);
    settle_viewport(&ui, &successor, &windows);
    refresh_saved_query(&ui, &successor, second);
    successor.preview.set_decoder(Arc::new(move |_, _| {
        Some(hydrus_media::Raster::new(5, 4, 3, vec![130; 60]).unwrap())
    }));
    select(&ui, &successor, second);
    wait(&ui, &successor);
    release.send(()).unwrap();
    finishes.recv_timeout(Duration::from_secs(5)).unwrap();
    let size = ui.get_preview_media().size();
    retired.refresh();
    retired.close();
    assert_eq!(ui.get_preview_media().size(), size);
    assert_eq!(successor.preview.displayed_file(), Some(second));
    assert_eq!(
        totals(&store, second),
        (0, 0),
        "retired pending reply creates no durable view"
    );
    now.set(4000);
    successor.preview.close();
    assert_eq!(totals(&store, second), (1, 1000));
}

#[test]
fn normal_pending_page_return_retries_with_return_time_and_rejects_obsolete_reply() {
    let (_dirs, store) = setup();
    let [first, _] = files(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let now = Rc::new(Cell::new(1000));
    clock(&bound, &now);
    settle_viewport(&ui, &bound, &windows);
    let (entered, entries) = crossbeam_channel::bounded(2);
    let (release, released) = crossbeam_channel::bounded(2);
    bound.preview.set_decoder(Arc::new(move |_, _| {
        entered.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(5)).unwrap();
        Some(hydrus_media::Raster::new(2, 2, 3, vec![90; 12]).unwrap())
    }));
    query(&ui);
    select(&ui, &bound, first);
    entries.recv_timeout(Duration::from_secs(5)).unwrap();
    now.set(2000);
    bound.pages.borrow_mut().new_search_page();
    let (_, i) = bound.pages.borrow().shown_position();
    tab(&ui, &bound, i32::try_from(i).unwrap());
    now.set(3000);
    tab(&ui, &bound, 0);
    assert!(ui.get_preview_loading());
    entries.recv_timeout(Duration::from_secs(5)).unwrap();
    release.send(()).unwrap();
    release.send(()).unwrap();
    wait(&ui, &bound);
    assert_eq!(bound.preview.displayed_file(), Some(first));
    assert_eq!(totals(&store, first), (0, 0));
    now.set(4000);
    bound.preview.close();
    assert_eq!(
        totals(&store, first),
        (1, 1000),
        "normal hidden gap is outside successful successor interval"
    );
}

#[test]
fn displaced_global_hide_request_retries_original_time_with_two_workers_and_one_queue() {
    let (_dirs, store) = setup();
    let [first, second] = files(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let now = Rc::new(Cell::new(1000));
    clock(&bound, &now);
    settle_viewport(&ui, &bound, &windows);
    let (entered, entries) = crossbeam_channel::bounded(4);
    let (finished, finishes) = crossbeam_channel::bounded(4);
    let (release, released) = crossbeam_channel::bounded(2);
    let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let peak = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    bound.preview.set_decoder(Arc::new({
        let active = active.clone();
        let peak = peak.clone();
        move |_, _| {
            let count = active.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
            peak.fetch_max(count, std::sync::atomic::Ordering::Relaxed);
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            active.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            finished.send(()).unwrap();
            Some(hydrus_media::Raster::new(2, 2, 3, vec![100; 12]).unwrap())
        }
    }));
    query(&ui);
    select(&ui, &bound, first);
    entries.recv_timeout(Duration::from_secs(5)).unwrap();
    now.set(2000);
    bound.pages.borrow_mut().new_search_page();
    tab(&ui, &bound, 1);
    query(&ui);
    select(&ui, &bound, second);
    entries.recv_timeout(Duration::from_secs(5)).unwrap();
    now.set(3000);
    bound.pages.borrow_mut().new_search_page();
    tab(&ui, &bound, 2);
    query(&ui);
    select(&ui, &bound, first);
    assert!(ui.get_preview_loading());
    // Hide refuses C's PageHidden clear, preserving its original queued start.
    preferences(&store, true);
    now.set(4000);
    bound.pages.borrow_mut().new_search_page();
    tab(&ui, &bound, 3);
    query(&ui);
    preferences(&store, false);
    super::sidebar_layout::restore(&ui);
    settle_viewport(&ui, &bound, &windows);
    select(&ui, &bound, second);
    assert!(ui.get_preview_loading());
    // D displaces C while both workers remain held. Returning to C must retry
    // its owned request, rather than leaving a permanent unaccepted blank pane.
    now.set(5000);
    preferences(&store, true);
    tab(&ui, &bound, 2);
    assert!(ui.get_preview_loading());
    release.send(()).unwrap();
    release.send(()).unwrap();
    finishes.recv_timeout(Duration::from_secs(5)).unwrap();
    finishes.recv_timeout(Duration::from_secs(5)).unwrap();
    entries.recv_timeout(Duration::from_secs(5)).unwrap();
    release.send(()).unwrap();
    wait(&ui, &bound);
    assert_eq!(
        peak.load(std::sync::atomic::Ordering::Relaxed),
        2,
        "page snapshots do not add worker pools"
    );
    assert_eq!(bound.preview.displayed_file(), Some(first));
    assert_eq!(totals(&store, first), (0, 0));
    assert_eq!(totals(&store, second), (0, 0));
    now.set(6000);
    bound.preview.close();
    assert_eq!(
        totals(&store, first),
        (1, 3000),
        "global-hide displacement preserves original admitted timestamp"
    );
    assert_eq!(totals(&store, second), (0, 0));
}

#[test]
fn closed_live_canvas_survives_unclose_but_forget_retires_even_retained_search_owner() {
    let (_dirs, store) = setup();
    let [first, second] = files(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let now = Rc::new(Cell::new(1000));
    clock(&bound, &now);
    settle_viewport(&ui, &bound, &windows);
    bound.preview.set_decoder(Arc::new(|_, _| {
        Some(hydrus_media::Raster::new(2, 2, 3, vec![110; 12]).unwrap())
    }));
    query(&ui);
    select(&ui, &bound, first);
    wait(&ui, &bound);
    preferences(&store, true);
    now.set(2000);
    bound.pages.borrow_mut().new_search_page();
    tab(&ui, &bound, 1);
    query(&ui);
    preferences(&store, false);
    super::sidebar_layout::restore(&ui);
    settle_viewport(&ui, &bound, &windows);
    select(&ui, &bound, second);
    wait(&ui, &bound);
    let retained = bound.current.borrow().clone();
    preferences(&store, true);
    now.set(3000);
    tab(&ui, &bound, 0);
    bound.pages.borrow_mut().close(0, 1).unwrap();
    bound.preview.refresh();
    assert_eq!(
        totals(&store, second),
        (0, 0),
        "restorable closed page remains a live canvas under hide"
    );
    assert!(bound.pages.borrow_mut().unclose());
    tab(&ui, &bound, 1);
    assert!(Rc::ptr_eq(&retained, &bound.current.borrow()));
    assert_eq!(bound.preview.displayed_file(), Some(second));
    assert!(!ui.get_preview_loading());
    tab(&ui, &bound, 0);
    bound.pages.borrow_mut().close(0, 1).unwrap();
    now.set(4000);
    bound.pages.borrow_mut().forget_closed();
    bound.preview.refresh();
    assert_eq!(
        totals(&store, second),
        (1, 2000),
        "membership retirement beats externally retained Rc"
    );
    assert_eq!(totals(&store, first), (0, 0));
    bound.preview.refresh();
    assert_eq!(totals(&store, second), (1, 2000));
    now.set(5000);
    bound.preview.close();
    assert_eq!(totals(&store, first), (1, 4000));
    assert_eq!(totals(&store, second), (1, 2000));
}

#[test]
fn same_monitor_persisted_same_keys_fresh_pages_drop_frames_and_reject_late_old_generation() {
    let (_dirs, store) = setup();
    let [first, second] = files(&store);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    let now = Rc::new(Cell::new(1000));
    clock(&bound, &now);
    settle_viewport(&ui, &bound, &windows);
    bound.preview.set_decoder(Arc::new(|_, _| {
        Some(hydrus_media::Raster::new(2, 2, 3, vec![120; 12]).unwrap())
    }));
    query(&ui);
    select(&ui, &bound, first);
    wait(&ui, &bound);
    let retained_a = bound.current.borrow().clone();
    preferences(&store, true);
    now.set(2000);
    bound.pages.borrow_mut().new_search_page();
    tab(&ui, &bound, 1);
    query(&ui);
    preferences(&store, false);
    super::sidebar_layout::restore(&ui);
    settle_viewport(&ui, &bound, &windows);
    let (entered, entries) = crossbeam_channel::bounded(1);
    let (release, released) = crossbeam_channel::bounded(1);
    let (published, publications) = crossbeam_channel::bounded(1);
    let publication = Published(published);
    bound.preview.set_decoder(Arc::new(move |_, _| {
        let _publication = &publication;
        entered.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(5)).unwrap();
        Some(hydrus_media::Raster::new(3, 3, 3, vec![130; 27]).unwrap())
    }));
    select(&ui, &bound, second);
    entries.recv_timeout(Duration::from_secs(5)).unwrap();
    let retained_b = bound.current.borrow().clone();
    preferences(&store, true);
    bound.pages.borrow_mut().save(1).unwrap();
    let key = bound.pages.borrow().shown().key;
    now.set(3000);
    // Actual persisted reopen replaces Pages within the existing binding, so
    // Monitor::bind/preview_retired never masks per-page owner validation.
    *bound.pages.borrow_mut() = Pages::open(store.clone()).unwrap();
    tab(&ui, &bound, 1);
    assert_eq!(bound.pages.borrow().shown().key, key);
    assert!(!Rc::ptr_eq(&retained_b, &bound.current.borrow()));
    assert_eq!(bound.preview.displayed_file(), None);
    assert!(!ui.get_preview_has_media());
    assert_eq!(bound.preview.resident_snapshot_bytes(), 0);
    assert_eq!(totals(&store, first), (1, 2000));
    assert!(
        Rc::strong_count(&retained_a) > 0,
        "retained stale owner does not keep its canvas live"
    );
    preferences(&store, false);
    super::sidebar_layout::restore(&ui);
    settle_viewport(&ui, &bound, &windows);
    refresh_saved_query(&ui, &bound, second);
    bound.preview.set_decoder(Arc::new(|_, _| {
        Some(hydrus_media::Raster::new(5, 4, 3, vec![140; 60]).unwrap())
    }));
    select(&ui, &bound, second);
    wait(&ui, &bound);
    let size = ui.get_preview_media().size();
    release.send(()).unwrap();
    publications.recv_timeout(Duration::from_secs(5)).unwrap();
    for _ in 0..3 {
        bound.preview.refresh();
    }
    assert_eq!(ui.get_preview_media().size(), size);
    assert_eq!(bound.preview.displayed_file(), Some(second));
    assert_eq!(
        totals(&store, second),
        (0, 0),
        "old held generation is never accepted by the fresh same-key canvas"
    );
    now.set(4000);
    bound.preview.close();
    assert_eq!(totals(&store, second), (1, 1000));
}
