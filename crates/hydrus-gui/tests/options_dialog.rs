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

const CONTROLS: &[&str] = &[
    "check",
    "int",
    "float",
    "noneable",
    "choice",
    "text",
    "noneable_text",
    "duration",
    "velocity",
];

/// A time's units as the recording names them.
fn unit_names(units: &[hydrus_gui::options::Unit]) -> Vec<&'static str> {
    units.iter().map(|u| u.name()).collect()
}

fn recorded_units(theirs: &Json) -> Vec<&str> {
    theirs
        .get("units")
        .and_then(Json::as_array)
        .map(|v| v.iter().filter_map(Json::as_str).collect())
        .unwrap_or_default()
}

/// Two times, the same to the millisecond.
fn same_time(a: f64, b: Option<f64>) -> bool {
    b.is_some_and(|b| (a - b).abs() < 0.0005)
}

/// The reference's widgets that are a control of ours.
const WIDGETS: &[&str] = &["MediaSortControl", "MediaCollectControl"];

fn is_control(item: &Json) -> bool {
    CONTROLS.iter().any(|k| item.get(*k).is_some())
        || item
            .get("widget")
            .and_then(Json::as_str)
            .is_some_and(|w| WIDGETS.contains(&w))
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
            && !is_control(item)
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
/// reference's (a sort's types are the store's).
fn compare(kind: &Kind, value: &Value, theirs: &Json, store: &Store) -> Option<String> {
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
        (Kind::NoneableText { none_phrase }, Value::NoneableText { none, text }) => {
            let ours = (!none).then_some(text.as_str());
            (theirs.get("noneable_text").map(Json::as_str) != Some(ours)
                || theirs["none_phrase"] != *none_phrase)
                .then(|| format!("noneable text {ours:?} {none_phrase:?}"))
        }
        (Kind::Duration { units, min }, Value::Duration(seconds)) => {
            (!same_time(*seconds, num("duration"))
                || recorded_units(theirs) != unit_names(units)
                || !same_time(*min, num("min")))
            .then(|| {
                format!(
                    "duration {seconds} {:?} (at least {min})",
                    unit_names(units)
                )
            })
        }
        (
            Kind::Velocity {
                number,
                per,
                units,
                min,
            },
            Value::Velocity(n, seconds),
        ) => {
            let velocity = theirs.get("velocity").and_then(Json::as_array);
            let at = |i: usize| velocity.and_then(|v| v.get(i)).and_then(Json::as_f64);
            (at(0) != Some(*n as f64)
                || !same_time(*seconds, at(1))
                || num("number_min") != Some(number.0 as f64)
                || num("number_max") != Some(number.1 as f64)
                || theirs["per"] != *per
                || recorded_units(theirs) != unit_names(units)
                || !same_time(*min, num("min")))
            .then(|| {
                format!(
                    "velocity {n} {per:?} {seconds} {:?} ({}-{}, at least {min})",
                    unit_names(units),
                    number.0,
                    number.1
                )
            })
        }
        (Kind::Sort, Value::Sort(sort)) => {
            // (the reference's type button, "sort by" its type, and its
            // order's)
            let choices = hydrus_gui::sort::page_choices(store, &sort.by);
            let chosen = choices.iter().find(|c| c.by == sort.by);
            let shown = chosen.map(|c| {
                (
                    format!("sort by {}", c.name),
                    c.orders[usize::from(!sort.ascending)],
                )
            });
            let buttons = theirs["items"].as_array().map(|items| {
                items
                    .iter()
                    .filter(|i| i.get("hidden").is_none())
                    .filter_map(|i| i["button"].as_str())
                    .collect::<Vec<_>>()
            });
            let ours = shown.as_ref().map(|(by, order)| vec![by.as_str(), *order]);
            (theirs["widget"] != "MediaSortControl" || ours.is_none() || buttons != ours)
                .then(|| format!("sort {shown:?}"))
        }
        (Kind::Collect, Value::Collect(collect)) => {
            // (its text, its choices each checked or not, and whether
            // unmatched files collect)
            let choices = hydrus_gui::collect::choices(store);
            let label = hydrus_gui::collect::label(&choices, collect);
            let ours: Vec<Json> = choices
                .iter()
                .map(|c| serde_json::json!([c.name, c.checked(collect)]))
                .collect();
            (theirs["widget"] != "MediaCollectControl"
                || theirs["collect"] != label.as_str()
                || theirs["choices"].as_array() != Some(&ours)
                || theirs["collect_unmatched"] != collect.collect_unmatched)
                .then(|| format!("collect {label:?} {ours:?} {}", collect.collect_unmatched))
        }
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
    // (as the driver booted the reference: the similar-files search off)
    let fact = |name: &str| recorded["facts"][name] == true;
    let active = fact("maintain_similar_files_duplicate_pairs_during_active");
    let idle = fact("maintain_similar_files_duplicate_pairs_during_idle");
    store
        .write(move |ctx| {
            let mut similar: hydrus_store::similar::SimilarFilesSettings =
                hydrus_store::settings::get(ctx.conn())?;
            similar.during_active = active;
            similar.during_idle = idle;
            hydrus_store::settings::set(ctx.conn(), &similar)
        })
        .unwrap();
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
            if let Some(why) = compare(&option.kind, &value, &row.control, &store) {
                problems.push(format!("{}: {:?}: {why}", page.name, option.label));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_options_search_offers_what_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let search = &recorded["search"];
    assert_eq!(
        search["placeholder"],
        hydrus_gui::options::SEARCH_PLACEHOLDER
    );
    assert_eq!(search["max_visible"], hydrus_gui::options::SEARCH_SHOWN);
    // (each of ours is one of the reference's, written as it writes it)
    let theirs: Vec<&str> = search["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Json::as_str)
        .collect();
    let ours = hydrus_gui::options::suggestions(&pages());
    let missing: Vec<&str> = ours
        .iter()
        .map(|s| s.text.as_str())
        .filter(|text| !theirs.contains(text))
        .collect();
    assert!(missing.is_empty(), "not the reference's: {missing:?}");
    assert!(ours.len() > 100, "{} suggestions", ours.len());
}
