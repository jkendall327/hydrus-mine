//! The simple "Delete files?" dialog (`use_advanced_file_deletion_dialog`
//! off), opened by the delete action on thumbnails, as
//! `oracle/fixtures/simple_delete_dialog.json` (`oracle/record_simple_delete_dialog.py`)
//! recorded it: its title, whether the question is already resolved, the one
//! action's label or the list of actions, the action it starts on, the reason
//! panel it hides, and what accepting each action does (the files it takes
//! from which domains, the reason it records, the options it remembers).

use std::collections::BTreeSet;
use std::sync::Arc;

use slint::ComponentHandle as _;

use hydrus_core::HashId;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::content::DomainRoles;
use hydrus_store::import::import_legacy;
use hydrus_store::settings::{DeletionAction, DeletionPreferences};

const REMEMBERED: &str = "remembered reason";

fn opened() -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let all_local = hydrus_core::ServiceKey::new(
        hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
    );
    store
        .write_and_refresh(move |ctx| {
            let mut defaults: hydrus_store::settings::SearchDefaults =
                hydrus_store::settings::get(ctx.conn())?;
            defaults.local_location = hydrus_search::LocationContext::single(all_local);
            hydrus_store::settings::set(ctx.conn(), &defaults)
        })
        .unwrap();
    (native, store)
}

/// The local file domains (by name) `file` is in, and its deletion reason.
fn local(store: &Store, file: HashId) -> (BTreeSet<String>, Option<String>) {
    let snapshot = store.snapshot();
    let roles = DomainRoles::new(&snapshot.services).unwrap();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
        .unwrap();
    let m = &batch.results[0];
    (
        m.current
            .iter()
            .filter(|c| roles.local.contains(&c.service))
            .map(|c| snapshot.services.get(c.service).unwrap().name.clone())
            .collect(),
        m.deletion_reason.clone(),
    )
}

fn in_storage(store: &Store, file: HashId) -> bool {
    let snapshot = store.snapshot();
    let roles = DomainRoles::new(&snapshot.services).unwrap();
    let current = store
        .read(|c| hydrus_store::media::current_domains(c, &[file]))
        .unwrap();
    current
        .get(&file)
        .is_some_and(|domains| domains.contains(&roles.local_file_storage))
}

/// The files of a case: the first in one local domain, the first in two (as
/// the recorder's `single` and `double`), the trashed form of the first.
fn case_files(store: &Store, case: &str, files: &[HashId]) -> Vec<HashId> {
    let count = |n: usize| {
        *files
            .iter()
            .find(|&&f| local(store, f).0.len() == n)
            .unwrap()
    };
    let (single, double) = (count(1), count(2));
    match case {
        "one domain" => vec![single],
        "two domains" => vec![double],
        "both files" => vec![single, double],
        "trashed" => {
            for name in local(store, single).0 {
                let domain = store.snapshot().services.by_name(&name).unwrap().id;
                hydrus_gui::media_actions::delete(
                    store,
                    &[single],
                    &hydrus_gui::media_actions::Deletion::FromDomain { domain, name },
                )
                .unwrap();
            }
            vec![single]
        }
        other => panic!("unknown case {other}"),
    }
}

fn set_preferences(store: &Store, confirm_trash: bool) {
    let preferences = DeletionPreferences {
        advanced: false,
        confirm_trash,
        remember_action: true,
        remember_reason: true,
        last_reason: Some(REMEMBERED.into()),
        ..DeletionPreferences::default()
    };
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &preferences))
        .unwrap();
}

/// A window on all local files, `files` selected, the delete action invoked.
struct Opened {
    _dir: tempfile::TempDir,
    store: Arc<Store>,
    _ui: MainWindow,
    bound: hydrus_gui::Bound,
    files: Vec<HashId>,
}

fn open(case: &str, confirm_trash: bool) -> Opened {
    let (dir, store) = opened();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    ui.show().unwrap();
    ui.invoke_search_accepted();
    let all = bound.current.borrow().borrow().results().to_vec();
    let files = case_files(&store, case, &all);
    set_preferences(&store, confirm_trash);
    ui.invoke_select_none();
    bound.current.borrow().borrow_mut().select_files(&files);
    ui.invoke_delete_selected();
    Opened {
        _dir: dir,
        store,
        _ui: ui,
        bound,
        files,
    }
}

