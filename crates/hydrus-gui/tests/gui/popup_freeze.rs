//! Real owned toaster, staged checkbox and genuine software-window minimized state.
//! Physical OS minimized reads and Wayland unavailable state remain separately bounded.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    popup_freeze::{self, Preferences},
    popups::{self, Job},
    settings,
};
use slint::{ComponentHandle as _, Model as _};
use std::time::{Duration, Instant};
const LABEL: &str = "Freeze the popup toaster when the main gui is minimised: ";
fn now() -> i64 {
    hydrus_core::TimestampMs::now().0 / 1000
}
fn add(store: &Store, job: Job) {
    store
        .write(move |c| popups::add(c.conn(), &job, now()))
        .unwrap();
}
fn text(store: &Store, key: [u8; 32], value: &str) {
    let value = value.to_string();
    store
        .write(move |c| {
            popups::update(c.conn(), &key, now(), |j| j.status_text_1 = Some(value)).map(|_| ())
        })
        .unwrap();
}
fn rows(ui: &MainWindow) -> Vec<String> {
    ui.get_popups()
        .iter()
        .map(|r| r.text_1.to_string())
        .collect()
}
fn wait(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while !done() {
        assert!(Instant::now() < deadline, "owned toaster did not settle");
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(8));
    }
}
fn pump() {
    let end = Instant::now() + Duration::from_millis(330);
    while Instant::now() < end {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(8));
    }
}
fn save(store: &Store, minimized: bool) {
    store
        .write(move |c| settings::set(c.conn(), &Preferences { minimized }))
        .unwrap();
}
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20., 22.);
    let menu = ui.get_menu_panes().row_data(0).unwrap();
    let i = menu
        .lines
        .iter()
        .position(|row| row.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(i).unwrap(), 0., 0., 0.);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|p| p.text == "popup notifications")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = window
        .get_rows()
        .iter()
        .position(|r| r.label == LABEL)
        .unwrap();
    (window, i32::try_from(row).unwrap())
}
#[test]
fn queued_jobs_expire_and_change_under_frozen_cards_then_restore_and_live_false_resume() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let (options, row) = open(&ui, &bound);
    assert!(
        !options
            .get_rows()
            .row_data(usize::try_from(row).unwrap())
            .unwrap()
            .checked
    );
    options.invoke_check_toggled(row, true);
    options.invoke_apply();
    assert!(store.read(popup_freeze::load).unwrap().minimized);
    let mut running = Job::new(true, true, 0.0);
    running.status_text_1 = Some("working before freeze".into());
    let live = running.key;
    add(&store, running);
    let mut expiring = Job::text("expires while frozen", 0.0);
    expiring.finish_and_dismiss(Some(60), now());
    let expiry = expiring.key;
    add(&store, expiring);
    wait(|| rows(&ui) == ["working before freeze", "expires while frozen"]);
    let summary = ui.get_popup_summary();
    let original_width = ui.get_popups().row_data(0).unwrap().width_characters;
    ui.window().set_minimized(true);
    assert!(ui.window().is_minimized());
    text(&store, live, "working changed while frozen");
    store
        .write(|c| {
            settings::set(
                c.conn(),
                &hydrus_store::popup_width::PopupWidth {
                    characters: 31,
                    fixed: true,
                },
            )
        })
        .unwrap();
    let mut pending = Job::text("queued while frozen", 0.0);
    pending.finish();
    add(&store, pending);
    store
        .write(move |c| {
            popups::update(c.conn(), &expiry, now(), |j| j.dismiss_at = Some(now() - 1)).map(|_| ())
        })
        .unwrap();
    pump();
    assert_eq!(rows(&ui), ["working before freeze", "expires while frozen"]);
    assert_eq!(ui.get_popup_summary(), summary);
    assert!(
        store
            .read(move |c| popups::get(c, &expiry, now()))
            .unwrap()
            .is_none(),
        "Store expiry is independent of cached GUI cards"
    );
    ui.invoke_popup_cancel(0);
    ui.invoke_popup_pause_play(0);
    ui.invoke_popups_dismiss_all();
    let running = store
        .read(move |c| popups::get(c, &live, now()))
        .unwrap()
        .unwrap();
    assert!(!running.cancelled && !running.paused && !running.done);
    ui.window().set_minimized(false);
    wait(|| rows(&ui) == ["working changed while frozen", "queued while frozen"]);
    assert_eq!(ui.get_popup_summary(), "2 messages");
    assert_eq!(
        ui.get_popups().row_data(0).unwrap().width_characters,
        original_width
    );
    assert_eq!(ui.get_popups().row_data(1).unwrap().width_characters, 31);
    let pixels = super::popup_width::render_settled(&ui, &windows.get(0).unwrap(), 1000, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("popup-freeze-restored-native.png"),
        &pixels,
        1000,
        700,
    )
    .unwrap();
    ui.hide().unwrap();
    text(&store, live, "working changed while hidden");
    let mut hidden = Job::text("queued while hidden", 0.0);
    hidden.finish();
    add(&store, hidden);
    save(&store, false);
    pump();
    assert_eq!(
        rows(&ui),
        ["working changed while frozen", "queued while frozen"]
    );
    assert_eq!(ui.get_popup_summary(), "2 messages");
    ui.invoke_popup_cancel(0);
    assert!(
        !store
            .read(move |c| popups::get(c, &live, now()))
            .unwrap()
            .unwrap()
            .cancelled
    );
    ui.show().unwrap();
    wait(|| {
        rows(&ui)
            == [
                "working changed while hidden",
                "queued while frozen",
                "queued while hidden",
            ]
    });
    ui.window().set_minimized(true);
    text(&store, live, "minimized policy false updates");
    wait(|| rows(&ui)[0] == "minimized policy false updates");
    save(&store, true);
    text(&store, live, "live true freezes new text");
    pump();
    assert_eq!(rows(&ui)[0], "minimized policy false updates");
    save(&store, false);
    wait(|| rows(&ui)[0] == "live true freezes new text");
    ui.window().set_minimized(false);
}
#[test]
fn real_options_cancel_reopen_concurrent_fields_hidden_and_retired_children_do_not_write() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let (w, row) = open(&ui, &bound);
    w.invoke_check_toggled(row, true);
    w.invoke_cancel();
    assert!(!store.read(popup_freeze::load).unwrap().minimized);
    let (w, row) = open(&ui, &bound);
    assert!(
        !w.get_rows()
            .row_data(usize::try_from(row).unwrap())
            .unwrap()
            .checked
    );
    w.hide().unwrap();
    w.invoke_check_toggled(row, true);
    w.show().unwrap();
    assert!(
        !w.get_rows()
            .row_data(usize::try_from(row).unwrap())
            .unwrap()
            .checked
    );
    w.invoke_check_toggled(row, true);
    store
        .write(|c| {
            settings::set(
                c.conn(),
                &hydrus_store::popup_width::PopupWidth {
                    characters: 31,
                    fixed: true,
                },
            )
        })
        .unwrap();
    w.invoke_apply();
    assert!(store.read(popup_freeze::load).unwrap().minimized);
    assert_eq!(
        store
            .read(settings::get::<hydrus_store::popup_width::PopupWidth>)
            .unwrap()
            .characters,
        31
    );
    let (w, row) = open(&ui, &bound);
    assert!(
        w.get_rows()
            .row_data(usize::try_from(row).unwrap())
            .unwrap()
            .checked
    );
    w.invoke_check_toggled(row, false);
    let successor = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    assert!(!w.window().is_visible());
    w.show().unwrap();
    w.invoke_apply();
    assert!(store.read(popup_freeze::load).unwrap().minimized);
    let (w, row) = open(&ui, &successor);
    w.invoke_check_toggled(row, false);
    w.invoke_cancel();
    assert!(store.read(popup_freeze::load).unwrap().minimized);
}
#[test]
fn hidden_rebind_and_accepted_close_retire_ui_only_and_never_admit_to_a_successor() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let old = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let mut job = Job::new(true, true, 0.0);
    job.status_text_1 = Some("ongoing owned card".into());
    let key = job.key;
    add(&store, job);
    wait(|| rows(&ui) == ["ongoing owned card"]);
    let old_source = ui.get_popups().row_data(0).unwrap().gui_owner;
    ui.hide().unwrap();
    let successor = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    assert!(rows(&ui).is_empty());
    pump();
    assert!(rows(&ui).is_empty());
    assert!(ui.get_popup_summary().is_empty());
    ui.show().unwrap();
    wait(|| rows(&ui) == ["ongoing owned card"]);
    assert_ne!(ui.get_popups().row_data(0).unwrap().gui_owner, old_source);
    ui.invoke_popup_answer(
        hex::encode(key).into(),
        "".into(),
        old_source,
        "stale".into(),
        true,
    );
    assert!(
        !store
            .read(move |c| popups::get(c, &key, now()))
            .unwrap()
            .unwrap()
            .done
    );
    store
        .write(|c| {
            let mut prefs: hydrus_store::settings::GuiSettings = settings::get(c.conn())?;
            prefs.confirm_exit = true;
            settings::set(c.conn(), &prefs)
        })
        .unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!ui.get_question().is_empty());
    ui.invoke_answer(false);
    text(&store, key, "declined exit still updates");
    wait(|| rows(&ui) == ["declined exit still updates"]);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    ui.show().unwrap();
    text(&store, key, "retired UI must not update");
    let mut queued = Job::text("queued after retirement", 0.0);
    queued.finish();
    add(&store, queued);
    pump();
    assert_eq!(rows(&ui), ["declined exit still updates"]);
    ui.invoke_popup_cancel(0);
    assert!(
        !store
            .read(move |c| popups::get(c, &key, now()))
            .unwrap()
            .unwrap()
            .cancelled
    );
    drop(old);
    drop(successor);
}
