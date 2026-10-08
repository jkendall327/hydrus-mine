//! The editors of system predicates offered without a value, against the
//! reference's own (`oracle/record_system_predicate_editors.py`, on the
//! `basic` fixture): what the empty search box offers, each editor's pages,
//! ready-made buttons and panels, what each panel makes at first and after
//! each of its fields is changed; and the editor window adding what it
//! makes to the page's search.

use std::sync::Arc;

use serde_json::Value as Json;
use slint::{ComponentHandle as _, Model as _, platform::WindowAdapter as _};

use hydrus_core::search::context::{LocationContext, TagContext};
use hydrus_core::service::builtin_keys;
use hydrus_core::{ServiceKey, ServiceType};
use hydrus_gui::autocomplete::Autocomplete;
use hydrus_gui::predicate_editor_window::button_label;
use hydrus_gui::predicate_editors::{Blank, Context, Editor, Field, Panel, Pressed};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{CivilDateTime, Predicate, TextContext, predicate_text};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

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

fn recorded() -> Json {
    hydrus_testkit::fixture_json("system_predicate_editors.json")
}

/// The context as the reference had it, on the day it was recorded.
fn context(store: &Store, recorded: &Json) -> Context {
    let today = recorded["today"].as_str().unwrap();
    let mut parts = today.split('-').map(|p| p.parse::<u16>().unwrap());
    let (year, month, day) = (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    );
    Context::new(
        &store.snapshot().services,
        Vec::new(),
        CivilDateTime::new(year, month as u8, day as u8, 0, 0).unwrap(),
    )
}

fn text_context(store: &Store) -> TextContext {
    let viewing = store.read(hydrus_store::settings::get).unwrap_or_default();
    TextContext::from_store(&store.snapshot().services, &viewing)
}

fn texts(predicates: &[Predicate], text: &TextContext) -> Vec<String> {
    predicates.iter().map(|p| predicate_text(p, text)).collect()
}

fn strings(json: &Json) -> Vec<String> {
    json.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_empty_search_box_offers_the_reference_s_system_predicates() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let mut autocomplete = Autocomplete::new(store.clone());
    autocomplete.set_context(
        &LocationContext::single(ServiceKey::new(builtin_keys::MY_FILES.to_vec())),
        &TagContext::default(),
    );
    autocomplete.set_text("");
    let ours: Vec<(String, bool)> = autocomplete
        .suggestions()
        .iter()
        .map(|s| (s.label.clone(), s.editor.is_some()))
        .collect();
    let theirs: Vec<(String, bool)> = recorded["offered"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["text"].as_str().unwrap().to_owned(),
                o["opens_editor"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(ours, theirs);
    // and searching all known files, those needing no file's metadata
    autocomplete.set_context(
        &LocationContext::single(ServiceKey::new(builtin_keys::COMBINED_FILE.to_vec())),
        &TagContext::default(),
    );
    let everywhere: Vec<&str> = autocomplete
        .suggestions()
        .iter()
        .map(|s| s.predicate.as_str())
        .collect();
    assert_eq!(
        everywhere,
        [
            "system:everything",
            "system:file relationships",
            "system:file service",
            "system:file viewing statistics",
            "system:hash",
            "system:limit",
            "system:number of tags",
            "system:rating",
            "system:tag (advanced)",
            "system:tag as number",
        ]
    );
}

// leaf: audit-options-predicate-duration-presets-system-framerate-30fps-1fps
// leaf: audit-options-predicate-duration-presets-system-framerate-60fps-1fps
// leaf: audit-options-predicate-duration-presets-system-has-duration
// leaf: audit-options-predicate-duration-presets-system-no-duration
// leaf: audit-options-predicate-file-relationships-presets-system-is-not-the-best-quality-file-of-its-duplicate-group
// leaf: audit-options-predicate-file-relationships-presets-system-is-the-best-quality-file-of-its-duplicate-group
// leaf: audit-options-predicate-limit-presets-system-limit-is-1-024
// leaf: audit-options-predicate-limit-presets-system-limit-is-256
// leaf: audit-options-predicate-limit-presets-system-limit-is-64
// leaf: audit-options-predicate-notes-presets-system-has-notes
// leaf: audit-options-predicate-notes-presets-system-no-notes
// leaf: audit-options-predicate-number-of-tags-presets-system-has-tags
// leaf: audit-options-predicate-number-of-tags-presets-system-untagged
// leaf: audit-options-predicate-time-import-presets-system-import-time-since-1-day-ago
// leaf: audit-options-predicate-time-import-presets-system-import-time-since-1-month-ago
// leaf: audit-options-predicate-time-import-presets-system-import-time-since-7-days-ago
// leaf: audit-options-predicate-urls-number-of-urls-presets-system-has-urls
// leaf: audit-options-predicate-urls-number-of-urls-presets-system-no-urls
// leaf: audit-options-predicate-file-properties-presets-system-has-audio
// leaf: audit-options-predicate-file-properties-presets-system-no-audio
// leaf: audit-options-predicate-file-properties-presets-system-has-duration
// leaf: audit-options-predicate-file-properties-presets-system-no-duration
// leaf: audit-options-predicate-file-properties-presets-system-has-exif
// leaf: audit-options-predicate-file-properties-presets-system-no-exif
// leaf: audit-options-predicate-file-properties-presets-system-has-forced-filetype
// leaf: audit-options-predicate-file-properties-presets-system-no-forced-filetype
// leaf: audit-options-predicate-file-properties-presets-system-has-human-readable-metadata
// leaf: audit-options-predicate-file-properties-presets-system-no-human-readable-metadata
// leaf: audit-options-predicate-file-properties-presets-system-has-icc-profile
// leaf: audit-options-predicate-file-properties-presets-system-no-icc-profile
// leaf: audit-options-predicate-file-properties-presets-system-has-iptc
// leaf: audit-options-predicate-file-properties-presets-system-no-iptc
// leaf: audit-options-predicate-file-properties-presets-system-has-software-source-metadata
// leaf: audit-options-predicate-file-properties-presets-system-no-software-source-metadata
// leaf: audit-options-predicate-file-properties-presets-system-has-transparency
// leaf: audit-options-predicate-file-properties-presets-system-no-transparency
// leaf: audit-options-predicate-file-properties-presets-system-has-xmp
// leaf: audit-options-predicate-file-properties-presets-system-no-xmp
#[test]
fn each_editor_is_the_reference_s() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let context = context(&store, &recorded);
    let text = text_context(&store);
    for editor in recorded["editors"].as_array().unwrap() {
        let name = editor["text"].as_str().unwrap();
        let blank = Blank::from_text(name).unwrap_or_else(|| panic!("no {name}"));
        let ours = Editor::new(blank, &context);
        let pages = editor["pages"].as_array().unwrap();
        assert_eq!(ours.pages.len(), pages.len(), "{name}");
        for (page, theirs) in ours.pages.iter().zip(pages) {
            assert_eq!(page.name, theirs["name"].as_str().unwrap(), "{name}");
            let buttons: Vec<(String, Vec<String>)> = page
                .buttons
                .iter()
                .map(|b| {
                    let predicates: Vec<Predicate> = b
                        .predicates
                        .iter()
                        .cloned()
                        .map(Predicate::System)
                        .collect();
                    (button_label(b, &text), texts(&predicates, &text))
                })
                .collect();
            let their_buttons: Vec<(String, Vec<String>)> = theirs["buttons"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| {
                    (
                        b["label"].as_str().unwrap().to_owned(),
                        strings(&b["predicates"]),
                    )
                })
                .collect();
            assert_eq!(buttons, their_buttons, "{name}");
            let panels = theirs["panels"].as_array().unwrap();
            assert_eq!(page.panels.len(), panels.len(), "{name}");
            for (panel, theirs) in page.panels.iter().zip(panels) {
                let class = theirs["class"].as_str().unwrap();
                assert_eq!(panel.kind.class_name(), class, "{name}");
                // a tree's groups and their filetypes, in order
                for widget in theirs["widgets"].as_array().unwrap() {
                    if widget["kind"] != "tree" {
                        continue;
                    }
                    let Some(Field::Tree { groups }) = panel
                        .fields
                        .iter()
                        .find(|f| matches!(f, Field::Tree { .. }))
                    else {
                        panic!("{class} has no tree");
                    };
                    let ours: Vec<(String, Vec<String>)> = groups
                        .iter()
                        .map(|g| (g.name.clone(), g.options.clone()))
                        .collect();
                    let theirs: Vec<(String, Vec<String>)> = widget["groups"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|g| {
                            (
                                g["text"].as_str().unwrap().to_owned(),
                                strings(&g["children"]),
                            )
                        })
                        .collect();
                    assert_eq!(ours, theirs);
                }
                let made = panel.predicates(&context).map(|p| texts(&p, &text));
                match theirs["predicates"].as_array() {
                    Some(_) => assert_eq!(made, Ok(strings(&theirs["predicates"])), "{class}"),
                    // (no URL class to choose)
                    None => assert!(made.is_err(), "{class}"),
                }
            }
        }
    }
}

/// Change our panel as the recording changed the reference's widget
/// `widget`: a radio button or drop-down by the choice offering the same
/// options, a number or text by its place among those shown. `false` if
/// ours has no such field.
fn change(
    panel: &mut Panel,
    widgets: &[Json],
    widget: usize,
    set: &Json,
    warnings: &mut Vec<String>,
) -> bool {
    let fact = &widgets[widget];
    let kind = fact["kind"].as_str().unwrap();
    let choice_with = |panel: &Panel, wanted: &dyn Fn(&[String]) -> bool| {
        panel
            .fields
            .iter()
            .position(|f| matches!(f, Field::Choice { options, .. } if wanted(options)))
    };
    let nth_shown = |panel: &Panel, k: usize, number: bool| {
        (0..panel.fields.len())
            .filter(|&i| panel.shown(i))
            .filter(|&i| match &panel.fields[i] {
                Field::Number { .. } => number,
                Field::Text { .. } | Field::Lines { .. } => !number,
                _ => false,
            })
            .nth(k)
    };
    let before = |what: &str| {
        widgets[..widget]
            .iter()
            .filter(|w| {
                w["kind"] == what
                    && (what != "text"
                        || w["class"] == "QLineEdit"
                        || w["class"] == "QPlainTextEdit")
            })
            .count()
    };
    match kind {
        "radio" | "choice" => {
            let options = strings(if kind == "radio" {
                &fact["group"]
            } else {
                &fact["choices"]
            });
            let Some(i) = choice_with(panel, &|o| o == options.as_slice()) else {
                return false;
            };
            let wanted = set.as_str().or(fact["text"].as_str()).unwrap();
            let option = options.iter().position(|o| o == wanted).unwrap();
            panel.choose(i, option);
            true
        }
        "number" => {
            let Some(i) = nth_shown(panel, before("number"), true) else {
                return false;
            };
            panel.set_number(i, set.as_i64().unwrap());
            true
        }
        "text" => {
            let Some(i) = nth_shown(panel, before("text"), false) else {
                return false;
            };
            panel.set_text(i, set.as_str().unwrap());
            true
        }
        // (a calendar and a time box in the reference; typed here)
        "date" | "time" => {
            let Some(i) = nth_shown(panel, usize::from(kind == "time"), false) else {
                return false;
            };
            panel.set_text(i, set.as_str().unwrap());
            true
        }
        "ticks" => {
            let options = strings(&fact["options"]);
            let Some(i) = panel
                .fields
                .iter()
                .position(|f| matches!(f, Field::Ticks { options: o, .. } if *o == options))
            else {
                return false;
            };
            let option = options.iter().position(|o| o == set).unwrap();
            let on = match &panel.fields[i] {
                Field::Ticks { ticked, .. } => !ticked[option],
                _ => unreachable!(),
            };
            panel.tick(i, option, on);
            true
        }
        "tree" => {
            let i = panel
                .fields
                .iter()
                .position(|f| matches!(f, Field::Tree { .. }))
                .unwrap();
            let Field::Tree { groups } = &panel.fields[i] else {
                unreachable!()
            };
            let (group, child) = set
                .as_str()
                .unwrap()
                .split_once('/')
                .map_or((set.as_str().unwrap(), None), |(g, c)| (g, Some(c)));
            let g = groups.iter().position(|x| x.name == group).unwrap();
            let option = child.map(|c| groups[g].options.iter().position(|o| o == c).unwrap());
            let on = match option {
                Some(o) => !groups[g].ticked[o],
                None => !groups[g].ticked.iter().all(|t| *t),
            };
            panel.tick_tree(i, g, option, on);
            true
        }
        // (the reference's like/dislike and star controls, drop-downs here)
        "like" => {
            let i = choice_with(panel, &|o| o.iter().any(|x| x == "like")).unwrap();
            let wanted = match set.as_str().unwrap() {
                "none" => "(not set)",
                other => other,
            };
            let option = match &panel.fields[i] {
                Field::Choice { options, .. } => options.iter().position(|o| o == wanted).unwrap(),
                _ => unreachable!(),
            };
            panel.choose(i, option);
            true
        }
        "stars" => {
            let stars = fact["num_stars"].as_u64().unwrap();
            let wanted = format!("{}/{stars}", (set.as_f64().unwrap() * stars as f64).round());
            let i = choice_with(panel, &|o| {
                o.iter().any(|x| x.ends_with(&format!("/{stars}")))
            })
            .unwrap();
            let option = match &panel.fields[i] {
                Field::Choice { options, .. } => options.iter().position(|o| *o == wanted).unwrap(),
                _ => unreachable!(),
            };
            panel.choose(i, option);
            true
        }
        // (the reference's service specifier button, its choices in place)
        "services" => {
            let modes: Vec<usize> = (0..panel.fields.len())
                .filter(|&i| {
                    matches!(&panel.fields[i], Field::Choice { options, .. }
                        if options == &["service type", "service"])
                })
                .collect();
            let mode = modes[before("services")];
            if let Some(types) = set.get("types") {
                panel.choose(mode, 0);
                let wanted: Vec<&str> = types
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| {
                        ServiceType::from_code(u8::try_from(t.as_u64().unwrap()).unwrap())
                            .unwrap()
                            .short_name()
                    })
                    .collect();
                let Field::Ticks { options, .. } = panel.fields[mode + 1].clone() else {
                    unreachable!()
                };
                for (o, option) in options.iter().enumerate() {
                    panel.tick(mode + 1, o, wanted.contains(&option.as_str()));
                }
            } else {
                panel.choose(mode, 1);
                let wanted = strings(&set["services"]);
                let Field::Ticks { options, .. } = panel.fields[mode + 2].clone() else {
                    unreachable!()
                };
                for (o, option) in options.iter().enumerate() {
                    let name = option.split_once(": ").unwrap().1;
                    panel.tick(mode + 2, o, wanted.iter().any(|w| w == name));
                }
            }
            true
        }
        "button" => {
            let text = fact["text"].as_str().unwrap();
            let i = panel
                .fields
                .iter()
                .position(|f| matches!(f, Field::Button(label) if label == text))
                .unwrap();
            let pressed = panel.press(i);
            let pressed = if matches!(pressed, Pressed::Confirm(_)) {
                // The original recording answers the cleanup question yes.
                panel.press_confirmed(i)
            } else {
                pressed
            };
            if let Pressed::Warning(said) = pressed {
                warnings.push(said);
            }
            true
        }
        other => panic!("can't change a {other}"),
    }
}

