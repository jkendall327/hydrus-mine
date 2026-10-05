//! A gallery and a watcher page's lists with several rows selected, as
//! `oracle/record_downloader_lists.py` recorded the reference's: what can
//! be highlighted (one selected, not shown; activating the shown one
//! clears it), what removing asks (how many, how many still work or
//! aren't yet DEAD, whether the shown one goes), and when "update
//! selected with current options" shows and what it gives them.

use std::sync::Arc;

use serde_json::Value;
use slint::ComponentHandle as _;

use hydrus_core::subscriptions::CheckerOptions;
use hydrus_gui::{Bound, MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedType};

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Gallery,
    Watcher,
}

/// The page's rows, by name (a query or a URL), in the list's order.
fn rows(bound: &Bound, kind: Kind) -> Vec<(i64, String)> {
    let page = bound.current.borrow();
    let page = page.borrow();
    match kind {
        Kind::Gallery => page
            .gallery()
            .unwrap()
            .queries
            .iter()
            .map(|q| (q.queue, q.query.clone()))
            .collect(),
        Kind::Watcher => page
            .watchers()
            .unwrap()
            .watchers
            .iter()
            .map(|w| (w.queue, w.state.url.clone()))
            .collect(),
    }
}

fn checker_list(c: &CheckerOptions) -> Value {
    // (a whole number of files, as the reference writes it)
    let intended = c.intended_files_per_check;
    #[allow(clippy::cast_possible_truncation)]
    let intended = if intended.fract() == 0.0 {
        serde_json::json!(intended as i64)
    } else {
        serde_json::json!(intended)
    };
    serde_json::json!([
        intended,
        c.never_faster_than,
        c.never_slower_than,
        [c.death_file_velocity.0, c.death_file_velocity.1]
    ])
}

/// What a recorded step's state compares: the selected and highlighted
/// rows' names, whether highlight is offered and "update selected with
/// current options" shows, the rows left (in the recording's order), and
/// each one's file limit or checker options.
fn state(ui: &MainWindow, bound: &Bound, store: &Store, kind: Kind, order: &[String]) -> Value {
    let rows = rows(bound, kind);
    let name = |queue: i64| rows.iter().find(|r| r.0 == queue).unwrap().1.clone();
    let page = bound.current.borrow();
    let page = page.borrow();
    let (selected, highlighted, can_highlight, set_options) = match kind {
        Kind::Gallery => {
            let view = page.gallery().unwrap();
            let data = ui.get_gallery_data();
            (
                view.selected(),
                view.state.highlighted,
                data.can_highlight,
                data.can_set_options,
            )
        }
        Kind::Watcher => {
            let view = page.watchers().unwrap();
            let data = ui.get_watcher_data();
            (
                view.selected(),
                view.state.highlighted,
                data.can_highlight,
                data.can_set_options,
            )
        }
    };
    // (the reference's list order is the recording's)
    let mut selected: Vec<String> = selected.into_iter().map(name).collect();
    selected.sort_by_key(|s| order.iter().position(|o| o == s));
    let left: Vec<&String> = order
        .iter()
        .filter(|o| rows.iter().any(|r| &r.1 == *o))
        .collect();
    let options: Vec<Value> = left
        .iter()
        .map(|o| {
            let queue = rows.iter().find(|r| &r.1 == *o).unwrap().0;
            let row = store
                .read(move |c| queues::queue(c, queue))
                .unwrap()
                .unwrap();
            match kind {
                Kind::Gallery => {
                    let search: hydrus_core::gallery::GallerySearch =
                        serde_json::from_value(row.extra).unwrap();
                    serde_json::json!(search.file_limit)
                }
                Kind::Watcher => {
                    checker_list(&hydrus_store::watchers::watcher_state(&row).unwrap().checker)
                }
            }
        })
        .collect();
    serde_json::json!({
        "selected": selected,
        "highlighted": highlighted.map(name),
        "can_highlight": can_highlight,
        "set_options_shown": set_options,
        "rows_now": left,
        "options": options,
    })
}

