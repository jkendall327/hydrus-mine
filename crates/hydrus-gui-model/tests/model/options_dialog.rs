//! The options window's pages against the reference's options dialog
//! (`oracle/record_options_dialog.py`, the running client's dialog on the
//! `basic` fixture): the pages hydrus-rs has must be the reference's, in
//! its order; each option must be one of the page's, in its box, with its
//! label, control and limits, in the same order; and on the fixture,
//! migrated, each must show the value the reference showed.

use serde_json::Value as Json;

use hydrus_gui_model::options::{Item, Kind, Page, Settings, Value, pages};
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
    // (a button opening an editor, as checker options')
    "button",
];

/// A time's units as the recording names them.
fn unit_names(units: &[hydrus_gui_model::options::Unit]) -> Vec<&'static str> {
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
const WIDGETS: &[&str] = &[
    "MediaSortControl",
    "MediaCollectControl",
    "TagSortControl",
    "DirPickerCtrl",
    "BetterCheckBoxList",
    "NoneableTimeDeltaWidget",
];

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
        (Kind::NoneableDuration { .. }, Value::NoneableDuration { none, seconds }) => {
            (theirs["widget"] != "NoneableTimeDeltaWidget"
                || if *none {
                    !theirs["value"].is_null()
                } else {
                    !same_time(*seconds, theirs["value"].as_f64())
                })
            .then(|| format!("noneable duration {none}/{seconds}"))
        }
        (Kind::CanvasTicks, Value::Canvases(canvases)) => {
            let codes: Vec<_> = canvases.iter().map(|c| c.code()).collect();
            (theirs["widget"] != "BetterCheckBoxList"
                || theirs["value"] != serde_json::json!(codes))
            .then(|| format!("viewing canvases {codes:?}"))
        }
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
        ) => {
            // Qt records GetValue in seconds for these multiplier-60 controls;
            // native values are displayed minutes. The new Qt replay checks
            // both the displayed spinner and persisted seconds independently.
            let multiplier = if matches!(
                *none_phrase,
                "ignore normal browsing" | "ignore mouse movements" | "ignore client api"
            ) {
                60
            } else {
                1
            };
            (theirs.get("noneable").map(Json::as_i64) != Some(n.map(|number| number * multiplier))
                || theirs["none_phrase"] != *none_phrase
                || num("min") != Some(*min as f64)
                || num("max") != Some(*max as f64)
                || theirs.get("unit").and_then(Json::as_str) != *unit)
                .then(|| format!("noneable {n:?} {none_phrase:?} ({min}-{max}) {unit:?}"))
        }
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
        (Kind::SavedSession, Value::SavedSession(name)) => {
            let choices = hydrus_gui_model::options::session_choices(store);
            let ours: Vec<_> = choices.iter().map(|(_, label)| label.as_str()).collect();
            let theirs_items: Vec<_> = theirs["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item.as_str().unwrap())
                .collect();
            (ours != theirs_items
                || theirs["choice"] != name.as_deref().unwrap_or("just a blank page"))
            .then(|| format!("session {name:?} of {ours:?}"))
        }
        (Kind::TagService { combined }, Value::TagService(key)) => {
            let choices = hydrus_gui_model::options::tag_service_choices(store, *combined);
            let names = choices
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>();
            let theirs_names = theirs["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Json::as_str)
                .collect::<Vec<_>>();
            let chosen = choices
                .iter()
                .find(|(service, _)| service == key)
                .map(|(_, name)| name.as_str());
            (names != theirs_names || chosen != theirs["choice"].as_str())
                .then(|| format!("service {chosen:?} of {names:?}"))
        }
        (Kind::Directory, Value::Text(path)) => (theirs["widget"] != "DirPickerCtrl"
            || theirs["items"][0]["text"] != *path
            || theirs["items"][1]["button"] != "browse")
            .then(|| format!("directory {path:?}")),
        (Kind::Text, Value::PlainNoneableText(text)) => {
            let shown = text.as_deref().unwrap_or_default();
            (theirs["text"] != shown).then(|| format!("text {shown:?}"))
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
            let choices = hydrus_gui_model::sort::page_choices(store, &sort.by);
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
        (Kind::TagSort, Value::TagSort(sort)) => {
            // (the choices it shows: its type, its order, and its grouping
            // where the type groups)
            use hydrus_gui_model::options::{TAG_SORT_GROUPS, TAG_SORT_TYPES, tag_sort_orders};
            let mut ours: Vec<&str> = TAG_SORT_TYPES
                .iter()
                .filter(|(_, t)| *t == sort.sort_type)
                .map(|(name, _)| *name)
                .collect();
            let (orders, order) = tag_sort_orders(sort);
            ours.push(orders[order]);
            if sort.sort_type != hydrus_core::tag_sort::TagSortType::Subtag {
                ours.extend(
                    TAG_SORT_GROUPS
                        .iter()
                        .filter(|(_, g)| *g == sort.group_by)
                        .map(|(name, _)| *name),
                );
            }
            let theirs_shown: Option<Vec<&str>> = theirs["tag_sort"]
                .as_array()
                .map(|v| v.iter().filter_map(Json::as_str).collect());
            (theirs["widget"] != "TagSortControl" || theirs_shown.as_ref() != Some(&ours))
                .then(|| format!("tag sort {ours:?}"))
        }
        (Kind::Collect, Value::Collect(collect)) => {
            // (its text, its choices each checked or not, and whether
            // unmatched files collect)
            let choices = hydrus_gui_model::collect::choices(store);
            let label = hydrus_gui_model::collect::label(&choices, collect);
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
        (Kind::LocalLocation, Value::Location(location)) => {
            let label =
                hydrus_gui_model::domains::location_label(&store.snapshot().services, location);
            (theirs["button"] != label).then(|| format!("location {label:?}"))
        }
        // (the button; its checker options are checker_options' test's)
        (Kind::Checker, Value::Checker(_)) => {
            (theirs["button"] != "checker options").then(|| "checker options".to_owned())
        }
        _ => Some(format!("{kind:?} holding {value:?}")),
    };
    problem.map(|ours| format!("ours {ours}, theirs {theirs}"))
}

/// The fixture's store, as the driver booted the reference on it.
pub fn fixture_store(recorded: &Json) -> (tempfile::TempDir, std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    // (the similar-files search off)
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
    (native, store)
}

/// How `page`'s options differ from the reference's recorded `items`: each
/// of ours is one of the reference's rows, in order, its control and value
/// (from `settings`) as the reference's.
fn page_problems(page: &Page, items: &Json, settings: &Settings, store: &Store) -> Vec<String> {
    let mut problems = Vec::new();
    let mut rows = Vec::new();
    recorded_rows(items.as_array().unwrap(), &[], &mut rows);
    let mut mine = Vec::new();
    our_rows(&page.items, &[], &mut mine);
    // Compare Qt controls with the values the native controls display,
    // including spin-box clamping of imported/default out-of-range values.
    let mut displayed = hydrus_gui_model::options::values(std::slice::from_ref(page), settings)
        .remove(0)
        .into_iter();
    let mut after = 0;
    for (boxes, item) in mine {
        let Item::Opt(option) = item else { continue };
        let value = displayed.next().expect("one displayed value per option");
        // This reference page embeds the list; native opens the same transaction
        // in a child window, covered by dedicated regex/write-tag/gallery-source/import-options/namespace-queue recordings.
        // The inline namespace RGB list has its own exact namespace_colour_controls replay.
        // Shortcut capture/policies replay the actual nested command controls separately.
        // Open-externally queues/MIME rows/washing replay actual Qt lists separately;
        // this generic recorder captures scalar controls rather than these inline lists.
        if matches!(
            option.kind,
            Kind::ExternalCalls
                | Kind::OpenExternally
                | Kind::Shortcuts
                | Kind::RegexFavourites
                | Kind::NamespaceColours
                | Kind::DeletionReasons
                | Kind::FrameLocations
                | Kind::FavouriteTags
                | Kind::MostUsedTags
                | Kind::RelatedWeights
                | Kind::GallerySource
                | Kind::ImportOptions
                | Kind::NamespaceSorts
                | Kind::TagBanner(_)
                | Kind::ProviderOrder
        ) {
            continue;
        }
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
        // This older whole-dialog fixture did not introspect this private
        // noneable widget. Its real Qt bounds/value states are recorded and
        // replayed separately in subscription_failure_limit.json.
        if option.label
            == "If a subscription has this many failed file imports, stop and continue later:"
        {
            continue;
        }
        if let Some(why) = compare(&option.kind, &value, &row.control, store) {
            problems.push(format!("{}: {:?}: {why}", page.name, option.label));
        }
    }
    problems
}

#[test]
fn the_options_pages_are_the_references() {
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let (_dir, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();

    let theirs: Vec<(&str, &Json)> = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["page"].as_str().unwrap(), &p["items"]))
        .collect();
    let ours: Vec<Page> = pages(&settings);

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
        problems.extend(page_problems(page, items, &settings, &store));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_thumbnail_rating_sizes_go_up_to_the_thumbnails_width() {
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let wider = &recorded["wider_thumbnails"];
    let (_dir, store) = fixture_store(&recorded);
    let mut settings = store.read(Settings::load).unwrap();
    let dimensions = &wider["thumbnail_dimensions"];
    settings.thumbnails.bounding_width = dimensions[0].as_u64().unwrap() as u32;
    settings.thumbnails.bounding_height = dimensions[1].as_u64().unwrap() as u32;
    let ours = pages(&settings);
    let ratings = ours.iter().find(|p| p.name == "ratings").unwrap();
    let problems = page_problems(ratings, &wider["ratings"], &settings, &store);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    // (and so not the reference's with its thumbnails as wide as before)
    let before = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["page"] == "ratings")
        .unwrap();
    assert!(!page_problems(ratings, &before["items"], &settings, &store).is_empty());
}

#[test]
fn the_options_search_offers_what_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let search = &recorded["search"];
    assert_eq!(
        search["placeholder"],
        hydrus_gui_model::options::SEARCH_PLACEHOLDER
    );
    assert_eq!(
        search["max_visible"],
        hydrus_gui_model::options::SEARCH_SHOWN
    );
    // (each of ours is one of the reference's, written as it writes it)
    let theirs: Vec<&str> = search["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Json::as_str)
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let settings = Store::open(dir.path())
        .unwrap()
        .read(Settings::load)
        .unwrap();
    let ours = hydrus_gui_model::options::suggestions(&pages(&settings));
    let missing: Vec<&str> = ours
        .iter()
        .map(|s| s.text.as_str())
        .filter(|text| !theirs.contains(text))
        .collect();
    assert!(missing.is_empty(), "not the reference's: {missing:?}");
    for (label, expected_page, routing) in [
        ("URL calls (open externally)", "open externally", true),
        (
            "single file calls (open externally)",
            "open externally",
            true,
        ),
        (
            "built-in hydrus shortcut sets (shortcuts)",
            "shortcuts",
            false,
        ),
        ("custom user sets (shortcuts)", "shortcuts", false),
    ] {
        assert!(
            theirs.contains(&label),
            "alias must be actual recorded Qt text"
        );
        let suggestion = ours.iter().find(|value| value.text == label).unwrap();
        let actual_pages = pages(&settings);
        let page = &actual_pages[suggestion.page];
        assert_eq!(page.name, expected_page);
        let mut rows = Vec::new();
        our_rows(&page.items, &[], &mut rows);
        let Item::Opt(option) = rows[suggestion.row].1 else {
            panic!("search must target the real compound control")
        };
        assert!(if routing {
            matches!(option.kind, Kind::OpenExternally)
        } else {
            matches!(option.kind, Kind::Shortcuts)
        });
    }
    assert!(ours.len() > 100, "{} suggestions", ours.len());
}

#[test]
fn advanced_network_ranges_and_clamps_match_the_reference() {
    let recorded = hydrus_testkit::fixture_json("options_ranges.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let mut settings = store.read(Settings::load).unwrap();
    let labels = [
        "network timeout (seconds): ",
        "connection error retry wait (seconds): ",
        "serverside bandwidth retry wait (seconds): ",
        "max number of simultaneous active network jobs: ",
        "max number of simultaneous active network jobs per domain: ",
        "Delay time on a gallery/watcher network error:",
        "Delay time on a subscription network error:",
        "Delay time on a subscription other error:",
    ];
    for mode in recorded.as_array().unwrap() {
        settings.advanced.0 = mode["advanced"].as_bool().unwrap();
        let pages = pages(&settings);
        for (label, control) in labels.iter().zip(mode["controls"].as_array().unwrap()) {
            let option = pages
                .iter()
                .flat_map(Page::options)
                .find(|o| o.label == *label)
                .unwrap();
            match option.kind {
                Kind::Int { min, max } => {
                    assert_eq!(min, control["min"].as_i64().unwrap(), "{label}");
                    assert_eq!(max, control["max"].as_i64().unwrap(), "{label}");
                    for (input, expected) in [(0, "below"), (4_000_000, "above")] {
                        let mut changed = settings.clone();
                        (option.set)(&mut changed, &Value::Int(input)).unwrap();
                        assert_eq!(
                            (option.get)(&changed),
                            Value::Int(control[expected].as_i64().unwrap()),
                            "{label}"
                        );
                    }
                }
                Kind::Duration { min, .. } => {
                    assert!(
                        (min - control["min"].as_f64().unwrap()).abs() < f64::EPSILON,
                        "{label}"
                    );
                    let mut changed = settings.clone();
                    (option.set)(&mut changed, &Value::Duration(0.0)).unwrap();
                    assert_eq!((option.get)(&changed), Value::Duration(min), "{label}");
                    (option.set)(&mut changed, &Value::Duration(61.0)).unwrap();
                    assert_eq!(
                        (option.get)(&changed),
                        Value::Duration(61.0_f64.max(min)),
                        "{label}"
                    );
                }
                _ => panic!("unexpected control {label}"),
            }
        }
    }
}

#[test]
fn remembered_options_pages_and_auxiliary_search_match_the_reference() {
    use hydrus_gui_model::options::{Editor, Row};
    let recorded = hydrus_testkit::fixture_json("options_preferences.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let mut settings = store.read(Settings::load).unwrap();
    for case in recorded["cases"].as_array().unwrap() {
        settings.options_preferences.remember_panel = case["remember"].as_bool().unwrap();
        settings.options_preferences.last_panel = case["last"].as_str().unwrap().into();
        settings.options_preferences.search_at_top = case["top"].as_bool().unwrap();
        let mut editor = Editor::new(settings.clone());
        assert_eq!(
            editor.page_names()[editor.page()],
            case["opened"].as_str().unwrap()
        );
        let audio = editor
            .page_names()
            .iter()
            .position(|name| *name == "audio")
            .unwrap();
        editor.show_page(audio);
        let (after, _, problems) = editor.applied();
        assert!(problems.is_empty());
        assert_eq!(
            after.options_preferences.last_panel,
            case["remembered_after_page_change"].as_str().unwrap()
        );
        let reopened = Editor::new(after);
        assert_eq!(
            reopened.page_names()[reopened.page()],
            case["reopened"].as_str().unwrap()
        );
    }
    let mut editor = Editor::new(settings);
    let theirs = recorded["suggestions"].as_array().unwrap();
    for text in [
        "no size limit (files and trash)",
        "s (media viewer)",
        "hours (connection)",
        "top of this window (gui)",
    ] {
        // Snapshot reflects the last recorded case's bottom search placement.
        let text = if text == "top of this window (gui)" {
            "bottom of this window (gui)"
        } else {
            text
        };
        assert!(theirs.iter().any(|value| value == text), "{text}");
        let suggestion = editor
            .search(text)
            .into_iter()
            .find(|s| s.text == text)
            .unwrap()
            .clone();
        editor.go_to(&suggestion);
        assert!(matches!(editor.rows()[suggestion.row], Row::Opt { .. }));
        assert!(editor.found(suggestion.row));
    }
}

#[test]
fn application_names_and_exit_switch_apply_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("gui_settings.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let mut settings = store.read(Settings::load).unwrap();
    let pages = pages(&settings);
    let gui = pages.iter().find(|page| page.name == "gui").unwrap();
    let options = gui.options();
    let name = options
        .iter()
        .find(|option| option.label == "Application display name: ")
        .unwrap();
    for case in recorded["names"].as_array().unwrap() {
        (name.set)(
            &mut settings,
            &Value::Text(case["typed"].as_str().unwrap().into()),
        )
        .unwrap();
        assert_eq!(
            settings.gui.application_display_name,
            case["saved"].as_str().unwrap()
        );
    }
    let confirm = options
        .iter()
        .find(|option| option.label == "Confirm client exit: ")
        .unwrap();
    for case in recorded["exits"].as_array().unwrap() {
        (confirm.set)(
            &mut settings,
            &Value::Check(case["confirm"].as_bool().unwrap()),
        )
        .unwrap();
        assert_eq!(settings.gui.confirm_exit, case["saved"].as_bool().unwrap());
    }
}

#[test]
fn session_backup_count_matches_recorded_control_and_clamps() {
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let (_directory, store) = fixture_store(&recorded);
    let mut settings = store.read(Settings::load).unwrap();
    let reference = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "gui sessions")
        .unwrap();
    let mut rows = Vec::new();
    recorded_rows(reference["items"].as_array().unwrap(), &[], &mut rows);
    let control = &rows
        .iter()
        .find(|row| row.label == "Number of session backups to keep: ")
        .unwrap()
        .control;
    let pages = pages(&settings);
    let ours = pages
        .iter()
        .find(|page| page.name == "gui sessions")
        .unwrap();
    assert!(page_problems(ours, &reference["items"], &settings, &store).is_empty());
    let options = ours.options();
    let option = options
        .into_iter()
        .find(|option| option.label == "Number of session backups to keep: ")
        .unwrap();
    for (input, boundary) in [(0, "min"), (100, "max")] {
        (option.set)(&mut settings, &Value::Int(input)).unwrap();
        assert_eq!(
            settings.session_backups.keep as i64,
            control[boundary].as_i64().unwrap()
        );
    }
}

#[test]
fn reopening_numeric_options_clamps_saved_advanced_values() {
    use hydrus_gui_model::options::{applied, values};
    let recorded = hydrus_testkit::fixture_json("options_ranges.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let mut settings = store.read(Settings::load).unwrap();
    settings.network.network_timeout = 4_000_000;
    settings.network.connection_error_wait_time = 4_000_000;
    settings.network.serverside_bandwidth_wait_time = 4_000_000;
    settings.network.max_jobs = 4_000_000;
    settings.network.max_jobs_per_domain = 4_000_000;
    for mode in recorded.as_array().unwrap() {
        settings.advanced.0 = mode["advanced"].as_bool().unwrap();
        let pages = pages(&settings);
        let shown = values(&pages, &settings);
        let connection = pages
            .iter()
            .position(|page| page.name == "connection")
            .unwrap();
        let options = pages[connection].options();
        let (after, problems) = applied(&pages, &settings, &shown);
        assert!(problems.is_empty());
        for (index, control) in [2, 3, 4, 6, 7]
            .into_iter()
            .zip(mode["controls"].as_array().unwrap())
        {
            let expected = Value::Int(control["initial_from_4000000"].as_i64().unwrap());
            assert_eq!(shown[connection][index], expected);
            assert_eq!((options[index].get)(&after), expected);
        }
    }
}

#[test]
fn notebook_navigation_preferences_match_recorded_controls() {
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let (_directory, store) = fixture_store(&recorded);
    let mut settings = store.read(Settings::load).unwrap();
    let pages = pages(&settings);
    let page = pages.iter().find(|page| page.name == "gui pages").unwrap();
    let reference = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "gui pages")
        .unwrap();
    assert!(page_problems(page, &reference["items"], &settings, &store).is_empty());
    let options = page.options();
    let focus = options
        .iter()
        .find(|option| option.label == "When closing the current tab, move focus: ")
        .unwrap();
    for index in [0, 1] {
        (focus.set)(&mut settings, &Value::Choice(index)).unwrap();
        assert_eq!(settings.notebooks.close_focus_left, index == 0);
    }
    let creation = options
        .iter()
        .find(|option| {
            option.label == "Automatically prompt to rename new 'page of pages' after creation: "
        })
        .unwrap();
    (creation.set)(&mut settings, &Value::Check(true)).unwrap();
    assert!(settings.notebook_creation.rename_new_notebooks);
    let rename = options
        .iter()
        .find(|option| {
            option.label == "  Also automatically prompt when sending some pages to one: "
        })
        .unwrap();
    (rename.set)(&mut settings, &Value::Check(true)).unwrap();
    assert!(settings.notebooks.rename_sent_notebooks);
}

#[test]
fn regex_favourites_child_edits_wait_for_parent_apply() {
    use hydrus_gui_model::options::Editor;
    use hydrus_store::regex_favourites::RegexFavourites;
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let before = store.read(Settings::load).unwrap();
    let mut editor = Editor::new(before.clone());
    let changed = RegexFavourites(vec![("^example$".into(), "example favourite".into())]);
    editor.set_regex_favourites(changed.clone());
    assert_eq!(editor.edited_regex_favourites(), changed);
    assert_eq!(store.read(Settings::load).unwrap(), before);
    let (after, original, problems) = editor.applied();
    assert!(problems.is_empty());
    assert_eq!(after.regex_favourites, changed);
    let original = original.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &original))
        .unwrap();
    assert_eq!(
        store.read(Settings::load).unwrap().regex_favourites,
        changed
    );
}

