//! Flow 3 (`oracle/record_flow_import_folder.py`): an import folder with
//! `.txt` sidecars set up through the manage import folders dialogs (three
//! routers: tags, URLs and a note), run by the daemon, reopened, and run
//! again when a file arrives and "check import folder now" is chosen. After
//! each step what the user sees must be what the reference showed.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value as Json, json};
use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Bound, MainWindow, Pages, SidecarNodeWindow, bind, headless};
use hydrus_store::import_folders;

use super::{Daemon, Known, Normaliser, Site, State, compare, install};
use crate::files_io_file_menu::file_menu;
use crate::folders::{import_list, open};

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[];

/// The recording's source folder, copied under `to` (the files for the
/// folder to find, or `later`'s) with settled modified times.
fn place(to: &Path, later: bool) {
    let source = hydrus_testkit::fixture_path("flow_import_folder");
    let mut stack = vec![source.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let relative = path.strip_prefix(&source).unwrap();
            let in_later = relative.starts_with("later");
            if in_later != later {
                continue;
            }
            let target = to.join(if later {
                relative.strip_prefix("later").unwrap()
            } else {
                relative
            });
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(&path, &target).unwrap();
            let settled = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
            std::fs::File::options()
                .write(true)
                .open(&target)
                .unwrap()
                .set_modified(settled)
                .unwrap();
        }
    }
}

/// Every file under `dir`: its path and size (`listing` in the recorder).
fn listing(dir: &Path) -> Json {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir_here) = stack.pop() {
        for entry in std::fs::read_dir(&dir_here).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(meta) = std::fs::metadata(&path) {
                out.insert(
                    path.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    meta.len(),
                );
            }
        }
    }
    json!(out)
}

fn strings(model: &slint::ModelRc<slint::SharedString>) -> Vec<String> {
    (0..model.row_count())
        .map(|i| model.row_data(i).unwrap().to_string())
        .collect()
}

