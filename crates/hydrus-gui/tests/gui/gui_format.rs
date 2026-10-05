//! Owned Options staging and existing log/status consumers of the recorded preferences.
use hydrus_gui::{FileLogWindow, MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    queues::{self, FileSeedMeta, GallerySeedMeta, NewFileSeed, NewGallerySeed, SeedType},
    settings::{self, GuiFormatting},
};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
const ISO: &str = "Prefer ISO time (\"2018-03-01 12:40:23\") to \"5 days ago\": ";
const FIGURES: &str = "EXPERIMENTAL: Bytes strings >1KB pseudo significant figures: ";
fn value(p: &GuiFormatting) -> Value {
    json!({"iso":p.iso,"figures":p.figures})
}
fn row(w: &OptionsWindow, label: &str) -> i32 {
    i32::try_from(w.get_rows().iter().position(|r| r.label == label).unwrap()).unwrap()
}
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let w = bound.options.borrow().as_ref().unwrap().clone_strong();
    let index = w.get_pages().iter().position(|p| p.text == "gui").unwrap();
    w.invoke_page_chosen(i32::try_from(index).unwrap());
    w
}
fn edit(w: &OptionsWindow, event: &Value) {
    w.invoke_check_toggled(row(w, ISO), event["input"][0].as_bool().unwrap());
    w.invoke_number_edited(
        row(w, FIGURES),
        i32::try_from(event["input"][1].as_i64().unwrap()).unwrap(),
    );
}
fn cells(w: &FileLogWindow) -> Vec<String> {
    w.get_rows()
        .row_data(0)
        .unwrap()
        .cells
        .iter()
        .map(|v| v.to_string())
        .collect()
}
#[test]
fn staged_controls_reopen_and_reach_real_log_rows_and_page_size_status() {
    let (_dirs, store) = crate::subscriptions::store();
    let fixture = hydrus_testkit::fixture_json("gui_format.json");
    let file = store
        .write(|ctx| {
            let file: hydrus_core::HashId = ctx.conn().query_row(
                "SELECT hash_id FROM files ORDER BY hash_id LIMIT 1",
                [],
                |r| r.get(0),
            )?;
            ctx.conn()
                .execute("UPDATE files SET size = 1536 WHERE hash_id = ?", [file])?;
            Ok(file)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    bound.current.borrow().borrow_mut().add_files(&[file]);
    let seeds = fixture["seeds"].clone();
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            queues::add_file_seeds(
                conn,
                queue,
                &[NewFileSeed {
                    seed_type: SeedType::Url,
                    data: seeds["file"]["data"].as_str().unwrap().into(),
                    data_for_comparison: seeds["file"]["data"].as_str().unwrap().into(),
                    source_time: seeds["file"]["source_time"].as_i64(),
                    referral_url: None,
                    meta: FileSeedMeta::default(),
                }],
                false,
                0,
            )?;
            let mut seed = queues::file_seeds(conn, queue)?.remove(0);
            seed.created = seeds["file"]["created"].as_i64().unwrap();
            seed.modified = seeds["file"]["modified"].as_i64().unwrap();
            queues::update_file_seed(conn, &seed)?;
            queues::add_gallery_seeds(
                conn,
                queue,
                &[NewGallerySeed {
                    url: seeds["gallery"]["url"].as_str().unwrap().into(),
                    can_generate_more_pages: true,
                    referral_url: None,
                    meta: GallerySeedMeta::default(),
                }],
                None,
                0,
            )?;
            let mut seed = queues::gallery_seeds(conn, queue)?.remove(0);
            seed.created = seeds["gallery"]["created"].as_i64().unwrap();
            seed.modified = seeds["gallery"]["modified"].as_i64().unwrap();
            queues::update_gallery_seed(conn, &seed)
        })
        .unwrap();
    for event in fixture["events"].as_array().unwrap() {
        let before = store.read(settings::get::<GuiFormatting>).unwrap();
        let cancelled = options(&ui, &bound);
        edit(&cancelled, event);
        assert_eq!(store.read(settings::get::<GuiFormatting>).unwrap(), before);
        cancelled.invoke_cancel();
        let current = options(&ui, &bound);
        cancelled.invoke_apply();
        assert_eq!(
            store.read(settings::get::<GuiFormatting>).unwrap(),
            before,
            "retained Cancel owner cannot Apply"
        );
        edit(&current, event);
        current.invoke_apply();
        let saved = store.read(settings::get::<GuiFormatting>).unwrap();
        assert_eq!(value(&saved), event["saved"]);
        let reopened = options(&ui, &bound);
        assert_eq!(
            reopened
                .get_rows()
                .row_data(usize::try_from(row(&reopened, ISO)).unwrap())
                .unwrap()
                .checked,
            saved.iso
        );
        assert_eq!(
            reopened
                .get_rows()
                .row_data(usize::try_from(row(&reopened, FIGURES)).unwrap())
                .unwrap()
                .number,
            i32::from(saved.figures)
        );
        let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 980, 850);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gui_format_options.png"),
            &pixels,
            980,
            850,
        )
        .unwrap();
        reopened.invoke_cancel();
        ui.invoke_select_none();
        let expected = event["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|sample| sample["size"] == 1536)
            .unwrap()["text"]
            .as_str()
            .unwrap();
        assert!(
            ui.get_status().ends_with(&format!("totalling {expected}")),
            "{}",
            ui.get_status()
        );
        ui.invoke_select_all();
        assert!(
            ui.get_status().contains(&format!("selected, {expected} ")),
            "{}",
            ui.get_status()
        );
        ui.invoke_select_none();
        let png_slots = hydrus_gui::png_export_window::Slots::default();
        let png = hydrus_gui::png_export_window::open(
            &png_slots,
            &store,
            "x".repeat(1536),
            std::rc::Rc::new(|| {}),
        )
        .unwrap();
        assert_eq!(
            png.get_payload_description(),
            format!("String - {expected}")
        );
        png.invoke_action("close".into());
        assert!(!png_slots.has_open());
        let formula_slots = hydrus_gui::formula_window::Slots::default();
        let preview = &event["consumers"]["parser_previews"][0];
        let formula = hydrus_gui::formula_window::open(
            &store,
            &hydrus_gui::formula_editors::new_formula(false),
            hydrus_gui::formula_window::FormulaTestData {
                text: preview["input"].as_str().unwrap().into(),
                ..hydrus_gui::formula_window::FormulaTestData::default()
            },
            &formula_slots,
            std::rc::Rc::new(|_| panic!("inspecting a preview must not apply a formula")),
        )
        .unwrap();
        *formula_slots.formula.borrow_mut() = Some(formula.clone_strong());
        assert_eq!(
            formula.get_raw_description(),
            preview["label"].as_str().unwrap()
        );
        assert_eq!(formula.get_document(), preview["raw"].as_str().unwrap());
        formula.invoke_cancel();
        assert!(formula_slots.formula.borrow().is_none());
        ui.invoke_open_file_log();
        let log = bound.file_log.borrow().as_ref().unwrap().clone_strong();
        if saved.iso {
            assert_eq!(json!(cells(&log)), event["file_row"]);
        } else {
            assert!(cells(&log)[3].ends_with(" ago"));
            assert!(!cells(&log)[3].contains("2023-"));
        }
        log.invoke_close_window();
        ui.invoke_open_search_log();
        let log = bound.file_log.borrow().as_ref().unwrap().clone_strong();
        if saved.iso {
            assert_eq!(json!(cells(&log)), event["gallery_row"]);
        } else {
            assert!(cells(&log)[3].ends_with(" ago"));
        }
        log.invoke_close_window();
        assert_eq!(
            value(
                &hydrus_store::Store::open(store.dir())
                    .unwrap()
                    .read(settings::get)
                    .unwrap()
            ),
            event["reopened"]
        );
    }
    ui.hide().unwrap();
    drop(bound);
}