#[test]
fn system_sleep_controls_match_reference_and_clamp() {
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let sleep = hydrus_testkit::fixture_json("system_sleep_options.json");
    let (_directory, store) = fixture_store(&recorded);
    let mut settings = store.read(Settings::load).unwrap();
    let pages = pages(&settings);
    let page = pages.iter().find(|page| page.name == "system").unwrap();
    let reference = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "system")
        .unwrap();
    assert!(page_problems(page, &reference["items"], &settings, &store).is_empty());
    let options = page.options();
    assert_eq!(
        (options[0].get)(&settings),
        Value::Check(sleep["controls"]["enabled"].as_bool().unwrap())
    );
    assert_eq!(
        (options[1].get)(&settings),
        Value::Int(sleep["controls"]["delay"].as_i64().unwrap())
    );
    for case in sleep["cases"].as_array().unwrap() {
        let enabled = case["enabled"].as_bool().unwrap();
        (options[0].set)(&mut settings, &Value::Check(enabled)).unwrap();
        (options[1].set)(&mut settings, &Value::Int(case["delay"].as_i64().unwrap())).unwrap();
        assert_eq!(settings.network.detect_sleep, enabled);
        assert_eq!(
            settings.network.wake_delay_period,
            case["delay"].as_u64().unwrap()
        );
    }
    for clamp in sleep["clamps"].as_array().unwrap() {
        (options[1].set)(&mut settings, &Value::Int(clamp["typed"].as_i64().unwrap())).unwrap();
        assert_eq!(
            settings.network.wake_delay_period,
            clamp["saved"].as_u64().unwrap()
        );
    }
}

