//! The search page's active predicate list: its whole right-click menu, the
//! entries it inherits from tag lists included (siblings and parents, select,
//! favourites, maintenance), opened on the real main window and compared with
//! the reference's (`oracle/record_active_predicate_menu.py`), then its
//! inherited entries chosen as a user would.

use std::{cell::RefCell, rc::Rc};

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext, Predicate};
use hydrus_store::settings;

/// The open tag menu as the recorder writes it: labels, "---" for a
/// separator, a submenu as {title: entries}, a greyed-out line as
/// ["label", text]; each submenu opened by hovering it.
fn tree(ui: &MainWindow, pane: usize) -> Json {
    let lines = ui.get_tag_menu_panes().row_data(pane).unwrap().lines;
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        out.push(match line.kind {
            2 => json!("---"),
            3 => {
                ui.invoke_tag_menu_hovered(
                    i32::try_from(pane).unwrap(),
                    i32::try_from(i).unwrap(),
                    300.0,
                    20.0,
                    10.0,
                );
                json!({ line.label.to_string(): tree(ui, pane + 1) })
            }
            _ if !line.usable => json!(["label", line.label.to_string()]),
            _ => json!(line.label.to_string()),
        });
    }
    json!(out)
}

/// Chooses the entry at `path` in the open tag menu.
fn choose(ui: &MainWindow, path: &[Json]) {
    for (pane, label) in path.iter().enumerate() {
        let lines = ui.get_tag_menu_panes().row_data(pane).unwrap().lines;
        let line = lines
            .iter()
            .position(|row| json!(row.label.to_string()) == *label)
            .unwrap_or_else(|| panic!("no {label} in {path:?}"));
        ui.invoke_tag_menu_clicked(
            i32::try_from(pane).unwrap(),
            i32::try_from(line).unwrap(),
            300.0,
            20.0,
            10.0,
        );
    }
}

