//! Help > debug > report modes, clicked in the real menu and compared with the
//! reference's own reports for the same situations (the recordings
//! `oracle/fixtures/debug_*.json`, made by `oracle/record_debug_*.py`).
use super::debug_menu_actions::{ONE_AT_A_TIME, Reset, capture, capture_here, start};
use hydrus_core::debug_flags;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::settings::{self, GuiIdleSettings};
use serde_json::Value;
use slint::ComponentHandle as _;

// leaf: audit-options-help-debug-action-idle-report-mode
#[test]
fn idle_report_mode_says_what_the_reference_says_in_each_situation() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // the flags are process-wide: put them back even if an assertion fails
    let _reset = Reset;
    let d = start();
    let seen = capture_here();
    let recording = hydrus_testkit::fixture_json("debug_idle_report.json");
    assert_eq!(
        recording["menu_path"],
        serde_json::json!(["help", "debug", "report modes", "idle report mode"])
    );
    let now: i64 = recording["now_ms"].as_i64().unwrap();
    let monitor = d.bound.session_autosave.clone();
    // off, it says nothing
    assert!(!d.report_mode_checked("idle report mode"));
    monitor.idle_at(now);
    assert!(seen.lock().unwrap().is_empty());
    d.click(&["report modes", "idle report mode"]);
    assert!(d.report_mode_checked("idle report mode"));
    let seconds = |sc: &Value, key: &str, default: Option<u64>| -> Option<u64> {
        match sc.get(key) {
            Some(Value::Null) => None,
            Some(v) => v.as_u64(),
            None => default,
        }
    };
    let mut forced_on = false;
    let mut shutting_down = None;
    for sc in recording["scenarios"].as_array().unwrap() {
        let name = sc["name"].as_str().unwrap();
        if sc["shutting_down"] == true {
            shutting_down = Some(sc.clone());
            continue;
        }
        let forced = sc["force"] == true;
        if forced != forced_on {
            d.click(&["debug modes", "force idle mode"]);
            forced_on = forced;
        }
        let config = GuiIdleSettings {
            enabled: sc.get("normal").is_none_or(|n| n == true),
            user_seconds: seconds(sc, "user_period", Some(1800)),
            mouse_seconds: seconds(sc, "mouse_period", Some(600)),
            api_seconds: seconds(sc, "api_timeout", None),
            ..GuiIdleSettings::default()
        };
        d.store
            .write(move |ctx| settings::set(ctx.conn(), &config))
            .unwrap();
        let ago = |key: &str| now - sc[key].as_i64().unwrap() * 1000;
        monitor.boot_at(ago("boot"));
        monitor.user_at(ago("user"));
        monitor.mouse_at(ago("mouse"));
        monitor.api_at(ago("api"));
        monitor.set_previously_idle(sc["was_idle"] == true);
        seen.lock().unwrap().clear();
        let answer = monitor.idle_at(now);
        let said: Vec<String> = std::mem::take(&mut *seen.lock().unwrap());
        let recorded: Vec<String> = sc["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(said, recorded, "{name}");
        assert_eq!(answer, sc["result"] == true, "{name}");
    }
    if forced_on {
        d.click(&["debug modes", "force idle mode"]);
    }
    // closing the program: never idle (the binding is retired)
    if let Some(sc) = shutting_down {
        drop(d.bound);
        seen.lock().unwrap().clear();
        let answer = monitor.idle_at(now);
        assert_eq!(
            *seen.lock().unwrap(),
            [sc["messages"][0].as_str().unwrap().to_owned()]
        );
        assert!(!answer);
    }
}

/// Puts the switches these tests turn on, and the sink, back when dropped:
/// they are the whole client's, so a failure must not leave them on.
struct FlagsReset;
impl Drop for FlagsReset {
    fn drop(&mut self) {
        for flag in [
            debug_flags::Flag::ShortcutReport,
            debug_flags::Flag::GuiReport,
            debug_flags::Flag::AutocompleteDelay,
            debug_flags::Flag::CacheReport,
            debug_flags::Flag::Blurhash,
            debug_flags::Flag::SubprocessReport,
            debug_flags::Flag::FileImportReport,
            debug_flags::Flag::FileReport,
            debug_flags::Flag::SimilarFilesMetadataGenerationReport,
            debug_flags::Flag::NetworkReport,
            debug_flags::Flag::DaemonReport,
        ] {
            flag.set(false);
        }
        debug_flags::set_sink(None);
    }
}

