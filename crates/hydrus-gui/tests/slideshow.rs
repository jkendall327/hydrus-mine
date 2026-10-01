//! The media viewer's slideshow is the reference's
//! (`oracle/record_slideshow.py`): the period a file with a duration bends
//! a slideshow's to, for many durations, periods and slideshow options,
//! is the one the reference's viewer works out; and the slideshow
//! submenu, as a viewer's slideshow starts, stops, resumes and shuffles,
//! is the one the reference's viewer shows.

use hydrus_core::media_viewer::SlideshowSettings;
use hydrus_gui::slideshow::{Shown, Slideshow, Special, slideshow_menu, special_period};
use serde_json::{Value, json};

mod common;
use common::menus::described;

#[test]
fn files_with_durations_bend_the_period_as_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("slideshow.json");
    let cases = fixture["periods"].as_array().unwrap();
    let mut bent = 0;
    for case in cases {
        let option = |i: usize| case["options"][i].as_i64();
        let settings = SlideshowSettings {
            short_loop_percentage: option(0),
            short_loop_seconds: option(1),
            short_cutoff_percentage: option(2),
            long_overspill_percentage: option(3),
            ..SlideshowSettings::default()
        };
        let period = case["period"].as_f64().unwrap();
        let ours = special_period(case["duration"].as_f64(), period, &settings);
        let theirs = if case["stopped"].as_bool().unwrap() {
            Special::Stop
        } else {
            match case["special"].as_f64() {
                Some(seconds) => Special::Period {
                    seconds,
                    stop_at_end: case["stop_at_end"].as_bool().unwrap(),
                },
                None => Special::None,
            }
        };
        assert_eq!(ours, theirs, "{case}");
        bent += usize::from(matches!(theirs, Special::Period { .. }));
    }
    // (every case seen, and the bending in many)
    assert_eq!(cases.len(), 9 * 7 * 35);
    assert!(bent > 300, "{bent}");
}

#[test]
fn the_slideshow_menu_is_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("slideshow.json");
    let mut settings = SlideshowSettings::default();
    let mut slideshow = Slideshow::new(&settings);
    let mut now = 0.0;
    let steps = fixture["menus"].as_array().unwrap();
    for step in steps {
        now += 1.0;
        let name = step["step"].as_str().unwrap();
        // (a still is shown, as in the reference's viewer)
        let shown = Shown::Still;
        match name {
            "fresh" => {}
            "started at a minute" => slideshow.start(60.0, now, shown, &settings),
            "stopped" | "resumed" => slideshow.pause_play(now, shown, &settings),
            "this one shuffles" => slideshow.set_shuffling(!slideshow.shuffling()),
            "this one plays through" => slideshow.set_once_through(!slideshow.once_through()),
            "very fast, stopped" => {
                slideshow.start(0.08, now, shown, &settings);
                slideshow.pause_play(now, shown, &settings);
            }
            "two and a half seconds" => slideshow.start(2.5, now, shown, &settings),
            "every one shuffles" | "every one shuffles no more" => {
                settings.shuffle = !settings.shuffle;
                slideshow.set_shuffling(settings.shuffle);
            }
            "every one plays through" => {
                settings.once_through = !settings.once_through;
                slideshow.set_once_through(settings.once_through);
            }
            "started at nothing" => slideshow.start(0.0, now, shown, &settings),
            "two hours" => {
                slideshow.start(7200.0, now, shown, &settings);
                slideshow.pause_play(now, shown, &settings);
            }
            other => panic!("a step not replayed: {other}"),
        }
        let ours: Vec<Value> = described(&[slideshow_menu(&slideshow, &settings)]);
        let theirs = json!([step["menu"]]);
        assert_eq!(json!(ours), theirs, "{name}");
    }
    assert_eq!(steps.len(), 13);
}