/// The number fields a panel shows (value, least and most) are the
/// reference's spin boxes shown, in order.
fn numbers_match(panel: &Panel, widgets: &[Json], what: &str) {
    let ours: Vec<(i64, i64, i64)> = (0..panel.fields.len())
        .filter(|&i| panel.shown(i))
        .filter_map(|i| match &panel.fields[i] {
            Field::Number {
                value, min, max, ..
            } => Some((*value, *min, *max)),
            _ => None,
        })
        .collect();
    let theirs: Vec<(i64, i64, i64)> = widgets
        .iter()
        .filter(|w| w["kind"] == "number")
        .map(|w| {
            (
                w["value"].as_i64().unwrap(),
                w["min"].as_i64().unwrap(),
                w["max"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(ours, theirs, "{what}");
}

// leaf: audit-options-predicate-dimensions-width-operator
// leaf: audit-options-predicate-dimensions-width-value
// leaf: audit-options-predicate-dimensions-height-operator
// leaf: audit-options-predicate-dimensions-height-value
// leaf: audit-options-predicate-dimensions-ratio-operator
// leaf: audit-options-predicate-dimensions-ratio-ratio
// leaf: audit-options-predicate-dimensions-numpixels-operator
// leaf: audit-options-predicate-dimensions-numpixels-value
// leaf: audit-options-predicate-duration-duration-operator
// leaf: audit-options-predicate-duration-duration-value
// leaf: audit-options-predicate-duration-numframes-operator
// leaf: audit-options-predicate-duration-numframes-value
// leaf: audit-options-predicate-duration-framerate-operator
// leaf: audit-options-predicate-duration-framerate-value
// leaf: audit-options-predicate-file-relationships-duplicaterelationships
// leaf: audit-options-predicate-file-service-fileservice-service
// leaf: audit-options-predicate-file-service-fileservice-state
// leaf: audit-options-predicate-file-viewing-statistics-fileviewingstatsviews-test
// leaf: audit-options-predicate-file-viewing-statistics-fileviewingstatsviews-canvas
// leaf: audit-options-predicate-filetype-mime-mode
// leaf: audit-options-predicate-limit-limit
// leaf: audit-options-predicate-notes-hasnotename
// leaf: audit-options-predicate-notes-numnotes-operator
// leaf: audit-options-predicate-notes-numnotes-value
// leaf: audit-options-predicate-number-of-tags-numtags-test
// leaf: audit-options-predicate-number-of-tags-numtags-scope
// leaf: audit-options-predicate-number-of-words-numwords-operator
// leaf: audit-options-predicate-number-of-words-numwords-value
// leaf: audit-options-predicate-similar-files-files-similartofiles-distance
// leaf: audit-options-predicate-tag-as-number-tagasnumber-test
// leaf: audit-options-predicate-tag-as-number-tagasnumber-scope
// leaf: audit-options-predicate-time-archived-archiveddelta-operator
// leaf: audit-options-predicate-time-archived-archiveddelta-delta
// leaf: audit-options-predicate-time-import-agedelta-operator
// leaf: audit-options-predicate-time-import-agedelta-delta
// leaf: audit-options-predicate-time-last-viewed-lastvieweddelta-operator
// leaf: audit-options-predicate-time-last-viewed-lastvieweddelta-delta
// leaf: audit-options-predicate-time-modified-modifieddelta-operator
// leaf: audit-options-predicate-time-modified-modifieddelta-delta
// leaf: audit-options-predicate-urls-known-urls-knownurlsexacturl-has
// leaf: audit-options-predicate-urls-known-urls-knownurlsdomain-has
// leaf: audit-options-predicate-urls-known-urls-knownurlsregex-has
// leaf: audit-options-predicate-urls-known-urls-knownurlsexacturl-rule
// leaf: audit-options-predicate-urls-known-urls-knownurlsdomain-rule
// leaf: audit-options-predicate-urls-known-urls-knownurlsregex-rule
// leaf: audit-options-predicate-urls-number-of-urls-numurls-operator
// leaf: audit-options-predicate-urls-number-of-urls-numurls-value
// leaf: audit-options-predicate-time-archived-archiveddate-operator
// leaf: audit-options-predicate-time-import-agedate-operator
// leaf: audit-options-predicate-time-last-viewed-lastvieweddate-operator
// leaf: audit-options-predicate-time-modified-modifieddate-operator
// leaf: audit-options-predicate-rating-ratingadvanced-state
// leaf: audit-options-predicate-rating-ratingincdec-state
// leaf: audit-options-predicate-rating-ratinglike-state
// leaf: audit-options-predicate-rating-ratingnumerical-state
// leaf: audit-options-predicate-file-viewing-statistics-fileviewingstatsviewtime-canvas
// leaf: audit-options-predicate-similar-files-data-similartodata-distance
// leaf: audit-options-predicate-tag-advanced-tagadvanced-test
// leaf: audit-options-predicate-tag-advanced-tagadvanced-scope
#[test]
fn each_change_to_a_panel_makes_what_the_reference_s_makes() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let context = context(&store, &recorded);
    let text = text_context(&store);
    let mut checked = 0;
    let mut skipped: Vec<String> = Vec::new();
    for editor in recorded["editors"].as_array().unwrap() {
        let blank = Blank::from_text(editor["text"].as_str().unwrap()).unwrap();
        let ours = Editor::new(blank, &context);
        for (page, theirs) in ours.pages.iter().zip(editor["pages"].as_array().unwrap()) {
            for (panel, theirs) in page.panels.iter().zip(theirs["panels"].as_array().unwrap()) {
                let class = theirs["class"].as_str().unwrap();
                let widgets = theirs["widgets"].as_array().unwrap();
                numbers_match(panel, widgets, class);
                let at_first = panel.predicates(&context).map(|p| texts(&p, &text));
                for change_made in theirs["changes"].as_array().unwrap() {
                    let widget = usize::try_from(change_made["widget"].as_u64().unwrap()).unwrap();
                    let set = &change_made["set"];
                    let expected = change_made["predicates"]
                        .as_array()
                        .map(|_| strings(&change_made["predicates"]));
                    let mut changed = panel.clone();
                    if !change(&mut changed, widgets, widget, set, &mut Vec::new()) {
                        // a widget of the reference's we don't have (tag
                        // advanced's autocomplete) changes nothing
                        assert_eq!(expected, at_first.clone().ok(), "{class} {change_made}");
                        skipped.push(format!("{class} {widget}"));
                        continue;
                    }
                    let made = changed.predicates(&context).map(|p| texts(&p, &text));
                    // (the reference offers terabytes, but can't write them)
                    if set == "TB" {
                        assert!(
                            expected
                                .as_ref()
                                .is_some_and(|e| e[0].starts_with("error:")),
                            "{expected:?}"
                        );
                        assert_eq!(made, Ok(vec!["system:filesize < 200TB".to_owned()]));
                        checked += 1;
                        continue;
                    }
                    match expected {
                        Some(expected) => {
                            assert_eq!(made, Ok(expected), "{class} {widget} {set}");
                        }
                        None => assert!(made.is_err(), "{class} {widget} {set}"),
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 150, "{checked}");
    // only the reference's tag autocomplete has no counterpart here
    assert_eq!(skipped, ["PanelPredicateSystemTagAdvanced 7"]);
}

/// Several fields set in turn make what the reference's make: the number of
/// tags swapped for a namespace's own predicate, typed dates and times,
/// tags cleaned, and the amounts either side of "≈".
// leaf: audit-options-predicate-similar-files-files-similartofiles-hashes
// leaf: audit-options-predicate-similar-files-data-similartodata-hashes
// leaf: audit-options-predicate-time-import-agedate-date-time
// leaf: audit-options-predicate-time-last-viewed-lastvieweddate-date-time
// leaf: audit-options-predicate-rating-ratingadvanced-service
#[test]
fn panels_set_in_several_ways_make_what_the_reference_s_make() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let context = context(&store, &recorded);
    let text = text_context(&store);
    for scenario in recorded["scenarios"].as_array().unwrap() {
        let blank = Blank::from_text(scenario["editor"].as_str().unwrap()).unwrap();
        let editor = Editor::new(blank, &context);
        let page = usize::try_from(scenario["page"].as_u64().unwrap()).unwrap();
        let panel = usize::try_from(scenario["panel"].as_u64().unwrap()).unwrap();
        let mut panel = editor.pages[page].panels[panel].clone();
        let mut warnings = Vec::new();
        for step in scenario["steps"].as_array().unwrap() {
            let widgets = step["widgets"].as_array().unwrap();
            let widget = usize::try_from(step["widget"].as_u64().unwrap()).unwrap();
            numbers_match(&panel, widgets, &scenario.to_string());
            assert!(
                change(&mut panel, widgets, widget, &step["set"], &mut warnings),
                "{scenario}"
            );
        }
        let steps = scenario["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| format!("{} {}", s["widget"], s["set"]))
            .collect::<Vec<_>>()
            .join(", ");
        let made = panel.predicates(&context).map(|p| texts(&p, &text));
        // what it makes, or why it can't, word for word; and what it warned
        match scenario["predicates"].as_array() {
            Some(_) => assert_eq!(made, Ok(strings(&scenario["predicates"])), "{steps}"),
            None => assert_eq!(
                made,
                Err(scenario["error"].as_str().unwrap().to_owned()),
                "{steps}"
            ),
        }
        assert_eq!(warnings, strings(&scenario["warnings"]), "{steps}");
    }
}

/// The page's predicates, as shown.
fn shown_predicates(ui: &MainWindow) -> Vec<String> {
    ui.get_predicates()
        .iter()
        .map(|p| p.text.to_string())
        .collect()
}

fn suggestion(ui: &MainWindow, text: &str) -> i32 {
    let suggestions = ui.get_suggestions();
    let i = suggestions
        .iter()
        .position(|s| s.text == text)
        .unwrap_or_else(|| panic!("no {text}"));
    i32::try_from(i).unwrap()
}

#[test]
fn the_editor_window_adds_what_it_makes_to_the_search() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let editor = || {
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("an editor")
    };
    ui.invoke_search_edited("".into());

    // a ready-made button adds its predicate, and closes the editor
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:limit"));
    let window = editor();
    assert!(window.get_note().starts_with("system:limit clips"));
    let buttons: Vec<String> = window
        .get_buttons()
        .iter()
        .map(|b: slint::SharedString| b.to_string())
        .collect();
    assert_eq!(
        buttons,
        [
            "system:limit is 64",
            "system:limit is 256",
            "system:limit is 1,024"
        ]
    );
    window.invoke_button_clicked(0);
    assert!(bound.predicate_editor.borrow().is_none());
    assert_eq!(shown_predicates(&ui), ["system:limit is 64"]);

    // enter on the highlighted one opens it too; a page of several, and a
    // panel's "ok"
    ui.invoke_search_edited("".into());
    let time = suggestion(&ui, "system:time");
    ui.invoke_move_highlight(time);
    ui.invoke_search_accepted();
    let window = editor();
    let pages: Vec<String> = window
        .get_pages()
        .iter()
        .map(|p: slint::SharedString| p.to_string())
        .collect();
    assert_eq!(pages, ["import", "modified", "last viewed", "archived"]);
    window.invoke_page_chosen(1);
    assert_eq!(window.get_buttons().row_count(), 0);
    window.invoke_ok(0);
    assert_eq!(
        shown_predicates(&ui),
        [
            "system:limit is 64",
            "system:modified time: since 7 days ago"
        ]
    );

    // what can't be made says why, and cancelling adds nothing
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:time"));
    let window = editor();
    window.invoke_text_edited(1, 2, "June".into());
    window.invoke_ok(1);
    assert!(window.get_error().contains("year-month-day"));
    assert!(bound.predicate_editor.borrow().is_some());
    window.invoke_cancel();
    assert!(bound.predicate_editor.borrow().is_none());
    assert_eq!(shown_predicates(&ui).len(), 2);

    // choosing "≈" shows the amount either side, which is then set
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:dimensions"));
    let window = editor();
    let width = window.get_panels().row_data(0).unwrap().fields;
    assert!(!width.row_data(3).unwrap().shown);
    window.invoke_chose(0, 1, 2);
    assert!(width.row_data(3).unwrap().shown);
    assert!(!width.row_data(4).unwrap().shown);
    window.invoke_number_edited(0, 3, 50);
    window.invoke_ok(0);
    assert_eq!(
        shown_predicates(&ui).last().unwrap(),
        "system:width \u{2248} 1,920 \u{b1}50"
    );
}

// leaf: audit-options-predicate-file-viewing-statistics-fileviewingstatsviewtime-test
#[test]
fn native_viewtime_milliseconds_survive_accept_recent_reopen_and_cancel() {
    use hydrus_core::search::{predicate::ViewingStat, recent::RecentPredicates};
    let fixture = hydrus_testkit::fixture_json("viewtime_milliseconds.json");
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("".into());
    for milliseconds in [345, 1001] {
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| {
                case["milliseconds"] == milliseconds
                    && case["operator"] == "="
                    && case["locations"] == serde_json::json!(["media"])
            })
            .unwrap();
        ui.invoke_suggestion_chosen(suggestion(&ui, "system:file viewing statistics"));
        let window = bound
            .predicate_editor
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        window.invoke_chose(1, 2, 2);
        for (field, value) in [
            (3, 0),
            (4, 0),
            (5, 0),
            (6, milliseconds / 1000),
            (7, milliseconds % 1000),
        ] {
            window.invoke_number_edited(1, field, value);
        }
        if milliseconds == 345 {
            let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1040, 400);
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("viewtime-milliseconds.png"),
                &pixels,
                1040,
                400,
            )
            .unwrap();
        }
        window.invoke_ok(1);
        assert!(bound.predicate_editor.borrow().is_none());
        assert!(shown_predicates(&ui).contains(&case["text"].as_str().unwrap().to_owned()));
        let recent: RecentPredicates = store.read(hydrus_store::settings::get).unwrap();
        let hydrus_search::SystemPredicate::FileViewingStats { stat, value, .. } =
            recent.by_type[&29][0]
        else {
            panic!("missing viewing predicate");
        };
        assert_eq!(stat, ViewingStat::ViewTimeMilliseconds);
        assert_eq!(value, u64::try_from(milliseconds).unwrap());
    }
    let before: RecentPredicates = store.read(hydrus_store::settings::get).unwrap();
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:file viewing statistics"));
    let window = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        window.get_recent().row_data(0).unwrap(),
        "system:viewtime in media = 1.0 seconds"
    );
    window.invoke_number_edited(1, 7, 999);
    window.invoke_cancel();
    window.invoke_ok(1);
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<RecentPredicates>)
            .unwrap(),
        before
    );
    assert_eq!(shown_predicates(&ui).len(), 2);
}

