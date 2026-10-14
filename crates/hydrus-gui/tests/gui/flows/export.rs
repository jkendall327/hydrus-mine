//! Flow 4 (`oracle/record_flow_export.py`): a search page's files all
//! selected and exported through share > export files, with a filename
//! pattern and the JSON sidecars template. After each step what the user
//! sees, and what the export wrote, must be what the reference showed.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};

use super::{Known, Normaliser, Site, State, compare, install};

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[];

/// Each file under `dir`: a media file's SHA-256, a sidecar's JSON
/// (`contents` in the recorder).
fn contents(dir: &Path) -> Json {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(here) = stack.pop() {
        for entry in std::fs::read_dir(&here).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let data = std::fs::read(&path).unwrap();
            let relative = path
                .strip_prefix(dir)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let value = if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                serde_json::from_slice(&data).unwrap()
            } else {
                json!(hydrus_media::hash_bytes(&data).sha256.to_hex())
            };
            out.insert(relative, value);
        }
    }
    json!(out)
}

fn rows(window: &hydrus_gui::ExportFilesWindow, n: &Normaliser) -> Json {
    let rows = window.get_rows();
    Json::Array(
        (0..rows.row_count())
            .map(|r| {
                let row = rows.row_data(r).unwrap();
                Json::Array(
                    (0..row.cells.row_count())
                        .map(|c| json!(n.text(&row.cells.row_data(c).unwrap())))
                        .collect(),
                )
            })
            .collect(),
    )
}

#[test]
fn files_are_exported_with_a_pattern_and_sidecars_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_export.json");
    let (_dirs, store) = super::basic_store();
    install(&store, &recording);
    let site = Site::serve("flow_export.json", &recording);
    let work = tempfile::tempdir().unwrap();
    let destination = work.path().join("out");
    std::fs::create_dir_all(&destination).unwrap();
    let n = Normaliser::new(&site).place(&destination, "EXPORT");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let state = State {
        store: &store,
        ui: &ui,
        site: &site,
        n: &n,
    };
    let step = |name: &str, dialog: Json| -> Json {
        json!({
            "step": name,
            "dialog": dialog,
            // (nothing was asked or warned of: checked where it could be)
            "asked": [],
            "exported": contents(&destination),
            "popups": state.popups(),
            "hits": site.take_hits(),
        })
    };
    let mut steps = Vec::new();

    // 1. open: a search, everything selected, share > export files
    ui.invoke_search_edited("safe".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_clicked(0, false, false);
    ui.invoke_select_all();
    ui.invoke_thumbnail_menu_requested(0);
    let share = ui.get_thumbnail_menu().share_export;
    let export = (0..share.row_count())
        .map(|i| share.row_data(i).unwrap())
        .find(|r| r.label == "export files")
        .expect("share > export files");
    ui.invoke_menu_chosen(export.id);
    let window = bound
        .export_files
        .window
        .borrow()
        .as_ref()
        .expect("the export window")
        .clone_strong();
    let numbers: Vec<i64> = (1..=i64::try_from(window.get_rows().row_count()).unwrap()).collect();
    let seen = json!({
        // (without the application's name, which is added to the window)
        "title": window
            .get_composed_title()
            .strip_suffix(window.get_title_suffix().as_str())
            .unwrap(),
        "phrase starts as": window.get_phrase().as_str(),
        "sidecars start as": window.get_sidecars().as_str(),
        "trash after export": window.get_trash(),
        "symlinks": window.get_symlinks(),
        "file numbers": numbers,
        "thumbnails": window.get_rows().row_count(),
    });
    steps.push(step("open", seen));

    // 2. pattern: a destination, two patterns, "update" after each
    let mut shown = Vec::new();
    for pattern in ["[creator] [character]", "{#} - [character] - {hash}"] {
        window.set_destination(destination.to_string_lossy().into_owned().into());
        window.set_phrase(pattern.into());
        window.invoke_update();
        shown.push(json!({"pattern": pattern, "rows": rows(&window, &n)}));
    }
    steps.push(step("pattern", json!({"rows": shown})));

    // 3. sidecars: the sidecars button, the JSON template, apply
    window.invoke_edit_sidecars();
    let routers = bound
        .export_files
        .sidecars
        .routers
        .borrow()
        .as_ref()
        .expect("the routers dialog")
        .clone_strong();
    let templates = routers.get_templates();
    let offered: Vec<String> = (0..templates.row_count())
        .map(|i| templates.row_data(i).unwrap().to_string())
        .collect();
    routers.invoke_template(0);
    let router_rows = routers.get_rows();
    let router_rows: Vec<String> = (0..router_rows.row_count())
        .map(|r| {
            router_rows
                .row_data(r)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect();
    routers.invoke_apply();
    let seen = json!({
        "templates": offered,
        "router rows": router_rows,
        "sidecars end as": window.get_sidecars().as_str(),
    });
    steps.push(step("sidecars", seen));

    // 4. export: the button, until it is done
    window.set_phrase("{#} - [character] - {hash}".into());
    window.invoke_export(false);
    let started = std::time::Instant::now();
    while window.get_working() && started.elapsed() < std::time::Duration::from_secs(60) {
        std::thread::sleep(std::time::Duration::from_millis(20));
        slint::platform::update_timers_and_animations();
    }
    assert!(!window.get_working());
    // the main window shows the popup's last words on its next look at the
    // store (its timer), not at once
    let shown = |state: &State<'_>| state.popups()[0]["text_1"] == json!("Done!");
    let started = std::time::Instant::now();
    while !shown(&state) && started.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(20));
        slint::platform::update_timers_and_animations();
    }
    let seen = json!({"rows": rows(&window, &n)});
    steps.push(step("export", seen));

    compare(&steps, &recording, KNOWN);
}
