//! Every ordinary dialog that ends through the dialog helper, swept: opened
//! as a user opens it, then ended each way the reference's `NewDialog` is,
//! replaying `oracle/record_regular_dialog.py`'s `DialogEdit` / `DialogManage`
//! cases: a real Escape key press and its cancel close it and keep nothing of
//! the `regular_dialog` frame; its apply (or a `DialogNullipotent`'s close
//! button) closes it and keeps its size. Then each window's own sub-states
//! (a question, an inline editor, a modal child) take Escape first, as their
//! own modal dialogs do in the reference, and the dialog stays open.
//!
//! A later batch of dialogs moved onto the helper adds its windows here.

// (window sizes are whole pixels; each case's apply and cancel read as the
// buttons they press)
#![allow(
    clippy::cast_possible_truncation,
    clippy::redundant_closure_for_method_calls
)]

use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::windows::{FrameLocation, WindowSettings};
use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless, windows};
use hydrus_store::Store;
use serde_json::Value;
use slint::platform::Key;
use slint::{ComponentHandle, Model as _};

use crate::common::widgets;

/// A window opened, and how it is applied and cancelled.
struct Opened {
    window: Box<dyn crate::frame_geometry::Shown>,
    apply: Box<dyn Fn()>,
    cancel: Box<dyn Fn()>,
    /// What a user does first so that Escape reaches the dialog (a
    /// shortcut's capture box takes Escape as the shortcut to record).
    before_escape: Option<Box<dyn Fn()>>,
}

/// The store a client shows, and how a user opens the window in it (again
/// and again, the client kept alive by the closure).
type Case = (Arc<Store>, Box<dyn Fn() -> Opened>);

fn opened<C: ComponentHandle + 'static>(
    window: C,
    apply: impl Fn(&C) + 'static,
    cancel: impl Fn(&C) + 'static,
) -> Opened {
    let (a, c) = (window.clone_strong(), window.clone_strong());
    Opened {
        window: Box::new(window),
        apply: Box::new(move || apply(&a)),
        cancel: Box::new(move || cancel(&c)),
        before_escape: None,
    }
}

fn take<C: ComponentHandle + 'static>(slot: &std::cell::RefCell<Option<C>>, what: &str) -> C {
    slot.borrow()
        .as_ref()
        .map_or_else(|| panic!("{what} opens"), ComponentHandle::clone_strong)
}

/// A main window on `store`, shown.
fn client(store: &Arc<Store>) -> (MainWindow, Rc<Bound>) {
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = Rc::new(bind(&ui, Pages::open(store.clone()).unwrap()));
    (ui, bound)
}

/// file > options… on its page `page`, kept open for the children it opens.
fn options_page(ui: &MainWindow, bound: &Bound, page: &str) -> hydrus_gui::OptionsWindow {
    crate::options_window::open(ui);
    let options = take(&bound.options, "options");
    crate::options_window::show_page(&options, page);
    options
}

