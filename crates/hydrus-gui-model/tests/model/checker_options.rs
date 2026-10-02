//! The checker options editor against the reference's, recorded by
//! `oracle/record_checker_options.py`: its reasonable defaults, its layout
//! (labels, ranges, least values, which box shows) as it opens on given
//! checker options in normal and advanced mode, its state after each of a
//! run of edits, and "ok"'s question.

use serde_json::Value as Json;

use hydrus_core::subscriptions::CheckerOptions;
use hydrus_gui_model::checker_options::{self as checker, Editor, Which};

fn recorded() -> Json {
    hydrus_testkit::fixture_json("checker_options.json")
}

fn options(v: &Json) -> CheckerOptions {
    CheckerOptions {
        intended_files_per_check: v[0].as_f64().unwrap(),
        never_faster_than: v[1].as_i64().unwrap(),
        never_slower_than: v[2].as_i64().unwrap(),
        death_file_velocity: (v[3][0].as_i64().unwrap(), v[3][1].as_i64().unwrap()),
    }
}

/// A time's fields, as recorded (by unit name).
fn fields(units: &[hydrus_gui_model::options::Unit], recorded: &Json) -> Vec<i64> {
    units
        .iter()
        .map(|u| recorded[u.name()].as_i64().unwrap())
        .collect()
}

/// How the editor's state differs from the recorded state, if it does.
fn state_problem(editor: &Editor, state: &Json) -> Option<String> {
    let mut problems = Vec::new();
    if editor.value() != options(&state["value"]) {
        problems.push(format!("value {:?}", editor.value()));
    }
    if editor.flat != state["flat"].as_bool().unwrap()
        || state["reactive_hidden"] != editor.flat
        || state["static_hidden"] != !editor.flat
    {
        problems.push(format!("flat {}", editor.flat));
    }
    for (which, key) in [
        (Which::Faster, "faster"),
        (Which::Slower, "slower"),
        (Which::Period, "flat_period"),
    ] {
        let time = editor.time(which);
        if time.fields != fields(time.units, &state[key]) {
            problems.push(format!("{key} {:?}", time.fields));
        }
    }
    #[allow(clippy::float_cmp)] // (as the spin box holds it)
    if editor.intended != state["intended"].as_f64().unwrap() {
        problems.push(format!("intended {}", editor.intended));
    }
    let velocity = &state["velocity"];
    if editor.velocity_files != velocity[0].as_i64().unwrap()
        || editor.velocity.fields != fields(editor.velocity.units, &velocity[1])
    {
        problems.push(format!(
            "velocity {} {:?}",
            editor.velocity_files, editor.velocity.fields
        ));
    }
    (!problems.is_empty()).then(|| format!("{}: theirs {state}", problems.join(", ")))
}

fn which(name: &str) -> Which {
    match name {
        "faster" => Which::Faster,
        "slower" => Which::Slower,
        "flat" => Which::Period,
        other => panic!("{other}"),
    }
}

/// An edit as the recorder made it.
fn edit(editor: &mut Editor, edit: &Json) {
    let field = |time: &hydrus_gui_model::checker_options::Time, name: &Json| {
        time.units
            .iter()
            .position(|u| u.name() == name.as_str().unwrap())
            .unwrap()
    };
    match edit[0].as_str().unwrap() {
        "preset" => {
            let index = checker::PRESETS
                .iter()
                .position(|(label, _)| *label == edit[1])
                .unwrap();
            editor.preset(index);
        }
        "field" => {
            let which = which(edit[1].as_str().unwrap());
            let f = field(editor.time(which), &edit[2]);
            editor.set_field(which, f, edit[3].as_i64().unwrap());
        }
        "intended" => editor.set_intended(edit[1].as_f64().unwrap()),
        "velocity" => {
            editor.set_velocity_files(edit[1].as_i64().unwrap());
            if !edit[2].is_null() {
                let f = field(&editor.velocity, &edit[2]);
                editor.set_field(Which::Velocity, f, edit[3].as_i64().unwrap());
            }
        }
        "flat" => editor.toggle_flat(),
        other => panic!("{other}"),
    }
}