#[test]
fn hash_prefix_control_matches_reference_default_and_checkbox() {
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let copied = hydrus_testkit::fixture_json("hash_clipboard_options.json");
    let (_directory, store) = fixture_store(&recorded);
    let mut settings = store.read(Settings::load).unwrap();
    let pages = pages(&settings);
    let page = pages
        .iter()
        .find(|page| page.name == "files and trash")
        .unwrap();
    let reference = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "files and trash")
        .unwrap();
    assert!(page_problems(page, &reference["items"], &settings, &store).is_empty());
    let options = page.options();
    let option = options
        .iter()
        .find(|option| {
            option.label == "When copying file hashes, prefix with booru-friendly hash type: "
        })
        .unwrap();
    assert_eq!(
        (option.get)(&settings),
        Value::Check(copied["default"].as_bool().unwrap())
    );
    for prefix in [true, false] {
        (option.set)(&mut settings, &Value::Check(prefix)).unwrap();
        assert_eq!(settings.file_handling.prefix_hash_when_copying, prefix);
    }
}

#[test]
fn tag_service_controls_match_reference_and_remember_interlock() {
    use hydrus_gui_model::options::{Editor, Row, tag_service_choices};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    let pages = pages(&settings);
    for name in ["file search", "tag editing"] {
        let page = pages.iter().find(|page| page.name == name).unwrap();
        let reference = recorded["pages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|page| page["page"] == name)
            .unwrap();
        assert!(page_problems(page, &reference["items"], &settings, &store).is_empty());
    }
    let fixture = hydrus_testkit::fixture_json("tag_dialog_defaults.json");
    let mut editor = Editor::new(settings);
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "tag editing")
        .unwrap();
    editor.show_page(page);
    let row = |editor: &Editor, label: &str| {
        editor
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Opt { option, .. } if option.label == label))
            .unwrap()
    };
    let checkbox = row(
        &editor,
        "Remember last used default tag service in manage tag dialogs: ",
    );
    let chooser = row(&editor, "Default tag service in tag dialogs: ");
    let choices = tag_service_choices(&store, false);
    for control in fixture["controls"].as_array().unwrap() {
        editor.check(checkbox, control["remember"].as_bool().unwrap());
        assert!(
            matches!(editor.rows()[chooser], Row::Opt { enabled, .. } if enabled == control["default_enabled"].as_bool().unwrap())
        );
        let previous = editor.applied().0.tag_editing.default_service;
        editor.tag_service(chooser, choices[0].0.clone());
        assert_eq!(
            editor.applied().0.tag_editing.default_service,
            if control["default_enabled"] == true {
                choices[0].0.clone()
            } else {
                previous
            }
        );
    }
}

