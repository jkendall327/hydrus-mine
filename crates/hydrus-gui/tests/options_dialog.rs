//! The options window's pages against the reference's options dialog
//! (`oracle/record_options_dialog.py`, the running client's dialog on the
//! `basic` fixture): the pages hydrus-rs has must be the reference's, in
//! its order; each option must be one of the page's, in its box, with its
//! label, control and limits, in the same order; and on the fixture,
//! migrated, each must show the value the reference showed.

use serde_json::Value as Json;

use hydrus_gui::options::{Item, Kind, Page, Settings, Value, pages};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

/// A row of a recorded page: the boxes it is in, its label, its control.
#[derive(Debug, Clone)]
struct Row {
    boxes: Vec<String>,
    label: String,
    control: Json,
}

const CONTROLS: &[&str] = &["check", "int", "float", "noneable", "choice", "text"];

fn is_control(item: &Json) -> bool {
    CONTROLS.iter().any(|k| item.get(*k).is_some())
}

/// The reference's grid rows: a label, then its control; or a checkbox
/// with its own text.
fn recorded_rows(items: &[Json], boxes: &[String], out: &mut Vec<Row>) {
    let mut i = 0;
    while i < items.len() {
        let item = &items[i];
        i += 1;
        if item.get("hidden").is_some() {
            continue;
        }
        if let Some(title) = item.get("box").and_then(Json::as_str) {
            let mut inner = boxes.to_vec();
            inner.push(title.to_owned());
            recorded_rows(
                item.get("items")
                    .and_then(Json::as_array)
                    .map_or(&[][..], |v| v),
                &inner,
                out,
            );
        } else if let Some(label) = item.get("label").and_then(Json::as_str) {
            if let Some(next) = items.get(i)
                && is_control(next)
            {
                out.push(Row {
                    boxes: boxes.to_vec(),
                    label: label.to_owned(),
                    control: next.clone(),
                });
                i += 1;
            }
        } else if let Some(text) = item.get("check").and_then(Json::as_str)
            && !text.is_empty()
        {
            out.push(Row {
                boxes: boxes.to_vec(),
                label: text.to_owned(),
                control: item.clone(),
            });
        } else if item.get("widget").is_some()
            && let Some(inner) = item.get("items").and_then(Json::as_array)
        {
            recorded_rows(inner, boxes, out);
        }
    }
}

/// Ours, likewise: each option with the boxes it is in.
fn our_rows<'a>(items: &'a [Item], boxes: &[String], out: &mut Vec<(Vec<String>, &'a Item)>) {
    for item in items {
        match item {
            Item::Box(title, items) => {
                let mut inner = boxes.to_vec();
                inner.push((*title).to_owned());
                our_rows(items, &inner, out);
            }
            Item::Opt(_) => out.push((boxes.to_vec(), item)),
        }
    }
}

/// What is wrong with an option's control and value, against the
/// reference's.
fn compare(kind: &Kind, value: &Value, theirs: &Json) -> Option<String> {
    let num = |key: &str| theirs.get(key).and_then(Json::as_f64);
    let problem = match (kind, value) {
        (Kind::Check, Value::Check(b)) => {
            (theirs.get("check").is_none() || theirs["value"] != *b).then(|| format!("check {b}"))
        }
        (Kind::Int { min, max }, Value::Int(n)) => (theirs.get("int").and_then(Json::as_i64)
            != Some(*n)
            || num("min") != Some(*min as f64)
            || num("max") != Some(*max as f64))
        .then(|| format!("int {n} ({min}-{max})")),
        (
            Kind::Noneable {
                none_phrase,
                min,
                max,
                unit,
                ..
            },
            Value::Noneable(n),
        ) => (theirs.get("noneable").map(Json::as_i64) != Some(*n)
            || theirs["none_phrase"] != *none_phrase
            || num("min") != Some(*min as f64)
            || num("max") != Some(*max as f64)
            || theirs.get("unit").and_then(Json::as_str) != *unit)
            .then(|| format!("noneable {n:?} {none_phrase:?} ({min}-{max}) {unit:?}")),
        (Kind::Float { min, max }, Value::Float(text)) => (num("float") != text.parse().ok()
            || num("min") != Some(*min)
            || num("max") != Some(*max))
        .then(|| format!("float {text} ({min}-{max})")),
        (Kind::Choice(items), Value::Choice(i)) => {
            let theirs_items: Vec<&str> = theirs
                .get("items")
                .and_then(Json::as_array)
                .map(|v| v.iter().filter_map(Json::as_str).collect())
                .unwrap_or_default();
            (theirs_items != *items || theirs["choice"] != items[*i])
                .then(|| format!("choice {:?} of {items:?}", items[*i]))
        }
        (Kind::Text, Value::Text(t)) => (theirs["text"] != *t).then(|| format!("text {t:?}")),
        _ => Some(format!("{kind:?} holding {value:?}")),
    };
    problem.map(|ours| format!("ours {ours}, theirs {theirs}"))
}

#[test]
fn the_options_pages_are_the_references() {
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let settings = store.read(Settings::load).unwrap();

    let theirs: Vec<(&str, &Json)> = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["page"].as_str().unwrap(), &p["items"]))
        .collect();
    let ours: Vec<Page> = pages();

    // the pages, in the reference's order
    let names: Vec<&str> = ours.iter().map(|p| p.name).collect();
    let in_order: Vec<&str> = theirs
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| names.contains(name))
        .collect();
    assert_eq!(
        names, in_order,
        "pages that aren't the reference's, or out of order"
    );

    let mut problems = Vec::new();
    for page in &ours {
        let items = theirs.iter().find(|(n, _)| *n == page.name).unwrap().1;
        let mut rows = Vec::new();
        recorded_rows(items.as_array().unwrap(), &[], &mut rows);
        let mut mine = Vec::new();
        our_rows(&page.items, &[], &mut mine);
        let mut after = 0;
        for (boxes, item) in mine {
            let Item::Opt(option) = item else { continue };
            let found = rows
                .iter()
                .enumerate()
                .skip(after)
                .find(|(_, r)| r.label == option.label && r.boxes == boxes);
            let Some((at, row)) = found else {
                problems.push(format!(
                    "{}: {boxes:?} {:?} isn't the reference's (or is out of order)",
                    page.name, option.label
                ));
                continue;
            };
            after = at + 1;
            let value = (option.get)(&settings);
            if let Some(why) = compare(&option.kind, &value, &row.control) {
                problems.push(format!("{}: {:?}: {why}", page.name, option.label));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
