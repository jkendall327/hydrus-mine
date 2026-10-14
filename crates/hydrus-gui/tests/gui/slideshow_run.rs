//! The media viewer's slideshow moves on as the reference's does
//! (`oracle/record_slideshow_run.py`): a real viewer, its own timer and
//! menu, with time held still (the slideshow clock is held at the
//! recording's scripted times) and the recording's ticks replayed one by
//! one. Plain, stopped and resumed, shuffling, and "play media once through"
//! with a real video (libmpv; skipped where it is missing).

use std::time::{Duration, Instant};

use hydrus_gui::slideshow::{hold_clock, timer_ticks};
use hydrus_gui::{Bound, MainWindow, MediaViewerWindow, Pages, SearchPage, bind, headless, mpv};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use serde_json::Value;

/// Puts the slideshow clock back to the real one when a test ends.
struct HeldClock;

impl Drop for HeldClock {
    fn drop(&mut self) {
        hold_clock(None);
    }
}

fn manifest_hash(name: &str) -> String {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(hydrus_testkit::fixture_path(
            "legacy_db/basic.manifest.json",
        ))
        .unwrap(),
    )
    .unwrap();
    manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap()["hash"]
        .as_str()
        .unwrap()
        .to_string()
}

struct Opened {
    _dir: tempfile::TempDir,
    store: std::sync::Arc<Store>,
    ui: MainWindow,
    bound: Bound,
}

impl Opened {
    fn new() -> Self {
        let legacy = hydrus_testkit::legacy_fixture("basic");
        let dir = tempfile::tempdir().unwrap();
        import_legacy(
            legacy.path(),
            &dir.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store = Store::open(dir.path()).unwrap();
        let _windows = headless::init();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        Self {
            _dir: dir,
            store,
            ui,
            bound,
        }
    }

    /// A page of just these files, in the order the page gives them, and the
    /// viewer opened on `first` of them.
    fn viewer_on(
        &self,
        names: &[&str],
        first: &str,
    ) -> (MediaViewerWindow, Vec<hydrus_core::HashId>) {
        // (the fixture's videos are not found by a hash search, so a page of
        // everything stands for a page of just these)
        let everything = names.len() > 2;
        let query = if everything {
            let hashes: Vec<String> = names.iter().map(|n| manifest_hash(n)).collect();
            format!("system:hash = {}", hashes.join(" "))
        } else {
            "system:everything".to_owned()
        };
        self.ui.invoke_search_edited(query.into());
        self.ui.invoke_search_accepted();
        let files = self.bound.current.borrow().borrow().results().to_vec();
        if everything {
            assert_eq!(files.len(), names.len());
        }
        let id = self
            .store
            .read(|c| hydrus_store::master::hash_id(c, &manifest_hash(first).parse().unwrap()))
            .unwrap()
            .unwrap();
        let at = files.iter().position(|&f| f == id).unwrap();
        self.ui
            .invoke_thumbnail_activated(i32::try_from(at).unwrap());
        let viewer = self
            .bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the viewer opened");
        (viewer, files)
    }
}

fn spin(until: &dyn Fn() -> bool) -> bool {
    let started = Instant::now();
    while !until() && started.elapsed() < Duration::from_secs(20) {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(5));
    }
    until()
}

/// Let the viewer's timer read `seconds` on the held clock: it has run twice
/// since, so one whole pass has.
fn tick(seconds: f64) {
    hold_clock(Some(seconds));
    let before = timer_ticks();
    assert!(spin(&|| timer_ticks() >= before + 2), "the timer never ran");
}

fn menu_id(viewer: &MediaViewerWindow, label: &str) -> i32 {
    use slint::Model as _;
    viewer.invoke_context_menu_requested();
    let menu = viewer.get_context_menu();
    let groups = &menu.slideshow;
    [&groups.g1, &groups.g2, &groups.g3, &groups.g4]
        .into_iter()
        .flat_map(|rows| rows.iter().collect::<Vec<_>>())
        .find(|r| r.label.starts_with(label))
        .unwrap_or_else(|| panic!("{label} in the slideshow menu"))
        .id
}

fn start(viewer: &MediaViewerWindow, seconds: f64, at: f64) {
    hold_clock(Some(at));
    viewer.invoke_menu_chosen(menu_id(viewer, "custom interval"));
    viewer.invoke_period_answered(true, format!("{seconds}").into());
    assert!(!viewer.get_period_asked());
}

/// The position shown, from the viewer's caption ("2/4" is 1).
fn position(viewer: &MediaViewerWindow) -> usize {
    let caption = viewer.get_caption().to_string();
    caption.split('/').next().unwrap().parse::<usize>().unwrap() - 1
}