#[test]
fn removed_service_choices_clamp_drafts_and_service_names_are_searchable() {
    use hydrus_gui_model::options::Editor;
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let fixture = hydrus_testkit::fixture_json("search_default_service.json");
    let (_directory, store) = fixture_store(&recorded);
    let mut settings = store.read(Settings::load).unwrap();
    settings.search_defaults.tag_service =
        hydrus_core::ServiceKey::new(b"missing service".to_vec());
    let mut editor = Editor::new(settings);
    editor.resolve_tag_services(&store);
    let (after, before, problems) = editor.applied();
    assert!(problems.is_empty());
    assert_eq!(
        after.search_defaults.tag_service.to_hex(),
        fixture["missing_control"].as_str().unwrap()
    );
    assert_ne!(
        after.search_defaults.tag_service,
        before.search_defaults.tag_service
    );
    assert!(
        editor
            .search("all known tags")
            .iter()
            .any(|suggestion| suggestion.text == "all known tags (file search)")
    );
    assert!(
        editor
            .search("my tags")
            .iter()
            .any(|suggestion| suggestion.text == "my tags (tag editing)")
    );
}

#[test]
fn file_search_boolean_controls_replay_reference_defaults_and_staged_edits() {
    use hydrus_gui_model::options::{Editor, Row};
    let reference = hydrus_testkit::fixture_json("options_dialog.json");
    let (_directory, store) = fixture_store(&reference);
    let settings = store.read(Settings::load).unwrap();
    let page_list = pages(&settings);
    let page = page_list
        .iter()
        .find(|page| page.name == "file search")
        .unwrap();
    let recorded = reference["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "file search")
        .unwrap();
    assert!(page_problems(page, &recorded["items"], &settings, &store).is_empty());
    let fixture = hydrus_testkit::fixture_json("file_search_defaults.json");
    let mut editor = Editor::new(settings.clone());
    let index = editor
        .page_names()
        .iter()
        .position(|name| *name == "file search")
        .unwrap();
    editor.show_page(index);
    let find = |label: &str| {
        editor
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Opt { option, .. } if option.label == label))
            .unwrap()
    };
    let sync_row = find("Start new search pages in 'searching immediately':");
    let everything_row = find("Show system:everything:");
    for event in fixture["events"].as_array().unwrap() {
        let enabled = event["enabled"].as_bool().unwrap();
        editor.check(sync_row, enabled);
        editor.check(everything_row, enabled);
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty());
        assert_eq!(applied.file_search.search_immediately, enabled);
        assert_eq!(applied.file_search.show_system_everything, enabled);
        assert_eq!(
            store.read(Settings::load).unwrap(),
            settings,
            "editor values stay staged"
        );
    }
}