// leaf: audit-options-predicate-filetype-mime-tree
// leaf: audit-options-predicate-hash-hash-clean
#[test]
fn the_editor_window_shows_what_trees_and_buttons_change() {
    let boundaries = hydrus_testkit::fixture_json("predicate_boundaries.json");
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let editor = || {
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("an editor")
    };
    let field = |window: &hydrus_gui::PredicateEditorWindow, p: usize, f: usize| {
        window
            .get_panels()
            .row_data(p)
            .unwrap()
            .fields
            .row_data(f)
            .unwrap()
    };
    let last = |ui: &MainWindow| shown_predicates(ui).last().cloned().unwrap_or_default();
    ui.invoke_search_edited("".into());

    // the filetype tree: a group's filetypes show once it is expanded, and
    // a group ticked ticks them all
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:filetype"));
    let window = editor();
    let shown_rows = |window: &hydrus_gui::PredicateEditorWindow| -> Vec<(String, bool)> {
        field(window, 0, 2)
            .rows
            .iter()
            .filter(|r| r.shown)
            .map(|r| (r.text.to_string(), r.ticked))
            .collect()
    };
    let groups = shown_rows(&window);
    assert_eq!(groups.len(), 7);
    assert_eq!(groups[0], ("image".to_owned(), false));
    window.invoke_expanded(0, 2, 0, true);
    let rows = shown_rows(&window);
    assert_eq!(rows.len(), 7 + 12);
    assert_eq!(rows[1], ("jpeg".to_owned(), false));
    assert_eq!(rows[13], ("animation".to_owned(), false));
    let check_tree = |step: usize| {
        let rows = field(&window, 0, 2).rows;
        let group = rows.row_data(0).unwrap();
        let state = if group.partial {
            1
        } else if group.ticked {
            2
        } else {
            0
        };
        assert_eq!(state, boundaries["mime"][step]["state"].as_u64().unwrap());
        let selected: Vec<u64> = rows
            .iter()
            .skip(1)
            .take(12)
            .map(|row| if row.ticked { 2 } else { 0 })
            .collect();
        let recorded: Vec<u64> = boundaries["mime"][step]["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap())
            .collect();
        assert_eq!(selected, recorded);
    };
    check_tree(0);
    window.invoke_tree_ticked(0, 2, 0, 1, true);
    check_tree(1);
    window.invoke_tree_ticked(0, 2, 0, -1, true);
    check_tree(2);
    window.invoke_tree_ticked(0, 2, 0, 0, false);
    check_tree(3);
    window.invoke_tree_ticked(0, 2, 0, -1, false);
    check_tree(4);
    window.invoke_tree_ticked(0, 2, 0, 1, true);
    window.invoke_tree_ticked(0, 2, 1, -1, true);
    let rows = shown_rows(&window);
    assert!(rows[2].1, "png ticked");
    assert!(rows[13].1, "animation ticked");
    let tree = field(&window, 0, 2).rows;
    assert!(tree.row_data(0).unwrap().partial);
    assert!(!tree.row_data(13).unwrap().partial);
    window.invoke_expanded(0, 2, 0, false);
    assert_eq!(shown_rows(&window).len(), 7);
    window.invoke_ok(0);
    assert_eq!(last(&ui), "system:filetype is animation, png");

    // "system:hash": a hash of another type is said to be; the clean-up
    // button says which lines aren't hashes, its forced one drops them and
    // chooses the type the rest are, which then makes them
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:hash"));
    let window = editor();
    let md5 = "d41d8cd98f00b204e9800998ecf8427e";
    window.invoke_text_edited(0, 2, format!("{md5}\nnot a hash").into());
    window.invoke_pressed(0, 3);
    assert!(
        window
            .get_notice_message()
            .starts_with("Unfortunately, some hashes did not parse correctly."),
        "{}",
        window.get_notice_message()
    );
    assert!(window.get_notice_message().contains("\"not a hash\""));
    assert!(window.get_notice_open());
    // Acknowledge the actual owned warning before opening its removal question.
    windows
        .get(windows.count() - 1)
        .unwrap()
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Return.into(),
        });
    assert!(!window.get_notice_open());
    assert_eq!(
        field(&window, 0, 2).text,
        boundaries["hash"][0]["text"].as_str().unwrap()
    );
    window.invoke_pressed(0, 4);
    assert_eq!(
        window.get_question(),
        boundaries["hash"][0]["questions"][0].as_str().unwrap()
    );
    window.invoke_text_edited(0, 2, "changed while question pending".into());
    let frozen_error = window.get_error();
    window.invoke_ok(0);
    assert_eq!(window.get_error(), frozen_error);
    assert!(bound.predicate_editor.borrow().is_some());
    assert!(field(&window, 0, 2).text.contains("not a hash"));
    window.invoke_answer(false);
    assert!(window.get_question().is_empty());
    assert_eq!(
        field(&window, 0, 2).text,
        boundaries["hash"][0]["text"].as_str().unwrap()
    );
    window.invoke_pressed(0, 4);
    window.invoke_answer(true);
    assert_eq!(window.get_error(), "");
    assert_eq!(
        field(&window, 0, 2).text,
        boundaries["hash"][1]["text"].as_str().unwrap()
    );
    let types = field(&window, 0, 5);
    assert_eq!(
        types
            .options
            .row_data(usize::try_from(types.chosen).unwrap()),
        Some("md5".into())
    );
    window.invoke_ok(0);
    assert_eq!(last(&ui), format!("system:hash (md5) is {md5}"));

    // "system:rating": a like chosen chooses "is" with it
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:rating"));
    let window = editor();
    assert_eq!(field(&window, 1, 0).text, "favourites");
    window.invoke_chose(1, 2, 1);
    assert_eq!(field(&window, 1, 1).chosen, 2);
    window.invoke_ok(1);
    assert_eq!(last(&ui), "system:rating for favourites is like");

    // "system:similar files": "clear" empties both hashes
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:similar files"));
    let window = editor();
    window.invoke_text_edited(0, 4, "ab".repeat(32).into());
    window.invoke_text_edited(0, 5, "cd".repeat(8).into());
    window.invoke_pressed(0, 1);
    assert_eq!(field(&window, 0, 4).text, "");
    assert_eq!(field(&window, 0, 5).text, "");
}