// leaf: audit-media-viewer-slideshow
#[test]
fn a_slideshow_moves_on_and_stops_and_shuffles_as_the_reference_s() {
    let _clock = HeldClock;
    let fixture = hydrus_testkit::fixture_json("slideshow_run.json");
    let names: Vec<&str> = fixture["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    let rank = |name: &Value| {
        names
            .iter()
            .position(|n| n == &name.as_str().unwrap())
            .unwrap()
    };
    let scenario = |name: &str| -> &Value {
        fixture["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap()
    };
    let opened = Opened::new();
    let (viewer, _files) = opened.viewer_on(&names, names[0]);
    // (the reference's viewer started on its first file; ours is the one the
    // page gives first, and the files are told apart by their place)
    let first = position(&viewer);

    // a plain slideshow, around its period and past the end of the list
    let plain = scenario("plain");
    let period = plain["period"].as_f64().unwrap();
    let begun = 2000.0;
    start(&viewer, period, begun);
    let mut checked = 0;
    for tick_case in plain["ticks"].as_array().unwrap() {
        tick(begun + tick_case["at"].as_f64().unwrap());
        let want = (first + rank(&tick_case["shown"])) % names.len();
        assert_eq!(position(&viewer), want, "{tick_case}");
        checked += 1;
    }
    assert_eq!(checked, 11);

    // stopped, it never moves; resumed, it moves a period after
    let stopped = scenario("stopped and resumed");
    viewer.invoke_menu_chosen(menu_id(&viewer, "stop"));
    let held = position(&viewer);
    for tick_case in stopped["stopped"].as_array().unwrap() {
        tick(begun + 100.0 + tick_case["at"].as_f64().unwrap());
        assert_eq!(position(&viewer), held, "{tick_case}");
    }
    let resumed_at = begun + 300.0;
    hold_clock(Some(resumed_at));
    viewer.invoke_menu_chosen(menu_id(&viewer, "resume"));
    for tick_case in stopped["resumed"].as_array().unwrap() {
        tick(resumed_at + tick_case["at"].as_f64().unwrap());
        let moved = position(&viewer) != held;
        // (the recording's list had four files and it resumed on the last;
        // what it did is whether it had moved by then)
        let shown = rank(&tick_case["shown"]);
        assert_eq!(
            moved,
            shown != rank(&stopped["resumed_from"]),
            "{tick_case}"
        );
    }
    viewer.invoke_menu_chosen(menu_id(&viewer, "stop"));

    // shuffling: nothing moves before the period; at it, to another file
    let shuffle = scenario("shuffle");
    viewer.invoke_menu_chosen(menu_id(&viewer, "shuffle this slideshow"));
    let mut now = 5000.0;
    start(&viewer, period, now);
    let mut seen = std::collections::BTreeSet::new();
    for tick_case in shuffle["ticks"].as_array().unwrap() {
        let before = position(&viewer);
        tick(now + period - 0.1);
        assert_eq!(position(&viewer), before, "early: {tick_case}");
        now += period + 0.5;
        tick(now);
        let after = position(&viewer);
        // the reference's never picks the file it is on, and the recording
        // shows the same
        assert_ne!(tick_case["from"], tick_case["to"]);
        assert_ne!(after, before, "shuffled to the same file: {tick_case}");
        seen.insert(after);
    }
    assert!(seen.len() > 1, "{seen:?}");
    viewer.invoke_close_requested();
}

// leaf: audit-media-viewer-slideshow
#[test]
fn a_slideshow_waits_for_a_video_to_play_once_through_when_told_to() {
    if mpv::skip_without_libmpv() {
        return;
    }
    let _clock = HeldClock;
    let fixture = hydrus_testkit::fixture_json("slideshow_run.json");
    let scenario = fixture["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "file with a duration")
        .unwrap();
    let period = scenario["period"].as_f64().unwrap();
    // (the recording turned the options that bend a period off)
    let opened = Opened::new();
    // (a file with a duration that libmpv plays; the fixture's own media
    // files are not in the store)
    hydrus_import::FileImporter::new(opened.store.clone(), hydrus_media::MediaTools::new())
        .import_path(
            &hydrus_testkit::fixture_path("media/flac.flac"),
            &hydrus_import::FileImportOptions::default(),
        )
        .unwrap();
    opened
        .ui
        .invoke_search_edited("system:filetype is flac".into());
    opened.ui.invoke_search_accepted();
    let flac = opened.bound.current.borrow().borrow().results()[0];
    opened
        .store
        .write(|ctx| {
            let mut settings: hydrus_core::media_viewer::SlideshowSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.short_loop_percentage = None;
            settings.short_loop_seconds = None;
            settings.short_cutoff_percentage = None;
            settings.long_overspill_percentage = None;
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    for run in scenario["runs"].as_array().unwrap() {
        let once_through = run["once_through"].as_bool().unwrap();
        opened.ui.invoke_search_edited("system:everything".into());
        opened.ui.invoke_search_accepted();
        let all = opened.bound.current.borrow().borrow().results().to_vec();
        opened.ui.invoke_thumbnail_activated(
            i32::try_from(all.iter().position(|&f| f == flac).unwrap()).unwrap(),
        );
        let viewer = opened
            .bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .unwrap();
        let video = position(&viewer);
        // the video does not play on until the slideshow has asked it to
        let text = || viewer.get_scanbar_text().to_string();
        let before = text();
        assert!(
            spin(&|| text() != before),
            "the file never began to play ({once_through}) {before}"
        );
        viewer.invoke_toggle_pause();
        if once_through {
            viewer.invoke_menu_chosen(menu_id(&viewer, "this slideshow plays media once through"));
        }
        let begun = 2000.0;
        start(&viewer, period, begun);
        for tick_case in run["ticks"].as_array().unwrap() {
            let at = tick_case["at"].as_f64().unwrap();
            let played = tick_case["played_once_through"].as_bool().unwrap();
            if played {
                // the video plays through, in real time
                viewer.invoke_toggle_pause();
            }
            tick(begun + at);
            let moved = tick_case["shown"] != scenario["files"][0];
            if moved && played {
                assert!(spin(&|| position(&viewer) != video), "{tick_case}");
            } else {
                assert_eq!(position(&viewer) != video, moved, "{tick_case}");
            }
            if moved {
                break;
            }
        }
        viewer.invoke_close_requested();
    }
}