#[test]
fn local_location_option_matches_reference_and_keeps_child_changes_staged() {
    use hydrus_core::ServiceKey;
    use hydrus_core::search::context::LocationContext;
    use hydrus_gui_model::options::Editor;
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    let fixture = hydrus_testkit::fixture_json("default_search_location.json");
    let mut editor = Editor::new(settings.clone());
    for event in fixture["events"].as_array().unwrap() {
        let chosen = LocationContext::new(
            event["selected"]["current"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| ServiceKey::from_hex(value.as_str().unwrap()).unwrap()),
            [],
        );
        editor.set_local_location(chosen.clone());
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty());
        assert_eq!(applied.search_defaults.local_location, chosen);
        let resolved = applied
            .search_defaults
            .resolved_local_location(&store.snapshot().services);
        assert_eq!(
            resolved
                .current()
                .iter()
                .map(ServiceKey::to_hex)
                .collect::<Vec<_>>(),
            event["resolved"]["current"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        assert_eq!(store.read(Settings::load).unwrap(), settings);
    }
}

#[test]
fn read_presentation_controls_match_reference_ranges_and_staged_values() {
    use hydrus_gui_model::options::{Editor, Row};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    let fixture = hydrus_testkit::fixture_json("file_search_presentation.json");
    let mut editor = Editor::new(settings.clone());
    let index = editor
        .page_names()
        .iter()
        .position(|name| *name == "file search")
        .unwrap();
    editor.show_page(index);
    let find = |label: &str| {
        editor
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Opt { option, .. } if option.label == label))
            .unwrap()
    };
    let active = find("Active Search Predicates list height:");
    let suggestions = find("Autocomplete list height:");
    let float = find("Autocomplete dropdown floats over file search pages:");
    for event in fixture["events"].as_array().unwrap() {
        editor.number(active, event["active_rows"].as_i64().unwrap());
        editor.number(suggestions, event["autocomplete_rows"].as_i64().unwrap());
        editor.check(float, event["floating"].as_bool().unwrap());
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty());
        assert_eq!(
            applied.file_search.active_predicate_rows,
            event["view"]["active"]["rows"].as_u64().unwrap() as u32
        );
        assert_eq!(
            applied.file_search.autocomplete_rows,
            event["view"]["autocomplete"]["rows"].as_u64().unwrap() as u32
        );
        assert_eq!(
            applied.file_search.float_autocomplete,
            event["view"]["floating"].as_bool().unwrap()
        );
        for view in ["active", "autocomplete"] {
            let list = &event["view"][view];
            assert_eq!(
                list["hint"].as_i64().unwrap(),
                list["rows"].as_i64().unwrap() * list["text_height"].as_i64().unwrap()
                    + 2 * list["frame"].as_i64().unwrap()
            );
        }
        assert_eq!(store.read(Settings::load).unwrap(), settings);
    }
    editor.number(active, 0);
    editor.number(suggestions, 129);
    let (applied, _, problems) = editor.applied();
    assert!(problems.is_empty());
    assert_eq!(applied.file_search.active_predicate_rows, 1);
    assert_eq!(applied.file_search.autocomplete_rows, 128);
}

#[test]
fn file_search_limit_controls_stage_and_clamp_reference_values() {
    use hydrus_gui_model::options::{Editor, Row};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    let pages = pages(&settings);
    let page = pages
        .iter()
        .find(|page| page.name == "file search")
        .unwrap();
    let reference = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "file search")
        .unwrap();
    assert!(page_problems(page, &reference["items"], &settings, &store).is_empty());
    let mut editor = Editor::new(settings.clone());
    let index = editor
        .page_names()
        .iter()
        .position(|name| *name == "file search")
        .unwrap();
    editor.show_page(index);
    let find = |label: &str| {
        editor
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Opt { option, .. } if option.label == label))
            .unwrap()
    };
    let limit = find("Implicit system:limit for all searches: ");
    let refresh = find("If explicit system:limit, then refresh search when file sort changes: ");
    editor.none(limit, false);
    editor.number(limit, 0);
    editor.check(refresh, false);
    let (applied, _, problems) = editor.applied();
    assert!(problems.is_empty());
    assert_eq!(applied.file_search.implicit_limit, Some(1));
    assert!(!applied.file_search.refresh_limited_sort);
    editor.number(limit, 100_000_001);
    assert_eq!(
        editor.applied().0.file_search.implicit_limit,
        Some(100_000_000)
    );
    editor.none(limit, true);
    assert!(editor.applied().0.file_search.implicit_limit.is_none());
    assert_eq!(store.read(Settings::load).unwrap(), settings);
}