#[test]
fn more_suggestions_than_fit_scroll_rather_than_spill_over() {
    let (_dirs, store) = store();
    // Exercise the embedded twelve-row boundary explicitly: the current
    // reference defaults are a floating, twenty-two-row dropdown.
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::FileSearchSettings {
                    float_autocomplete: false,
                    autocomplete_rows: 12,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let main_window = windows.get(0).unwrap();
    let (width, height) = (1100_usize, 900_usize);
    // (twice: the first frame lays the window out)
    let draw = || {
        headless::render(&main_window, width as u32, height as u32);
        headless::render(&main_window, width as u32, height as u32)
    };
    // the sidebar's rows of pixels that differ (the grid's thumbnails load
    // as they will)
    let sidebar = 290;
    let differing = |a: &[u8], b: &[u8]| -> Vec<usize> {
        (0..height)
            .filter(|y| {
                a[y * width * 4..(y * width + sidebar) * 4]
                    != b[y * width * 4..(y * width + sidebar) * 4]
            })
            .collect()
    };
    ui.invoke_search_edited("".into());
    let all: Vec<_> = ui.get_suggestions().iter().collect();
    assert!(all.len() > 14, "{} suggestions", all.len());

    // Read autocomplete paints its actual dense selection mask, as Qt paints
    // selected terms. Mutating only `highlighted` leaves that mask unchanged.
    ui.invoke_suggestion_selection_clicked(0, false, false);
    assert!(ui.invoke_suggestions_deselected());
    assert_eq!(ui.get_suggestion_selected().row_count(), all.len());
    assert!(
        ui.get_suggestion_selected()
            .iter()
            .all(|selected| !selected)
    );
    let plain = draw();
    ui.invoke_suggestion_selection_clicked(0, false, false);
    assert_eq!(ui.get_highlighted(), 0);
    let selected: Vec<_> = ui.get_suggestion_selected().iter().collect();
    assert_eq!(selected.len(), all.len());
    assert!(selected[0]);
    assert!(selected[1..].iter().all(|selected| !selected));
    let first = differing(&plain, &draw());
    let top = *first.first().expect("the highlight drawn");
    // twelve rows of 22 pixels show, under which the window is as it is
    // with no more than twelve suggestions
    let bottom = top + 12 * 22 + 2;
    assert!(ui.invoke_suggestions_deselected());
    assert!(
        ui.get_suggestion_selected()
            .iter()
            .all(|selected| !selected)
    );
    let everything = draw();
    ui.set_suggestions(slint::ModelRc::new(slint::VecModel::from(
        all[..12].to_vec(),
    )));
    let twelve = draw();
    let below: Vec<usize> = differing(&everything, &twelve)
        .into_iter()
        .filter(|y| *y >= bottom)
        .collect();
    assert_eq!(
        below,
        Vec::<usize>::new(),
        "suggestions drawn under the dropdown"
    );

    // the last highlighted is scrolled into view
    ui.set_suggestions(slint::ModelRc::new(slint::VecModel::from(all.clone())));
    draw();
    let last = i32::try_from(all.len() - 1).unwrap();
    ui.invoke_suggestion_selection_clicked(last, false, false);
    assert_eq!(ui.get_highlighted(), last);
    let selected: Vec<_> = ui.get_suggestion_selected().iter().collect();
    assert_eq!(selected.len(), all.len());
    assert!(selected[all.len() - 1]);
    assert!(selected[..all.len() - 1].iter().all(|selected| !selected));
    let scrolled = differing(&everything, &draw());
    let scroll_y = ui.get_read_scroll_y();
    let view_height = ui.get_read_view_height();
    let content_height = ui.get_read_content_height();
    let last_top = last as f32 * 22.0;
    let last_bottom = last_top + 22.0;
    assert!(scroll_y < 0.0, "last row did not move the scroll viewport");
    assert!(view_height > 0.0);
    assert!(content_height >= last_bottom);
    assert!(scroll_y >= view_height - content_height - 1.0);
    assert!(last_top + scroll_y >= -1.0);
    assert!(last_bottom + scroll_y <= view_height + 1.0);
    assert!(!scrolled.is_empty(), "the last suggestion not shown");
    assert!(
        scrolled.iter().all(|y| (top - 2..bottom).contains(y)),
        "{scrolled:?} outside {top}..{bottom}"
    );

    // Moving back to the first row uses the same real selection callback and
    // scrolls upward, rather than leaving an off-screen dense selection mask.
    ui.invoke_suggestion_selection_clicked(0, false, false);
    let first_again = differing(&plain, &draw());
    assert!(
        ui.get_read_scroll_y().abs() < f32::EPSILON,
        "the first row returns the viewport to its origin"
    );
    assert!(!first_again.is_empty(), "the first suggestion not restored");
    assert!(first_again.iter().all(|y| (top - 2..bottom).contains(y)));
}

// leaf: audit-options-predicate-custom-defaults
#[test]
fn predicate_star_save_and_reset_survive_cancel_and_reach_future_searches() {
    use hydrus_gui::predicate_editors::defaults::CustomDefaults;
    use slint::ComponentHandle as _;
    let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
    let (dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let editor = || {
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    };
    let open = || {
        ui.invoke_search_edited("".into());
        ui.invoke_suggestion_chosen(suggestion(&ui, "system:limit"));
        editor()
    };
    let actions = |window: &hydrus_gui::PredicateEditorWindow| {
        window
            .get_defaults_actions()
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
    };
    let case = recording["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["class"] == "PanelPredicateSystemLimit")
        .unwrap();
    let window = open();
    window.invoke_defaults_menu(0);
    assert_eq!(actions(&window), strings(&case["menu_before"]));
    window.invoke_number_edited(0, 1, 731);
    window.invoke_defaults_action(0, "set this as new default".into());
    assert!(window.get_error().is_empty());
    window.invoke_defaults_menu(0);
    assert_eq!(actions(&window), strings(&case["menu_after_save"]));
    let kept = store
        .read(hydrus_store::settings::get::<CustomDefaults>)
        .unwrap();
    assert_eq!(
        kept.predicates,
        vec![Predicate::System(hydrus_search::SystemPredicate::Limit(
            731
        ))]
    );
    window.invoke_cancel();
    assert!(shown_predicates(&ui).is_empty());
    let window = open();
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        731
    );
    window.invoke_defaults_action(0, "reset to original default".into());
    assert!(
        store
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap()
            .predicates
            .is_empty()
    );
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        731
    );
    window.invoke_defaults_menu(0);
    assert_eq!(actions(&window), strings(&case["menu_before"]));
    window.invoke_cancel();
    window.invoke_defaults_action(0, "set this as new default".into());
    assert!(
        store
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap()
            .predicates
            .is_empty()
    );
    let window = open();
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        256
    );
    window.invoke_number_edited(0, 1, 731);
    window.invoke_defaults_action(0, "set this as new default".into());
    window.invoke_cancel();
    let window = open();
    window.invoke_defaults_menu(0);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1020, 540);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("predicate_custom_defaults.png"),
        &pixels,
        1020,
        540,
    )
    .unwrap();
    window.invoke_ok(0);
    assert_eq!(shown_predicates(&ui), ["system:limit is 731"]);
    drop(bound);
    drop(ui);
    drop(store);
    let reopened = Store::open(dirs[1].path()).unwrap();
    assert_eq!(
        reopened
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap()
            .predicates,
        kept.predicates
    );
}

