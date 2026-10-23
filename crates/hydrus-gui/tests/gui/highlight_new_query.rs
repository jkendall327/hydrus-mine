//! "If new query entered and no current highlight, highlight the new query"
//! at both of the reference's sites: a gallery page's query entry and the
//! subscription gap downloader (reached here, as in the reference, from the
//! "sub gap downloader test" popup's button), with the option on and off
//! through the options window, against oracle/fixtures/highlight_new_query.json.
use super::debug_menu_actions::{Debug, start};
use super::options_system_consumers::{new_page, row_in, show_page};
use hydrus_gui_model::debug_actions as debug;
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
use std::time::{Duration, Instant};

const LABEL: &str = "If new query entered and no current highlight, highlight the new query:";

fn add_downloader(d: &Debug) {
    let gug = hydrus_core::url::AnyGug::Single(hydrus_core::url::Gug {
        name: "booru tag search".into(),
        key: "bb".into(),
        url_template: "https://booru.example/search/%tags%/1".into(),
        replacement_phrase: "%tags%".into(),
        separator: "+".into(),
        initial_search_text: "tag".into(),
        example_search_text: "blue_eyes".into(),
    });
    let downloaders = hydrus_parse::Downloaders {
        gugs: hydrus_core::url::Gugs {
            gugs: vec![gug],
            keys_to_display: vec!["bb".into()],
        },
        ..hydrus_parse::Downloaders::default()
    };
    let defaults = hydrus_core::subscriptions::GalleryDefaults {
        file_limit: Some(2000),
        gug: Some(("bb".into(), "booru tag search".into())),
    };
    // (the recorder's pages start their queues paused: nothing is fetched)
    d.store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &downloaders)?;
            hydrus_store::settings::set(ctx.conn(), &defaults)?;
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::Pauses {
                    network_traffic: true,
                    ..hydrus_store::settings::Pauses::default()
                },
            )
        })
        .unwrap();
}

/// File > options > downloading: set the option, and apply.
fn set_option(d: &Debug, on: bool) {
    let ui = &d.ui;
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .expect("file > options");
    ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
    let window = d.bound.options.borrow().as_ref().unwrap().clone_strong();
    show_page(&window, "downloading");
    let (i, row) = row_in(&window, "gallery downloader", LABEL);
    if row.checked != on {
        window.invoke_check_toggled(i, on);
    }
    window.invoke_apply();
}

/// The shown gallery page as the recorder writes it: its name, its queries
/// (text and file limit) and the index of the highlighted one.
fn shown(d: &Debug) -> Value {
    let page = d.bound.current.borrow().clone();
    let page = page.borrow();
    described(&page)
}

fn described(page: &hydrus_gui::SearchPage) -> Value {
    let gallery = page.gallery().expect("a gallery page");
    let queries: Vec<Value> = gallery
        .queries
        .iter()
        .map(|q| json!([q.query, q.file_limit]))
        .collect();
    let highlighted = gallery
        .queries
        .iter()
        .position(|q| Some(q.queue) == gallery.state.highlighted);
    json!({
        "page": gallery.page_name,
        "queries": queries,
        "highlighted": highlighted,
    })
}

fn pump_until(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < Duration::from_secs(10), "{what}");
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(20));
    }
}

// leaf: audit-options-downloading-gallery-downloader-if-new-query-entered-and-no-current-highlight-highlight-the-new-query
#[test]
fn a_new_query_is_highlighted_at_both_of_the_reference_s_sites_only_when_the_option_says() {
    let recording = hydrus_testkit::fixture_json("highlight_new_query.json");
    let d = start();
    add_downloader(&d);

    // the gallery page's query entry
    for run in recording["queries"].as_array().unwrap() {
        set_option(&d, run["option"].as_bool().unwrap());
        new_page(&d.ui, true);
        for (query, recorded) in ["blue_eyes", "red_eyes"]
            .iter()
            .zip(run["steps"].as_array().unwrap())
        {
            d.ui.invoke_gallery_queries((*query).into());
            assert_eq!(&shown(&d), recorded, "option {}", run["option"]);
        }
    }

    // the gap downloader, from the popup's button
    d.click(&["gui actions", "make some popups"]);
    let gap = || {
        d.jobs()
            .into_iter()
            .find(|j| j.user_callable_label.as_deref() == Some(debug::GAP_LABEL))
    };
    pump_until("the gap popup", || gap().is_some());
    let gui_owner = || {
        d.ui.get_popups()
            .row_data(0)
            .expect("a popup is shown")
            .gui_owner
    };
    pump_until("the popups shown", || d.ui.get_popups().row_count() > 0);
    for run in recording["gap"].as_array().unwrap() {
        set_option(&d, run["option"].as_bool().unwrap());
        for (n, recorded) in run["steps"].as_array().unwrap().iter().enumerate() {
            let job = gap().unwrap();
            d.ui.invoke_popup_call(
                hex::encode(job.key).into(),
                hex::encode(job.action_owner.unwrap()).into(),
                gui_owner(),
            );
            // (read as the recorder reads it: the page of that name)
            let gap_page = || {
                d.bound
                    .pages
                    .borrow_mut()
                    .gallery_page_named(debug::GAP_PAGE_NAME)
                    .unwrap()
            };
            pump_until("the gap search", || {
                gap_page().borrow().gallery().unwrap().queries.len() == n + 1
            });
            assert_eq!(
                &described(&gap_page().borrow()),
                recorded,
                "option {}",
                run["option"]
            );
        }
        // (the recorder closes the gap page between its runs)
        d.bound.pages.borrow_mut().close_shown().unwrap();
        (d.bound.sync)();
    }
}
