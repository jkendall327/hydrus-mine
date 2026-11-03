//! The ordinary dialogs and frames keeping the `regular_dialog` frame as
//! the reference's do, replayed from `oracle/record_regular_dialog.py`: a
//! dialog (`DialogEdit`: "choose distance", "edit multiple location",
//! "force filetypes") keeps its
//! size and place when applied and nothing when cancelled or closed by its
//! window; a frame (`FrameThatTakesScrollablePanel`: help > about) keeps
//! them a moment after it is resized, and nothing more as it closes; and a
//! window whose remembered place is off the screens is fitted back onto
//! them as it opens, the user told by a popup message.
//!
//! Each window is opened, resized and ended as a user does it, on a real
//! store. A headless window has no place of its own (it reports 0, 0), so
//! the place kept is the one the window reports, and the place it is fitted
//! to is the one `windows::fitted_at` says it was given (the screens are
//! the recording's, as a headless window has none).

// (window sizes are whole pixels)
#![allow(clippy::float_cmp, clippy::cast_possible_truncation)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use hydrus_core::windows::{FrameLocation, WindowSettings};
use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless, windows};
use hydrus_gui_model::window_rescue::{Rect, Screen};
use hydrus_store::Store;
use serde_json::Value;
use slint::{ComponentHandle, Model as _};

struct Client {
    store: Arc<Store>,
    ui: MainWindow,
    bound: Bound,
    _dir: tempfile::TempDir,
}

fn client() -> Client {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(dir.path()).unwrap();
    // (the copied fixture's files are the test's to rename)
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::transfer::MediaOwnership::default(),
            )
        })
        .unwrap();
    crate::common::remove_trashed_from_view(&store);
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_clicked(0, false, false);
    Client {
        store,
        ui,
        bound,
        _dir: dir,
    }
}

impl Client {
    fn frame(&self) -> FrameLocation {
        self.store
            .read(hydrus_store::settings::get::<WindowSettings>)
            .unwrap()
            .frame("regular_dialog")
            .unwrap()
            .clone()
    }

    fn keep(&self, frame: FrameLocation) {
        self.store
            .write(move |ctx| {
                let mut settings: WindowSettings = hydrus_store::settings::get(ctx.conn())?;
                settings.set_frame("regular_dialog", frame);
                hydrus_store::settings::set(ctx.conn(), &settings)
            })
            .unwrap();
    }

    /// A thumbnail menu row, from one of its submenus.
    fn thumbnail_menu(
        &self,
        rows: fn(&hydrus_gui::ThumbnailMenu) -> slint::ModelRc<hydrus_gui::MenuRow>,
        label: &str,
    ) {
        self.ui.invoke_thumbnail_menu_requested(0);
        let id = rows(&self.ui.get_thumbnail_menu())
            .iter()
            .find(|r| r.label == label)
            .unwrap_or_else(|| panic!("{label} in the thumbnail menu"))
            .id;
        self.ui.invoke_menu_chosen(id);
    }

    fn wait_for_file_work(&self) {
        let until = Instant::now() + Duration::from_secs(5);
        while self.bound.metadata_jobs.running() != 0 {
            assert!(Instant::now() < until, "the file work finishes");
            std::thread::sleep(Duration::from_millis(5));
            slint::platform::update_timers_and_animations();
        }
    }
}

/// An ordinary dialog a user opens, and how they end it.
struct Ordinary {
    name: &'static str,
    open: fn(&Client) -> Box<dyn Ends>,
}

trait Ends {
    fn win(&self) -> &slint::Window;
    fn apply(&self);
    fn cancel(&self);
}

impl Ends for hydrus_gui::EditValueWindow {
    fn win(&self) -> &slint::Window {
        self.window()
    }
    fn apply(&self) {
        self.invoke_apply();
    }
    fn cancel(&self) {
        self.invoke_cancel();
    }
}

impl Ends for hydrus_gui::LocationsWindow {
    fn win(&self) -> &slint::Window {
        self.window()
    }
    fn apply(&self) {
        self.invoke_apply();
    }
    fn cancel(&self) {
        self.invoke_cancel();
    }
}

impl Ends for hydrus_gui::ForceFiletypeWindow {
    fn win(&self) -> &slint::Window {
        self.window()
    }
    fn apply(&self) {
        self.invoke_apply();
    }
    fn cancel(&self) {
        self.invoke_cancel();
    }
}