#[test]
fn regex_star_save_is_immediate_but_acceptance_checks_and_viewtime_keeps_milliseconds() {
    use hydrus_gui::predicate_editors::defaults::CustomDefaults;
    use slint::ComponentHandle as _;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("".into());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:urls"));
    let window = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    window.invoke_text_edited(2, 3, "predicate-defaults\\.example".into());
    window.invoke_defaults_action(2, "set this as new default".into());
    window.invoke_text_edited(2, 3, "[".into());
    window.invoke_defaults_action(2, "set this as new default".into());
    assert!(window.get_error().is_empty());
    let kept = store
        .read(hydrus_store::settings::get::<CustomDefaults>)
        .unwrap();
    assert_eq!(
        kept.predicates,
        vec![Predicate::System(
            hydrus_search::SystemPredicate::KnownUrl {
                rule: hydrus_core::search::predicate::UrlRule::Regex("[".into()),
                has: true,
            }
        )]
    );
    window.invoke_ok(2);
    assert!(window.get_error().contains("regex"));
    assert!(shown_predicates(&ui).is_empty());
    window.invoke_cancel();
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap(),
        kept
    );
    ui.invoke_search_edited("".into());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:urls"));
    let window = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        window
            .get_panels()
            .row_data(2)
            .unwrap()
            .fields
            .row_data(3)
            .unwrap()
            .text,
        "["
    );
    window.invoke_defaults_menu(2);
    let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
    assert_eq!(
        window
            .get_defaults_actions()
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>(),
        strings(&recording["invalid_regex"]["menu_after_save"])
    );
    window.invoke_cancel();
    ui.invoke_search_edited("".into());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:file viewing statistics"));
    let window = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    window.invoke_number_edited(1, 7, 37);
    window.invoke_defaults_action(1, "set this as new default".into());
    window.invoke_cancel();
    ui.invoke_search_edited("".into());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:file viewing statistics"));
    let window = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        window
            .get_panels()
            .row_data(1)
            .unwrap()
            .fields
            .row_data(7)
            .unwrap()
            .value,
        37
    );
    window.invoke_ok(1);
    let defaults = store
        .read(hydrus_store::settings::get::<CustomDefaults>)
        .unwrap();
    let recent = store
        .read(hydrus_store::settings::get::<hydrus_core::search::recent::RecentPredicates>)
        .unwrap();
    let value = defaults
        .predicates
        .iter()
        .find(|p| {
            matches!(
                p,
                Predicate::System(hydrus_search::SystemPredicate::FileViewingStats { .. })
            )
        })
        .unwrap();
    assert!(matches!(
        value,
        Predicate::System(hydrus_search::SystemPredicate::FileViewingStats {
            stat: hydrus_core::search::predicate::ViewingStat::ViewTimeMilliseconds,
            value: 600_037,
            ..
        })
    ));
    assert!(
        recent.by_type[&29]
            .iter()
            .any(|p| Predicate::System(p.clone()) == *value)
    );
}