fn replay(kind: Kind, recorded: &Value) {
    let (_dirs, store) = store();
    let gug = hydrus_core::url::AnyGug::Single(hydrus_core::url::Gug {
        name: "example tag search".into(),
        key: "aa".into(),
        url_template: "https://booru.example/search/%tags%/1".into(),
        replacement_phrase: "%tags%".into(),
        separator: "+".into(),
        initial_search_text: "tag".into(),
        example_search_text: "blue_eyes".into(),
    });
    let downloaders = hydrus_parse::Downloaders {
        gugs: hydrus_core::url::Gugs {
            gugs: vec![gug],
            keys_to_display: vec!["aa".into()],
        },
        ..hydrus_parse::Downloaders::default()
    };
    // (the reference's default file limit, as recorded)
    let defaults = hydrus_core::subscriptions::GalleryDefaults {
        file_limit: Some(2000),
        gug: Some(("aa".into(), "example tag search".into())),
    };
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &downloaders)?;
            hydrus_store::settings::set(ctx.conn(), &defaults)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // download, then gallery or watcher
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(if kind == Kind::Gallery { 6 } else { 4 });

    let steps = recorded["steps"].as_array().unwrap();
    let order: Vec<String> = steps[0]["rows_now"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect();
    let entered = order.join("\n");
    match kind {
        Kind::Gallery => ui.invoke_gallery_queries(entered.into()),
        Kind::Watcher => ui.invoke_watcher_urls(entered.into()),
    }
    let compare = |at: &str, step: &Value| {
        let ours = state(&ui, &bound, &store, kind, &order);
        for key in [
            "selected",
            "highlighted",
            "can_highlight",
            "set_options_shown",
            "rows_now",
            "options",
        ] {
            assert_eq!(ours[key], step[key], "{at}: {key}");
        }
    };
    compare("start", &steps[0]);
    for (i, step) in steps.iter().enumerate().skip(1) {
        let at = format!("step {i} ({})", step["do"]);
        // select the step's rows: a click, then ctrl+clicks
        let rows_now = rows(&bound, kind);
        let picked: Vec<i32> = step["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let name = &order[usize::try_from(r.as_u64().unwrap()).unwrap()];
                let at = rows_now.iter().position(|row| &row.1 == name).unwrap();
                i32::try_from(at).unwrap()
            })
            .collect();
        for (n, &row) in picked.iter().enumerate() {
            match kind {
                Kind::Gallery => ui.invoke_gallery_row_clicked(row, n > 0, false),
                Kind::Watcher => ui.invoke_watcher_row_clicked(row, n > 0, false),
            }
        }
        let asked = step["asked"].as_array().unwrap();
        match step["do"].as_str().unwrap() {
            "select" => {}
            "highlight" => match kind {
                Kind::Gallery => ui.invoke_gallery_highlight(),
                Kind::Watcher => ui.invoke_watcher_highlight(),
            },
            "remove" => match kind {
                Kind::Gallery => ui.invoke_gallery_remove(),
                Kind::Watcher => ui.invoke_watcher_remove(),
            },
            "set options" => match kind {
                Kind::Gallery => ui.invoke_gallery_set_options(),
                Kind::Watcher => ui.invoke_watcher_set_options(),
            },
            "work" => {
                // (a file to import, its files not paused: it works)
                let queues: Vec<i64> = picked
                    .iter()
                    .map(|&r| rows_now[usize::try_from(r).unwrap()].0)
                    .collect();
                store
                    .write(move |ctx| {
                        for &queue in &queues {
                            let seed = NewFileSeed {
                                seed_type: SeedType::Url,
                                data: "https://booru.example/post/1".into(),
                                data_for_comparison: "https://booru.example/post/1".into(),
                                source_time: None,
                                referral_url: None,
                                meta: FileSeedMeta::default(),
                            };
                            queues::add_file_seeds(ctx.conn(), queue, &[seed], false, 0)?;
                        }
                        Ok(())
                    })
                    .unwrap();
                (bound.sync)();
                // The Qt fixture calls _UpdateImportStatusNow after this injected work.
                bound.downloader_updates.force();
            }
            "file limit" => {
                let limit = i32::try_from(step["argument"].as_i64().unwrap()).unwrap();
                ui.invoke_gallery_limit(false, limit);
            }
            "checker" => {
                // (the editor's "slow thread" preset, as recorded)
                let slow_thread = &hydrus_gui::checker_options::PRESETS[1].1;
                assert_eq!(checker_list(slow_thread), step["argument"], "{at}");
                ui.invoke_watcher_page_checker();
                let editor = bound
                    .checker_options
                    .borrow()
                    .as_ref()
                    .expect("the editor opens")
                    .clone_strong();
                editor.invoke_preset(1);
                editor.invoke_ok();
            }
            other => panic!("{other}"),
        }
        // what was asked, answered as the recording was
        match asked.as_slice() {
            [] => assert_eq!(ui.get_question(), "", "{at}"),
            [question] => {
                assert_eq!(
                    ui.get_question(),
                    question["message"].as_str().unwrap(),
                    "{at}"
                );
                ui.invoke_answer(question["answer"].as_bool().unwrap());
            }
            more => panic!("{at}: {more:?}"),
        }
        compare(&at, step);
    }
    // (the page's own options, at the end)
    let page_options = match kind {
        Kind::Gallery => serde_json::json!(ui.get_gallery_data().file_limit),
        Kind::Watcher => checker_list(
            &bound
                .current
                .borrow()
                .borrow()
                .watcher_page_checker()
                .unwrap(),
        ),
    };
    assert_eq!(page_options, recorded["page_options"]);
}

#[test]
fn a_gallery_pages_list_acts_on_its_selection_as_the_references() {
    let recorded = hydrus_testkit::fixture_json("downloader_lists.json");
    replay(Kind::Gallery, &recorded["gallery"]);
}

#[test]
fn a_watcher_pages_list_acts_on_its_selection_as_the_references() {
    let recorded = hydrus_testkit::fixture_json("downloader_lists.json");
    replay(Kind::Watcher, &recorded["watcher"]);
}