fn ordinary() -> Vec<Ordinary> {
    vec![
        Ordinary {
            name: "choose distance",
            open: |c| {
                c.thumbnail_menu(|m| m.open_similar.clone(), "custom");
                Box::new(
                    hydrus_gui::similar_distance_window::editor().expect("choose distance opens"),
                )
            },
        },
        Ordinary {
            name: "edit multiple location",
            open: |c| {
                // the page's file domain button, "multiple/deleted locations"
                c.ui.invoke_domain_menu_requested(0, 10.0, 200.0);
                let lines = c.ui.get_menu_panes().row_data(0).expect("its menu").lines;
                let at = (0..lines.row_count())
                    .position(|i| lines.row_data(i).unwrap().label == "multiple/deleted locations")
                    .expect("multiple/deleted locations");
                c.ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
                Box::new(hydrus_gui::locations_window::last_opened().expect("the list opens"))
            },
        },
        Ordinary {
            name: "force filetypes",
            open: |c| {
                c.thumbnail_menu(|m| m.manage.clone(), "force filetype");
                Box::new(
                    c.bound
                        .force_filetype
                        .borrow()
                        .as_ref()
                        .map(ComponentHandle::clone_strong)
                        .expect("force filetypes opens"),
                )
            },
        },
    ]
}

fn pair(value: &Value) -> Option<(i32, i32)> {
    let value = value.as_array()?;
    Some((value[0].as_i64()? as i32, value[1].as_i64()? as i32))
}

/// A frame location as `GetFrameLocation` returns it.
fn recorded_frame(row: &Value) -> FrameLocation {
    FrameLocation {
        remember_size: row[0].as_bool().unwrap(),
        remember_position: row[1].as_bool().unwrap(),
        last_size: pair(&row[2]),
        last_position: pair(&row[3]),
        default_gravity: pair(&row[4]).unwrap(),
        default_position: row[5].as_str().unwrap().into(),
        maximised: row[6].as_bool().unwrap(),
        fullscreen: row[7].as_bool().unwrap(),
    }
}

fn size(window: &slint::Window) -> (i32, i32) {
    windows::state(window).size
}

/// Spin the event loop for `time`.
fn spin(time: Duration) {
    let until = Instant::now() + time;
    while Instant::now() < until {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// What a recorded case's frame is once the window was ended: the
/// recording's, with the place kept being the one the headless window
/// reports.
fn expected_after(case: &Value, window: &slint::Window) -> FrameLocation {
    let mut after = recorded_frame(&case["after"]);
    let before = recorded_frame(&case["before"]);
    if after != before {
        assert_eq!(
            after.last_position,
            pair(&case["actual_window"]["position"]),
            "the reference kept where it was"
        );
        after.last_position = Some(windows::state(window).position);
    }
    after
}

#[test]
fn ordinary_dialogs_keep_their_frame_when_applied_and_not_when_cancelled() {
    let recorded = hydrus_testkit::fixture_json("regular_dialog.json");
    let baseline = recorded_frame(&recorded["baseline"]);
    let moved = pair(&recorded["moved"]["size"]).unwrap();
    let cases: Vec<&Value> = recorded["lifecycle"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            c["kind"] == "edit"
                && ["apply", "cancel", "window_close"].contains(&c["action"].as_str().unwrap())
        })
        .collect();
    assert_eq!(cases.len(), 3);
    let _windows = headless::init();
    let client = client();
    for dialog in ordinary() {
        for case in &cases {
            let action = case["action"].as_str().unwrap();
            let what = format!("{}: {action}", dialog.name);
            client.keep(baseline.clone());
            assert_eq!(client.frame(), recorded_frame(&case["before"]));
            let shown = (dialog.open)(&client);
            let window = shown.win();
            assert!(window.is_visible(), "{what}: opened");
            assert_eq!(
                Some(size(window)),
                pair(&case["opened"]["size"]),
                "{what}: opened at its remembered size"
            );
            // the user resizes it
            window.set_size(slint::LogicalSize::new(moved.0 as f32, moved.1 as f32));
            assert_eq!(Some(size(window)), pair(&case["actual_window"]["size"]));
            match action {
                "apply" => shown.apply(),
                "cancel" => shown.cancel(),
                _ => window.dispatch_event(slint::platform::WindowEvent::CloseRequested),
            }
            assert_eq!(
                window.is_visible(),
                case["still_open"].as_bool().unwrap(),
                "{what}: closed"
            );
            assert_eq!(client.frame(), expected_after(case, window), "{what}: kept");
            client.wait_for_file_work();
        }
    }
    // reopened, an applied dialog opens where it was kept
    client.keep(baseline.clone());
    let dialog = &ordinary()[0];
    let shown = (dialog.open)(&client);
    shown
        .win()
        .set_size(slint::LogicalSize::new(moved.0 as f32, moved.1 as f32));
    shown.apply();
    let reopened = (dialog.open)(&client);
    assert_eq!(
        size(reopened.win()),
        moved,
        "reopened at the size it was kept"
    );
    reopened.cancel();
}

#[test]
fn an_ordinary_frame_keeps_its_frame_as_it_is_resized_and_nothing_as_it_closes() {
    let recorded = hydrus_testkit::fixture_json("regular_dialog.json");
    let baseline = recorded_frame(&recorded["baseline"]);
    let moved = pair(&recorded["moved"]["size"]).unwrap();
    let _windows = headless::init();
    let client = client();
    for case in recorded["lifecycle"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "frame")
    {
        let action = case["action"].as_str().unwrap();
        client.keep(baseline.clone());
        crate::common::main_menu::choose(&client.ui, &["help"], "about");
        let about = client
            .bound
            .about
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
            .expect("about opens");
        let window = about.window();
        assert_eq!(
            Some(size(window)),
            pair(&case["opened"]["size"]),
            "{action}: opened"
        );
        // the user resizes it; a moment later it is kept
        window.set_size(slint::LogicalSize::new(moved.0 as f32, moved.1 as f32));
        spin(Duration::from_millis(700));
        let mut after_move = recorded_frame(&case["after_move"]);
        after_move.last_position = Some(windows::state(window).position);
        assert_eq!(
            client.frame(),
            after_move,
            "{action}: kept as it was resized"
        );
        // as it closes, nothing more
        client.keep(baseline.clone());
        assert_eq!(client.frame(), recorded_frame(&case["before"]));
        match action {
            "close_button" => about.invoke_close_clicked(),
            _ => window.dispatch_event(slint::platform::WindowEvent::CloseRequested),
        }
        assert!(!window.is_visible(), "{action}: closed");
        spin(Duration::from_millis(700));
        assert_eq!(
            client.frame(),
            recorded_frame(&case["after"]),
            "{action}: nothing kept"
        );
    }
}

fn screens(recorded: &Value) -> Vec<Screen> {
    recorded["screens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|screen| {
            let g = &screen["geometry"];
            let a = &screen["available"];
            Screen {
                geometry: Rect {
                    x: g[0].as_i64().unwrap(),
                    y: g[1].as_i64().unwrap(),
                    width: g[2].as_i64().unwrap(),
                    height: g[3].as_i64().unwrap(),
                },
                available_top_left: (a[0].as_i64().unwrap(), a[1].as_i64().unwrap()),
            }
        })
        .collect()
}