// leaf: audit-options-predicate-custom-defaults
#[test]
fn imported_predicate_defaults_reach_panels_and_reset_never_resurrects_legacy_values() {
    use hydrus_gui::predicate_editors::defaults::CustomDefaults;
    use slint::ComponentHandle as _;
    let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
    let source = hydrus_testkit::legacy_fixture("basic");
    let destination = tempfile::tempdir().unwrap();
    let conn = rusqlite::Connection::open(source.path().join("client.db")).unwrap();
    conn.execute(
        "UPDATE json_dumps SET dump = ?1 WHERE dump_type = 22",
        [recording["options_saved_families"][2]
            .to_string()
            .into_bytes()],
    )
    .unwrap();
    drop(conn);
    import_legacy(
        source.path(),
        &destination.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(destination.path()).unwrap();
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap()
            .predicates
            .len(),
        38
    );
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let open = || {
        ui.invoke_search_edited("".into());
        ui.invoke_suggestion_chosen(suggestion(&ui, "system:limit"));
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    };
    let window = open();
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        259
    );
    window.invoke_defaults_menu(0);
    assert_eq!(window.get_defaults_actions().row_count(), 2);
    window.invoke_defaults_action(0, "reset to original default".into());
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        259
    );
    window.invoke_cancel();
    assert!(shown_predicates(&ui).is_empty());
    let window = open();
    assert_eq!(
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(1)
            .unwrap()
            .value,
        256
    );
    window.invoke_defaults_menu(0);
    assert_eq!(window.get_defaults_actions().row_count(), 1);
    window.invoke_cancel();
    drop(bound);
    drop(ui);
    drop(store);
    let reopened = Store::open(destination.path()).unwrap();
    let defaults = reopened
        .read(hydrus_store::settings::get::<CustomDefaults>)
        .unwrap();
    assert_eq!(defaults.predicates.len(), 37);
    assert!(!defaults.predicates.iter().any(|p| matches!(
        p,
        Predicate::System(hydrus_search::SystemPredicate::Limit(_))
    )));
}

// leaf: audit-options-predicate-dimensions-presets-1080p
// leaf: audit-options-predicate-dimensions-presets-4k
// leaf: audit-options-predicate-dimensions-presets-720p
// leaf: audit-options-predicate-dimensions-presets-system-ratio-16-9
// leaf: audit-options-predicate-dimensions-presets-system-ratio-4-3
// leaf: audit-options-predicate-dimensions-presets-system-ratio-9-16
// leaf: audit-options-predicate-dimensions-presets-system-ratio-is-landscape
// leaf: audit-options-predicate-dimensions-presets-system-ratio-is-portrait
// leaf: audit-options-predicate-dimensions-presets-system-ratio-is-square
#[test]
fn dimensions_presets_pointer_acceptance_reaches_page_and_persistent_recent_history() {
    use hydrus_core::search::recent::RecentPredicates;
    use slint::platform::{PointerEventButton, WindowEvent};
    use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
    let fixture = hydrus_testkit::fixture_json("dimensions_presets.json");
    let windows = headless::init();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    for case in cases.iter().filter(|case| !case["label"].is_null()) {
        let (dirs, store) = store();
        store
            .write(|c| hydrus_store::settings::set(c.conn(), &RecentPredicates::default()))
            .unwrap();
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let main_native = windows.get(windows.count() - 1).unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        ui.invoke_search_edited("".into());
        ui.invoke_suggestion_chosen(suggestion(&ui, "system:dimensions"));
        let window = bound
            .predicate_editor
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        let labels: Vec<String> = window.get_buttons().iter().map(|s| s.to_string()).collect();
        assert_eq!(labels, strings(&case["observed_labels"]));
        let index = i32::try_from(
            labels
                .iter()
                .position(|s| s == case["label"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
        let geometry = Rc::new(RefCell::new(BTreeMap::new()));
        window.on_preset_placed({
            let geometry = geometry.clone();
            move |i, x, y, w, h| {
                geometry.borrow_mut().insert(i, (x, y, w, h));
            }
        });
        let native = windows.get(windows.count() - 1).unwrap();
        // Changed-only geometry observers must see a real resize even if the
        // initial native opening size already equals a measured viewport.
        drop(headless::render(&native, 1040, 740));
        for (width, height, suffix) in [(1020, 720, "normal"), (760, 900, "narrow")] {
            let pixels = headless::render(&native, width, height);
            assert_eq!(geometry.borrow().len(), 9);
            for (x, y, w, h) in geometry.borrow().values() {
                assert!(*x >= 0.0 && *y >= 0.0 && *w > 50.0 && *h > 10.0);
                assert!(*x + *w <= f32::from(u16::try_from(width).unwrap()) + 1.0);
                assert!(*y + *h <= f32::from(u16::try_from(height).unwrap()) + 1.0);
            }
            if index == 0 {
                headless::save_png(
                    &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                        .join(format!("dimensions-presets-{suffix}.png")),
                    &pixels,
                    width,
                    height,
                )
                .unwrap();
            }
        }
        let (x, y, w, h) = geometry.borrow()[&index];
        let position = slint::LogicalPosition::new(x + w / 2.0, y + h / 2.0);
        window.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        window
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
        assert!(
            bound.predicate_editor.borrow().is_none(),
            "{} did not accept",
            case["label"]
        );
        let mut expected = strings(&case["predicates"]);
        expected.sort();
        let mut shown = shown_predicates(&ui);
        shown.sort();
        assert_eq!(shown, expected);
        if case["label"] == "1080p" {
            let pixels = headless::render(&main_native, 1020, 720);
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("dimensions-presets-accepted.png"),
                &pixels,
                1020,
                720,
            )
            .unwrap();
        }
        let recent: RecentPredicates = store.read(hydrus_store::settings::get).unwrap();
        let mut saved = texts(
            &recent
                .by_type
                .values()
                .flatten()
                .cloned()
                .map(Predicate::System)
                .collect::<Vec<_>>(),
            &text_context(&store),
        );
        saved.sort();
        let mut reference_recent = strings(&case["recent"]);
        reference_recent.sort();
        assert_eq!(saved, reference_recent);
        // A retained, even forcibly shown, retired editor cannot accept twice.
        window.show().unwrap();
        window.invoke_button_clicked((index + 1) % 9);
        assert_eq!(
            store
                .read(hydrus_store::settings::get::<RecentPredicates>)
                .unwrap(),
            recent
        );
        assert_eq!(shown_predicates(&ui).len(), expected.len());
        window.hide().unwrap();
        drop(window);
        drop(bound);
        ui.hide().unwrap();
        drop(ui);
        drop(store);
        let reopened = Store::open(dirs[1].path()).unwrap();
        assert_eq!(
            reopened
                .read(hydrus_store::settings::get::<RecentPredicates>)
                .unwrap(),
            recent
        );
    }
}

#[test]
fn dimensions_presets_cancel_hidden_and_retired_callbacks_do_not_change_owner() {
    use hydrus_core::search::recent::RecentPredicates;
    let fixture = hydrus_testkit::fixture_json("dimensions_presets.json");
    let cancelled = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["label"].is_null())
        .unwrap();
    let (_dirs, store) = store();
    store
        .write(|c| hydrus_store::settings::set(c.conn(), &RecentPredicates::default()))
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("".into());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:dimensions"));
    let old = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    old.hide().unwrap();
    old.invoke_button_clicked(0);
    assert_eq!(shown_predicates(&ui), strings(&cancelled["predicates"]));
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<RecentPredicates>)
            .unwrap(),
        RecentPredicates::default()
    );
    old.show().unwrap();
    old.invoke_cancel();
    assert!(bound.predicate_editor.borrow().is_none());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:dimensions"));
    let successor = bound
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    old.show().unwrap();
    old.invoke_button_clicked(6);
    old.hide().unwrap();
    assert!(successor.window().is_visible());
    assert_eq!(shown_predicates(&ui), strings(&cancelled["predicates"]));
    assert!(strings(&cancelled["recent"]).is_empty());
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<RecentPredicates>)
            .unwrap(),
        RecentPredicates::default()
    );
    successor.invoke_cancel();
    ui.hide().unwrap();
}