#[test]
fn viewer_canvas_controls_match_reference_and_stage_bounded_values() {
    use hydrus_gui_model::options::{Editor, Row as EditorRow};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let fixture = hydrus_testkit::fixture_json("viewer_canvas_options.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    for name in ["media playback", "media viewer"] {
        let registry = pages(&settings);
        let page = registry.iter().find(|page| page.name == name).unwrap();
        let reference = recorded["pages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|page| page["page"] == name)
            .unwrap();
        let problems = page_problems(page, &reference["items"], &settings, &store);
        assert!(problems.is_empty(), "{problems:?}");
    }
    assert_eq!(
        settings.viewer_canvas.recenter_on_resize,
        fixture["initial"]["media_viewer_recenter_media_on_window_resize"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        settings.viewer_canvas.seek_height,
        fixture["initial"]["animated_scanbar_height"]
            .as_u64()
            .unwrap() as u32
    );
    assert_eq!(
        settings.viewer_canvas.seek_nub_width,
        fixture["initial"]["animated_scanbar_nub_width"]
            .as_u64()
            .unwrap() as u32
    );
    let mut editor = Editor::new(settings.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "media viewer")
        .unwrap();
    editor.show_page(page);
    let indices: Vec<usize> = [
        "Seek bar height:",
        "Seek bar height when mouse away:",
        "Seek bar nub width:",
    ]
    .iter()
    .map(|label| {
        editor
            .rows()
            .iter()
            .position(|row| matches!(row, EditorRow::Opt { option, .. } if option.label == *label))
            .unwrap()
    })
    .collect();
    for event in fixture["seek"].as_array().unwrap() {
        editor.number(indices[0], event["height"].as_i64().unwrap());
        editor.none(indices[1], event["hidden_height"].is_null());
        if let Some(height) = event["hidden_height"].as_i64() {
            editor.number(indices[1], height);
        }
        editor.number(indices[2], event["nub"].as_i64().unwrap());
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            applied.viewer_canvas.seek_height,
            event["height"].as_u64().unwrap() as u32
        );
        assert_eq!(
            applied.viewer_canvas.seek_hidden_height,
            event["hidden_height"].as_u64().map(|value| value as u32)
        );
        assert_eq!(
            applied.viewer_canvas.seek_nub_width,
            event["nub"].as_u64().unwrap() as u32
        );
    }
    editor.number(indices[0], 0);
    editor.none(indices[1], false);
    editor.number(indices[1], 256);
    editor.number(indices[2], 64);
    let applied = editor.applied().0.viewer_canvas;
    assert_eq!(
        (
            applied.seek_height,
            applied.seek_hidden_height,
            applied.seek_nub_width
        ),
        (1, Some(255), 63)
    );
    assert_eq!(
        store.read(Settings::load).unwrap(),
        settings,
        "all edits remain drafts"
    );
}

#[test]
fn viewer_hover_controls_replay_reference_enabled_states() {
    use hydrus_gui_model::options::{Editor, Row as EditorRow};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let fixture = hydrus_testkit::fixture_json("viewer_hover_options.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    let registry = pages(&settings);
    let page = registry
        .iter()
        .find(|page| page.name == "media viewer hovers")
        .unwrap();
    let reference = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "media viewer hovers")
        .unwrap();
    let problems = page_problems(page, &reference["items"], &settings, &store);
    assert!(problems.is_empty(), "{problems:?}");
    let initial = &settings.viewer_hovers;
    assert_eq!(
        serde_json::json!([
            initial.tags,
            initial.ratings,
            initial.notes,
            initial.index_background
        ]),
        fixture["initial"]
    );
    let mut editor = Editor::new(settings.clone());
    let index = editor
        .page_names()
        .iter()
        .position(|name| *name == "media viewer hovers")
        .unwrap();
    editor.show_page(index);
    let labels = [
        "Pop-in tags (left) hover window on mouseover:",
        "Pop-in ratings and locations (top-right) hover window on mouseover:",
        "Pop-in notes (right) hover window on mouseover:",
        "Draw index text (bottom-right) in the viewer background:",
    ];
    let rows: Vec<usize> = labels
        .iter()
        .map(|label| {
            editor
                .rows()
                .iter()
                .position(|row| matches!(row,EditorRow::Opt {option,..} if option.label == *label))
                .unwrap()
        })
        .collect();
    for event in fixture["events"].as_array().unwrap() {
        for (i, row) in rows.iter().enumerate() {
            editor.check(*row, event["values"][i].as_bool().unwrap());
        }
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        let hovers = applied.viewer_hovers;
        assert_eq!(
            serde_json::json!([
                hovers.tags,
                hovers.ratings,
                hovers.notes,
                hovers.index_background
            ]),
            event["values"]
        );
        for i in 0..3 {
            assert_eq!(
                event["stored"][i].as_bool().unwrap(),
                !event["values"][i].as_bool().unwrap()
            );
        }
        assert_eq!(event["stored"][3], event["values"][3]);
    }
    assert_eq!(store.read(Settings::load).unwrap(), settings);
}

#[test]
fn viewer_pointer_controls_stage_the_reference_drag_preferences() {
    use hydrus_gui_model::options::{Editor, Row as EditorRow};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let fixture = hydrus_testkit::fixture_json("viewer_pointer_options.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    let registry = pages(&settings);
    let page = registry
        .iter()
        .find(|page| page.name == "media viewer")
        .unwrap();
    let reference = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "media viewer")
        .unwrap();
    let problems = page_problems(page, &reference["items"], &settings, &store);
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(
        settings.viewer_pointer.disallow_duration_drag,
        fixture["initial"]["disallow_duration_drag"]
            .as_bool()
            .unwrap()
    );
    // The imported fixture is Linux; a fresh native default is platform-specific.
    assert_eq!(
        settings.viewer_pointer.hide_during_drag,
        fixture["initial"]["hide_drag"].as_bool().unwrap()
    );
    let mut editor = Editor::new(settings.clone());
    let index = editor
        .page_names()
        .iter()
        .position(|name| *name == "media viewer")
        .unwrap();
    editor.show_page(index);
    let labels = [
        "Do not allow mouse media drag-panning when the media has duration:",
        "Hide mouse cursor during media viewer drags:",
    ];
    let rows: Vec<_> = labels
        .iter()
        .map(|label| {
            editor
                .rows()
                .iter()
                .position(|row| matches!(row, EditorRow::Opt {option,..} if option.label == *label))
                .unwrap()
        })
        .collect();
    for event in fixture["drags"].as_array().unwrap() {
        editor.check(rows[0], event["disallow"].as_bool().unwrap());
        editor.check(rows[1], event["hide"].as_bool().unwrap());
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            applied.viewer_pointer.disallow_duration_drag,
            event["disallow"].as_bool().unwrap()
        );
        assert_eq!(
            applied.viewer_pointer.hide_during_drag,
            event["hide"].as_bool().unwrap()
        );
    }
    assert_eq!(
        store.read(Settings::load).unwrap(),
        settings,
        "drafts do not persist"
    );
}

#[test]
fn viewer_focus_controls_stage_independent_reference_policies() {
    use hydrus_gui_model::options::{Editor, Row as EditorRow};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let fixture = hydrus_testkit::fixture_json("viewer_focus_options.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    assert_eq!(
        serde_json::json!([
            settings.viewer_focus.seek_requires_focus,
            settings.viewer_focus.hovers_require_focus
        ]),
        fixture["initial"]
    );
    let registry = pages(&settings);
    let labels = [
        (
            "media viewer",
            "Seek bar full-height pop-in requires window focus:",
        ),
        (
            "media viewer hovers",
            "Hover window pop-in requires window focus:",
        ),
    ];
    for (name, _) in labels {
        let page = registry.iter().find(|page| page.name == name).unwrap();
        let reference = recorded["pages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|page| page["page"] == name)
            .unwrap();
        let problems = page_problems(page, &reference["items"], &settings, &store);
        assert!(problems.is_empty(), "{problems:?}");
    }
    let mut editor = Editor::new(settings.clone());
    for event in fixture["events"].as_array().unwrap() {
        for ((name, label), field) in labels
            .iter()
            .zip(["seek_requires_focus", "hover_requires_focus"])
        {
            let page = editor
                .page_names()
                .iter()
                .position(|page| *page == *name)
                .unwrap();
            editor.show_page(page);
            let row = editor
                .rows()
                .iter()
                .position(|row| matches!(row,EditorRow::Opt {option,..} if option.label == *label))
                .unwrap();
            editor.check(row, event[field].as_bool().unwrap());
        }
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            applied.viewer_focus.seek_requires_focus,
            event["seek_requires_focus"].as_bool().unwrap()
        );
        assert_eq!(
            applied.viewer_focus.hovers_require_focus,
            event["hover_requires_focus"].as_bool().unwrap()
        );
    }
    assert_eq!(store.read(Settings::load).unwrap(), settings);
}

#[test]
fn closing_controls_stage_all_reference_preferences_without_persisting() {
    use hydrus_gui_model::options::{Editor, Row as EditorRow};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let fixture = hydrus_testkit::fixture_json("viewer_closing_options.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    let registry = pages(&settings);
    let page = registry
        .iter()
        .find(|page| page.name == "media viewer")
        .unwrap();
    let reference = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "media viewer")
        .unwrap();
    let problems = page_problems(page, &reference["items"], &settings, &store);
    assert!(problems.is_empty(), "{problems:?}");
    let mut editor = Editor::new(settings.clone());
    let index = editor
        .page_names()
        .iter()
        .position(|name| *name == "media viewer")
        .unwrap();
    editor.show_page(index);
    let labels = [
        "When closing the media viewer, re-select original search page: ",
        "When closing the media viewer, tell original search page to select exit media: ",
        "ADVANCED: When closing the media viewer with the above focusing options, activate Main GUI: ",
        "DEBUG: When closing the media viewer at any time, activate Main GUI: ",
    ];
    let rows: Vec<_> = labels
        .iter()
        .map(|label| {
            editor
                .rows()
                .iter()
                .position(|row| matches!(row, EditorRow::Opt {option,..} if option.label == *label))
                .unwrap()
        })
        .collect();
    for event in fixture["events"].as_array().unwrap() {
        for (&index, value) in rows.iter().zip(event["values"].as_array().unwrap()) {
            editor.check(index, value.as_bool().unwrap());
        }
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            serde_json::json!([
                applied.viewer_closing.reselect_page,
                applied.viewer_closing.select_exit_media,
                applied.viewer_closing.activate_focusing,
                applied.viewer_closing.activate_always
            ]),
            event["values"]
        );
    }
    assert_eq!(store.read(Settings::load).unwrap(), settings);
}

