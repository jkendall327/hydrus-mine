//! Actual Help/debug command, Store jobs and the owned native toaster scheduler.
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::{Store, popups, settings};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::Cell, rc::Rc, time::Duration};

fn jobs(store: &Store) -> Vec<popups::Job> {
    store
        .read(|conn| popups::all(conn, hydrus_core::TimestampMs::now().0 / 1000))
        .unwrap()
}
fn choose(ui: &MainWindow, pane: i32, label: &str) {
    let lines = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
    let index = lines.iter().position(|row| row.label == label).unwrap();
    assert!(lines.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(pane, index as i32, 0.0, 0.0, 0.0);
}
fn launch(ui: &MainWindow) {
    let help = ui
        .get_menu_titles()
        .iter()
        .position(|title| title.label == "help")
        .unwrap();
    ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
    choose(ui, 0, "debug");
    let debug = ui.get_menu_panes().row_data(1).unwrap().lines;
    let gui = debug
        .iter()
        .position(|row| row.label == "gui actions")
        .unwrap();
    let memory = debug
        .iter()
        .position(|row| row.label == "memory actions")
        .unwrap();
    assert!(gui < memory, "preserve the reference debug group order");
    choose(ui, 1, "gui actions");
    choose(ui, 2, "make a long text popup");
}
fn clock(control: &hydrus_gui::debug_long_popup::Control) -> Rc<Cell<Duration>> {
    let now = Rc::new(Cell::new(Duration::ZERO));
    control.set_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    let words = Cell::new(0);
    control.set_word_source(Rc::new(move || {
        let next = words.get();
        words.set(next + 1);
        next
    }));
    now
}
fn assert_cards(ui: &MainWindow, saved: &[popups::Job]) {
    assert_eq!(ui.get_popups().row_count(), saved.len());
    for (i, job) in saved.iter().enumerate() {
        let card = ui.get_popups().row_data(i).unwrap();
        assert_eq!(card.key, hex::encode(job.key));
        assert_eq!(card.title, job.status_title.as_deref().unwrap_or_default());
        assert_eq!(
            card.text_1,
            job.status_text_1.as_deref().unwrap_or_default()
        );
        assert!(!card.pausable && !card.cancellable);
    }
}

#[test]
fn actual_menu_and_toaster_replay_every_recorded_deadline_and_durable_string() {
    let fixture = hydrus_testkit::fixture_json("debug_long_popup.json");
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let now = clock(&bound.debug_long_popup);
    launch(&ui);
    let initial = jobs(&store);
    assert_eq!(initial.len(), 2);
    assert_eq!(bound.debug_long_popup.pending_updates(), 124);
    assert!(bound.debug_long_popup.timer_running());
    for (job, reference) in initial.iter().zip(fixture["published"].as_array().unwrap()) {
        assert_eq!(job.status_text_1.as_deref(), reference["text"].as_str());
        assert_eq!(job.status_title.as_deref(), reference["title"].as_str());
        assert_eq!(job.done, reference["done"].as_bool().unwrap());
        assert_eq!(job.cancellable, reference["cancellable"].as_bool().unwrap());
        assert_eq!(job.pausable, reference["pausable"].as_bool().unwrap());
    }
    assert_cards(&ui, &initial);
    let native = windows.get(0).unwrap();
    let initial_pixels = super::popup_width::render_settled(&ui, &native, 1000, 700);
    let initial_width = ui.get_popup_card_widths().row_data(0).unwrap();
    let caps: Vec<_> = ui.get_popup_card_caps().iter().collect();
    let mut expected = initial.clone();
    for (n, event) in fixture["schedule"].as_array().unwrap().iter().enumerate() {
        let deadline = event["at_ms"].as_u64().unwrap();
        now.set(Duration::from_millis(deadline - 1));
        bound.debug_long_popup.tick();
        assert_eq!(jobs(&store), expected, "before deadline {deadline}");
        now.set(Duration::from_millis(deadline));
        bound.debug_long_popup.tick();
        let index = event["index"].as_u64().unwrap() as usize;
        let value = Some(event["value"].as_str().unwrap().to_owned());
        if event["method"] == "SetStatusTitle" {
            expected[index].status_title = value;
        } else {
            expected[index].status_text_1 = value;
        }
        assert_eq!(jobs(&store), expected, "at deadline {deadline}");
        assert_cards(&ui, &expected);
        assert_eq!(bound.debug_long_popup.pending_updates(), 123 - n);
    }
    assert!(!bound.debug_long_popup.timer_running());
    assert_eq!(
        expected[0]
            .status_text_1
            .as_ref()
            .unwrap()
            .split_whitespace()
            .count(),
        63
    );
    assert_eq!(
        expected[1]
            .status_title
            .as_ref()
            .unwrap()
            .split_whitespace()
            .count(),
        63
    );
    assert_eq!(
        expected[1].status_text_1.as_deref(),
        Some("test long title")
    );
    // Settle actual card measurement publication, rather than synthesize sizes.
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    let final_pixels = loop {
        let pixels = headless::render(&native, 1000, 700);
        if ui.get_popup_card_widths().row_data(0).unwrap() > initial_width {
            break pixels;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "long content must grow the actual native card"
        );
        std::thread::yield_now();
    };
    assert_ne!(initial_pixels, final_pixels);
    for (i, cap) in caps.iter().enumerate() {
        assert!((ui.get_popup_card_caps().row_data(i).unwrap() - cap).abs() < 0.001);
        assert!(ui.get_popup_card_widths().row_data(i).unwrap() <= cap + 1.0);
    }
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(jobs(&reopened), expected);
    ui.hide().unwrap();
}

#[test]
fn dismissal_prunes_future_title_jobs_before_any_deadline_and_overlapping_runs_remain_owned() {
    let (_dirs, store) = super::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let now = clock(&bound.debug_long_popup);
    launch(&ui);
    let first_key = jobs(&store)[0].key;
    ui.invoke_popup_dismiss(1);
    bound.debug_long_popup.tick();
    assert_eq!(bound.debug_long_popup.pending_updates(), 62);
    assert_eq!(jobs(&store).len(), 1);
    assert_eq!(jobs(&store)[0].key, first_key);
    ui.invoke_popup_dismiss(0);
    bound.debug_long_popup.tick();
    assert_eq!(bound.debug_long_popup.pending_updates(), 0);
    assert!(!bound.debug_long_popup.timer_running());
    assert!(jobs(&store).is_empty());
    launch(&ui);
    now.set(Duration::from_millis(100));
    launch(&ui);
    assert_eq!(jobs(&store).len(), 4);
    assert_eq!(bound.debug_long_popup.pending_updates(), 248);
    let before = jobs(&store);
    now.set(Duration::from_millis(200));
    bound.debug_long_popup.tick();
    let first = jobs(&store);
    assert_ne!(first[0].status_text_1, before[0].status_text_1);
    assert_eq!(first[2], before[2]);
    now.set(Duration::from_millis(300));
    bound.debug_long_popup.tick();
    let second = jobs(&store);
    assert_ne!(second[2].status_text_1, first[2].status_text_1);
    assert_cards(&ui, &second);
    assert_eq!(bound.debug_long_popup.pending_updates(), 246);
    bound.debug_long_popup.retire();
    assert_eq!(bound.debug_long_popup.pending_updates(), 0);
    assert!(!bound.debug_long_popup.timer_running());
    ui.hide().unwrap();
}

#[test]
fn hidden_launch_exit_cancel_rebind_bound_drop_and_main_destruction_have_distinct_lifetimes() {
    let (_dirs, store) = super::subscriptions::store();
    store
        .write(|ctx| {
            let mut gui: settings::GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let now = clock(&bound.debug_long_popup);
    bound.debug_long_popup.start();
    assert!(
        jobs(&store).is_empty(),
        "hidden main cannot launch a new producer"
    );
    ui.show().unwrap();
    launch(&ui);
    ui.hide().unwrap();
    let hidden = jobs(&store);
    bound.debug_long_popup.start();
    assert_eq!(jobs(&store), hidden);
    now.set(Duration::from_millis(200));
    bound.debug_long_popup.tick();
    assert_ne!(
        jobs(&store)[0].status_text_1,
        hidden[0].status_text_1,
        "already published Qt backend jobs progress while merely hidden"
    );
    ui.show().unwrap();
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        ui.get_question()
            .starts_with("Are you sure you want to exit the client?")
    );
    ui.invoke_answer(false);
    assert!(bound.debug_long_popup.timer_running());
    now.set(Duration::from_millis(400));
    bound.debug_long_popup.tick();
    assert_eq!(bound.debug_long_popup.pending_updates(), 122);
    let retained = bound.debug_long_popup.clone();
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    assert_eq!(retained.pending_updates(), 0);
    assert!(!retained.timer_running());
    let snapshot = jobs(&store);
    retained.start();
    retained.tick();
    assert_eq!(jobs(&store), snapshot);
    let next_clock = clock(&successor.debug_long_popup);
    launch(&ui);
    assert_eq!(jobs(&store).len(), 4);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    assert_eq!(successor.debug_long_popup.pending_updates(), 0);
    assert!(!successor.debug_long_popup.timer_running());
    ui.show().unwrap();
    let snapshot = jobs(&store);
    next_clock.set(Duration::from_secs(30));
    successor.debug_long_popup.tick();
    successor.debug_long_popup.start();
    assert_eq!(
        jobs(&store),
        snapshot,
        "accepted exit cannot resurrect an old producer"
    );
    let fresh = bind(&ui, Pages::open(store.clone()).unwrap());
    clock(&fresh.debug_long_popup);
    launch(&ui);
    let handle = fresh.debug_long_popup.clone();
    let snapshot = jobs(&store);
    drop(fresh);
    assert_eq!(handle.pending_updates(), 0);
    assert!(!handle.timer_running());
    handle.start();
    handle.tick();
    assert_eq!(
        jobs(&store),
        snapshot,
        "Bound destruction disposes retained handles"
    );
    let final_owner = bind(&ui, Pages::open(store.clone()).unwrap());
    let final_clock = clock(&final_owner.debug_long_popup);
    launch(&ui);
    let snapshot = jobs(&store);
    ui.hide().unwrap();
    let weak = ui.as_weak();
    drop(ui);
    assert!(
        weak.upgrade().is_none(),
        "the producer must not strongly own MainWindow"
    );
    final_clock.set(Duration::from_secs(30));
    final_owner.debug_long_popup.tick();
    assert_eq!(final_owner.debug_long_popup.pending_updates(), 0);
    assert!(!final_owner.debug_long_popup.timer_running());
    assert_eq!(jobs(&store), snapshot);
}
