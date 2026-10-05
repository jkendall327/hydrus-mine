//! Owned native Options and real Store-to-current-list scheduler effects.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    downloader_update_times::Preferences,
    queues::{self, FileSeedMeta, NewFileSeed, SeedType},
    settings::{self, GuiSettings},
};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::Cell, rc::Rc};
fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = lines
        .iter()
        .position(|line| line.label == "options…")
        .unwrap();
    ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
    let w = bound.options.borrow().as_ref().unwrap().clone_strong();
    let at = w
        .get_pages()
        .iter()
        .position(|page| page.text == "speed and memory")
        .unwrap();
    w.invoke_page_chosen(at as i32);
    w
}
fn row(w: &OptionsWindow, label: &str) -> i32 {
    w.get_rows()
        .iter()
        .position(|row| row.label == label)
        .unwrap() as i32
}
fn raw(p: &Preferences) -> serde_json::Value {
    serde_json::json!([
        p.gallery_minimum_ms,
        p.gallery_denominator,
        p.watcher_minimum_ms,
        p.watcher_denominator
    ])
}
fn policy(value: &serde_json::Value) -> Preferences {
    Preferences {
        gallery_minimum_ms: value[0].as_i64().unwrap(),
        gallery_denominator: value[1].as_i64().unwrap(),
        watcher_minimum_ms: value[2].as_i64().unwrap(),
        watcher_denominator: value[3].as_i64().unwrap(),
    }
}
fn assert_controls(w: &OptionsWindow, fixture: &serde_json::Value, values: &serde_json::Value) {
    for (index, label) in fixture["labels"].as_array().unwrap().iter().enumerate() {
        let shown = w
            .get_rows()
            .row_data(row(w, label.as_str().unwrap()) as usize)
            .unwrap();
        assert!(shown.enabled);
        if index % 2 == 0 {
            let seconds = shown.fields.row_data(0).unwrap();
            let milliseconds = shown.fields.row_data(1).unwrap();
            assert_eq!(seconds.maximum, 59);
            assert_eq!(milliseconds.maximum, 999);
            let displayed = f64::from(seconds.value) + f64::from(milliseconds.value) / 1000.0;
            assert_eq!(serde_json::json!(displayed), values[index]["value"]);
        } else {
            assert_eq!(
                i64::from(shown.number),
                values[index]["value"].as_i64().unwrap()
            );
        }
    }
}
#[test]
fn real_options_constructor_cancel_normalization_save_reopen_and_retained_cancel() {
    let fixture = hydrus_testkit::fixture_json("downloader_update_times.json");
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for case in fixture["controls"].as_array().unwrap() {
        let imported = policy(&case["imported"]);
        store
            .write(|ctx| settings::set(ctx.conn(), &imported))
            .unwrap();
        let cancelled = options(&ui, &bound);
        cancelled.invoke_number_edited(row(&cancelled, fixture["labels"][1].as_str().unwrap()), 7);
        cancelled.invoke_cancel();
        cancelled.invoke_apply();
        assert_eq!(store.read(settings::get::<Preferences>).unwrap(), imported);
        let w = options(&ui, &bound);
        assert_controls(&w, &fixture, &case["displayed"]);
        let last = windows.count() - 1;
        let pixels = headless::render(&windows.get(last).unwrap(), 1100, 900);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                .join("downloader_update_times_options.png"),
            &pixels,
            1100,
            900,
        );
        w.invoke_apply();
        assert_eq!(raw(&store.read(settings::get).unwrap()), case["saved"]);
        let reopened = options(&ui, &bound);
        assert_controls(&reopened, &fixture, &case["reopened"]);
        reopened.invoke_cancel();
    }
}
fn list(ui: &MainWindow, watcher: bool) -> Vec<Vec<String>> {
    let rows = if watcher {
        ui.get_watcher_rows()
    } else {
        ui.get_gallery_rows()
    };
    rows.iter()
        .map(|row| row.cells.iter().map(|cell| cell.to_string()).collect())
        .collect()
}
fn pending(bound: &hydrus_gui::Bound, watcher: bool, queue: i64) -> usize {
    let p = bound.current.borrow();
    let p = p.borrow();
    let counts = if watcher {
        &p.watchers().unwrap().watcher(queue).unwrap().files
    } else {
        &p.gallery().unwrap().query(queue).unwrap().files
    };
    counts.values().sum()
}
fn tick(native: &slint::platform::software_renderer::MinimalSoftwareWindow) {
    std::thread::sleep(std::time::Duration::from_millis(260));
    headless::render(native, 1100, 700);
}
fn scheduler(watcher: bool) {
    let (_dirs, store) = crate::subscriptions::store();
    let gug = hydrus_core::url::AnyGug::Single(hydrus_core::url::Gug {
        name: "private deadline fixture".into(),
        key: "aa".into(),
        url_template: "https://booru.example/search/%tags%/1".into(),
        replacement_phrase: "%tags%".into(),
        separator: "+".into(),
        initial_search_text: "tag".into(),
        example_search_text: "test".into(),
    });
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &hydrus_parse::Downloaders {
                    gugs: hydrus_core::url::Gugs {
                        gugs: vec![gug],
                        keys_to_display: vec!["aa".into()],
                    },
                    ..Default::default()
                },
            )?;
            settings::set(
                ctx.conn(),
                &hydrus_core::subscriptions::GalleryDefaults {
                    file_limit: Some(2000),
                    gug: Some(("aa".into(), "private deadline fixture".into())),
                },
            )?;
            settings::set(
                ctx.conn(),
                &Preferences {
                    gallery_minimum_ms: 1000,
                    gallery_denominator: 3,
                    watcher_minimum_ms: 1000,
                    watcher_denominator: 3,
                },
            )?;
            let mut gui: GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(if watcher { 4 } else { 6 });
    let entered = (0..12)
        .map(|i| {
            if watcher {
                format!("https://boards.example/thread/{i}")
            } else {
                format!("private-{i}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if watcher {
        ui.invoke_watcher_urls(entered.into());
        ui.invoke_watcher_row_clicked(0, false, false);
        ui.invoke_watcher_highlight();
    } else {
        ui.invoke_gallery_queries(entered.into());
        ui.invoke_gallery_row_clicked(0, false, false);
        ui.invoke_gallery_highlight();
    }
    let page = bound.current.borrow().clone();
    let queues = if watcher {
        page.borrow().watchers().unwrap().queues.clone()
    } else {
        page.borrow().gallery().unwrap().queues.clone()
    };
    assert_eq!(queues.len(), 12);
    let queue = page.borrow().importer().unwrap().queue;
    store
        .write(|ctx| {
            for queue in &queues {
                queues::set_paused(ctx.conn(), *queue, Some(true), Some(true))?;
            }
            Ok(())
        })
        .unwrap();
    let clock = Rc::new(Cell::new(100.0));
    bound.downloader_updates.set_clock(Rc::new({
        let clock = clock.clone();
        move || clock.get()
    }));
    bound.downloader_updates.force();
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(104.0)
    );
    let before = list(&ui, watcher);
    store
        .write(|ctx| {
            queues::add_file_seeds(
                ctx.conn(),
                queue,
                &[NewFileSeed {
                    seed_type: SeedType::Url,
                    data: "https://booru.example/post/1".into(),
                    data_for_comparison: "https://booru.example/post/1".into(),
                    source_time: None,
                    referral_url: None,
                    meta: FileSeedMeta::default(),
                }],
                false,
                0,
            )
        })
        .unwrap();
    (bound.sync)();
    assert_eq!(
        pending(&bound, watcher, queue),
        0,
        "list remains on its pending deadline"
    );
    assert_eq!(list(&ui, watcher), before);
    assert_eq!(
        page.borrow()
            .importer()
            .unwrap()
            .files
            .values()
            .sum::<usize>(),
        1,
        "highlighted importer updates independently"
    );
    assert_eq!(
        page.borrow().import_progress(),
        (0, 1),
        "tab aggregate is fresh"
    );
    assert!(
        page.borrow().close_veto(true).is_some(),
        "close eligibility reads current work/history"
    );
    clock.set(104.0);
    tick(&windows.get(0).unwrap());
    assert_eq!(
        pending(&bound, watcher, queue),
        0,
        "strict equality does not refresh"
    );
    // Saved edits retain the existing pending deadline and apply to the next period.
    let fixture = hydrus_testkit::fixture_json("downloader_update_times.json");
    let edited = options(&ui, &bound);
    for index in [0, 2] {
        let minimum = row(&edited, fixture["labels"][index].as_str().unwrap());
        edited.invoke_field_edited(minimum, 0, 2);
        edited.invoke_field_edited(minimum, 1, 0);
        edited.invoke_number_edited(
            row(&edited, fixture["labels"][index + 1].as_str().unwrap()),
            99,
        );
    }
    edited.invoke_apply();
    assert_eq!(
        raw(&store.read(settings::get).unwrap()),
        serde_json::json!([2000, 99, 2000, 99])
    );
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(104.0)
    );
    clock.set(104.25);
    tick(&windows.get(0).unwrap());
    assert_eq!(pending(&bound, watcher, queue), 1);
    assert_ne!(list(&ui, watcher), before);
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(106.25)
    );
    let pixels = headless::render(&windows.get(0).unwrap(), 1100, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(if watcher {
            "downloader_watcher_update_times.png"
        } else {
            "downloader_gallery_update_times.png"
        }),
        &pixels,
        1100,
        700,
    );
    // Current-page identity controls polling; offscreen pages keep their own deadline.
    let first_key = bound.pages.borrow().shown().key;
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(if watcher { 4 } else { 6 });
    let second_key = bound.pages.borrow().shown().key;
    let second_page = bound.current.borrow().clone();
    assert!(!Rc::ptr_eq(&page, &second_page));
    bound.downloader_updates.force();
    assert_eq!(
        serde_json::json!(second_page.borrow().next_import_status_time()),
        serde_json::json!(106.25)
    );
    store
        .write(|ctx| {
            queues::add_file_seeds(
                ctx.conn(),
                queue,
                &[NewFileSeed {
                    seed_type: SeedType::Url,
                    data: "https://booru.example/post/2".into(),
                    data_for_comparison: "https://booru.example/post/2".into(),
                    source_time: None,
                    referral_url: None,
                    meta: FileSeedMeta::default(),
                }],
                false,
                0,
            )
        })
        .unwrap();
    clock.set(105.0);
    bound.downloader_updates.refresh();
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(106.25)
    );
    let first_index = bound
        .pages
        .borrow()
        .session()
        .pages
        .iter()
        .position(|p| p.key == first_key)
        .unwrap() as i32;
    let second_index = bound
        .pages
        .borrow()
        .session()
        .pages
        .iter()
        .position(|p| p.key == second_key)
        .unwrap() as i32;
    let waiting_rows = if watcher {
        page.borrow()
            .watchers()
            .unwrap()
            .watcher(queue)
            .unwrap()
            .files
            .clone()
    } else {
        page.borrow()
            .gallery()
            .unwrap()
            .query(queue)
            .unwrap()
            .files
            .clone()
    };
    ui.invoke_tab_chosen(0, first_index);
    assert!(Rc::ptr_eq(&page, &bound.current.borrow()));
    assert_eq!(
        pending(&bound, watcher, queue),
        1,
        "return before the deadline keeps cached rows"
    );
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(106.25)
    );
    ui.invoke_tab_chosen(0, second_index);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Preferences {
                    gallery_minimum_ms: 3000,
                    gallery_denominator: 99,
                    watcher_minimum_ms: 3000,
                    watcher_denominator: 99,
                },
            )
        })
        .unwrap();
    clock.set(106.5);
    bound.downloader_updates.refresh();
    assert_eq!(
        serde_json::json!(second_page.borrow().next_import_status_time()),
        serde_json::json!(109.5)
    );
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(106.25)
    );
    let offscreen_rows = if watcher {
        page.borrow()
            .watchers()
            .unwrap()
            .watcher(queue)
            .unwrap()
            .files
            .clone()
    } else {
        page.borrow()
            .gallery()
            .unwrap()
            .query(queue)
            .unwrap()
            .files
            .clone()
    };
    assert_eq!(
        offscreen_rows, waiting_rows,
        "offscreen list was not refreshed"
    );
    ui.invoke_tab_chosen(0, first_index);
    assert_eq!(
        pending(&bound, watcher, queue),
        2,
        "overdue return reads new rows"
    );
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(109.5),
        "overdue return uses saved policy"
    );
    assert_eq!(
        serde_json::json!(second_page.borrow().next_import_status_time()),
        serde_json::json!(109.5)
    );
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Preferences {
                    gallery_minimum_ms: 2000,
                    gallery_denominator: 99,
                    watcher_minimum_ms: 2000,
                    watcher_denominator: 99,
                },
            )
        })
        .unwrap();
    let next = page.borrow().next_import_status_time();
    ui.hide().unwrap();
    clock.set(200.0);
    bound.downloader_updates.refresh();
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(next)
    );
    ui.show().unwrap();
    bound.downloader_updates.refresh();
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(202.0)
    );
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    clock.set(203.0);
    bound.downloader_updates.refresh();
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(205.0)
    );
    // A new live owner replaces the old current-page scheduler without reviving it.
    let key = hydrus_core::pages::PageKey::random();
    let replacement = if watcher {
        hydrus_gui::SearchPage::watcher_downloader(
            store.clone(),
            key,
            "replacement",
            queues,
            None,
            None,
            Vec::new(),
        )
    } else {
        hydrus_gui::SearchPage::gallery_downloader(
            store.clone(),
            key,
            "replacement",
            queues,
            None,
            None,
            Vec::new(),
        )
    };
    let successor = bind(&ui, Pages::single(replacement));
    let successor_page = successor.current.borrow().clone();
    let retired = page.borrow().next_import_status_time();
    clock.set(300.0);
    bound.downloader_updates.force();
    (bound.sync)();
    assert_eq!(
        serde_json::json!(page.borrow().next_import_status_time()),
        serde_json::json!(retired)
    );
    let successor_clock = Rc::new(Cell::new(400.0));
    successor.downloader_updates.set_clock(Rc::new({
        let clock = successor_clock.clone();
        move || clock.get()
    }));
    successor.downloader_updates.force();
    assert_eq!(
        serde_json::json!(successor_page.borrow().next_import_status_time()),
        serde_json::json!(402.0)
    );
    assert_eq!(list(&ui, watcher).len(), 12);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    ui.show().unwrap();
    successor_clock.set(500.0);
    successor.downloader_updates.force();
    (successor.sync)();
    assert_eq!(
        serde_json::json!(successor_page.borrow().next_import_status_time()),
        serde_json::json!(402.0),
        "accepted close remains terminal after retained re-show"
    );
}
#[test]
fn gallery_owned_timer_reads_rows_at_deadline_keeps_live_details_and_retires() {
    scheduler(false);
}
#[test]
fn watcher_owned_timer_reads_rows_at_deadline_keeps_live_details_and_retires() {
    scheduler(true);
}