#[test]
fn passive_background_controls_match_reference_and_stage_independent_copies() {
    use hydrus_gui_model::options::{Editor, Row as EditorRow};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let fixture = hydrus_testkit::fixture_json("viewer_background_options.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    assert_eq!(
        serde_json::json!([
            settings.viewer_background.tags,
            settings.viewer_background.information,
            settings.viewer_background.ratings,
            settings.viewer_background.notes
        ]),
        fixture["initial"]
    );
    let registry = pages(&settings);
    let page = registry
        .iter()
        .find(|page| page.name == "media viewer hovers")
        .unwrap();
    let reference = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "media viewer hovers")
        .unwrap();
    let problems = page_problems(page, &reference["items"], &settings, &store);
    assert!(problems.is_empty(), "{problems:?}");
    let mut editor = Editor::new(settings.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "media viewer hovers")
        .unwrap();
    editor.show_page(page);
    let labels = [
        "Draw tags (left) in the viewer background:",
        "Draw file information (top) in the viewer background:",
        "Draw ratings and locations (top-right) in the viewer background:",
        "Draw notes (right) in the viewer background:",
    ];
    let rows: Vec<_> = labels
        .iter()
        .map(|label| {
            editor
                .rows()
                .iter()
                .position(|row| matches!(row, EditorRow::Opt {option,..} if option.label == *label))
                .unwrap()
        })
        .collect();
    for event in fixture["events"].as_array().unwrap() {
        for (&index, value) in rows.iter().zip(event["values"].as_array().unwrap()) {
            editor.check(index, value.as_bool().unwrap());
        }
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            serde_json::json!([
                applied.viewer_background.tags,
                applied.viewer_background.information,
                applied.viewer_background.ratings,
                applied.viewer_background.notes
            ]),
            event["values"]
        );
        assert_eq!(
            applied.viewer_hovers, settings.viewer_hovers,
            "passive copies do not change popups"
        );
    }
    assert_eq!(store.read(Settings::load).unwrap(), settings);
}

#[test]
fn subscription_file_failure_limit_replays_qt_noneable_states_in_parent_transaction() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(native.path()).unwrap();
    let fixture = hydrus_testkit::fixture_json("subscription_failure_limit.json");
    for state in fixture["options"]["states"].as_array().unwrap() {
        let before_settings = store.read(Settings::load).unwrap();
        let stored_before = before_settings
            .network
            .subscription_file_error_cancel_threshold;
        assert_eq!(serde_json::json!(stored_before), state["saved_before"]);
        let mut editor = hydrus_gui_model::options::Editor::new(before_settings);
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "downloading")
            .unwrap();
        editor.show_page(page);
        let row = editor.rows().iter().position(|row| matches!(row, hydrus_gui_model::options::Row::Opt {option,..} if option.label == "If a subscription has this many failed file imports, stop and continue later:")).unwrap();
        let value = state["given"].as_i64();
        editor.none(row, value.is_none());
        if let Some(value) = value {
            editor.number(row, value);
        }
        let (after, before, errors) = editor.applied();
        assert!(errors.is_empty());
        assert_eq!(
            serde_json::json!(after.network.subscription_file_error_cancel_threshold),
            state["value"]
        );
        assert_eq!(
            store
                .read(Settings::load)
                .unwrap()
                .network
                .subscription_file_error_cancel_threshold,
            stored_before
        );
        let before = before.clone();
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        assert_eq!(
            serde_json::json!(
                store
                    .read(Settings::load)
                    .unwrap()
                    .network
                    .subscription_file_error_cancel_threshold
            ),
            state["saved_after"]
        );
    }
}

#[test]
fn cursor_autohide_control_matches_reference_default_bounds_and_none() {
    use hydrus_gui_model::options::{Editor, Row as EditorRow};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let fixture = hydrus_testkit::fixture_json("viewer_cursor_options.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    assert_eq!(
        settings.viewer_cursor.autohide_ms.map(u64::from),
        fixture["initial"].as_u64()
    );
    let registry = pages(&settings);
    let page = registry
        .iter()
        .find(|page| page.name == "media viewer")
        .unwrap();
    let reference = recorded["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"] == "media viewer")
        .unwrap();
    let problems = page_problems(page, &reference["items"], &settings, &store);
    assert!(problems.is_empty(), "{problems:?}");
    let mut editor = Editor::new(settings.clone());
    let index = editor
        .page_names()
        .iter()
        .position(|name| *name == "media viewer")
        .unwrap();
    editor.show_page(index);
    let row = editor.rows().iter().position(|row| matches!(row, EditorRow::Opt {option,..} if option.label == "Time until mouse cursor autohides on media viewer:")).unwrap();
    for event in fixture["events"].as_array().unwrap() {
        let value = event["value"].as_i64();
        editor.none(row, value.is_none());
        if let Some(value) = value {
            editor.number(row, value);
        }
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(applied.viewer_cursor.autohide_ms.map(i64::from), value);
    }
    editor.number(row, 99);
    assert_eq!(editor.applied().0.viewer_cursor.autohide_ms, Some(100));
    editor.number(row, 100_001);
    assert_eq!(editor.applied().0.viewer_cursor.autohide_ms, Some(100_000));
    assert_eq!(store.read(Settings::load).unwrap(), settings);
}

#[test]
fn subscription_concurrency_replays_recorded_clamps_and_committed_parent_states() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let fixture = hydrus_testkit::fixture_json("subscription_concurrency.json");
    for state in fixture["options"]["states"].as_array().unwrap() {
        let settings = store.read(Settings::load).unwrap();
        assert_eq!(
            serde_json::json!(settings.network.max_simultaneous_subscriptions),
            state["saved_before"]
        );
        let mut editor = hydrus_gui_model::options::Editor::new(settings);
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "downloading")
            .unwrap();
        editor.show_page(page);
        let row = editor.rows().iter().position(|row| matches!(row, hydrus_gui_model::options::Row::Opt {option,..} if option.label == "Maximum number of subscriptions that can sync simultaneously:")).unwrap();
        let hydrus_gui_model::options::Row::Opt { option, .. } = &editor.rows()[row] else {
            unreachable!()
        };
        let Kind::Int { min, max } = option.kind else {
            panic!("not an integer spin")
        };
        assert_eq!(serde_json::json!(min), fixture["options"]["minimum"]);
        assert_eq!(serde_json::json!(max), fixture["options"]["maximum"]);
        editor.number(row, state["given"].as_i64().unwrap());
        let (after, before, errors) = editor.applied();
        assert!(errors.is_empty());
        assert_eq!(
            serde_json::json!(after.network.max_simultaneous_subscriptions),
            state["value"]
        );
        assert_eq!(
            serde_json::json!(
                store
                    .read(Settings::load)
                    .unwrap()
                    .network
                    .max_simultaneous_subscriptions
            ),
            state["saved_before"],
            "draft edits do not reach the runner"
        );
        let before = before.clone();
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        assert_eq!(
            serde_json::json!(
                store
                    .read(Settings::load)
                    .unwrap()
                    .network
                    .max_simultaneous_subscriptions
            ),
            state["saved_after"]
        );
    }
}

