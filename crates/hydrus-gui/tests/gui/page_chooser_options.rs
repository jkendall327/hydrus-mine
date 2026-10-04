//! Staged GUI Pages controls reach real new-page search locations after Apply.
use hydrus_core::{ServiceKey, service::builtin_keys};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    Store, services,
    settings::{self, PageChooserSettings},
};
use slint::{ComponentHandle as _, Model as _};

fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = options.get_pages();
    let page = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "gui pages")
        .unwrap();
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    options
}

fn rows(options: &OptionsWindow, labels: &serde_json::Value) -> [i32; 4] {
    let rows = options.get_rows();
    let first = (0..rows.row_count())
        .find(|&i| rows.row_data(i).unwrap().label == labels[0].as_str().unwrap())
        .unwrap();
    std::array::from_fn(|offset| {
        let row = rows.row_data(first + offset).unwrap();
        assert_eq!(row.label, labels[offset].as_str().unwrap());
        assert_eq!(row.kind, 1);
        assert!(row.enabled);
        i32::try_from(first + offset).unwrap()
    })
}

fn labels(ui: &MainWindow) -> Vec<String> {
    let labels = ui.get_chooser_labels();
    (0..labels.row_count())
        .map(|i| labels.row_data(i).unwrap().to_string())
        .collect()
}

#[test]
fn checkbox_drafts_cancel_apply_reopen_and_choose_exact_reference_locations() {
    let windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("page_chooser_options.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let second = ServiceKey::from_hex(fixture["services"][1]["key"].as_str().unwrap()).unwrap();
    store
        .write_and_refresh(move |ctx| {
            ctx.conn().execute(
                "UPDATE services SET name = 'domain 01' WHERE service_key = ?",
                [ServiceKey::new(builtin_keys::MY_FILES.to_vec())],
            )?;
            services::insert(
                ctx.conn(),
                &second,
                "domain 02",
                &services::ServiceKind::LocalFiles,
            )?;
            Ok(())
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before = store.read(settings::get::<PageChooserSettings>).unwrap();
    let cancelled = open(&ui, &bound);
    for row in rows(&cancelled, &fixture["labels"]) {
        cancelled.invoke_check_toggled(row, true);
    }
    cancelled.invoke_cancel();
    assert_eq!(
        store.read(settings::get::<PageChooserSettings>).unwrap(),
        before
    );
    let options = open(&ui, &bound);
    let controls = rows(&options, &fixture["labels"]);
    assert_eq!(
        controls.map(|i| options
            .get_rows()
            .row_data(usize::try_from(i).unwrap())
            .unwrap()
            .checked),
        [true, false, false, false]
    );
    for step in fixture["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["count"] == 2)
    {
        let current = if bound.options.borrow().is_some() {
            bound.options.borrow().as_ref().unwrap().clone_strong()
        } else {
            open(&ui, &bound)
        };
        let controls = rows(&current, &fixture["labels"]);
        for (i, row) in controls.into_iter().enumerate() {
            current.invoke_check_toggled(row, step["flags"][i].as_bool().unwrap());
        }
        current.invoke_apply();
        let saved = store.read(settings::get::<PageChooserSettings>).unwrap();
        assert_eq!(
            [
                saved.show_combined,
                saved.combined_at_top,
                saved.show_storage,
                saved.storage_at_top
            ],
            std::array::from_fn::<_, 4, _>(|i| step["applied_flags"][i].as_bool().unwrap())
        );
        let original = bound.pages.borrow().session().clone();
        ui.invoke_tab_space_pressed(0, false);
        ui.invoke_tab_space_pressed(0, false);
        ui.invoke_chooser_pressed(8);
        assert_eq!(
            labels(&ui),
            step["labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        ui.invoke_chooser_cancel();
        assert_eq!(bound.pages.borrow().session(), &original);
        assert_eq!(ui.get_chooser_labels().row_count(), 0);
        // Every recorded entry makes a search on exactly its chosen domain,
        // including trash/storage/combined, rather than the fallback location.
        for choice in step["choices"].as_array().unwrap() {
            ui.invoke_tab_space_pressed(0, false);
            ui.invoke_tab_space_pressed(0, false);
            ui.invoke_chooser_pressed(8);
            ui.invoke_chooser_pressed(i32::try_from(choice["button"].as_u64().unwrap()).unwrap());
            assert_eq!(ui.get_chooser_labels().row_count(), 0);
            let page = bound.pages.borrow_mut().current();
            let page = page.borrow();
            assert_eq!(
                page.location()
                    .current()
                    .iter()
                    .map(ServiceKey::to_hex)
                    .collect::<Vec<_>>(),
                vec![choice["key"].as_str().unwrap().to_owned()]
            );
            assert!(page.location().deleted().is_empty());
        }
    }
    // Persisted subchoices survive reopen, even while their primary is off.
    let hidden = open(&ui, &bound);
    let controls = rows(&hidden, &fixture["labels"]);
    for (i, row) in controls.into_iter().enumerate() {
        hidden.invoke_check_toggled(row, i == 1 || i == 3);
    }
    hidden.invoke_apply();
    let reopened = open(&ui, &bound);
    let controls = rows(&reopened, &fixture["labels"]);
    assert_eq!(
        controls.map(|i| reopened
            .get_rows()
            .row_data(usize::try_from(i).unwrap())
            .unwrap()
            .checked),
        [false, true, false, true]
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 980, 850);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
    if let Some(path) = std::env::var_os("HYDRUS_GUI_SCREENSHOT_DIR") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        headless::save_png(&path.join("page-chooser-options.png"), &pixels, 980, 850).unwrap();
    }
    reopened.invoke_cancel();
    drop(reopened);
    drop(hidden);
    drop(options);
    drop(cancelled);
    drop(ui);
    drop(bound);
    drop(store);
    let reopened_store = Store::open(dir.path()).unwrap();
    assert_eq!(
        reopened_store
            .read(settings::get::<PageChooserSettings>)
            .unwrap(),
        PageChooserSettings {
            show_combined: false,
            combined_at_top: true,
            show_storage: false,
            storage_at_top: true
        }
    );
}
