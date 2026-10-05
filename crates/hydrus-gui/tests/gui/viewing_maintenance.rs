//! Real database menu dispatch and owned clear/cull questions.
use hydrus_core::Sha256;
use hydrus_gui::{MainWindow, Pages, bind, headless, viewing_maintenance_window};
use hydrus_gui_model::viewing_maintenance::Operation;
use hydrus_store::{
    Store,
    settings::{self, FileViewingStatistics},
};
use slint::{ComponentHandle as _, Model as _};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn seed(store: &Store) {
    let fixture = hydrus_testkit::fixture_json("viewing_maintenance.json");
    store.write(move |ctx| {
        for row in fixture["rows"].as_array().unwrap() {
            let index=row[0].as_i64().unwrap();
            let id=hydrus_store::master::intern_hash(ctx.conn(),&Sha256([u8::try_from(index).unwrap();32]))?;
            ctx.conn().execute("INSERT INTO file_viewing_stats(hash_id,canvas_type,last_viewed_ms,views,viewtime_ms) VALUES(?1,?2,?3,?4,?5)",
                rusqlite::params![id,row[1].as_i64().unwrap(),row[2].as_i64().unwrap(),row[3].as_i64().unwrap(),row[4].as_i64().unwrap()])?;
        }
        Ok(())
    }).unwrap();
}
fn rows(store: &Store) -> Vec<[i64; 5]> {
    store.read(|conn| {
        let mut stmt=conn.prepare("SELECT hash_id,canvas_type,last_viewed_ms,views,viewtime_ms FROM file_viewing_stats ORDER BY hash_id,canvas_type")?;
        Ok(stmt.query_map([],|r|Ok([r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?]))?.collect::<rusqlite::Result<Vec<_>>>()?)
    }).unwrap()
}
fn dispatch(ui: &MainWindow, cull: bool) {
    let database = ui
        .get_menu_titles()
        .iter()
        .position(|r| r.label == "database")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 10.0, 22.0);
    let first = ui.get_menu_panes().row_data(0).unwrap();
    let clear = first.lines.iter().position(|r| r.label == "clear").unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(clear).unwrap(), 0.0, 0.0, 0.0);
    let second = ui.get_menu_panes().row_data(1).unwrap();
    let line = second
        .lines
        .iter()
        .position(|r| {
            r.label
                .starts_with(if cull { "cull file" } else { "clear all file" })
        })
        .unwrap();
    assert!(second.lines.row_data(line).unwrap().usable);
    ui.invoke_menu_line_clicked(1, i32::try_from(line).unwrap(), 0.0, 0.0, 0.0);
}

#[test]
fn real_menu_clear_and_cull_read_live_rules_preserve_declines_and_reopen() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    seed(&store);
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before = rows(&store);
    dispatch(&ui, false);
    let declined = bound
        .viewing_maintenance
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(declined.get_message(), Operation::Clear.question());
    assert_eq!(declined.get_yes_label(), "do it");
    assert_eq!(declined.get_no_label(), "forget it");
    declined.invoke_answered(false);
    assert!(bound.viewing_maintenance.borrow().is_none());
    assert_eq!(rows(&store), before);
    let index = windows.count();
    dispatch(&ui, true);
    let cull = bound
        .viewing_maintenance
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(cull.get_message(), Operation::Cull.question());
    // Change the saved rules after opening: acceptance must use the current ones.
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &FileViewingStatistics {
                    media_min_ms: Some(2000),
                    media_max_ms: Some(60_000),
                    preview_min_ms: Some(5000),
                    preview_max_ms: Some(10_000),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let pixels = headless::render(&windows.get(index).unwrap(), 680, 420);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("viewing_statistics_cull_question.png"),
        &pixels,
        680,
        420,
    )
    .unwrap();
    cull.invoke_answered(true);
    let fixture = hydrus_testkit::fixture_json("viewing_maintenance.json");
    assert_eq!(
        serde_json::json!(rows(&store)),
        fixture["events"][1]["after"]
    );
    assert!(cull.get_notice_only());
    assert_eq!(cull.get_window_title(), "Information");
    assert_eq!(cull.get_message(), Operation::Cull.completed());
    // A retained answer cannot reapply after the one-shot acceptance.
    cull.invoke_answered(true);
    assert_eq!(
        serde_json::json!(rows(&store)),
        fixture["events"][1]["after"]
    );
    cull.invoke_cancelled();
    dispatch(&ui, false);
    let clear = bound
        .viewing_maintenance
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    clear.invoke_answered(true);
    assert!(rows(&store).is_empty());
    assert_eq!(clear.get_message(), Operation::Clear.completed());
    clear.invoke_cancelled();
    assert!(rows(&Store::open(dir.path()).unwrap()).is_empty());
    ui.hide().unwrap();
}

#[test]
fn retired_hidden_and_successor_owners_cannot_clear_or_cull_current_records() {
    headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    seed(&store);
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let slot = Rc::new(RefCell::new(None));
    let open = || {
        viewing_maintenance_window::open(&store, &slot, Operation::Clear, {
            let weak = ui.as_weak();
            Rc::new(move || weak.upgrade().is_some_and(|w| w.window().is_visible()))
        })
        .unwrap()
    };
    let before = rows(&store);
    let retired = open();
    let successor = open();
    retired.invoke_cancelled();
    retired.invoke_answered(true);
    assert!(slot.borrow().is_some());
    assert!(successor.window().is_visible());
    assert_eq!(rows(&store), before);
    ui.hide().unwrap();
    successor.invoke_answered(true);
    assert!(slot.borrow().is_none());
    assert_eq!(rows(&store), before);
    ui.show().unwrap();
    let successor = open();
    retired.invoke_answered(true);
    assert_eq!(rows(&store), before);
    successor.invoke_cancelled();
    let invalid =
        viewing_maintenance_window::open(&store, &slot, Operation::Cull, Rc::new(|| true)).unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &FileViewingStatistics {
                    preview_min_ms: Some(10_001),
                    preview_max_ms: Some(10_000),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    invalid.invoke_answered(true);
    assert_eq!(invalid.get_window_title(), "Warning");
    assert!(invalid.get_message().contains("Preview min was greater"));
    assert_eq!(rows(&store), before);
    invalid.invoke_cancelled();
    ui.hide().unwrap();
}

#[test]
fn closing_and_dropping_the_question_releases_its_timer_owner() {
    headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let slot = Rc::new(RefCell::new(None));
    let owner = Rc::new(Cell::new(true));
    let weak = Rc::downgrade(&owner);
    let question = viewing_maintenance_window::open(
        &store,
        &slot,
        Operation::Clear,
        Rc::new(move || owner.get()),
    )
    .unwrap();
    question.invoke_cancelled();
    assert!(slot.borrow().is_none());
    drop(question);
    assert!(
        weak.upgrade().is_none(),
        "the stopped timer retained its owner through its close callback"
    );
}