// leaf: audit-options-search-active-edit
#[test]
fn the_active_lists_whole_menu_and_its_inherited_entries_act_as_the_references_do() {
    let fixture = hydrus_testkit::fixture_json("active_predicate_menu.json");
    let seed = json!({
        "corpus": fixture["corpus"],
        "siblings": fixture["siblings"],
        "parents": fixture["parents"],
        "events": [{ "favourites": fixture["favourites"] }],
    });
    let (_dirs, store, my_tags) = super::read_autocomplete::seeded(&seed);
    let most_used: Vec<String> = serde_json::from_value(fixture["most_used"].clone()).unwrap();
    store
        .write({
            let key = my_tags.to_hex();
            move |ctx| {
                let mut tabs: settings::TagAutocompleteTabs = settings::get(ctx.conn())?;
                tabs.most_used.insert(key, most_used);
                settings::set(ctx.conn(), &tabs)
            }
        })
        .unwrap();
    let _windows = headless::init();
    let copies = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copies = copies.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copies.borrow_mut().push(text.clone());
            }
        }
    });
    // the recorder's search: three tags, inbox, an OR of two and a namespace
    let parsed = |text: &str| hydrus_search::parse_api_search(&json!([text])).unwrap();
    let mut predicates: Vec<Predicate> = [
        "series:active alpha",
        "series:active beta",
        "series:active root",
        "system:inbox",
        "series:*",
    ]
    .iter()
    .flat_map(|t| parsed(t))
    .collect();
    predicates.push(Predicate::Or(
        ["series:active alpha", "series:active beta"]
            .iter()
            .flat_map(|t| parsed(t))
            .collect(),
    ));
    let context = FileSearchContext {
        location: LocationContext::single(hydrus_core::ServiceKey::new(b"local files".to_vec())),
        predicates,
        ..Default::default()
    };
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::restored(
            store.clone(),
            context,
            false,
            None,
            Vec::new(),
        )),
    );
    let rows = || -> Vec<String> {
        ui.get_predicates()
            .iter()
            .map(|r| r.text.to_string())
            .collect()
    };
    let cases: std::collections::BTreeMap<String, Vec<String>> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            (
                case["name"].as_str().unwrap().to_owned(),
                serde_json::from_value(case["selected"].clone()).unwrap(),
            )
        })
        .collect();
    // selects the case's rows with clicks (Ctrl after the first), then
    // right-clicks the first
    let open = |name: &str| {
        let shown = rows();
        let selected = &cases[name];
        let at = |text: &String| {
            i32::try_from(
                shown
                    .iter()
                    .position(|row| row == text)
                    .unwrap_or_else(|| panic!("no row {text} in {shown:?}")),
            )
            .unwrap()
        };
        for (i, text) in selected.iter().enumerate() {
            ui.invoke_active_predicate_clicked(at(text), i > 0, false);
        }
        ui.invoke_active_predicate_menu_opened(at(&selected[0]));
        ui.invoke_active_predicate_menu_requested(at(&selected[0]), 40.0, 200.0);
        assert!(ui.get_tag_menu_panes().row_count() > 0, "{name}: no menu");
    };
    let close = || ui.invoke_tag_menu_dismissed();
    let mut differences = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        open(name);
        let ours = tree(&ui, 0);
        close();
        if ours != case["menu"] {
            differences.push(format!("{name}: menu\n {ours}\nnot\n {}", case["menu"]));
        }
    }
    let favourites = || -> Json {
        json!(
            store
                .read(settings::get::<settings::FavouriteTags>)
                .unwrap()
                .0
        )
    };
    let most_used = || -> Json {
        let tabs: settings::TagAutocompleteTabs = store.read(settings::get).unwrap();
        json!(
            tabs.most_used
                .get(&my_tags.to_hex())
                .cloned()
                .unwrap_or_default()
        )
    };
    for trigger in fixture["triggers"].as_array().unwrap() {
        let name = trigger["case"].as_str().unwrap();
        let path = trigger["path"].as_array().unwrap();
        copies.borrow_mut().clear();
        open(name);
        choose(&ui, path);
        let what = format!("{name} {path:?}");
        let mut differ = |key: &str, ours: Json| {
            if ours != trigger[key] {
                differences.push(format!("{what}: {key} {ours} not {}", trigger[key]));
            }
        };
        // a question, answered yes
        let questions = if ui.get_tag_menu_question().is_empty() {
            json!([])
        } else {
            let asked = json!([{
                "message": ui.get_tag_menu_question().to_string(),
                "title": if ui.get_tag_menu_question_title().is_empty() {
                    "Are you sure?".to_owned()
                } else {
                    ui.get_tag_menu_question_title().to_string()
                },
                "yes": ui.get_tag_menu_yes_label().to_string(),
                "no": ui.get_tag_menu_no_label().to_string(),
            }]);
            ui.invoke_tag_menu_answered(true);
            asked
        };
        differ("questions", questions);
        let dialog = bound
            .tag_menu_relationships
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        let dialogs = match &dialog {
            Some(child) => {
                let title = child.get_composed_title();
                let title = title
                    .strip_suffix(child.get_title_suffix().as_str())
                    .unwrap_or(&title)
                    .to_owned();
                json!([{
                    "title": title,
                    "panel": if child.get_siblings() { "ManageTagSiblings" } else { "ManageTagParents" },
                }])
            }
            None => json!([]),
        };
        differ("dialogs", dialogs);
        if let Some(child) = dialog {
            child.invoke_cancel();
        }
        differ(
            "clipboard",
            json!(
                copies
                    .borrow()
                    .iter()
                    .map(|text| json!({ "type": "text", "text": text }))
                    .collect::<Vec<_>>()
            ),
        );
        differ("favourite_tags", favourites());
        differ("most_used", most_used());
        // (the rows, unchanged by these entries; the recorder's search was set
        // with `SetFileSearchContext`, which sorts it, where this page is
        // restored as given)
        let mut shown = rows();
        shown.sort();
        differ("shown", json!(shown));
        assert!(ui.get_error().is_empty(), "{what}: {}", ui.get_error());
        open(name);
        differ("menu_after", tree(&ui, 0));
        close();
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