#[test]
fn simple_deletion_opens_the_delete_files_dialog_as_recorded() {
    let _windows = headless::init();
    let recording = hydrus_testkit::fixture_json("simple_delete_dialog.json");
    for case in recording["cases"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        if name.contains("suggested") || name == "existing reason" || name == "last action" {
            // (a suggested domain comes from the menu entry, a file's earlier
            // reason from the reference's own database)
            continue;
        }
        let what = format!("{name}, confirm {}", case["confirm_trash"]);
        let panel = &case["panel"];
        let confirm = case["confirm_trash"].as_bool().unwrap();
        let opening = open(name, confirm);
        let dialog = super::common::delete_dialog(&opening.bound);
        if panel["resolved"].as_bool().unwrap() {
            assert!(dialog.is_none(), "{what}: no dialog");
            for &file in &opening.files {
                assert!(local(&opening.store, file).0.is_empty(), "{what}: deleted");
            }
            continue;
        }
        let dialog = dialog.unwrap_or_else(|| panic!("{what}: the dialog"));
        assert!(
            dialog
                .get_composed_title()
                .starts_with(panel["title"].as_str().unwrap())
        );
        assert!(dialog.get_simple(), "{what}: no reason panel");
        let actions: Vec<String> = slint::Model::iter(&dialog.get_actions())
            .map(|a| a.to_string())
            .collect();
        match (panel["description"].as_str(), panel["actions"].as_array()) {
            (Some(description), None) => assert_eq!(actions, [description], "{what}"),
            (None, Some(recorded)) => {
                let recorded: Vec<&str> = recorded.iter().map(|a| a.as_str().unwrap()).collect();
                assert_eq!(actions, recorded, "{what}");
            }
            other => panic!("{what}: {other:?}"),
        }
        assert_eq!(
            usize::try_from(dialog.get_selected_action()).unwrap(),
            panel["selected"].as_u64().unwrap() as usize,
            "{what}"
        );
        drop(dialog);
        super::common::answer_delete(&opening.bound, false);
        drop(opening);
        if !confirm {
            continue;
        }
        // accepting each action
        for (index, value) in case["values"].as_array().unwrap().iter().enumerate() {
            let what = format!("{name}, action {index}");
            let before_run = open(name, true);
            let before: Vec<_> = before_run
                .files
                .iter()
                .map(|&f| local(&before_run.store, f).0)
                .collect();
            super::common::delete_dialog(&before_run.bound)
                .unwrap()
                .invoke_action_selected(i32::try_from(index).unwrap());
            super::common::answer_delete(&before_run.bound, true);
            let store = &before_run.store;
            let updates = value["updates"].as_array().unwrap();
            assert_eq!(updates.len(), 1, "{what}");
            let service = updates[0]["service"].as_str().unwrap();
            let physical = value["physically_deleted"].as_u64().unwrap() > 0;
            let touched = usize::try_from(updates[0]["files"].as_u64().unwrap()).unwrap();
            let mut changed = 0;
            for (&file, was) in before_run.files.iter().zip(&before) {
                let (now, reason) = local(store, file);
                let expected: BTreeSet<String> = match service {
                    "all local file domains" => BTreeSet::new(),
                    "local file storage" => was.clone(),
                    domain => was
                        .iter()
                        .filter(|d| d.as_str() != domain)
                        .cloned()
                        .collect(),
                };
                if now != *was || (physical && !in_storage(store, file)) {
                    changed += 1;
                    assert_eq!(now, expected, "{what}: the domains left");
                    if !physical {
                        assert_eq!(
                            reason.as_deref(),
                            updates[0]["reason"].as_str(),
                            "{what}: the reason"
                        );
                    }
                }
            }
            assert_eq!(changed, touched, "{what}: files taken");
            let saved: DeletionPreferences = store.read(hydrus_store::settings::get).unwrap();
            assert_eq!(saved.last_reason.as_deref(), Some(REMEMBERED), "{what}");
            let recorded = value["saved_action"].as_str();
            match (&saved.last_action, recorded) {
                (None, None) => {}
                (Some(DeletionAction::Domain(key)), Some(_)) => {
                    // (the recording names builtin services by their keys' hex)
                    let remembered = store.snapshot().services.by_key(key).unwrap().name.clone();
                    let service = updates[0]["service"].as_str().unwrap();
                    assert!(
                        remembered == service || service == "all local file domains",
                        "{what}: remembered {remembered}, recorded {service}"
                    );
                }
                (saved, recorded) => panic!("{what}: remembered {saved:?}, recorded {recorded:?}"),
            }
        }
    }
}