/// The keys the reference's recording pressed, as the main window's key
/// callback is given them: its text and modifier bits (ctrl 1, alt 2, shift
/// 4, keypad 8).
fn window_key(name: &str) -> (&'static str, i32) {
    match name {
        "j" => ("j", 0),
        "f5" => ("\u{F708}", 0),
        "shift+j" => ("J", 4),
        "ctrl+j" => ("j", 1),
        "alt+j" => ("j", 2),
        "ctrl+f5" => ("\u{F708}", 1),
        "numpad 5" => ("5", 8),
        "return" => ("\n", 0),
        "space" => (" ", 0),
        "escape" => ("\u{1b}", 0),
        other => panic!("a key the recording pressed that this test does not know: {other}"),
    }
}

/// What the reference said, made into what hydrus-rs says: its Python repr of
/// the main window is named, and the Qt list boxes' "virtual size" lines,
/// which have no counterpart here, are left out.
fn reference_lines(lines: &Value) -> Vec<String> {
    lines
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .filter(|l| !l.starts_with("Setting a virtual size on "))
        .map(|l| l.replace("<main window>", "main window"))
        .collect()
}

// leaf: audit-options-help-debug-action-shortcut-report-mode
#[test]
fn shortcut_report_mode_says_what_the_reference_says_for_each_key() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = FlagsReset;
    let d = start();
    let seen = capture_here();
    let recording = hydrus_testkit::fixture_json("debug_report_modes_gui.json");
    assert_eq!(
        recording["menu_path"],
        serde_json::json!(["help", "debug", "report modes"])
    );
    // the reference's words for the commands it ran are the ones named here
    for (id, words) in recording["commands"].as_object().unwrap() {
        let id: i32 = id.parse().unwrap();
        assert_eq!(
            hydrus_gui_model::shortcut_capture::command_name(id),
            words.as_str().unwrap()
        );
    }
    for key in recording["keys"].as_array().unwrap() {
        let name = key["name"].as_str().unwrap();
        let (text, bits) = window_key(name);
        // off: nothing
        assert!(!d.report_mode_checked("shortcut report mode"));
        seen.lock().unwrap().clear();
        d.ui.invoke_shortcut_key(text.into(), bits);
        assert!(seen.lock().unwrap().is_empty(), "{name}: silent while off");
        // on: what the reference said, with its Python repr of the window named
        d.click(&["report modes", "shortcut report mode"]);
        assert!(d.report_mode_checked("shortcut report mode"));
        seen.lock().unwrap().clear();
        d.ui.invoke_shortcut_key(text.into(), bits);
        assert_eq!(
            *seen.lock().unwrap(),
            reference_lines(&key["shortcut"]),
            "{name}"
        );
        d.click(&["report modes", "shortcut report mode"]);
    }
    debug_flags::set_sink(None);
}