fn rescue_messages(store: &Store) -> Vec<String> {
    store
        .read(|conn| hydrus_store::popups::all(conn, i64::MAX / 4))
        .unwrap()
        .into_iter()
        .filter_map(|job| job.status_text_1)
        .filter(|text| text.contains("was rescued from apparent off-screen"))
        .collect()
}

/// Unsets the recorded screens however the test ends.
struct Screens;

impl Drop for Screens {
    fn drop(&mut self) {
        windows::set_screens(None);
    }
}

#[test]
fn a_remembered_place_off_the_screens_is_fitted_back_onto_them_as_the_dialog_opens() {
    let recorded = hydrus_testkit::fixture_json("regular_dialog.json");
    let _windows = headless::init();
    let client = client();
    windows::set_screens(Some(screens(&recorded)));
    let _screens = Screens;
    let cases: Vec<&Value> = recorded["opening"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"].as_str().unwrap().starts_with("remembered_"))
        .collect();
    assert_eq!(cases.len(), 5);
    let dialog = &ordinary()[0];
    for case in cases {
        let name = case["name"].as_str().unwrap();
        client.keep(recorded_frame(&case["frame"]));
        let earlier = rescue_messages(&client.store);
        let shown = (dialog.open)(&client);
        let window = shown.win();
        if size(window) == (0, 0) {
            // (laid out, as a native window is as it is shown: headless,
            // it has no size until given one)
            let hint = pair(&case["placed"]["size"]).unwrap();
            window.set_size(slint::LogicalSize::new(hint.0 as f32, hint.1 as f32));
        }
        let started = Instant::now();
        while windows::fitted_at(window).is_none() && started.elapsed() < Duration::from_secs(3) {
            spin(Duration::from_millis(20));
        }
        assert_eq!(
            windows::fitted_at(window),
            pair(&case["shown"]["position"]),
            "{name}: where it opens"
        );
        let messages: Vec<String> = case["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap().replace("PySide6.QtCore.QPoint", ""))
            .collect();
        let told: Vec<String> = rescue_messages(&client.store)
            .into_iter()
            .filter(|m| !earlier.contains(m))
            .collect();
        assert_eq!(told, messages, "{name}: told");
        shown.cancel();
    }
}