/// A table's first cells.
fn labels(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<String> {
    (0..rows.row_count())
        .map(|r| {
            rows.row_data(r)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect()
}

/// The list's rows, each cell normalised.
fn rows(list: &hydrus_gui::FoldersWindow, n: &Normaliser) -> Json {
    let rows = list.get_rows();
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

/// The choice labelled `label` among the node window's, chosen.
fn choose_in_node(node: &SidecarNodeWindow, label: &str) {
    let offered = strings(&node.get_asking_choices());
    let at = offered
        .iter()
        .position(|c| c == label)
        .unwrap_or_else(|| panic!("{label} in {offered:?}"));
    node.invoke_chosen(i32::try_from(at).unwrap());
}

/// What a dialog asked or warned of when it was applied, if it did (the
/// recorder notes the reference's warnings and questions).
fn note(asked: &mut Vec<Json>, asking: bool, message: &str, n: &Normaliser) {
    if asking {
        asked.push(json!({"kind": "asking", "message": n.text(message)}));
    }
}

/// Add a router through the routers window: its source (a .txt sidecar
/// with `suffix`, split by four pipes when `pipes`), and for a destination
/// other than tags the type to change it to (and a forced note name).
/// Returns what the dialogs showed.
fn add_router(
    bound: &Bound,
    suffix: &str,
    pipes: bool,
    destination: Option<(&str, Option<&str>)>,
    seen: &mut serde_json::Map<String, Json>,
    asked: &mut Vec<Json>,
    n: &Normaliser,
) {
    let slots = &bound.folders.sidecars;
    let routers = slots.routers.borrow().as_ref().unwrap().clone_strong();
    routers.invoke_add();
    let router = slots.router.borrow().as_ref().unwrap().clone_strong();
    router.invoke_add();
    let offered = strings(&router.get_asking_choices());
    assert_eq!(router.get_asking_title(), "Which type?");
    let mut sorted = offered.clone();
    sorted.sort();
    let at = offered.iter().position(|c| c == "a .txt sidecar").unwrap();
    seen.entry("type questions")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap()
        .push(json!({"chosen": "a .txt sidecar", "offered": sorted}));
    router.invoke_chosen(i32::try_from(at).unwrap());
    let node = slots.node.borrow().as_ref().unwrap().clone_strong();
    let mut source = json!({
        "type": node.get_type_label().as_str(),
        "example": node.get_result().as_str(),
    });
    node.set_suffix(suffix.into());
    if pipes {
        let choices = strings(&node.get_separators());
        let at = choices
            .iter()
            .position(|c| c == "four pipes (||||)")
            .unwrap();
        node.set_separator(i32::try_from(at).unwrap());
    }
    node.invoke_changed();
    source["result"] = json!(node.get_result().as_str());
    seen.entry("sources")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap()
        .push(source);
    node.invoke_apply();
    if let Some((label, forced)) = destination {
        router.invoke_edit_destination();
        let node = slots.node.borrow().as_ref().unwrap().clone_strong();
        node.invoke_change_type();
        let mut offered = strings(&node.get_asking_choices());
        offered.sort();
        seen.entry("type questions")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .push(json!({"chosen": label, "offered": offered}));
        choose_in_node(&node, label);
        if let Some(name) = forced {
            node.set_forced_on(true);
            node.set_forced_name(name.into());
        }
        node.invoke_apply();
    }
    seen.entry("routers")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .unwrap()
        .push(json!({
            "sources": labels(&router.get_sources()),
            "processing": router.get_processing().as_str(),
        }));
    router.invoke_apply();
    note(asked, router.get_asking(), &router.get_asking_message(), n);
    assert!(slots.router.borrow().is_none(), "the router dialog closes");
}

#[test]
fn an_import_folder_with_sidecars_is_set_up_run_and_checked_again_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_import_folder.json");
    let (_dirs, store) = super::basic_store();
    install(&store, &recording);
    let site = Site::serve("flow_import_folder.json", &recording);
    let work = tempfile::tempdir().unwrap();
    let watch = work.path().join("watch");
    let moved = work.path().join("moved");
    std::fs::create_dir_all(&watch).unwrap();
    std::fs::create_dir_all(&moved).unwrap();
    let n = Normaliser::new(&site)
        .place(&watch, "WATCH")
        .place(&moved, "MOVED");
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let daemon = Daemon::start(&store);
    let state = State {
        store: &store,
        ui: &ui,
        site: &site,
        n: &n,
    };
    let folder_id = || {
        store
            .read(|c| import_folders::find_import_folder(c, "drop box"))
            .unwrap()
            .map(|f| f.id())
    };
    let sources = recording["sources"].as_object().unwrap();
    // the files the flow brings that the client has
    let files = || -> Json {
        let hashes = sources
            .values()
            .map(|h| h.as_str().unwrap().to_owned())
            .filter(|h| {
                store
                    .read(|conn| {
                        let sha =
                            hydrus_core::Sha256::from_slice(&hex::decode(h).unwrap()).unwrap();
                        hydrus_store::master::hash_id(conn, &sha)
                    })
                    .unwrap()
                    .is_some()
            })
            .collect();
        let mut files = state.files(hashes);
        let map = files.as_object_mut().unwrap();
        map.retain(|_, f| !f["in"].as_array().unwrap().is_empty());
        for (hash, file) in map.iter_mut() {
            let notes = store
                .read(|conn| {
                    let sha = hydrus_core::Sha256::from_slice(&hex::decode(hash).unwrap()).unwrap();
                    let id = hydrus_store::master::hash_id(conn, &sha)?.unwrap();
                    hydrus_store::media::notes(conn, id)
                })
                .unwrap();
            file["notes"] = json!(
                notes
                    .iter()
                    .map(|(name, text)| (name.clone(), n.text(text)))
                    .collect::<BTreeMap<_, _>>()
            );
        }
        files
    };
    let log = || match folder_id() {
        Some(id) => state.file_log(id),
        None => Json::Null,
    };
    let settled = |expected: &dyn Fn() -> bool| {
        let unknown = folder_id().is_some_and(|id| {
            store
                .read(|c| hydrus_store::queues::file_seeds(c, id))
                .unwrap()
                .iter()
                .any(|f| f.status == hydrus_store::queues::SeedStatus::Unknown)
        });
        folder_id().is_some() && !unknown && expected()
    };
    let step = |name: &str, dialog: Json, asked: Vec<Json>| -> Json {
        json!({
            "step": name,
            "dialog": dialog,
            "asked": asked,
            "folder log": log(),
            "watch": listing(&watch),
            "moved": listing(&moved),
            "files": files(),
            "popups": state.popups(),
            "hits": site.take_hits(),
        })
    };
    let mut steps = Vec::new();

    // 1. add: file > manage import folders, "add", the folder's dialog
    place(&watch, false);
    open(&ui, "manage import folders\u{2026}");
    let list = import_list(&bound);
    let mut seen = serde_json::Map::new();
    seen.insert("rows when opened".into(), rows(&list, &n));
    list.invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .expect("the edit import folder dialog")
        .clone_strong();
    seen.insert("name starts as".into(), json!(edit.get_name().as_str()));
    seen.insert("check regularly".into(), json!(edit.get_check_regularly()));
    seen.insert(
        "search subdirectories".into(),
        json!(edit.get_search_subdirectories()),
    );
    seen.insert("check on ok".into(), json!(edit.get_check_now()));
    seen.insert(
        "sidecars start as".into(),
        json!(edit.get_sidecars().as_str()),
    );
    edit.set_name("drop box".into());
    edit.set_path(watch.to_string_lossy().into_owned().into());
    // new files are moved (the choices: delete, leave, move)
    edit.invoke_action_chosen(0, 2);
    edit.invoke_location_edited(0, moved.to_string_lossy().into_owned().into());
    let choices = strings(&edit.get_action_choices());
    let actions = edit.get_actions();
    seen.insert(
        "actions".into(),
        json!(
            (0..actions.row_count())
                .map(
                    |i| choices[usize::try_from(actions.row_data(i).unwrap().action).unwrap()]
                        .clone()
                )
                .collect::<Vec<_>>()
        ),
    );
    edit.invoke_edit_sidecars();
    let mut asked = Vec::new();
    add_router(&bound, "", false, None, &mut seen, &mut asked, &n);
    add_router(
        &bound,
        "urls",
        false,
        Some(("a file's URLs", None)),
        &mut seen,
        &mut asked,
        &n,
    );
    add_router(
        &bound,
        "notes",
        true,
        Some(("a file's notes", Some("comment"))),
        &mut seen,
        &mut asked,
        &n,
    );
    let routers = bound
        .folders
        .sidecars
        .routers
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    seen.insert("router rows".into(), json!(labels(&routers.get_rows())));
    routers.invoke_apply();
    seen.insert(
        "sidecars end as".into(),
        json!(edit.get_sidecars().as_str()),
    );
    edit.invoke_apply();
    note(
        &mut asked,
        edit.get_asking(),
        &edit.get_asking_message(),
        &n,
    );
    seen.insert("rows at apply".into(), rows(&list, &n));
    list.invoke_apply();
    note(
        &mut asked,
        list.get_asking(),
        &list.get_asking_message(),
        &n,
    );
    super::wait_until_idle(&daemon, || {
        settled(&|| listing(&moved).as_object().is_some_and(|m| !m.is_empty()))
    });
    steps.push(step("add", Json::Object(seen), asked));

    // 2. reopen: the list, then cancel
    open(&ui, "manage import folders\u{2026}");
    let list = import_list(&bound);
    let seen = json!({"rows when opened": rows(&list, &n)});
    list.invoke_cancel();
    super::wait_until_idle(&daemon, || settled(&|| true));
    steps.push(step("reopen", seen, Vec::new()));

    // 3. new file: it arrives, file > import/export folders > check import
    // folder now > drop box
    place(&watch, true);
    let before = listing(&watch).as_object().unwrap().len();
    file_menu(
        &ui,
        &[
            "import/export folders",
            "check import folder now",
            "drop box",
        ],
    );
    super::wait_until_idle(&daemon, || {
        settled(&|| listing(&watch).as_object().unwrap().len() < before)
    });
    steps.push(step("new file", json!({}), Vec::new()));

    compare(&steps, &recording, KNOWN);
}