#[test]
fn the_reasonable_defaults_are_the_references() {
    let recorded = recorded();
    let theirs: Vec<(&str, CheckerOptions)> = recorded["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["label"].as_str().unwrap(), options(&p["value"])))
        .collect();
    assert_eq!(checker::PRESETS.to_vec(), theirs);
}

/// The recorded layout's controls and texts, flattened: each `(kind,
/// text)`, a box's title as `("box", title)` and anything hidden marked.
fn flat_layout(items: &Json, hidden: bool, out: &mut Vec<String>) {
    for item in items.as_array().unwrap() {
        let hidden = hidden || item["hidden"] == true;
        let mark = if hidden { " (hidden)" } else { "" };
        if let Some(title) = item.get("box") {
            out.push(format!("box {}{mark}", title.as_str().unwrap()));
            flat_layout(&item["items"], hidden, out);
            continue;
        }
        let mut item = item.clone();
        let object = item.as_object_mut().unwrap();
        object.remove("tooltip");
        object.remove("hidden");
        // (the values are the state's, compared there)
        for key in ["value", "float", "duration"] {
            object.remove(key);
        }
        if let Some(velocity) = object.get_mut("velocity") {
            *velocity = Json::Null;
        }
        out.push(format!(
            "{}{mark}",
            canonical(&Json::Object(object.clone()))
        ));
    }
}

/// `v` written with its keys in order.
fn canonical(v: &Json) -> String {
    let sorted: std::collections::BTreeMap<&String, &Json> =
        v.as_object().unwrap().iter().collect();
    serde_json::to_string(&sorted).unwrap()
}

/// Ours, as the reference lays it out.
fn our_layout(editor: &Editor) -> Vec<String> {
    use serde_json::json;
    let units = |units: &[hydrus_gui_model::options::Unit]| -> Vec<&str> {
        units.iter().map(|u| u.name()).collect()
    };
    let (reactive, flat) = if editor.flat {
        (" (hidden)", "")
    } else {
        ("", " (hidden)")
    };
    let mut out = vec![
        json!({ "label": "help for this panel -->" }).to_string(),
        json!({ "button": "" }).to_string(),
        json!({ "label": checker::WARNING }).to_string(),
        format!("box {}", checker::DEFAULTS_BOX),
    ];
    out.extend(
        checker::PRESETS
            .iter()
            .map(|(label, _)| json!({ "button": label }).to_string()),
    );
    out.push(json!({ "label": checker::VELOCITY_LABEL }).to_string());
    out.push(
        json!({
            "velocity": null,
            "number_min": checker::VELOCITY_FILES.0,
            "number_max": checker::VELOCITY_FILES.1,
            "per": checker::VELOCITY_PER,
            "units": units(checker::VELOCITY_UNITS),
            "min": checker::VELOCITY_MIN,
        })
        .to_string(),
    );
    out.push(json!({ "label": checker::STATIC_LABEL }).to_string());
    out.push(json!({ "check": "" }).to_string());
    if editor.advanced {
        out.push(json!({ "label": checker::ADVANCED_WARNING }).to_string());
    }
    out.push(format!("box {}{reactive}", checker::REACTIVE_BOX));
    let time = |which: Which| {
        let time = editor.time(which);
        json!({ "units": units(time.units), "min": time.min }).to_string()
    };
    out.extend([
        format!("{}{reactive}", json!({ "label": checker::REACTIVE_TEXT })),
        format!("{}{reactive}", json!({ "label": checker::INTENDED_LABEL })),
        format!(
            "{}{reactive}",
            json!({ "min": checker::INTENDED_RANGE.0, "max": checker::INTENDED_RANGE.1 })
        ),
        format!("{}{reactive}", json!({ "label": checker::FASTER_LABEL })),
        format!("{}{reactive}", time(Which::Faster)),
        format!("{}{reactive}", json!({ "label": checker::SLOWER_LABEL })),
        format!("{}{reactive}", time(Which::Slower)),
        format!("box {}{flat}", checker::STATIC_BOX),
        format!("{}{flat}", json!({ "label": checker::PERIOD_LABEL })),
        format!("{}{flat}", time(Which::Period)),
    ]);
    // (each control's keys in order, as theirs)
    out.into_iter()
        .map(|line| match line.rfind('}') {
            Some(end) if line.starts_with('{') => {
                let v: Json = serde_json::from_str(&line[..=end]).unwrap();
                format!("{}{}", canonical(&v), &line[end + 1..])
            }
            _ => line,
        })
        .collect()
}

#[test]
fn the_editor_opens_as_the_references() {
    let recorded = recorded();
    for panel in recorded["panels"].as_array().unwrap() {
        let advanced = panel["advanced"].as_bool().unwrap();
        let editor = Editor::new(&options(&panel["given"]), advanced);
        let given = &panel["given"];
        if let Some(problem) = state_problem(&editor, &panel["state"]) {
            panic!("{given} (advanced {advanced}): {problem}");
        }
        let mut theirs = Vec::new();
        flat_layout(&panel["layout"], false, &mut theirs);
        assert_eq!(our_layout(&editor), theirs, "{given} (advanced {advanced})");
    }
}

#[test]
fn edits_change_the_editor_as_the_references() {
    let recorded = recorded();
    for run in recorded["steps"].as_array().unwrap() {
        let mut editor = Editor::new(&options(&run["given"]), false);
        for step in run["steps"].as_array().unwrap() {
            edit(&mut editor, &step["edit"]);
            if let Some(problem) = state_problem(&editor, step) {
                panic!("{} then {}: {problem}", run["given"], step["edit"]);
            }
        }
    }
}

#[test]
fn ok_asks_as_the_references_does() {
    let recorded = recorded();
    for case in recorded["ok"].as_array().unwrap() {
        let mut editor = Editor::new(&options(&case["given"]), false);
        for e in case["edits"].as_array().unwrap() {
            edit(&mut editor, e);
        }
        let asked = editor.ok();
        let theirs: Vec<&str> = case["asked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| q.as_str().unwrap())
            .collect();
        assert_eq!(asked.into_iter().collect::<Vec<_>>(), theirs, "{case}");
        // (closed unless asked and answered no)
        let answer = case["answer"].as_bool().unwrap();
        assert_eq!(asked.is_none() || answer, case["ok"], "{case}");
        assert_eq!(editor.value(), options(&case["value"]), "{case}");
    }
}

/// "ok" raises a time below its least value to it (as the reference's
/// does when the focus leaves it), then never slower than to never faster
/// than.
#[test]
fn ok_raises_times_below_their_least() {
    let mut editor = Editor::new(&checker::PRESETS[0].1, false);
    // (thread: never faster than 5 minutes, never slower than a day)
    editor.set_field(Which::Faster, 2, 0);
    editor.set_field(Which::Slower, 0, 0);
    editor.set_field(Which::Velocity, 0, 0);
    let value = editor.value();
    assert_eq!(
        (value.never_faster_than, value.never_slower_than),
        (0, 0),
        "(typed, they are kept)"
    );
    assert_eq!(value.death_file_velocity, (1, 0));
    // (never faster than raised moves never slower than up to its own
    // least value, so they aren't the same)
    assert_eq!(editor.ok(), None);
    let value = editor.value();
    assert_eq!(
        (value.never_faster_than, value.never_slower_than),
        (30, 600)
    );
    assert_eq!(value.death_file_velocity, (1, 60));
    // checking statically, its period
    editor.toggle_flat();
    editor.set_field(Which::Period, 2, 0);
    assert_eq!(editor.value().never_faster_than, 0);
    assert_eq!(editor.ok(), None);
    assert_eq!(
        (
            editor.value().never_faster_than,
            editor.value().never_slower_than
        ),
        (180, 180)
    );
    // in advanced mode, a second
    let mut editor = Editor::new(&checker::PRESETS[0].1, true);
    editor.set_field(Which::Faster, 2, 0);
    editor.ok();
    assert_eq!(editor.value().never_faster_than, 1);
}

/// The options window's downloading page: its two "checker options"
/// rows hold the fixture's default subscription and watcher checker
/// options, as the reference's buttons do, and "apply" stores those
/// edited, as the reference's options take them.
#[test]
fn the_downloading_page_edits_the_default_checker_options() {
    use hydrus_gui_model::options::{Kind, Row, Settings, Value};
    let recorded = recorded();
    let downloading = &recorded["downloading"];
    let options_recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let (_dir, store) = crate::options_dialog::fixture_store(&options_recorded);
    let settings = store.read(Settings::load).unwrap();
    let mut editor = hydrus_gui_model::options::Editor::new(settings);
    let page = editor
        .page_names()
        .iter()
        .position(|n| *n == "downloading")
        .unwrap();
    editor.show_page(page);
    let row = |editor: &hydrus_gui_model::options::Editor, label: &str| {
        editor
            .rows()
            .iter()
            .position(|r| matches!(r, Row::Opt { option, .. } if option.label == label))
            .unwrap()
    };
    let value = |editor: &hydrus_gui_model::options::Editor, at: usize| match &editor.rows()[at] {
        Row::Opt { option, value, .. } => {
            assert_eq!(option.kind, Kind::Checker);
            (*value).clone()
        }
        Row::Title { .. } => panic!("a title"),
    };
    let subscriptions = row(&editor, "Default subscription checker options:");
    let watchers = row(&editor, "Default watcher checker options:");
    let before = &downloading["before"];
    assert_eq!(
        value(&editor, subscriptions),
        Value::Checker(options(&before["subscriptions"]))
    );
    assert_eq!(
        value(&editor, watchers),
        Value::Checker(options(&before["watchers"]))
    );
    let set = &downloading["set"];
    editor.checker(subscriptions, options(&set["subscriptions"]));
    editor.checker(watchers, options(&set["watchers"]));
    let (after, before, problems) = editor.applied();
    assert!(problems.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let stored: hydrus_core::subscriptions::CheckerDefaults =
        store.read(hydrus_store::settings::get).unwrap();
    let applied = &downloading["applied"];
    assert_eq!(stored.subscriptions, options(&applied["subscriptions"]));
    assert_eq!(stored.watchers, options(&applied["watchers"]));
}