/// The slideshow submenu's rows, by label: their ids, and whether checked.
fn slideshow_rows(menu: &hydrus_gui::ThumbnailMenu) -> Vec<(String, i32, bool)> {
    use slint::Model as _;
    let groups = &menu.slideshow;
    [&groups.g1, &groups.g2, &groups.g3, &groups.g4]
        .into_iter()
        .flat_map(|rows| {
            rows.iter()
                .map(|r| (r.label.to_string(), r.id, r.checkable && r.checked))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn a_slideshow_runs_in_the_viewer_from_its_menu() {
    use std::time::{Duration, Instant};

    use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
    use hydrus_store::Store;
    use hydrus_store::import::import_legacy;

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let menu = || {
        viewer.invoke_context_menu_requested();
        let menu = viewer.get_context_menu();
        (menu.slideshow_title.to_string(), slideshow_rows(&menu))
    };
    let find = |rows: &[(String, i32, bool)], label: &str| {
        rows.iter()
            .find(|(l, ..)| l == label)
            .unwrap_or_else(|| panic!("{label} in {rows:?}"))
            .1
    };
    let checked = |rows: &[(String, i32, bool)], label: &str| {
        rows.iter().any(|(l, _, checked)| l == label && *checked)
    };
    // the timers run for a while: the captions shown
    let run = |time: Duration| {
        let mut shown = vec![viewer.get_caption().to_string()];
        let started = Instant::now();
        while started.elapsed() < time {
            slint::platform::update_timers_and_animations();
            let caption = viewer.get_caption().to_string();
            if shown.last() != Some(&caption) {
                shown.push(caption);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        shown
    };

    // nothing moves on its own
    let (title, rows) = menu();
    assert_eq!(title, "start slideshow");
    assert_eq!(run(Duration::from_millis(300)).len(), 1);
    // very fast: it moves on
    viewer.invoke_menu_chosen(find(&rows, "very fast"));
    let shown = run(Duration::from_secs(2));
    assert!(shown.len() > 1, "{shown:?}");
    // stopped, it stays where it is
    let (title, rows) = menu();
    assert_eq!(title, "slideshow running");
    viewer.invoke_menu_chosen(find(&rows, "stop (80 milliseconds)"));
    let (title, rows) = menu();
    assert_eq!(title, "start slideshow");
    assert_eq!(rows[0].0, "resume at 80 milliseconds");
    assert_eq!(run(Duration::from_millis(300)).len(), 1);

    // a custom interval is asked for; one that doesn't read is said so
    viewer.invoke_menu_chosen(find(&rows, "custom interval"));
    assert!(viewer.get_period_asked());
    viewer.invoke_period_answered(true, "soon".into());
    assert!(!viewer.get_period_asked());
    assert_eq!(
        viewer.get_warning(),
        "Could not parse that slideshow period!"
    );
    viewer.set_warning("".into());
    // escape asks no more, and starts nothing
    let (_, rows) = menu();
    viewer.invoke_menu_chosen(find(&rows, "custom interval"));
    viewer.invoke_period_answered(false, String::new().into());
    assert!(!viewer.get_period_asked());
    assert_eq!(menu().0, "start slideshow");
    // one that reads starts it
    let (_, rows) = menu();
    viewer.invoke_menu_chosen(find(&rows, "custom interval"));
    viewer.invoke_period_answered(true, "2.5".into());
    let (title, rows) = menu();
    assert_eq!(title, "slideshow running");
    assert_eq!(rows[0].0, "stop (2.5 seconds)");

    // shuffling: this slideshow's, then every one's, kept in the options
    assert!(!checked(&rows, "shuffle this slideshow"));
    viewer.invoke_menu_chosen(find(&rows, "shuffle this slideshow"));
    let (_, rows) = menu();
    assert!(checked(&rows, "shuffle this slideshow"));
    assert!(!checked(&rows, "all slideshows shuffle"));
    viewer.invoke_menu_chosen(find(&rows, "all slideshows shuffle"));
    let (_, rows) = menu();
    assert!(checked(&rows, "all slideshows shuffle"));
    let kept: SlideshowSettings = store.read(hydrus_store::settings::get).unwrap();
    assert!(kept.shuffle && !kept.once_through);
    viewer.invoke_menu_chosen(find(&rows, "this slideshow plays media once through"));
    let (_, rows) = menu();
    assert!(checked(&rows, "this slideshow plays media once through"));
    assert!(!checked(&rows, "always play media once through"));
    viewer.invoke_close_requested();
}

#[test]
fn a_slideshow_moving_on_leaves_the_menu_and_questions_with_their_file() {
    use std::time::{Duration, Instant};

    use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
    use hydrus_store::Store;
    use hydrus_store::import::import_legacy;

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:inbox".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    assert!(files.len() > 3);
    ui.invoke_thumbnail_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let rows = || {
        use slint::Model as _;
        viewer.invoke_context_menu_requested();
        let menu = viewer.get_context_menu();
        let mut rows: Vec<(String, i32)> = Vec::new();
        for part in [&menu.filter, &menu.delete, &menu.dismiss] {
            rows.extend(part.iter().map(|r| (r.label.to_string(), r.id)));
        }
        rows.extend(slideshow_rows(&menu).into_iter().map(|(l, id, _)| (l, id)));
        rows
    };
    let find = |rows: &[(String, i32)], label: &str| {
        rows.iter()
            .find(|(l, _)| l.starts_with(label))
            .unwrap_or_else(|| panic!("{label} in {rows:?}"))
            .1
    };
    let inboxed = |file| {
        !store
            .read(|c| hydrus_store::media::inboxed(c, &[file]))
            .unwrap()
            .is_empty()
    };
    // the menu for the first; then (as a slideshow would) the second shown:
    // archive archives the first
    let menu = rows();
    viewer.invoke_next();
    viewer.invoke_menu_chosen(find(&menu, "archive"));
    assert!(!inboxed(files[0]));
    assert!(inboxed(files[1]));
    // delete, asked, deletes the first (which leaves the viewer), the
    // second still shown
    let menu = rows();
    viewer.invoke_next();
    let shown = viewer.get_caption().to_string();
    assert_eq!(shown, format!("3/{}", files.len()));
    viewer.invoke_menu_chosen(find(&menu, "delete from "));
    assert!(!viewer.get_question().is_empty());
    viewer.invoke_answer(true);
    assert_eq!(viewer.get_caption(), format!("2/{}", files.len() - 1));
    assert!(!page.borrow().results().contains(&files[1]));
    assert!(page.borrow().results().contains(&files[2]));

    // a slideshow waits while the viewer asks
    let run = |time: Duration| {
        let before = viewer.get_caption().to_string();
        let started = Instant::now();
        while started.elapsed() < time {
            slint::platform::update_timers_and_animations();
            if viewer.get_caption() != before {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    };
    viewer.invoke_menu_chosen(find(&rows(), "very fast"));
    viewer.invoke_delete();
    assert!(!viewer.get_question().is_empty());
    assert!(!run(Duration::from_millis(500)), "moved on while asking");
    viewer.invoke_answer(false);
    assert!(run(Duration::from_secs(5)), "never moved on again");
    viewer.invoke_close_requested();
}
