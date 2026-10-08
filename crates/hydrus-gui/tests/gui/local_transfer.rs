//! Rendered transfer questions and actual thumbnail/Options callback consumers.
use hydrus_core::{HashId, Sha256};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless, local_transfer_window};
use hydrus_gui_model::local_transfer::Transfer;
use hydrus_store::{
    Store,
    content::TransferKind,
    settings::{self, LocalTransferPreferences},
};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
const LABELS: [&str; 2] = [
    "Confirm when copying files across local file domains: ",
    "Confirm when moving files across local file domains: ",
];
fn store() -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}
fn files(store: &Store, case: &Value) -> Vec<HashId> {
    case["before"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let hash: Sha256 = f["hash"].as_str().unwrap().parse().unwrap();
            store
                .read(|c| hydrus_store::master::hash_id(c, &hash))
                .unwrap()
                .unwrap()
        })
        .collect()
}
fn seed(store: &Store, case: &Value) -> Vec<HashId> {
    let ids = files(store, case);
    let rows = case["before"].as_array().unwrap().clone();
    let captured = ids.clone();
    store
        .write_content(move |writer| {
            let domains = writer.roles().local.clone();
            for domain in &domains {
                writer.delete_files(*domain, &captured, None)?;
            }
            let source = writer.snapshot().services.by_name("art").unwrap().id;
            // First restore every file to a local domain, permitting deletion-record clear.
            writer.add_files(
                source,
                &captured
                    .iter()
                    .map(|&f| (f, Some(1_234_567_890_000)))
                    .collect::<Vec<_>>(),
            )?;
            writer.clear_local_delete_records(Some(&captured))?;
            for (&id, row) in captured.iter().zip(&rows) {
                for (name, time) in row["imports"].as_object().unwrap() {
                    if let Some(time) = time.as_i64() {
                        let domain = writer.snapshot().services.by_name(name).unwrap().id;
                        writer.add_files(domain, &[(id, Some(time))])?;
                    }
                }
                if row["domains"].as_array().unwrap().is_empty() {
                    writer.delete_files(source, &[id], None)?;
                }
                if row["inbox"].as_bool().unwrap() {
                    writer.inbox(&[id])?;
                } else {
                    writer.archive(&[id])?;
                }
            }
            Ok(())
        })
        .unwrap();
    ids
}
fn state(store: &Store, ids: &[HashId]) -> Value {
    let snap = store.snapshot();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snap.services, None, ids))
        .unwrap();
    let rows = batch.results.iter().map(|media| {
        let mut domains = media.current.iter().filter_map(|location| snap.services.get(location.service).ok().filter(|service| service.service_type() == hydrus_core::ServiceType::LocalFileDomain).map(|service| service.name.clone())).collect::<Vec<_>>();
        domains.sort();
        let imports = ["art", "my files"].into_iter().map(|name| {
            let id = snap.services.by_name(name).unwrap().id;
            (name, media.current.iter().find(|location| location.service == id).and_then(|location| location.added).map(|time| time.0))
        }).collect::<std::collections::BTreeMap<_,_>>();
        json!({"hash":media.hash.to_string(),"domains":domains,"imports":imports,"inbox":media.inbox})
    }).collect::<Vec<_>>();
    json!(rows)
}