#[test]
fn zoom_switch_and_animation_loop_controls_match_reference_and_stage_changes() {
    use hydrus_gui_model::options::{Editor, Row as EditorRow};
    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let fixture = hydrus_testkit::fixture_json("viewer_zoom_loop_options.json");
    let (_directory, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    assert_eq!(settings.viewer_playback.zoom_switch, 0);
    assert_eq!(
        settings.viewer_playback.always_loop,
        fixture["initial"]["loop"].as_bool().unwrap()
    );
    for name in ["media playback", "media viewer hovers"] {
        let registry = pages(&settings);
        let page = registry.iter().find(|page| page.name == name).unwrap();
        let reference = recorded["pages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|page| page["page"] == name)
            .unwrap();
        let problems = page_problems(page, &reference["items"], &settings, &store);
        assert!(problems.is_empty(), "{problems:?}");
    }
    let mut editor = Editor::new(settings.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "media viewer hovers")
        .unwrap();
    editor.show_page(page);
    let row = editor.rows().iter().position(|row| matches!(row, EditorRow::Opt {option,..} if option.label == "Zoom switch button switches between:")).unwrap();
    let EditorRow::Opt { option, .. } = &editor.rows()[row] else {
        panic!("choice row")
    };
    let Kind::Choice(labels) = &option.kind else {
        panic!("choice control")
    };
    assert_eq!(serde_json::json!(labels), fixture["initial"]["choices"]);
    for choice in 0..4 {
        editor.choose(row, choice);
        assert_eq!(editor.applied().0.viewer_playback.zoom_switch, choice);
    }
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "media playback")
        .unwrap();
    editor.show_page(page);
    let row = editor.rows().iter().position(|row| matches!(row, EditorRow::Opt {option,..} if option.label == "Always Loop Animations:")).unwrap();
    editor.check(row, false);
    let (after, before, errors) = editor.applied();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!after.viewer_playback.always_loop);
    assert_eq!(
        store.read(Settings::load).unwrap(),
        settings,
        "drafts remain staged"
    );
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let saved = store.read(Settings::load).unwrap().viewer_playback;
    assert_eq!(saved.zoom_switch, 3);
    assert!(!saved.always_loop);
}

#[test]
fn export_default_directory_matches_recorded_blank_literal_and_portable_paths() {
    let reference = hydrus_testkit::fixture_json("export_default_directory.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let settings = store.read(Settings::load).unwrap();
    let offered = pages(&settings);
    let page = offered
        .iter()
        .find(|page| page.name == "exporting")
        .unwrap();
    let option = page
        .options()
        .into_iter()
        .find(|option| option.label == reference["label"].as_str().unwrap())
        .unwrap();
    assert_eq!(option.kind, Kind::Directory);
    assert!(
        matches!(page.items.last(), Some(Item::Box(title, _)) if *title == reference["box"].as_str().unwrap())
    );
    for case in reference["cases"].as_array().unwrap() {
        // Native settings hold resolved paths; imported reference portable paths
        // are resolved against the source database by the import consumer.
        let shown = case["shown"]
            .as_str()
            .unwrap()
            .replace("<DB>", directory.path().to_str().unwrap());
        let mut before = settings.clone();
        before.export.default_directory = (!shown.is_empty()).then_some(shown.clone());
        assert_eq!((option.get)(&before), Value::Text(shown));
        let entered = case["entered"]
            .as_str()
            .unwrap()
            .replace("<DB>", directory.path().to_str().unwrap());
        let mut after = before.clone();
        (option.set)(&mut after, &Value::Text(entered)).unwrap();
        assert_eq!(before.export.phrase, after.export.phrase);
        if case["saved"].is_null() {
            assert!(after.export.default_directory.is_none());
            assert!(
                std::path::Path::new(&hydrus_gui_model::export_files::default_directory(
                    &store,
                    &after.export
                ))
                .ends_with("hydrus_export")
            );
        } else {
            let expected = directory.path().join(case["saved"].as_str().unwrap());
            assert_eq!(
                hydrus_gui_model::export_files::default_directory(&store, &after.export),
                expected.to_string_lossy().as_ref()
            );
        }
    }
    let mut portable = settings.export;
    portable.default_directory = Some("synthetic exports 日本".into());
    assert_eq!(
        hydrus_gui_model::export_files::default_directory(&store, &portable),
        directory
            .path()
            .join("synthetic exports 日本")
            .to_string_lossy()
    );
}

#[test]
fn eye_menu_preferences_follow_all_reference_combinations_without_saving_detached_edits() {
    use hydrus_gui_model::options::{Editor, Row as EditorRow};
    use serde_json::json;

    let recorded = hydrus_testkit::fixture_json("options_dialog.json");
    let fixture = hydrus_testkit::fixture_json("viewer_eye_menu.json");
    let (_dirs, store) = fixture_store(&recorded);
    let settings = store.read(Settings::load).unwrap();
    let eye = &settings.viewer_eye_menu;
    assert_eq!(
        json!([
            eye.collapse_window,
            eye.collapse_hovers,
            eye.collapse_rendering
        ]),
        fixture["initial"]
    );
    let mut editor = Editor::new(settings.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "media viewer hovers")
        .unwrap();
    editor.show_page(page);
    let rows: Vec<_> = fixture["labels"].as_array().unwrap().iter().map(|label| {
        editor.rows().iter().position(|row| matches!(row, EditorRow::Opt {option,..} if option.label == label.as_str().unwrap())).unwrap()
    }).collect();
    for event in fixture["events"].as_array().unwrap() {
        for (row, value) in rows.iter().zip(event["values"].as_array().unwrap()) {
            editor.check(*row, value.as_bool().unwrap());
        }
        let (applied, _, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        let eye = applied.viewer_eye_menu;
        assert_eq!(
            json!([
                eye.collapse_window,
                eye.collapse_hovers,
                eye.collapse_rendering
            ]),
            event["stored"]
        );
        assert_eq!(store.read(Settings::load).unwrap(), settings);
    }
    // The existing Options storage merge preserves live eye-menu defaults,
    // even when the detached Options snapshot changes a collapse flag.
    let mut applied = settings.clone();
    applied.viewer_eye_menu.collapse_window = false;
    let baseline = settings.clone();
    store
        .write(move |ctx| {
            let mut live: hydrus_store::settings::ViewerEyeMenuSettings =
                hydrus_store::settings::get(ctx.conn())?;
            live.start_frameless = true;
            hydrus_store::settings::set(ctx.conn(), &live)?;
            applied.save(ctx.conn(), &baseline)
        })
        .unwrap();
    let saved = store.read(Settings::load).unwrap().viewer_eye_menu;
    assert!(!saved.collapse_window);
    assert!(saved.start_frameless);
    assert_eq!(
        saved.collapse_hovers,
        settings.viewer_eye_menu.collapse_hovers
    );
}