/// A window's name, and how its client is made.
type Named = (&'static str, fn() -> Case);

fn cases() -> Vec<Named> {
    vec![
        ("tag summary", || {
            let (dirs, store) = crate::options_window::store();
            let (ui, bound) = client(&store);
            let options = options_page(&ui, &bound, "tag presentation");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    let (at, _) = crate::options_window::row(&options, "On thumbnail top:");
                    options.invoke_banner_clicked(at);
                    opened(
                        take(&bound.options_banner_child, "the tag summary"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                }),
            )
        }),
        ("merge options", || {
            let crate::duplicates_lane_page::Opened {
                _windows: platform,
                _dir: kept,
                store,
                ui,
                bound,
            } = crate::duplicates_lane_page::opened();
            (
                store,
                Box::new(move || {
                    let _ = (&platform, &kept, &bound);
                    ui.invoke_duplicates_action("merge options".into(), 1, false, false);
                    opened(
                        hydrus_gui::merge_options_window::last_opened()
                            .expect("merge options opens"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("checker options", || {
            let (dirs, store) = crate::options_window::store();
            let (ui, bound) = client(&store);
            crate::options_window::open(&ui);
            let options = take(&bound.options, "options");
            options.invoke_search_edited("watcher checker".into());
            options.invoke_search_chosen(0);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    let (at, _) =
                        crate::options_window::row(&options, "Default watcher checker options:");
                    options.invoke_checker_clicked(at);
                    opened(
                        take(&bound.checker_options, "checker options"),
                        |w| w.invoke_ok(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("OR predicate", || {
            let (dirs, store) = crate::subscriptions::store();
            // (the advanced editor, which takes a string, asks for advanced mode)
            store
                .write(|ctx| {
                    hydrus_store::settings::set(
                        ctx.conn(),
                        &hydrus_store::settings::AdvancedMode(true),
                    )
                })
                .unwrap();
            let ui = MainWindow::new().unwrap();
            ui.show().unwrap();
            let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
            (
                store,
                Box::new(move || {
                    let _ = &dirs;
                    ui.invoke_search_edited("parent draft".into());
                    ui.invoke_search_or_action(4);
                    let w = take(&bound.search_or, "the OR editor");
                    w.invoke_edited("blue eyes or green eyes".into());
                    opened(w, |w| w.invoke_apply(), |w| w.invoke_cancel())
                }),
            )
        }),
        ("favourite searches", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            (
                store,
                Box::new(move || {
                    let _ = &dirs;
                    ui.invoke_favourites_menu_requested(10.0, 100.0);
                    ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
                    opened(
                        take(&bound.favourites.list, "favourite searches"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("favourite search", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            ui.invoke_favourites_menu_requested(10.0, 100.0);
            ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
            let list = take(&bound.favourites.list, "favourite searches");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    list.invoke_add();
                    opened(
                        take(&bound.favourites.edit, "a favourite search"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("shortcut set", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let options = crate::options_shortcut_sets::options(&ui, &bound);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    options.invoke_shortcut_set_action("add".into());
                    opened(
                        hydrus_gui::shortcut_windows::last_set().expect("a shortcut set opens"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("shortcut command", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let options = crate::options_shortcut_sets::options(&ui, &bound);
            options.invoke_shortcut_set_action("add".into());
            let set = hydrus_gui::shortcut_windows::last_set().expect("a shortcut set opens");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound, &options);
                    set.invoke_action("add".into());
                    let w = hydrus_gui::shortcut_windows::last_command()
                        .expect("a shortcut command opens");
                    w.invoke_key_capture("q".into(), 0);
                    let mouse = w.clone_strong();
                    let mut shown = opened(w, |w| w.invoke_apply(), |w| w.invoke_cancel());
                    // (the keyboard capture box records Escape as the shortcut,
                    // as the reference's: the mouse box doesn't)
                    shown.before_escape = Some(Box::new(move || mouse.set_mode(1)));
                    shown
                }),
            )
        }),
        ("external call", || {
            let (dirs, store) = crate::external_calls::store();
            crate::external_calls::seed(&store);
            let (ui, bound) = client(&store);
            let options = crate::external_calls::open(&ui, &bound);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    options.invoke_external_call_clicked(
                        crate::external_calls::named(&options, "synthetic call"),
                        false,
                        false,
                    );
                    options.invoke_external_call_action("edit".into());
                    opened(
                        crate::external_calls::call(&bound),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("external command", || {
            let (dirs, store) = crate::external_calls::store();
            crate::external_calls::seed(&store);
            let (ui, bound) = client(&store);
            let options = crate::external_calls::open(&ui, &bound);
            options.invoke_external_call_clicked(
                crate::external_calls::named(&options, "synthetic call"),
                false,
                false,
            );
            options.invoke_external_call_action("edit".into());
            let call = crate::external_calls::call(&bound);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &options);
                    call.invoke_command_edit();
                    opened(
                        take(&bound.options_external_calls.command, "the command"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("media view options", || {
            let client = crate::options_media_support::Media::basic();
            let options = client.open_options();
            crate::options_window::show_page(&options, "media playback");
            let store = client.store.clone();
            (
                store,
                Box::new(move || {
                    let _ = &client;
                    options.invoke_media_view_action("add".into());
                    let (chooser, _) = hydrus_gui::options_media_views::open_children();
                    let chooser = chooser.expect("a filetype is asked for");
                    let at = chooser
                        .get_choices()
                        .iter()
                        .position(|c| c == "image: jpeg")
                        .expect("image: jpeg");
                    chooser.invoke_chosen(at as i32);
                    let (_, editor) = hydrus_gui::options_media_views::open_children();
                    opened(
                        editor.expect("the media view editor opens"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("open externally calls", || {
            let (dirs, store) = crate::subscriptions::store();
            crate::open_externally::seed(&store);
            let (ui, bound) = client(&store);
            let options = crate::open_externally::open(&ui, &bound);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    options.invoke_routing_file_action("add".into());
                    crate::open_externally::choose(&bound, "image/png");
                    opened(
                        take(&bound.options_open_externally.files, "the calls"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("manage services", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            (
                store,
                Box::new(move || {
                    let _ = &dirs;
                    crate::services_editor::open(&ui);
                    opened(
                        take(&bound.services_editor.manage, "manage services"),
                        |w| w.invoke_apply_clicked(),
                        |w| w.invoke_cancel_clicked(),
                    )
                }),
            )
        }),
        ("edit service", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            crate::services_editor::open(&ui);
            let manage = take(&bound.services_editor.manage, "manage services");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    manage.invoke_add_clicked(3);
                    opened(
                        take(&bound.services_editor.edit, "a service"),
                        |w| w.invoke_apply_clicked(),
                        |w| w.invoke_cancel_clicked(),
                    )
                }),
            )
        }),
        ("simple downloader formulae", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            (bound.open_page)(&hydrus_gui::page_chooser::NewPage::SimpleDownloader);
            (
                store,
                Box::new(move || {
                    let _ = &dirs;
                    ui.invoke_simple_edit_formulae();
                    opened(
                        take(&bound.simple_formulae.list, "the formulae"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("export to png", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            (bound.open_page)(&hydrus_gui::page_chooser::NewPage::SimpleDownloader);
            ui.invoke_simple_edit_formulae();
            let list = take(&bound.simple_formulae.list, "the formulae");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    list.invoke_row_clicked(0, false, false);
                    list.invoke_exchange(1);
                    // (`DialogNullipotent`: its close button is its OK; Escape and the
                    // window's close its cancel)
                    opened(
                        hydrus_gui::png_export_window::last().expect("export to png opens"),
                        |w| w.invoke_action("close".into()),
                        |w| w.invoke_escape(),
                    )
                }),
            )
        }),
        ("export to pngs", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            (bound.open_page)(&hydrus_gui::page_chooser::NewPage::SimpleDownloader);
            ui.invoke_simple_edit_formulae();
            let list = take(&bound.simple_formulae.list, "the formulae");
            (
                store,
                Box::new(move || {
                    let _ = &dirs;
                    list.invoke_row_clicked(0, false, false);
                    list.invoke_row_clicked(1, true, false);
                    list.invoke_exchange(2);
                    opened(
                        bound
                            .simple_formulae
                            .pngs
                            .window()
                            .expect("export to pngs opens"),
                        |w| w.invoke_action("close".into()),
                        |w| w.invoke_escape(),
                    )
                }),
            )
        }),
        ("import options overwrite", || {
            let s = crate::import_options_fields::setup();
            let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
            let existing = hydrus_downloader_exchange::import_options::decode_text(
                &fixture["existing"].to_string(),
            )
            .unwrap();
            let editor = crate::import_options_paste::editor_of(&s, &existing);
            let incoming = fixture["incoming"].to_string();
            hydrus_gui::set_paster(move || incoming.clone());
            let store = s.store.clone();
            (
                store,
                Box::new(move || {
                    let _ = &s;
                    editor.invoke_paste_options(3);
                    opened(
                        hydrus_gui::import_options_overwrite_window::last_opened()
                            .expect("the overwrite chooser opens"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("tag filter", || {
            let crate::duplicates_lane_page::Opened {
                _windows: platform,
                _dir: kept,
                store,
                ui,
                bound,
            } = crate::duplicates_lane_page::opened();
            ui.invoke_duplicates_action("merge options".into(), 0, false, false);
            let merge =
                hydrus_gui::merge_options_window::last_opened().expect("merge options opens");
            (
                store,
                Box::new(move || {
                    let _ = (&platform, &kept, &bound);
                    // (a tag service's row, then "edit filter")
                    merge.invoke_row_clicked(0, 0, false, false);
                    merge.invoke_list_button(0, "edit filter".into());
                    opened(
                        hydrus_gui::tag_filter_window::last_opened().expect("the tag filter opens"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("incremental tagging", || {
            let recorded = hydrus_testkit::fixture_json("manage_tag_counts_incremental.json");
            let (dir, store, files) = crate::manage_tag_counts::fixture::seed(&recorded);
            let ui = MainWindow::new().unwrap();
            ui.show().unwrap();
            let bound = bind(
                &ui,
                Pages::single(SearchPage::fixed(
                    store.clone(),
                    "ordered incremental corpus",
                    None,
                    files,
                )),
            );
            ui.invoke_select_all();
            ui.invoke_manage_tags_selected();
            let parent = take(&bound.manage_tags, "manage tags");
            let mine = parent
                .get_service_names()
                .iter()
                .position(|n| n == "my tags")
                .unwrap();
            parent.invoke_service_chosen(mine as i32);
            (
                store,
                Box::new(move || {
                    let _ = (&dir, &ui);
                    parent.invoke_incremental_tags();
                    opened(
                        take(&bound.incremental_tags, "incremental tagging"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("tag siblings", || tags_case("siblings")),
        ("tag parents", || tags_case("parents")),
        ("login step", || {
            let (dir, store, _) = crate::login_workflows::store();
            let (ui, bound) = client(&store);
            crate::common::main_menu::choose(&ui, &["network", "logins"], "login scripts\u{2026}");
            let list = take(&bound.login_workflows.scripts, "login scripts");
            list.invoke_row_clicked(0, false, false);
            list.invoke_action("edit".into());
            let script = take(&bound.login_workflows.script, "a login script");
            (
                store,
                Box::new(move || {
                    let _ = (&dir, &list);
                    script.invoke_step_clicked(0);
                    script.invoke_action("edit-step".into());
                    opened(
                        take(&bound.login_workflows.step.step, "a login step"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                }),
            )
        }),
    ]
}

/// tags > manage tag siblings or parents.
fn tags_case(kind: &'static str) -> Case {
    let (dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    (
        store,
        Box::new(move || {
            let _ = &dirs;
            let lines = crate::common::main_menu::open(&ui, &["tags"]);
            let label = lines
                .iter()
                .find(|l| l.0.starts_with(kind))
                .unwrap_or_else(|| panic!("tags > {kind}"))
                .0
                .clone();
            crate::common::main_menu::choose(&ui, &["tags"], &label);
            opened(
                take(&bound.tag_relationships, "tag relationships"),
                |w| w.invoke_apply(),
                |w| w.invoke_cancel(),
            )
        }),
    )
}

fn pair(value: &Value) -> Option<(i32, i32)> {
    let value = value.as_array()?;
    Some((value[0].as_i64()? as i32, value[1].as_i64()? as i32))
}

fn recorded_frame(row: &Value) -> FrameLocation {
    FrameLocation {
        remember_size: row[0].as_bool().unwrap(),
        remember_position: row[1].as_bool().unwrap(),
        last_size: pair(&row[2]),
        last_position: pair(&row[3]),
        default_gravity: pair(&row[4]).unwrap(),
        default_position: row[5].as_str().unwrap().into(),
        maximised: row[6].as_bool().unwrap(),
        fullscreen: row[7].as_bool().unwrap(),
    }
}

fn frame(store: &Store) -> FrameLocation {
    store
        .read(hydrus_store::settings::get::<WindowSettings>)
        .unwrap()
        .frame("regular_dialog")
        .unwrap()
        .clone()
}

fn keep(store: &Store, frame: FrameLocation) {
    store
        .write(move |ctx| {
            let mut settings: WindowSettings = hydrus_store::settings::get(ctx.conn())?;
            settings.set_frame("regular_dialog", frame);
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
}

/// Run `body` for each case in a thread of its own (each with its own
/// headless platform: some cases' clients set theirs up themselves).
fn each_case(body: impl Fn(&'static str, Case) + Send + Sync + Copy + 'static) {
    for (name, make) in cases() {
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let _windows = (!INITS_ITSELF.contains(&name)).then(headless::init);
                body(name, make());
            })
            .unwrap()
            .join()
            .unwrap_or_else(|_| panic!("{name} failed"));
    }
}

/// The cases whose clients set up the headless platform themselves.
const INITS_ITSELF: &[&str] = &["merge options", "media view options", "tag filter"];

/// Each window, opened afresh three times: Escape and cancel keep nothing,
/// apply keeps the size it was given.
#[test]
fn every_helper_dialog_keeps_its_frame_only_when_applied() {
    each_case(|name, (store, open)| {
        let recorded = hydrus_testkit::fixture_json("regular_dialog.json");
        let baseline = recorded_frame(&recorded["baseline"]);
        let moved = pair(&recorded["moved"]["size"]).unwrap();
        let case = |action: &str| {
            recorded["lifecycle"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["kind"] == "edit" && c["action"] == action)
                .unwrap()
                .clone()
        };
        for (how, recorded) in [
            ("escape", case("escape")),
            ("cancel", case("cancel")),
            ("apply", case("apply")),
        ] {
            keep(&store, baseline.clone());
            let shown = open();
            let window = shown.window.win();
            assert!(window.is_visible(), "{name}: opened");
            window.set_size(slint::LogicalSize::new(moved.0 as f32, moved.1 as f32));
            match how {
                "escape" => {
                    if let Some(first) = &shown.before_escape {
                        first();
                    }
                    widgets::key(window, Key::Escape);
                }
                "cancel" => (shown.cancel)(),
                _ => (shown.apply)(),
            }
            assert!(!window.is_visible(), "{name}: {how} closes it");
            let mut after = recorded_frame(&recorded["after"]);
            if after != baseline {
                after.last_position = Some(windows::state(window).position);
            }
            assert_eq!(frame(&store), after, "{name}: {how}");
        }
    });
}

fn index(model: &slint::ModelRc<slint::SharedString>, text: &str) -> i32 {
    (0..model.row_count())
        .find(|&i| model.row_data(i).unwrap().contains(text))
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or_else(|| panic!("missing choice {text}"))
}

fn escape(window: &slint::Window) {
    widgets::key(window, Key::Escape);
}

/// A question or inline editor within a dialog, or a modal child of it,
/// takes Escape first, as its own modal dialog does in the reference: the
/// sub-state closes (a child stays open), and the dialog stays open.
#[test]
fn escape_closes_a_dialogs_own_question_or_waits_on_its_child_first() {
    std::thread::spawn(|| {
        // merge options: the note settings box, then the tag filter child
        let crate::duplicates_lane_page::Opened {
            _windows,
            _dir,
            ui,
            bound,
            ..
        } = crate::duplicates_lane_page::opened();
        let _ = &bound;
        ui.invoke_duplicates_action("merge options".into(), 1, false, false);
        let merge = hydrus_gui::merge_options_window::last_opened().unwrap();
        merge.invoke_note_settings();
        assert!(merge.get_note_settings_open());
        escape(merge.window());
        assert!(
            !merge.get_note_settings_open(),
            "merge: note settings closed"
        );
        assert!(merge.window().is_visible(), "merge: still open");
        merge.invoke_cancel();
        ui.invoke_duplicates_action("merge options".into(), 0, false, false);
        let merge = hydrus_gui::merge_options_window::last_opened().unwrap();
        merge.invoke_row_clicked(0, 0, false, false);
        merge.invoke_list_button(0, "edit filter".into());
        let filter = hydrus_gui::tag_filter_window::last_opened().unwrap();
        escape(merge.window());
        assert!(
            merge.window().is_visible() && filter.window().is_visible(),
            "merge: waits on its filter"
        );
    })
    .join()
    .unwrap();

    let _windows = headless::init();
    // login step: "Remove all selected?"
    let (_dir, store, _) = crate::login_workflows::store();
    let (ui, bound) = client(&store);
    crate::common::main_menu::choose(&ui, &["network", "logins"], "login scripts\u{2026}");
    let list = take(&bound.login_workflows.scripts, "login scripts");
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = take(&bound.login_workflows.script, "a login script");
    script.invoke_step_clicked(0);
    script.invoke_action("edit-step".into());
    let step = take(&bound.login_workflows.step.step, "a login step");
    step.invoke_content_clicked(0, false, false);
    step.invoke_action("delete-content".into());
    assert!(step.get_deleting());
    escape(step.window());
    assert!(!step.get_deleting(), "login step: the question answered no");
    assert!(step.window().is_visible(), "login step: still open");

    // services: an edit child open under manage services; the counter's
    // "edit value" question in a service's editor
    let (_dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    crate::services_editor::open(&ui);
    let manage = take(&bound.services_editor.manage, "manage services");
    manage.invoke_add_clicked(3);
    let child = take(&bound.services_editor.edit, "a service");
    escape(manage.window());
    assert!(
        manage.window().is_visible() && child.window().is_visible(),
        "manage services: waits on its child"
    );
    child.invoke_cancel_clicked();
    let counter = manage
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap() == "counter")
        .expect("a counter service");
    manage.invoke_row_clicked(counter as i32, false, false);
    manage.invoke_edit_clicked();
    let edit = take(&bound.services_editor.edit, "the counter");
    edit.invoke_counter_edit(3);
    assert!(edit.get_counter_editing());
    escape(edit.window());
    assert!(
        !edit.get_counter_editing(),
        "edit service: the question answered no"
    );
    assert!(edit.window().is_visible(), "edit service: still open");

    // simple downloader formulae: "add defaults"
    (bound.open_page)(&hydrus_gui::page_chooser::NewPage::SimpleDownloader);
    ui.invoke_simple_edit_formulae();
    let formulae = take(&bound.simple_formulae.list, "the formulae");
    formulae.invoke_defaults();
    assert_ne!(formulae.get_defaults_step(), 0);
    escape(formulae.window());
    assert_eq!(
        formulae.get_defaults_step(),
        0,
        "formulae: the question cancelled"
    );
    assert!(formulae.window().is_visible(), "formulae: still open");

    // favourites (on a search page): the list waits on its editor; the
    // editor's locations question answered no, and its predicate child
    // waited on
    let (_dirs, store) = crate::subscriptions::store();
    store
        .write(|ctx| {
            hydrus_store::settings::set(ctx.conn(), &hydrus_store::settings::AdvancedMode(true))
        })
        .unwrap();
    let (ui, bound) = client(&store);
    ui.invoke_favourites_menu_requested(10.0, 100.0);
    ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let favourites = take(&bound.favourites.list, "favourite searches");
    favourites.invoke_add();
    let edit = take(&bound.favourites.edit, "a favourite search");
    escape(favourites.window());
    assert!(
        favourites.window().is_visible() && edit.window().is_visible(),
        "favourites: waits on its editor"
    );
    edit.invoke_location_chosen(index(&edit.get_location_choices(), "multiple/deleted"));
    assert!(edit.get_selecting_locations());
    escape(edit.window());
    assert!(
        !edit.get_selecting_locations(),
        "favourite: the locations question answered no"
    );
    assert!(edit.window().is_visible(), "favourite: still open");
    edit.invoke_suggestion_chosen(index(&edit.get_suggestions(), "system:limit"));
    let predicate = take(&bound.favourites.predicate_editor, "a predicate editor");
    escape(edit.window());
    assert!(
        edit.window().is_visible() && predicate.window().is_visible(),
        "favourite: waits on its predicate"
    );
}

/// Tag siblings and parents open with the window's own keys focused (so
/// Escape reaches them), not a tag input: typing goes nowhere until the user
/// clicks into an input, as the reference's opens with Qt's default focus.
#[test]
fn tag_relationships_open_without_an_input_taking_the_typing() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    for kind in ["siblings", "parents"] {
        let lines = crate::common::main_menu::open(&ui, &["tags"]);
        let label = lines
            .iter()
            .find(|l| l.0.starts_with(kind))
            .unwrap()
            .0
            .clone();
        crate::common::main_menu::choose(&ui, &["tags"], &label);
        let tags = take(&bound.tag_relationships, "tag relationships");
        widgets::key(tags.window(), "a");
        assert_eq!(
            tags.get_left_input(),
            "",
            "{kind}: no input took the typing"
        );
        assert_eq!(tags.get_right_input(), "", "{kind}");
        escape(tags.window());
        assert!(
            !tags.window().is_visible(),
            "{kind}: Escape still reaches it"
        );
    }
}