fn kind(case: &Value) -> TransferKind {
    match case["kind"].as_str().unwrap() {
        "copy" => TransferKind::Copy,
        "move" => TransferKind::Move,
        _ => TransferKind::Merge,
    }
}
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines.iter().position(|r| r.label == "options…").unwrap();
    ui.invoke_menu_line_clicked(0, index as i32, 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let index = window
        .get_pages()
        .iter()
        .position(|p| p.text == "files and trash")
        .unwrap();
    window.invoke_page_chosen(index as i32);
    window
}
fn edit(window: &OptionsWindow, values: &Value) {
    for (label, value) in LABELS.iter().zip(values.as_array().unwrap()) {
        let index = window
            .get_rows()
            .iter()
            .position(|r| r.label == *label)
            .unwrap();
        window.invoke_check_toggled(index as i32, value.as_bool().unwrap());
    }
}
fn preferences(store: &Store) -> Value {
    let p: LocalTransferPreferences = store.read(settings::get).unwrap();
    json!([p.copy, p.move_files])
}
#[test]
fn staged_options_apply_cancel_reopen_and_retired_successor_callbacks() {
    let fixture = hydrus_testkit::fixture_json("local_transfer_confirmations.json");
    let (dir, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    headless::render(&windows.get(0).unwrap(), 1100, 750);
    for event in fixture["options"].as_array().unwrap() {
        let cancelled = options(&ui, &bound);
        edit(&cancelled, &event["input"]);
        assert_eq!(preferences(&store), event["staged"]);
        cancelled.invoke_cancel();
        let accepted = options(&ui, &bound);
        edit(&cancelled, &json!([false, false]));
        cancelled.invoke_apply();
        assert_eq!(preferences(&store), event["before"]);
        edit(&accepted, &event["input"]);
        headless::render(&windows.get(windows.count() - 1).unwrap(), 1050, 720);
        accepted.invoke_apply();
        assert_eq!(preferences(&store), event["saved"]);
        accepted.invoke_cancel();
        let reopened = options(&ui, &bound);
        assert_eq!(
            preferences(&Store::open(dir.path()).unwrap()),
            event["reopened"]
        );
        reopened.invoke_cancel();
    }
    // Saved preferences reach the real transfer model, independently by operation.
    let case = &fixture["transfers"][0];
    let ids = seed(&store, case);
    let snap = store.snapshot();
    let destination = snap.services.by_name("my files").unwrap().id;
    let source = snap.services.by_name("art").unwrap().id;
    let draft = options(&ui, &bound);
    edit(&draft, &json!([false, true]));
    assert!(
        Transfer::load(&store, TransferKind::Copy, destination, None, &ids)
            .unwrap()
            .unwrap()
            .confirm
    );
    draft.invoke_apply();
    draft.invoke_cancel();
    assert!(
        !Transfer::load(&store, TransferKind::Copy, destination, None, &ids)
            .unwrap()
            .unwrap()
            .confirm
    );
    assert!(
        Transfer::load(&store, TransferKind::Move, destination, Some(source), &ids)
            .unwrap()
            .unwrap()
            .confirm
    );
}
// leaf: audit-media-context-missing-move
#[test]
fn native_transfer_yes_no_cancel_and_stale_window_cannot_write() {
    let fixture = hydrus_testkit::fixture_json("local_transfer_confirmations.json");
    let (_dir, store) = store();
    let windows = headless::init();
    let slot = Rc::default();
    for case in fixture["transfers"].as_array().unwrap() {
        let ids = seed(&store, case);
        let confirm = case["confirm"].as_bool().unwrap();
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &LocalTransferPreferences {
                        copy: confirm,
                        move_files: confirm,
                    },
                )
            })
            .unwrap();
        let snap = store.snapshot();
        let destination = snap.services.by_name("my files").unwrap().id;
        let source = snap.services.by_name("art").unwrap().id;
        let kind = kind(case);
        let transfer = Transfer::load(
            &store,
            kind,
            destination,
            (kind != TransferKind::Copy).then_some(source),
            &ids,
        )
        .unwrap()
        .unwrap();
        let started = hydrus_core::TimestampMs::now().0;
        let child =
            local_transfer_window::open(&slot, &store, transfer, Rc::new(|| true), Rc::new(|| {}))
                .unwrap();
        if let Some(child) = child {
            assert_eq!(child.get_question(), case["questions"][0].as_str().unwrap());
            headless::render(&windows.get(windows.count() - 1).unwrap(), 620, 200);
            child.invoke_answer(case["accepted"].as_bool().unwrap());
            child.invoke_answer(true);
        }
        let ended = hydrus_core::TimestampMs::now().0;
        let mut actual = state(&store, &ids);
        let mut expected = case["after"].clone();
        for (actual, expected) in actual
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(expected.as_array_mut().unwrap())
        {
            if expected["imports"]["my files"] == json!(1_700_000_000_000_i64) {
                assert!(
                    (started..=ended).contains(&actual["imports"]["my files"].as_i64().unwrap())
                );
                actual["imports"]["my files"] = json!("new");
                expected["imports"]["my files"] = json!("new");
            }
        }
        assert_eq!(actual, expected);
    }
    store
        .write(|ctx| settings::set(ctx.conn(), &LocalTransferPreferences::default()))
        .unwrap();
    let ids = seed(&store, &fixture["transfers"][0]);
    let snap = store.snapshot();
    let dest = snap.services.by_name("my files").unwrap().id;
    let plan = || {
        Transfer::load(&store, TransferKind::Copy, dest, None, &ids)
            .unwrap()
            .unwrap()
    };
    let retired =
        local_transfer_window::open(&slot, &store, plan(), Rc::new(|| true), Rc::new(|| {}))
            .unwrap()
            .unwrap();
    retired.invoke_cancel();
    let alive = Rc::new(Cell::new(true));
    let successor = local_transfer_window::open(
        &slot,
        &store,
        plan(),
        Rc::new({
            let alive = alive.clone();
            move || alive.get()
        }),
        Rc::new(|| {}),
    )
    .unwrap()
    .unwrap();
    retired.invoke_answer(true);
    retired.invoke_cancel();
    assert!(slot.borrow().is_some());
    assert_eq!(state(&store, &ids), fixture["transfers"][0]["before"]);
    alive.set(false);
    successor.invoke_answer(true);
    assert!(slot.borrow().is_none());
    assert_eq!(state(&store, &ids), fixture["transfers"][0]["before"]);
    let closed =
        local_transfer_window::open(&slot, &store, plan(), Rc::new(|| true), Rc::new(|| {}))
            .unwrap()
            .unwrap();
    closed.window().hide().unwrap();
    closed.invoke_answer(true);
    closed.invoke_cancel();
    assert_eq!(state(&store, &ids), fixture["transfers"][0]["before"]);
}
fn menu_snapshot(menu: &hydrus_gui::ThumbnailMenu) -> Value {
    let mut groups = Vec::new();
    for (title, rows) in [
        ("currently in", menu.locations_current.clone()),
        ("add to", menu.locations_copy.clone()),
        ("move (merge)", menu.locations_merge.clone()),
        ("move (strict)", menu.locations_move.clone()),
    ] {
        let rows = rows
            .iter()
            .map(|r| json!({"label":r.label.to_string(),"children":[]}))
            .collect::<Vec<_>>();
        if rows.len() == 1 {
            groups.push(json!({"label":format!("{title} {}",rows[0]["label"].as_str().unwrap()),"children":[]}));
        } else if !rows.is_empty() {
            groups.push(json!({"label":title,"children":rows}));
        }
    }
    json!(groups)
}
fn find(rows: &slint::ModelRc<hydrus_gui::MenuRow>, prefix: &str) -> i32 {
    rows.iter()
        .find(|r| r.label.starts_with(prefix))
        .unwrap()
        .id
}
// leaf: audit-media-context-missing-move
#[test]
fn actual_thumbnail_transfer_menu_captures_selection_and_checks_parent_identity() {
    let fixture = hydrus_testkit::fixture_json("local_transfer_confirmations.json");
    let (_dir, store) = store();
    let ids = seed(&store, &fixture["transfers"][0]);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(super::common::all_local_page(store.clone())),
    );
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    headless::render(&windows.get(0).unwrap(), 1100, 750);
    let owner = bound.current.borrow().clone();
    owner.borrow_mut().select_files(&ids[..1]);
    ui.invoke_thumbnail_menu_requested(-1);
    let menu = ui.get_thumbnail_menu();
    ui.invoke_menu_chosen(find(&menu.locations_copy, "my files"));
    let child = bound
        .local_transfer
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(child.get_question(), "Add 1 files to my files?");
    owner.borrow_mut().select_files(&ids[1..2]);
    child.invoke_answer(true);
    let actual = state(&store, &ids);
    assert_eq!(actual[0]["domains"], json!(["art", "my files"]));
    assert_eq!(actual[1], fixture["transfers"][0]["before"][1]);
    // Strict skips destination-current file; merge captures it and asks using move gate.
    let ids = seed(&store, &fixture["transfers"][0]);
    owner.borrow_mut().select_files(&ids[..2]);
    ui.invoke_thumbnail_menu_requested(-1);
    let menu = ui.get_thumbnail_menu();
    assert_eq!(menu_snapshot(&menu), fixture["menu"]);
    ui.invoke_menu_chosen(find(&menu.locations_move, "from art to my files"));
    let strict = bound
        .local_transfer
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(strict.get_question(), "Move 1 files from art to my files?");
    strict.invoke_answer(false);
    ui.invoke_thumbnail_menu_requested(-1);
    let menu = ui.get_thumbnail_menu();
    ui.invoke_menu_chosen(find(&menu.locations_merge, "from art to my files"));
    let merge = bound
        .local_transfer
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        merge.get_question(),
        "Move-merge 2 files from art to my files?"
    );
    // A page successor cannot inherit the old question or the old menu target.
    ui.invoke_menu_chosen(find(&menu.open_a, "in a new page"));
    assert!(!Rc::ptr_eq(&owner, &bound.current.borrow()));
    assert!(bound.local_transfer.borrow().is_none());
    ui.invoke_menu_chosen(find(&menu.locations_move, "from art to my files"));
    assert!(bound.local_transfer.borrow().is_none());
    ui.invoke_tab_chosen(0, 0);
    assert!(Rc::ptr_eq(&owner, &bound.current.borrow()));
    merge.invoke_answer(true);
    assert_eq!(state(&store, &ids), fixture["transfers"][0]["before"]);
    ui.invoke_tab_chosen(0, 1);
    // Disabled copy confirms nothing and mutates via the actual menu immediately.
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &LocalTransferPreferences {
                    copy: false,
                    move_files: true,
                },
            )
        })
        .unwrap();
    let successor = bound.current.borrow().clone();
    successor.borrow_mut().select_files(&ids[..1]);
    ui.invoke_thumbnail_menu_requested(-1);
    ui.invoke_menu_chosen(find(&ui.get_thumbnail_menu().locations_copy, "my files"));
    assert!(bound.local_transfer.borrow().is_none());
    assert_eq!(
        state(&store, &ids)[0]["domains"],
        json!(["art", "my files"])
    );
}