// leaf: audit-options-help-debug-action-gui-report-mode
#[test]
fn gui_report_mode_says_what_the_reference_says_for_keys_and_the_manage_tags_panel() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = FlagsReset;
    let d = start();
    let seen = capture_here();
    let recording = hydrus_testkit::fixture_json("debug_report_modes_gui.json");
    // key events
    for key in recording["keys"].as_array().unwrap() {
        let name = key["name"].as_str().unwrap();
        let (text, bits) = window_key(name);
        assert!(!d.report_mode_checked("gui report mode"));
        seen.lock().unwrap().clear();
        d.ui.invoke_shortcut_key(text.into(), bits);
        assert!(seen.lock().unwrap().is_empty(), "{name}: silent while off");
        d.click(&["report modes", "gui report mode"]);
        assert!(d.report_mode_checked("gui report mode"));
        seen.lock().unwrap().clear();
        d.ui.invoke_shortcut_key(text.into(), bits);
        assert_eq!(
            *seen.lock().unwrap(),
            reference_lines(&key["gui"]),
            "{name}"
        );
        d.click(&["report modes", "gui report mode"]);
    }
    // the manage tags panel opening, on the same files
    d.ui.invoke_search_edited("system:everything".into());
    d.ui.invoke_search_accepted();
    d.ui.invoke_select_all();
    seen.lock().unwrap().clear();
    d.ui.invoke_manage_tags_selected();
    assert!(
        seen.lock().unwrap().is_empty(),
        "opening manage tags is silent while off"
    );
    let open = d
        .bound
        .manage_tags
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    open.invoke_cancel();
    d.click(&["report modes", "gui report mode"]);
    d.ui.invoke_select_all();
    seen.lock().unwrap().clear();
    d.ui.invoke_manage_tags_selected();
    assert!(d.bound.manage_tags.borrow().is_some(), "the panel opened");
    let services: Vec<(String, String)> = recording["tag_services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_owned(),
                s["key"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    // (the reference's repr of a service is its object, named here by its name)
    let expected: Vec<String> = recording["manage_tags"]["gui"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .filter(|l| !l.starts_with("Setting a virtual size on "))
        .scan(0, |at, line| {
            let line = if line.starts_with("Opening manage tags on these services: ") {
                let names: Vec<&str> = services.iter().map(|(n, _)| n.as_str()).collect();
                format!(
                    "Opening manage tags on these services: [{}]",
                    names.join(", ")
                )
            } else if line.starts_with("Opening manage tags panel on ") {
                let (name, _) = &services[*at];
                *at += 1;
                line.replacen("<service>", name, 1)
            } else {
                line.to_owned()
            };
            Some(line)
        })
        .collect();
    assert_eq!(expected.len(), 5, "{expected:?}");
    assert_eq!(*seen.lock().unwrap(), expected);
    d.click(&["report modes", "gui report mode"]);
    debug_flags::set_sink(None);
}

// leaf: audit-options-help-debug-action-autocomplete-delay-mode
#[test]
fn autocomplete_delay_mode_holds_a_typed_search_for_the_reference_s_three_seconds() {
    use slint::Model as _;
    use std::time::{Duration, Instant};

    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = FlagsReset;
    let d = start();
    let recording = hydrus_testkit::fixture_json("debug_report_modes_gui.json");
    let delay = &recording["autocomplete"];
    assert_eq!(
        delay["menu_path"],
        serde_json::json!(["help", "debug", "gui actions", "autocomplete delay mode"])
    );
    let held = Duration::from_secs_f64(delay["on"]["wildcard search"]["seconds"].as_f64().unwrap());
    let quick = delay["off"]["wildcard search"]["seconds"].as_f64().unwrap();
    assert!((held.as_secs_f64() - 3.0).abs() < f64::EPSILON && quick < 1.0);
    // typed into the search box, as the reference's wildcard search is
    let typed = || {
        let started = Instant::now();
        d.ui.invoke_search_edited("b*".into());
        (started.elapsed(), d.ui.get_suggestions().row_count())
    };
    assert!(!d.checked_in("gui actions", "autocomplete delay mode"));
    let (before, found) = typed();
    assert!(before < held, "{before:?}");
    assert!(found > 0, "the search finds something to delay");
    d.click(&["gui actions", "autocomplete delay mode"]);
    assert!(d.checked_in("gui actions", "autocomplete delay mode"));
    let (delayed, found_delayed) = typed();
    // (one side only: a slow machine may take longer)
    assert!(delayed >= held, "{delayed:?}");
    assert_eq!(found_delayed, found, "the delay changes nothing else");
    d.click(&["gui actions", "autocomplete delay mode"]);
    assert!(!d.checked_in("gui actions", "autocomplete delay mode"));
    let (after, _) = typed();
    assert!(after < held, "{after:?}");
}

/// A cache report with what varies named: which cache, which item, and sizes.
fn cache_shape(line: &str) -> String {
    let named =
        regex::Regex::new(r#"^Cache "[^"]*" (adding|removing (?:oldest|specific) item) "[^"]*""#)
            .unwrap()
            .replace(line, r#"Cache "<cache>" $1 "<key>""#)
            .into_owned();
    regex::Regex::new(r"\d[\d,]*(?:\.\d+)? ?[KMGT]?B")
        .unwrap()
        .replace_all(&named, "<size>")
        .into_owned()
}

// leaf: audit-options-help-debug-action-cache-report-mode
#[test]
fn cache_report_mode_says_what_the_reference_says_as_the_thumbnail_cache_fills() {
    use super::debug_menu_actions::Debug;
    use hydrus_store::settings::ThumbnailCacheSettings;
    use slint::Model as _;

    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = FlagsReset;
    let (dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // a cache too small for two thumbnails, so that filling it evicts
    bound.rows.set_cache_policy(ThumbnailCacheSettings {
        bytes: 1_000,
        timeout: 1200,
    });
    let adapter = windows.get(0).unwrap();
    for _ in 0..3 {
        headless::render(&adapter, 1400, 1000);
    }
    let d = Debug::from_parts(dirs, store, ui, bound, windows);
    let seen = capture_here();
    let recording = hydrus_testkit::fixture_json("debug_report_modes.json");
    let recorded: std::collections::BTreeSet<String> = recording["cache"]["on"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|step| step["messages"].as_array().unwrap())
        .map(|m| cache_shape(m["text"].as_str().unwrap()))
        .collect();
    assert_eq!(recorded.len(), 3, "{recorded:?}");
    assert!(
        recording["cache"]["off"]
            .as_array()
            .unwrap()
            .iter()
            .all(|step| step["messages"].as_array().unwrap().is_empty())
    );
    // the cache says what it adds and evicts, in the reference's words
    d.click(&["report modes", "cache report mode"]);
    assert!(d.report_mode_checked("cache report mode"));
    d.ui.invoke_search_edited("system:everything".into());
    d.ui.invoke_search_accepted();
    d.bound.rows.set_columns(1);
    let total = d.bound.current.borrow().borrow().results().len();
    for i in 0..total {
        d.bound.rows.row_data(i).unwrap();
        d.bound.rows.wait();
    }
    let said = seen.lock().unwrap().clone();
    assert!(said.len() >= 4, "{said:?}");
    for line in &said {
        assert!(recorded.contains(&cache_shape(line)), "{line}");
    }
    assert!(said.iter().any(|l| l.contains(" adding ")), "{said:?}");
    assert!(
        said.iter().any(|l| l.contains(" removing oldest item ")),
        "{said:?}"
    );
}

/// The pixel the recording sampled at `column` and `row` of 4 across and down.
fn grid_sample(
    image: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    column: u32,
    row: u32,
) -> [u8; 3] {
    let (width, height) = (image.width(), image.height());
    let at_x = ((column * width) / 4 + width / 8).min(width - 1);
    let at_y = ((row * height) / 4 + height / 8).min(height - 1);
    let pixel = image.as_slice()[(at_y * width + at_x) as usize];
    [pixel.r, pixel.g, pixel.b]
}

// leaf: audit-options-help-debug-action-blurhash-mode
#[test]
fn blurhash_mode_draws_the_thumbnails_the_reference_draws_from_each_blurhash() {
    use hydrus_store::master;
    use slint::Model as _;

    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let adapter = windows.get(0).unwrap();
    for _ in 0..3 {
        headless::render(&adapter, 1400, 1000);
    }
    let d = super::debug_menu_actions::Debug::from_parts(dirs, store, ui, bound, windows);
    let _reset = FlagsReset;
    let recording = hydrus_testkit::fixture_json("debug_report_modes_gui.json");
    d.ui.invoke_search_edited("system:everything".into());
    d.ui.invoke_search_accepted();
    d.bound.rows.set_columns(1);
    let results = d.bound.current.borrow().borrow().results().to_vec();
    let drawn = |hash: &str| {
        let hash = hydrus_core::Sha256::from_slice(&hex::decode(hash).unwrap()).unwrap();
        let id = d
            .store
            .read(|conn| master::hash_id(conn, &hash))
            .unwrap()
            .unwrap();
        let index = results.iter().position(|r| *r == id).unwrap();
        d.bound.rows.row_data(index).unwrap();
        d.bound.rows.wait();
        d.bound
            .rows
            .row_data(index)
            .unwrap()
            .thumbnails
            .row_data(0)
            .unwrap()
            .image
            .to_rgba8()
            .unwrap()
    };
    let rows: Vec<&serde_json::Value> = recording["blurhash"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["blurhash"].is_string() && r["on"]["shape"][2] == 3)
        .collect();
    assert_eq!(rows.len(), 30);
    let check = |which: &str| {
        let mut worst = 0;
        for row in &rows {
            let image = drawn(row["hash"].as_str().unwrap());
            let shape = &row[which]["shape"];
            assert_eq!(
                [i64::from(image.height()), i64::from(image.width())],
                [shape[0].as_i64().unwrap(), shape[1].as_i64().unwrap()],
                "{which}: {}",
                row["hash"]
            );
            for (i, sample) in row[which]["samples"].as_array().unwrap().iter().enumerate() {
                let mine = grid_sample(&image, (i % 4) as u32, (i / 4) as u32);
                for (c, theirs) in sample.as_array().unwrap().iter().take(3).enumerate() {
                    let off = (i32::from(mine[c])
                        - i32::try_from(theirs.as_i64().unwrap()).unwrap())
                    .abs();
                    worst = worst.max(off);
                }
            }
        }
        // (every sampled pixel is the reference's, to the digit)
        assert_eq!(worst, 0, "{which}: worst sample difference");
    };
    // off: the stored thumbnails, as the reference draws them
    assert!(!d.report_mode_checked("blurhash mode"));
    check("off");
    // on: the blurhash of each file, as the reference's recovery draws it
    // (the switch is the whole client's: on only while the thumbnails are
    // drawn, so that other tests' are not)
    d.click(&["report modes", "blurhash mode"]);
    assert!(d.report_mode_checked("blurhash mode"));
    d.bound.rows.clear_thumbnail_cache();
    check("on");
    d.click(&["report modes", "blurhash mode"]);
    // and the mode really changes what is drawn
    assert!(
        rows.iter()
            .any(|r| r["on"]["samples"] != r["off"]["samples"])
    );
}

// leaf: audit-options-help-debug-action-subprocess-report-mode
#[test]
fn subprocess_report_mode_says_what_the_reference_says_through_the_external_call_test() {
    use super::external_calls::{call, named, open};
    use hydrus_core::external_calls::{ActualCall, Callable, Manager, Pipeline, Process};

    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = FlagsReset;
    let d = start();
    let seen = capture();
    let recording = hydrus_testkit::fixture_json("debug_report_modes.json");
    let echo = recording["subprocess"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["on"] == true && c["cmd"] == serde_json::json!(["echo", "hello"]))
        .unwrap();
    let recorded: Vec<String> = echo["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["text"].as_str().unwrap().to_owned())
        .collect();
    // the call, saved, then opened in Options > external programs and tested
    let mut callable = Callable::new("report probe");
    callable.pipeline = Pipeline::File;
    callable.call = ActualCall::Process(Process {
        executable: "echo".into(),
        arguments: vec!["hello".into()],
        ..Process::default()
    });
    let manager = Manager {
        calls: vec![callable],
    };
    d.store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &manager))
        .unwrap();
    let options = open(&d.ui, &d.bound);
    options.invoke_external_call_clicked(named(&options, "report probe"), false, false);
    options.invoke_external_call_action("edit".into());
    let child = call(&d.bound);
    let run = || {
        child.invoke_test(false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while child.get_testing() {
            assert!(
                std::time::Instant::now() < deadline,
                "the test did not finish"
            );
            slint::platform::update_timers_and_animations();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    };
    run();
    assert!(seen.lock().unwrap().is_empty(), "silent while off");
    d.click(&["report modes", "subprocess report mode"]);
    assert!(d.report_mode_checked("subprocess report mode"));
    run();
    // (other tests' programs run at the same time and say things too: the
    // four lines are found in order among them)
    let said = std::mem::take(&mut *seen.lock().unwrap());
    let kwargs = said
        .iter()
        .position(|l| l.starts_with("KWargs are: ") && l.contains("\"echo\", \"hello\""))
        .unwrap_or_else(|| panic!("{said:?}"));
    // the environment (this process's, in the reference's layout), how the
    // call is made, and what came back, a Python repr's quotes being Rust's
    assert!(recorded[0].starts_with("Full environment:\n"));
    assert!(recorded[1].starts_with("KWargs are: "));
    assert!(
        said[..kwargs]
            .iter()
            .any(|l| *l == hydrus_core::debug_flags::current_env_text())
    );
    let after = &said[kwargs + 1..];
    let stdout = recorded[2].replace('\'', "\"");
    let stderr = recorded[3].replace('\'', "\"");
    let at = after.iter().position(|l| *l == stdout).expect("stdout");
    assert!(after[at + 1..].contains(&stderr), "{after:?}");
    d.click(&["report modes", "subprocess report mode"]);
}

// (not a leaf tag: the words these modes report are tested in the crates that
// report them, `hydrus-import`, `hydrus-net` and `hydrus-media`; this is the
// menu's half, from the entry to the switch the daemon reads)
#[test]
fn the_report_mode_entries_turn_their_switches_on_and_pass_them_to_the_daemon() {
    use debug_flags::Flag;
    use hydrus_store::debug_switches::DebugSwitches;

    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let d = start();
    let _reset = FlagsReset;
    // the imports, files, hashes and network jobs these modes report on run
    // in the daemon, which reads the switches the client writes to the store
    let stored = || -> Vec<String> {
        d.store
            .read(hydrus_store::settings::get::<DebugSwitches>)
            .unwrap()
            .flags
    };
    for flag in [
        Flag::FileImportReport,
        Flag::FileReport,
        Flag::SimilarFilesMetadataGenerationReport,
        Flag::NetworkReport,
        Flag::DaemonReport,
        Flag::SubprocessReport,
        Flag::CacheReport,
        Flag::Blurhash,
    ] {
        let label = flag.label();
        assert!(!flag.is_on() && !d.report_mode_checked(label), "{label}");
        d.click(&["report modes", label]);
        assert!(flag.is_on() && d.report_mode_checked(label), "{label}");
        assert!(
            stored().contains(&label.to_owned()),
            "{label}: {:?}",
            stored()
        );
        d.click(&["report modes", label]);
        assert!(!flag.is_on() && !d.report_mode_checked(label), "{label}");
        assert!(!stored().contains(&label.to_owned()), "{label}");
    }
}
