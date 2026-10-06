//! Real File-menu launch, saved Options staging and current binding retirement.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    Store,
    settings::{self, ExportSettings},
};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, path::Path, rc::Rc};
fn choose(ui: &MainWindow, pane: i32, label: &str) {
    let lines = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
    let index = lines.iter().position(|row| row.label == label).unwrap();
    assert!(lines.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(pane, index as i32, 0.0, 0.0, 0.0);
}
fn open(ui: &MainWindow) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    choose(ui, 0, "open");
    choose(ui, 1, "quick export directory");
}
fn save(store: &Store, path: Option<String>) {
    store
        .write_and_refresh(move |c| {
            let mut value: ExportSettings = settings::get(c.conn())?;
            value.default_directory = path;
            settings::set(c.conn(), &value)
        })
        .unwrap();
}
fn mapped(value: &str, store: &Store) -> String {
    value.replace("<DB>", &store.dir().to_string_lossy())
}
// Normalize only recorded filesystem results; saved Option strings stay raw.
fn filesystem(value: &str, store: &Store) -> String {
    Path::new(&mapped(value, store))
        .components()
        .collect::<std::path::PathBuf>()
        .to_string_lossy()
        .into_owned()
}
fn message(value: &str, store: &Store) -> String {
    value
        .split('"')
        .map(|part| {
            if part.starts_with("<DB>") {
                filesystem(part, store)
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\"")
}
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    choose(ui, 0, "options…");
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = window
        .get_pages()
        .iter()
        .position(|row| row.text == "exporting")
        .unwrap();
    window.invoke_page_chosen(page as i32);
    let row = window
        .get_rows()
        .iter()
        .position(|row| row.label == "Default export directory: ")
        .unwrap();
    (window, row as i32)
}
#[test]
fn actual_file_menu_replays_saved_paths_fallback_errors_cancel_and_reopen() {
    let fixture = hydrus_testkit::fixture_json("quick_export_directory.json");
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let home = store.dir().join("synthetic-home");
    bound.quick_export_directory.set_home_resolver(Rc::new({
        let home = home.clone();
        move || Some(home.clone())
    }));
    let launches = Rc::new(RefCell::new(Vec::<String>::new()));
    hydrus_gui::set_launcher({
        let launches = launches.clone();
        move |target| launches.borrow_mut().push(target.to_owned())
    });
    std::fs::create_dir_all(store.dir().join("synthetic exports 日本")).unwrap();
    #[cfg(not(windows))]
    std::fs::create_dir_all(store.dir().join("literal\\export")).unwrap();
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| !case["case"].as_str().unwrap().starts_with("options_"))
    {
        let name = case["case"].as_str().unwrap();
        if name == "invalid_legacy_value" || (cfg!(windows) && name.starts_with("legacy_backslash"))
        {
            continue;
        }
        let path = case["stored"].as_str().map(|v| mapped(v, &store));
        save(&store, path);
        if name == "unresolved_home" {
            bound
                .quick_export_directory
                .set_home_resolver(Rc::new(|| None));
        } else {
            bound.quick_export_directory.set_home_resolver(Rc::new({
                let home = home.clone();
                move || Some(home.clone())
            }));
        }
        if name == "fallback_conflicting_file" {
            std::fs::remove_dir(home.join("hydrus_export")).unwrap();
            std::fs::write(home.join("hydrus_export"), "synthetic conflicting file").unwrap();
        }
        launches.borrow_mut().clear();
        if name == "fallback" {
            store
                .write(|c| {
                    c.conn()
                        .execute("DELETE FROM settings WHERE key='export'", [])?;
                    Ok(())
                })
                .unwrap();
        }
        let before: ExportSettings = store.read(settings::get).unwrap();
        open(&ui);
        let expected: Vec<_> = case["launched"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| filesystem(v.as_str().unwrap(), &store))
            .collect();
        assert_eq!(*launches.borrow(), expected, "{name}");
        assert_eq!(
            store.read::<ExportSettings>(settings::get).unwrap(),
            before,
            "action does not save preferences"
        );
        if name == "fallback" {
            assert!(home.join("hydrus_export").is_dir());
        }
        if name == "configured_missing" {
            assert!(!Path::new(&expected[0]).exists());
        }
        if name == "unresolved_home" || name == "fallback_conflicting_file" {
            let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
            let popups = store.read(|c| hydrus_store::popups::all(c, now)).unwrap();
            let text = if name == "unresolved_home" {
                case["messages"][0].as_str().unwrap()
            } else {
                case["errors"][0]["message"].as_str().unwrap()
            };
            let text = message(text, &store);
            let popup = popups
                .iter()
                .find(|popup| popup.status_text_1.as_deref() == Some(text.as_str()))
                .unwrap();
            assert_eq!(popup.had_error, name == "fallback_conflicting_file");
            assert!(popup.done && !popup.cancellable && !popup.pausable);
        }
    }
    let original = mapped("<DB>/synthetic exports 日本", &store);
    save(&store, Some(original.clone()));
    let (cancelled, row) = options(&ui, &bound);
    cancelled.invoke_text_edited(row, mapped("<DB>/discarded directory", &store).into());
    cancelled.invoke_cancel();
    launches.borrow_mut().clear();
    open(&ui);
    assert_eq!(*launches.borrow(), [filesystem(&original, &store)]);
    cancelled.show().unwrap();
    cancelled.invoke_apply();
    let (applied, row) = options(&ui, &bound);
    let destination = mapped("<DB>/saved exports 日本", &store);
    applied.invoke_text_edited(row, destination.clone().into());
    applied.invoke_apply();
    assert!(!Path::new(&destination).exists());
    launches.borrow_mut().clear();
    open(&ui);
    assert_eq!(*launches.borrow(), [filesystem(&destination, &store)]);
    let reopened = Store::open(store.dir()).unwrap();
    assert_eq!(
        reopened
            .read::<ExportSettings>(settings::get)
            .unwrap()
            .default_directory,
        Some(destination.clone())
    );
    let (reopened_control, row) = options(&ui, &bound);
    assert_eq!(
        reopened_control
            .get_rows()
            .row_data(row as usize)
            .unwrap()
            .text,
        destination
    );
    reopened_control.invoke_cancel();
    // A malformed typed setting is reported, never replaced by a default or
    // launched. Qt's malformed legacy value likewise fails before its opener.
    launches.borrow_mut().clear();
    store
        .write(|c| {
            c.conn().execute(
                "UPDATE settings SET value=? WHERE key='export'",
                [r#"{"default_directory":7}"#],
            )?;
            Ok(())
        })
        .unwrap();
    open(&ui);
    assert!(launches.borrow().is_empty());
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    assert!(
        store
            .read(|c| hydrus_store::popups::all(c, now))
            .unwrap()
            .iter()
            .any(|job| {
                job.had_error
                    && job.status_text_1.as_deref().is_some_and(|text| {
                        text.starts_with("Could not read the export directory:")
                    })
            })
    );
    store
        .write(move |c| {
            settings::set(
                c.conn(),
                &ExportSettings {
                    default_directory: Some(destination),
                    ..ExportSettings::default()
                },
            )
        })
        .unwrap();
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    choose(&ui, 0, "open");
    let native = windows.get(0).unwrap();
    let pixels = headless::render(&native, 1000, 700);
    headless::save_png(
        &Path::new(env!("CARGO_TARGET_TMPDIR")).join("quick-export-directory-native.png"),
        &pixels,
        1000,
        700,
    )
    .unwrap();
}
#[test]
fn hidden_rebound_closed_and_reentrant_resolver_owners_cannot_launch_or_create() {
    let (_dirs, store) = super::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    save(
        &store,
        Some(
            store
                .dir()
                .join("first saved path")
                .to_string_lossy()
                .into_owned(),
        ),
    );
    let launches = Rc::new(RefCell::new(Vec::<String>::new()));
    hydrus_gui::set_launcher({
        let launches = launches.clone();
        move |path| launches.borrow_mut().push(path.to_owned())
    });
    ui.hide().unwrap();
    bound.quick_export_directory.open();
    assert!(launches.borrow().is_empty());
    ui.show().unwrap();
    open(&ui);
    assert_eq!(launches.borrow().len(), 1);
    launches.borrow_mut().clear();
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.quick_export_directory.open();
    assert!(launches.borrow().is_empty());
    save(
        &store,
        Some(
            store
                .dir()
                .join("successor saved path")
                .to_string_lossy()
                .into_owned(),
        ),
    );
    open(&ui);
    assert_eq!(
        launches.borrow().as_slice(),
        [store
            .dir()
            .join("successor saved path")
            .to_string_lossy()
            .into_owned()]
    );
    launches.borrow_mut().clear();
    save(&store, None);
    let home = store.dir().join("retired resolver home");
    successor.quick_export_directory.set_home_resolver(Rc::new({
        let weak = ui.as_weak();
        let home = home.clone();
        move || {
            weak.upgrade().unwrap().hide().unwrap();
            Some(home.clone())
        }
    }));
    successor.quick_export_directory.open();
    assert!(launches.borrow().is_empty());
    assert!(
        !home.exists(),
        "owner invalidation during lookup cannot create fallback"
    );
    ui.show().unwrap();
    let saved = store
        .dir()
        .join("accepted-close path")
        .to_string_lossy()
        .into_owned();
    save(&store, Some(saved));
    store
        .write(|c| {
            let mut gui: settings::GuiSettings = settings::get(c.conn())?;
            gui.confirm_exit = false;
            settings::set(c.conn(), &gui)?;
            // This boundary tests completed exit, independently of due maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(c.conn())?;
            shutdown.action = 0;
            settings::set(c.conn(), &shutdown)
        })
        .unwrap();
    let _ = ui
        .window()
        .dispatch_event_with_result(slint::platform::WindowEvent::CloseRequested);
    assert!(
        !ui.window().is_visible(),
        "completed exit precedes retained callbacks"
    );
    ui.show().unwrap();
    successor.quick_export_directory.open();
    open(&ui);
    assert!(
        launches.borrow().is_empty(),
        "accepted close permanently retires callback even after re-show"
    );
}
