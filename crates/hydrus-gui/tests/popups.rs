//! Popup messages in the main window, as the reference's popup message
//! manager shows them: the store's popups (which the daemon and the Client
//! API add), the oldest ten, with their texts, gauges and buttons, a line
//! counting them, and dismissing them.

use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::Sha256;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::popups::{self, Job};

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

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

fn add(store: &Store, job: Job) {
    store
        .write(move |ctx| popups::add(ctx.conn(), &job, now()))
        .unwrap();
}

fn texts(ui: &MainWindow) -> Vec<(String, String)> {
    let popups = ui.get_popups();
    (0..popups.row_count())
        .map(|i| {
            let p = popups.row_data(i).unwrap();
            (p.title.to_string(), p.text_1.to_string())
        })
        .collect()
}

#[test]
fn popups_show_at_the_bottom_right_and_their_buttons_work() {
    let (_dirs, store) = store();
    // a message; a job going, with gauges; one with the fixture's files
    add(&store, Job::text("hello", 0.0));
    let mut working = Job::new(true, true, 0.0);
    working.status_title = Some("subscription".into());
    working.status_text_1 = Some("downloading".into());
    working.popup_gauge_1 = Some((1, 4));
    working.network_job = Some(hydrus_store::live::JobLive {
        url: "https://example.com/file.jpg".into(),
        status: "downloading\u{2026}".into(),
        speed: 500,
        bytes_read: 1000,
        bytes_to_read: Some(4000),
        done: false,
        error: false,
    });
    add(&store, working.clone());
    let hashes: Vec<Sha256> = store
        .read(|conn| {
            let ids: Vec<hydrus_core::HashId> = conn
                .prepare("SELECT hash_id FROM hashes ORDER BY hash_id LIMIT 3")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            Ok(hydrus_store::master::hashes(conn, &ids)?
                .into_values()
                .collect())
        })
        .unwrap();
    assert_eq!(hashes.len(), 3);
    let mut files = Job::text("found some", 0.0);
    files.set_files(hashes, Some("my sub".into()));
    add(&store, files.clone());

    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    assert_eq!(
        texts(&ui),
        [
            (String::new(), "hello".to_owned()),
            ("subscription".to_owned(), "downloading".to_owned()),
            (String::new(), "found some".to_owned()),
        ]
    );
    assert_eq!(ui.get_popup_summary(), "3 messages");
    let row = ui.get_popups().row_data(1).unwrap();
    assert!(row.has_gauge_1 && !row.gauge_1_going);
    assert!((row.gauge_1 - 0.25).abs() < 1e-6);
    assert!(row.pausable && row.cancellable);
    // its download, whose stop button is the popup's own
    assert!(row.has_download);
    assert_eq!(row.download.left, "downloading\u{2026}");
    assert!((row.download.fraction - 0.25).abs() < 1e-6);
    assert!(!row.download.can_cancel);
    assert!(!ui.get_popups().row_data(0).unwrap().has_download);
    assert_eq!(
        ui.get_popups().row_data(2).unwrap().files,
        "my sub - show 3 files"
    );

    // pausing: "paused", in the store too
    ui.invoke_popup_pause_play(1);
    assert_eq!(texts(&ui)[1].1, "paused");
    let paused = store
        .read(|conn| popups::all(conn, now()))
        .unwrap()
        .into_iter()
        .find(|j| j.key == working.key)
        .unwrap();
    assert!(paused.paused);
    // a job going doesn't dismiss; a message does
    ui.invoke_popup_dismiss(1);
    assert_eq!(texts(&ui).len(), 3);
    ui.invoke_popup_dismiss(0);
    assert_eq!(texts(&ui).len(), 2);
    assert_eq!(ui.get_popup_summary(), "2 messages");

    // "show files": a page of them, named for them
    ui.invoke_popup_show_files(1);
    let shown = bound.pages.borrow().shown().name.clone();
    assert_eq!(shown, "my sub");
    assert_eq!(bound.current.borrow().borrow().files().len(), 3);

    // a popup the daemon adds shows within a quarter of a second
    add(&store, Job::text("from the daemon", 0.0));
    std::thread::sleep(Duration::from_millis(300));
    slint::platform::update_timers_and_animations();
    assert_eq!(texts(&ui).last().unwrap().1, "from the daemon");

    // a right click on one that is done dismisses it (the last, just above
    // the line under them)
    ui.show().unwrap();
    let window = windows.get(0).unwrap();
    headless::render(&window, 1100, 700);
    let at = |x: f32, y: f32| {
        use slint::platform::{PointerEventButton, WindowEvent};
        let position = slint::LogicalPosition::new(x, y);
        for event in [
            WindowEvent::PointerMoved { position },
            WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Right,
            },
            WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Right,
            },
        ] {
            ui.window().dispatch_event(event);
        }
        headless::render(&window, 1100, 700);
    };
    let before = texts(&ui).len();
    at(1100.0 - 20.0 - 200.0, 700.0 - 25.0 - 60.0);
    assert_eq!(texts(&ui).len(), before - 1, "{:?}", texts(&ui));
    assert!(!texts(&ui).iter().any(|t| t.1 == "from the daemon"));

    // "dismiss all": those done go; the job going stays
    ui.invoke_popups_dismiss_all();
    assert_eq!(
        texts(&ui),
        [("subscription".to_owned(), "paused".to_owned())]
    );
    assert_eq!(ui.get_popup_summary(), "1 message");
    // cancelled, it is done, and dismisses
    ui.invoke_popup_cancel(0);
    ui.invoke_popups_dismiss_all();
    assert!(texts(&ui).is_empty());
    assert_eq!(ui.get_popup_summary(), "", "with none, nothing shows");
}