// The URL class panel of system:urls, with the client's URL classes (the
// recording had none to offer): it offers those that file URLs go with, and
// makes the reference's `has url with class` / `does not have url with
// class` (`ClientGUIPredicatesSingle.PanelPredicateSystemKnownURLsURLClass`,
// text as recorded in `predicate_custom_defaults.json`).
// leaf: audit-options-predicate-urls-known-urls-knownurlsurlclass-has
// leaf: audit-options-predicate-urls-known-urls-knownurlsurlclass-rule
#[test]
fn the_url_class_panel_offers_the_clients_url_classes_and_makes_has_or_not_has() {
    use hydrus_core::url::{UrlClass, UrlClassSettings, UrlType};
    let (_dirs, store) = store();
    let _windows = headless::init();
    let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
    let recorded_text = recording["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["class"] == "PanelPredicateSystemKnownURLsURLClass")
        .map(|p| p["before"]["text"][0].as_str().unwrap().to_owned())
        .expect("recorded");
    let class = |name: &str, key: u8, files: bool| UrlClass {
        name: name.into(),
        key: vec![key],
        url_type: UrlType::Post,
        should_be_associated_with_files: files,
        ..UrlClass::default()
    };
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &UrlClassSettings {
                    url_classes: vec![
                        class("predicate defaults posts 0", 1, true),
                        class("not for files", 2, false),
                        class("second posts", 3, true),
                    ],
                    ..UrlClassSettings::default()
                },
            )
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let open = || {
        ui.invoke_search_edited("".into());
        ui.invoke_suggestion_chosen(suggestion(&ui, "system:urls"));
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    };
    let window = open();
    // panels: exact url, domain, regex, url class
    let panel = window.get_panels().row_data(3).unwrap();
    let field = |i: usize| panel.fields.row_data(i).unwrap();
    assert_eq!(
        field(3)
            .options
            .iter()
            .map(|o| o.to_string())
            .collect::<Vec<_>>(),
        ["predicate defaults posts 0", "second posts"],
        "only the classes that go with files"
    );
    assert_eq!(field(1).options.row_data(0).unwrap(), "has");
    // has, the first class
    window.invoke_ok(3);
    let first = shown_predicates(&ui);
    assert_eq!(
        first,
        ["system:has url with class predicate defaults posts 0"]
    );
    assert_eq!(first[0], recorded_text);
    // does not have, the second class
    let window = open();
    window.invoke_chose(3, 1, 1);
    window.invoke_chose(3, 3, 1);
    window.invoke_ok(3);
    assert!(
        shown_predicates(&ui)
            .contains(&"system:does not have url with class second posts".to_string())
    );
}

// system:rating offers one panel per rating service, each labelled with
// the service's name and making a predicate for that service's key, as the
// recorded editor does (`PredicateSystemRatingLike` for "favourites",
// `...Numerical` for "stars", `...IncDec` for "counter").
// (the reference has no selector in these three: each panel is constructed
// for one service and shows its name, so the "service or service-type
// selection" leaves of the like, numerical and inc/dec panels are this; the
// advanced panel's service chooser is tagged with its own scenarios)
// leaf: audit-options-predicate-rating-ratinglike-service
// leaf: audit-options-predicate-rating-ratingnumerical-service
// leaf: audit-options-predicate-rating-ratingincdec-service
#[test]
fn each_rating_panel_is_for_its_own_service() {
    use hydrus_core::search::predicate::{ServiceRef, SystemPredicate};
    let (_dirs, store) = store();
    let recorded = recorded();
    let context = context(&store, &recorded);
    let editor = Editor::new(Blank::from_text("system:rating").unwrap(), &context);
    let key_of = |name: &str| -> Vec<u8> {
        let service = recorded["services"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap();
        hex::decode(service["key"].as_str().unwrap()).unwrap()
    };
    let theirs: Vec<(&str, &str)> = recorded["editors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["text"] == "system:rating")
        .unwrap()["pages"][0]["panels"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| {
            let label = p["widgets"][0]["text"].as_str()?;
            (p["widgets"][0]["kind"] == "label").then(|| (p["class"].as_str().unwrap(), label))
        })
        .collect();
    assert_eq!(
        theirs.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
        ["favourites", "stars", "counter"]
    );
    let mut seen = 0;
    for panel in &editor.pages[0].panels {
        let class = panel.kind.class_name();
        let Some((_, name)) = theirs.iter().find(|(c, _)| *c == class) else {
            continue;
        };
        assert!(
            matches!(&panel.fields[0], Field::Label(l) if l == name),
            "{class} is labelled with its service"
        );
        let made = panel.predicates(&context).unwrap();
        let [Predicate::System(SystemPredicate::Rating { service, .. })] = &made[..] else {
            panic!("{class}: {made:?}");
        };
        assert_eq!(
            service,
            &ServiceRef::Key(hydrus_core::ServiceKey::new(key_of(name))),
            "{class}"
        );
        seen += 1;
    }
    assert_eq!(seen, 3);
}

// The archived and modified date panels, each replaying its own recorded
// scenarios (an operator, a date and a time, as the import and last-viewed
// ones above).
// (the date and time are typed and validated here, a calendar and a time box
// in the reference, which cannot be set to a date that does not exist)
// leaf: audit-options-predicate-time-archived-archiveddate-date-time
// leaf: audit-options-predicate-time-modified-modifieddate-date-time
#[test]
fn archived_and_modified_date_panels_make_the_recorded_date_and_time_predicates() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let context = context(&store, &recorded);
    let text = text_context(&store);
    let editor = Editor::new(Blank::from_text("system:time").unwrap(), &context);
    for (class, page) in [
        ("PanelPredicateSystemModifiedDate", 1),
        ("PanelPredicateSystemArchivedDate", 3),
    ] {
        let scenarios: Vec<&Json> = recorded["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["editor"] == "system:time" && s["page"] == page)
            .collect();
        assert_eq!(scenarios.len(), 2, "{class}: the recorded scenarios");
        let panel = editor
            .pages
            .iter()
            .flat_map(|p| &p.panels)
            .find(|p| p.kind.class_name() == class)
            .unwrap();
        for scenario in &scenarios {
            let mut panel = panel.clone();
            let mut warnings = Vec::new();
            for step in scenario["steps"].as_array().unwrap() {
                let widgets = step["widgets"].as_array().unwrap();
                let widget = usize::try_from(step["widget"].as_u64().unwrap()).unwrap();
                assert!(change(
                    &mut panel,
                    widgets,
                    widget,
                    &step["set"],
                    &mut warnings
                ));
            }
            let made = panel.predicates(&context).map(|p| texts(&p, &text));
            assert_eq!(
                made,
                Ok(strings(&scenario["predicates"])),
                "{class}: {scenario}"
            );
        }
        // a date or time that is not one is refused, never searched
        let mut panel = panel.clone();
        let dates = (0..panel.fields.len())
            .filter(|&i| matches!(panel.fields[i], Field::Text { .. }))
            .collect::<Vec<_>>();
        panel.set_text(dates[0], "2011-02-30");
        assert!(
            panel.predicates(&context).is_err(),
            "{class}: no 30 February"
        );
        panel.set_text(dates[0], "2011-06-04");
        panel.set_text(dates[1], "25:61");
        assert!(panel.predicates(&context).is_err(), "{class}: no 25:61");
    }
}

// "Paste image!" takes the clipboard's bitmap if it holds one, else a file
// path, as the reference's `_Paste` does; the hashes are those of a file of
// the same pixels.
#[test]
fn paste_image_takes_a_clipboard_bitmap_or_a_file_path_and_clear_empties_both_hashes() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let editor = || {
        bound
            .predicate_editor
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("an editor")
    };
    let text = |window: &hydrus_gui::PredicateEditorWindow, f: usize| {
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(f)
            .unwrap()
            .text
            .to_string()
    };
    // a 16 x 16 picture with some structure
    let (width, height) = (16_usize, 16_usize);
    let mut rgba = Vec::new();
    for y in 0..height {
        for x in 0..width {
            rgba.extend([
                (x * 16) as u8,
                (y * 16) as u8,
                if (x / 4 + y / 4) % 2 == 0 { 200 } else { 30 },
                255,
            ]);
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same pixels.png");
    {
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(file, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
    }

    // nothing on the clipboard: the reference's warning, nothing pasted
    hydrus_gui::set_clipboard_image_reader(|| None);
    hydrus_gui::set_clipboard_reader(|| Ok(None));
    ui.invoke_search_edited("".into());
    ui.invoke_suggestion_chosen(suggestion(&ui, "system:similar files"));
    let window = editor();
    window.invoke_pressed(0, 2);
    assert_eq!(
        window.get_error(),
        "Did not see an image bitmap or a file path in the clipboard!"
    );
    assert_eq!(text(&window, 4), "");

    // a file path: its pixel and perceptual hashes
    let shown = path.to_string_lossy().into_owned();
    hydrus_gui::set_clipboard_reader(move || Ok(Some(shown.clone())));
    window.invoke_pressed(0, 2);
    let (pixel, perceptual) = (text(&window, 4), text(&window, 5));
    assert_eq!(pixel.len(), 64, "{pixel}");
    assert_eq!(perceptual.len(), 16, "{perceptual}");

    // clear, then the bitmap (which is preferred to text, as the reference
    // prefers it): the same hashes as the file of those pixels
    window.invoke_pressed(0, 1);
    assert_eq!(text(&window, 4), "");
    let pixels = rgba.clone();
    hydrus_gui::set_clipboard_image_reader(move || {
        Some(hydrus_gui::ClipboardImage {
            width,
            height,
            rgba: pixels.clone(),
        })
    });
    hydrus_gui::set_clipboard_reader(|| Ok(Some("/no/such/file.png".to_owned())));
    window.invoke_pressed(0, 2);
    assert_eq!(text(&window, 4), pixel);
    assert_eq!(text(&window, 5), perceptual);
    // pasting again keeps each hash once
    window.invoke_pressed(0, 2);
    assert_eq!(text(&window, 4), pixel);

    // text that is no file says so
    hydrus_gui::set_clipboard_image_reader(|| None);
    window.invoke_pressed(0, 1);
    window.invoke_pressed(0, 2);
    assert_eq!(
        window.get_error(),
        "Sorry, that clipboard text did not look like a valid file path!"
    );
    hydrus_gui::clear_clipboard_reader();
    hydrus_gui::set_clipboard_image_reader(|| None);
}

// leaf: audit-options-predicate-similar-files-data-similartodata-paste
#[test]
fn paste_image_makes_the_hashes_the_reference_recorded_and_warns_as_it_did() {
    let recorded = hydrus_testkit::fixture_json("similar_data_paste.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let text = |window: &hydrus_gui::PredicateEditorWindow, f: usize| {
        window
            .get_panels()
            .row_data(0)
            .unwrap()
            .fields
            .row_data(f)
            .unwrap()
            .text
            .to_string()
    };
    let bitmap = |name: &str| {
        let hex = recorded["bitmaps"][name].as_str().unwrap();
        let rgba: Vec<u8> = (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
            .collect();
        hydrus_gui::ClipboardImage {
            width: 16,
            height: 16,
            rgba,
        }
    };
    let file = |name: &str| {
        let folder = if name.starts_with("pixels") || name.starts_with("not an") {
            "similar_data_paste"
        } else {
            "auto_resolution"
        };
        hydrus_testkit::fixture_path(format!("{folder}/{name}"))
            .to_string_lossy()
            .into_owned()
    };
    // (the recorder's cases, by what the clipboard held)
    let mut replayed = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        hydrus_gui::set_clipboard_image_reader(|| None);
        hydrus_gui::set_clipboard_reader(|| Ok(None));
        let presses = match name {
            "empty clipboard" => 1,
            "text that is no file" => {
                hydrus_gui::set_clipboard_reader(|| Ok(Some("/no/such/file.png".to_owned())));
                1
            }
            "text file path" => {
                let shown = file("not an image.txt");
                hydrus_gui::set_clipboard_reader(move || Ok(Some(shown.clone())));
                1
            }
            "png path as text" | "larger png path" | "jpeg path" | "gif path" | "bmp path" => {
                let shown = file(match name {
                    "png path as text" => "pixels.png",
                    "larger png path" => "p00_a.png",
                    "jpeg path" => "p00_f_exif.jpg",
                    "gif path" => "p00_h.gif",
                    _ => "p00_c.bmp",
                });
                hydrus_gui::set_clipboard_reader(move || Ok(Some(shown.clone())));
                1
            }
            "bitmap" | "pasted twice" => {
                let image = bitmap("pixels");
                hydrus_gui::set_clipboard_image_reader(move || Some(image.clone()));
                if name == "pasted twice" { 2 } else { 1 }
            }
            "bitmap preferred to a path" => {
                let image = bitmap("pixels");
                hydrus_gui::set_clipboard_image_reader(move || Some(image.clone()));
                let shown = file("p00_a.png");
                hydrus_gui::set_clipboard_reader(move || Ok(Some(shown.clone())));
                1
            }
            "translucent bitmap" => {
                let image = bitmap("translucent");
                hydrus_gui::set_clipboard_image_reader(move || Some(image.clone()));
                1
            }
            // (a local path on the clipboard is read from the system's)
            "png path as local path" => continue,
            other => panic!("a case the recorder made: {other}"),
        };
        ui.invoke_search_edited("".into());
        ui.invoke_suggestion_chosen(suggestion(&ui, "system:similar files"));
        let window = bound
            .predicate_editor
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("an editor");
        for _ in 0..presses {
            window.invoke_pressed(0, 2);
        }
        let warnings = case["warnings"].as_array().unwrap();
        assert_eq!(
            window.get_error(),
            warnings.first().and_then(Json::as_str).unwrap_or(""),
            "{name}: warning"
        );
        assert_eq!(
            text(&window, 4),
            case["fields"]["pixel"].as_str().unwrap(),
            "{name}: pixel hashes"
        );
        assert_eq!(
            text(&window, 5),
            case["fields"]["perceptual"].as_str().unwrap(),
            "{name}: perceptual hashes"
        );
        // clear empties both
        window.invoke_pressed(0, 1);
        assert_eq!(text(&window, 4), case["cleared"]["pixel"].as_str().unwrap());
        assert_eq!(
            text(&window, 5),
            case["cleared"]["perceptual"].as_str().unwrap()
        );
        replayed += 1;
    }
    assert_eq!(replayed, recorded["cases"].as_array().unwrap().len() - 1);
    hydrus_gui::clear_clipboard_reader();
    hydrus_gui::set_clipboard_image_reader(|| None);
}
