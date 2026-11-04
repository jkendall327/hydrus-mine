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
        ("manage tag display and search", || {
            menu_dialog(&["tags"], "display/search\u{2026}", |b| {
                opened(
                    take(&b.tag_display, "tag display"),
                    |w| w.invoke_apply(),
                    |w| w.invoke_cancel(),
                )
            })
        }),
        ("manage downloader display", || {
            menu_dialog(
                &["network", "downloaders"],
                "downloader and url display\u{2026}",
                |_| {
                    opened(
                        hydrus_gui::downloader_display_window::last_opened()
                            .expect("downloader display"),
                        |w| w.invoke_apply_clicked(),
                        |w| w.invoke_cancel_clicked(),
                    )
                },
            )
        }),
        ("manage url classes", || {
            menu_dialog(&["network", "downloaders"], "url classes\u{2026}", |b| {
                opened(
                    take(&b.downloader_definitions.classes, "url classes"),
                    |w| w.invoke_action("apply".into()),
                    |w| w.invoke_action("cancel".into()),
                )
            })
        }),
        ("manage gallery url generators", || {
            menu_dialog(
                &["network", "downloaders"],
                "gallery url generators\u{2026}",
                |b| {
                    opened(
                        take(&b.downloader_definitions.gugs, "gallery url generators"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                },
            )
        }),
        ("edit url class", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            crate::common::main_menu::choose(
                &ui,
                &["network", "downloaders"],
                "url classes\u{2026}",
            );
            let list = take(&bound.downloader_definitions.classes, "url classes");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    list.invoke_action("add".into());
                    opened(
                        take(&bound.downloader_definitions.class_edit, "edit url class"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                }),
            )
        }),
        ("manage parsers", || {
            menu_dialog(&["network", "downloaders"], "parsers\u{2026}", |b| {
                opened(
                    take(&b.parser_editors.list, "parsers"),
                    |w| w.invoke_action("apply".into()),
                    |w| w.invoke_action("cancel".into()),
                )
            })
        }),
        ("manage url class links", || {
            menu_dialog(
                &["network", "downloaders"],
                "url class links\u{2026}",
                |b| {
                    opened(
                        take(&b.parser_editors.links, "url class links"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                },
            )
        }),
        ("manage regex favourites", || {
            let (dirs, store) = crate::options_window::store();
            let (ui, bound) = client(&store);
            let options = options_page(&ui, &bound, "regex favourites");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    options.invoke_regex_favourites_clicked();
                    opened(
                        hydrus_gui::regex_favourites_window::last_opened()
                            .expect("regex favourites"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                }),
            )
        }),
        ("manage logins", || {
            let (dir, store, _) = crate::login_workflows::store();
            let (ui, bound) = client(&store);
            (
                store,
                Box::new(move || {
                    let _ = &dir;
                    crate::common::main_menu::choose(&ui, &["network", "logins"], "logins\u{2026}");
                    opened(
                        take(&bound.login_workflows.domains.domains, "manage logins"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                }),
            )
        }),
        ("edit login", || {
            let (dir, store, _) = crate::login_workflows::store();
            let (ui, bound) = client(&store);
            crate::common::main_menu::choose(&ui, &["network", "logins"], "logins\u{2026}");
            let domains = take(&bound.login_workflows.domains.domains, "manage logins");
            domains.invoke_row_clicked(0, false, false);
            (
                store,
                Box::new(move || {
                    let _ = (&dir, &ui);
                    domains.invoke_action("credentials".into());
                    opened(
                        take(&bound.login_workflows.domains.credentials, "edit login"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                }),
            )
        }),
        ("manage login scripts", || {
            let (dir, store, _) = crate::login_workflows::store();
            let (ui, bound) = client(&store);
            (
                store,
                Box::new(move || {
                    let _ = &dir;
                    crate::common::main_menu::choose(
                        &ui,
                        &["network", "logins"],
                        "login scripts\u{2026}",
                    );
                    opened(
                        take(&bound.login_workflows.scripts, "login scripts"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                }),
            )
        }),
        ("manage http headers", || {
            menu_dialog(&["network", "data"], "manage http headers\u{2026}", |b| {
                opened(
                    take(&b.network_sessions.headers, "http headers"),
                    |w| w.invoke_apply_clicked(),
                    |w| w.invoke_cancel_clicked(),
                )
            })
        }),
        ("edit header", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            crate::common::main_menu::choose(
                &ui,
                &["network", "data"],
                "manage http headers\u{2026}",
            );
            let headers = take(&bound.network_sessions.headers, "http headers");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    headers.invoke_row_clicked(0, false, false);
                    headers.invoke_edit_clicked();
                    opened(
                        hydrus_gui::network_sessions_window::last_edit_opened()
                            .expect("edit header"),
                        |w| w.invoke_apply_clicked(),
                        |w| w.invoke_cancel_clicked(),
                    )
                }),
            )
        }),
        ("edit api access permissions", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let (review, keys) = api_keys(&ui, &bound);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &review);
                    keys.invoke_add_clicked();
                    opened(
                        hydrus_gui::client_api_admin_window::last_edit_opened()
                            .expect("api permissions"),
                        |w| w.invoke_apply_clicked(),
                        |w| w.invoke_cancel_clicked(),
                    )
                }),
            )
        }),
        ("waiting for API access permissions request", || {
            let (dirs, store) = crate::subscriptions::store();
            let id = store.snapshot().services.by_name("client api").unwrap().id;
            store
                .write_and_refresh(move |ctx| {
                    hydrus_store::services::update_config(
                        ctx.conn(),
                        id,
                        &hydrus_store::services::ServiceKind::ClientApi(
                            hydrus_store::services::ServerConfig {
                                port: Some(45869),
                                ..hydrus_store::services::ServerConfig::default()
                            },
                        ),
                    )
                })
                .unwrap();
            let (ui, bound) = client(&store);
            let (review, keys) = api_keys(&ui, &bound);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &review);
                    keys.invoke_add_from_api_clicked();
                    // (`DialogNullipotent`: its close button is its OK; Escape and the
                    // window's close its cancel)
                    opened(
                        hydrus_gui::client_api_admin_window::last_request_opened()
                            .expect("waiting"),
                        |w| w.invoke_cancel_clicked(),
                        |w| w.invoke_escape(),
                    )
                }),
            )
        }),
        ("edit import folders", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            (
                store,
                Box::new(move || {
                    let _ = &dirs;
                    crate::folders::open(&ui, "manage import folders\u{2026}");
                    opened(
                        crate::folders::import_list(&bound),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("edit export folders", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            (
                store,
                Box::new(move || {
                    let _ = &dirs;
                    crate::folders::open(&ui, "manage export folders\u{2026}");
                    opened(
                        take(&bound.folders.export_list, "export folders"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("edit import folder", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let watched = tempfile::tempdir().unwrap();
            crate::folders::open(&ui, "manage import folders\u{2026}");
            let list = crate::folders::import_list(&bound);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &watched);
                    list.invoke_add();
                    let edit = take(&bound.folders.import_edit, "an import folder");
                    edit.set_path(watched.path().to_string_lossy().into_owned().into());
                    opened(edit, |w| w.invoke_apply(), |w| w.invoke_cancel())
                }),
            )
        }),
        ("edit export folder", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            crate::folders::open(&ui, "manage export folders\u{2026}");
            let list = take(&bound.folders.export_list, "export folders");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    list.invoke_add();
                    let edit = take(&bound.folders.export_edit, "an export folder");
                    edit.set_path("/tmp/exported".into());
                    opened(edit, |w| w.invoke_apply(), |w| w.invoke_cancel())
                }),
            )
        }),
        ("an import folder's file log", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            crate::folders::open(&ui, "manage import folders\u{2026}");
            let list = crate::folders::import_list(&bound);
            list.invoke_add();
            let folder = take(&bound.folders.import_edit, "an import folder");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &list);
                    folder.invoke_file_log();
                    // (a folder's copy: its `DialogEdit`'s apply, its close
                    // button a cancel)
                    opened(
                        take(&bound.folders.log, "the file log"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_close_window(),
                    )
                }),
            )
        }),
        ("edit import options", || {
            let s = crate::import_options_fields::setup();
            let store = s.store.clone();
            (
                store,
                Box::new(move || opened(s.editor(), |w| w.invoke_apply(), |w| w.invoke_cancel())),
            )
        }),
        ("edit external program call", || {
            let s = crate::import_options_fields::setup();
            crate::external_calls::seed(&s.store);
            let editor = s.editor();
            // (import options' external programs)
            editor.invoke_kind_clicked(6);
            editor.set_custom_index(1);
            editor.invoke_changed();
            let store = s.store.clone();
            (
                store,
                Box::new(move || {
                    let _ = &s;
                    editor.invoke_program_action("add".into());
                    opened(
                        hydrus_gui::external_program_entry_window().expect("the entry editor"),
                        |w| w.invoke_accepted(),
                        |w| w.invoke_cancelled(),
                    )
                }),
            )
        }),
        ("edit metadata migration routers", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let watched = tempfile::tempdir().unwrap();
            crate::folders::open(&ui, "manage import folders\u{2026}");
            let list = crate::folders::import_list(&bound);
            list.invoke_add();
            let edit = take(&bound.folders.import_edit, "an import folder");
            edit.set_path(watched.path().to_string_lossy().into_owned().into());
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &watched, &list);
                    edit.invoke_edit_sidecars();
                    opened(
                        take(&bound.folders.sidecars.routers, "routers"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("edit metadata migration router", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let watched = tempfile::tempdir().unwrap();
            crate::folders::open(&ui, "manage import folders\u{2026}");
            let list = crate::folders::import_list(&bound);
            list.invoke_add();
            let edit = take(&bound.folders.import_edit, "an import folder");
            edit.set_path(watched.path().to_string_lossy().into_owned().into());
            edit.invoke_edit_sidecars();
            let routers = take(&bound.folders.sidecars.routers, "routers");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &watched, &list, &edit);
                    routers.invoke_add();
                    opened(
                        take(&bound.folders.sidecars.router, "a router"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("edit metadata migration destination", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let watched = tempfile::tempdir().unwrap();
            crate::folders::open(&ui, "manage import folders\u{2026}");
            let list = crate::folders::import_list(&bound);
            list.invoke_add();
            let edit = take(&bound.folders.import_edit, "an import folder");
            edit.set_path(watched.path().to_string_lossy().into_owned().into());
            edit.invoke_edit_sidecars();
            let routers = take(&bound.folders.sidecars.routers, "routers");
            routers.invoke_add();
            let router = take(&bound.folders.sidecars.router, "a router");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &watched, &list, &edit, &routers);
                    router.invoke_edit_destination();
                    opened(
                        take(&bound.folders.sidecars.node, "a destination"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("database locations", || {
            let (dirs, store) = database_locations_store();
            let (ui, bound) = client(&store);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &bound);
                    crate::database_menu_jobs::from_menu_path(&ui, &[], "locations\u{2026}");
                    // (`DialogNullipotent`: its close button is its OK)
                    opened(
                        hydrus_gui::database_locations_window::opened().expect("locations"),
                        |w| w.invoke_close_window(),
                        |w| w.invoke_escape(),
                    )
                }),
            )
        }),
        ("edit max size", || {
            let (dirs, store) = database_locations_store();
            let (ui, bound) = client(&store);
            crate::database_menu_jobs::from_menu_path(&ui, &[], "locations\u{2026}");
            let window = hydrus_gui::database_locations_window::opened().expect("locations");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    window.invoke_clicked(0, false, false);
                    window.invoke_action("max".into());
                    opened(
                        hydrus_gui::database_locations_window::max_size_window()
                            .expect("edit max size"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("a rating's edit value", || {
            let (dirs, store) = crate::subscriptions::store();
            let counter = store.snapshot().services.by_name("counter").unwrap().id;
            let ui = MainWindow::new().unwrap();
            ui.show().unwrap();
            let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
            ui.invoke_search_edited("system:everything".into());
            ui.invoke_search_accepted();
            let a = bound.current.borrow().borrow().results()[0];
            store
                .write_content(move |w| w.set_incdec(counter, &[a], 3))
                .unwrap();
            ui.invoke_thumbnail_clicked(0, false, false);
            let dialog = crate::manage_ratings::open(&ui, &bound, 0);
            let row = dialog
                .get_names()
                .iter()
                .position(|n| n == "counter")
                .unwrap() as i32;
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    dialog.invoke_rating_middle(row);
                    opened(
                        take(&bound.rating_count_editor, "edit value"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("edit rules", || {
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
                    let _ = (&platform, &kept);
                    ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
                    opened(
                        crate::duplicates_lane_page::list_of(&bound),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("edit rule", || {
            let crate::duplicates_lane_page::Opened {
                _windows: platform,
                _dir: kept,
                store,
                ui,
                bound,
            } = crate::duplicates_lane_page::opened();
            ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
            let list = crate::duplicates_lane_page::list_of(&bound);
            list.invoke_add_suggested();
            list.invoke_suggested_chosen(0);
            (
                store,
                Box::new(move || {
                    let _ = (&platform, &kept, &ui);
                    list.invoke_row_clicked(0, false, false);
                    list.invoke_edit();
                    opened(
                        crate::duplicates_lane_page::rule_window(&bound),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("edit comparator", || {
            let crate::duplicates_lane_page::Opened {
                _windows: platform,
                _dir: kept,
                store,
                ui,
                bound,
            } = crate::duplicates_lane_page::opened();
            ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
            let list = crate::duplicates_lane_page::list_of(&bound);
            list.invoke_add();
            let rule = crate::duplicates_lane_page::rule_window(&bound);
            (
                store,
                Box::new(move || {
                    let _ = (&platform, &kept, &ui, &list);
                    rule.set_comparator_kind(crate::duplicates_lane_page::kind_index(
                        &rule,
                        "test A against B using file info",
                    ));
                    rule.invoke_comparator_add();
                    opened(
                        crate::duplicates_lane_page::newest_comparator(&bound),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("system:filesize", || {
            let (dirs, store) = crate::subscriptions::store();
            let ui = MainWindow::new().unwrap();
            ui.show().unwrap();
            let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
            (
                store,
                Box::new(move || {
                    // (the whole client kept: the editor's owner is in it)
                    let _ = (&dirs, &bound);
                    ui.invoke_search_edited("".into());
                    let i = ui
                        .get_suggestions()
                        .iter()
                        .position(|s| s.text == "system:filesize")
                        .unwrap();
                    ui.invoke_suggestion_chosen(i32::try_from(i).unwrap());
                    opened(
                        take(&bound.predicate_editor, "the predicate editor"),
                        |w| w.invoke_ok(0),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("a subscription query's file log", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let query = subscription_query(&store, &ui, &bound);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &query);
                    query.invoke_query_log(false);
                    // (`DialogNullipotent`, from a dialog: its close button
                    // is its OK)
                    opened(
                        take(&bound.folders.log, "the file log"),
                        |w| w.invoke_close_window(),
                        |w| w.invoke_escape(),
                    )
                }),
            )
        }),
        ("a subscription query's search log", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let query = subscription_query(&store, &ui, &bound);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &query);
                    query.invoke_query_log(true);
                    opened(
                        take(&bound.folders.log, "the search log"),
                        |w| w.invoke_close_window(),
                        |w| w.invoke_escape(),
                    )
                }),
            )
        }),
        // (batch 5: the windows the reference opens by other frame keys,
        // and the dialogs that keep no frame)
        ("string processor", || {
            // (a content parser's formula's "edit processing")
            parser_case(|b, content| {
                let formula = formula_of(b, content);
                formula.invoke_edit_processing();
                opened(
                    take(
                        &b.parser_editors.formula.strings.processor,
                        "the string processor",
                    ),
                    |w| w.invoke_apply(),
                    |w| w.invoke_cancel(),
                )
            })
        }),
        ("processing step", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let slots = hydrus_gui::string_processor_window::Slots::default();
            let processor = hydrus_gui::string_processor_window::open(
                &store,
                &hydrus_core::url::strings::StringProcessor::default(),
                vec!["example".into()],
                &slots,
                Rc::new(|_| {}),
            )
            .unwrap();
            *slots.processor.borrow_mut() = Some(processor.clone_strong());
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    // "add", a splitter
                    processor.invoke_add();
                    processor.invoke_chosen(0);
                    opened(
                        take(&slots.step, "a processing step"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("string match", || {
            keyed_case(|store, _| {
                let slots = hydrus_gui::string_processor_window::Slots::default();
                hydrus_gui::string_processor_window::open_match(
                    store,
                    &hydrus_core::url::strings::StringMatch::any(),
                    &slots,
                    Rc::new(|_| {}),
                );
                opened(
                    take(&slots.step, "a string match"),
                    |w| w.invoke_apply(),
                    |w| w.invoke_cancel(),
                )
            })
        }),
        ("string converter", || {
            // (a parser's "converter")
            let (dir, store, _) = crate::parser_editors::setup();
            let (ui, bound) = client(&store);
            crate::common::main_menu::choose(&ui, &["network", "downloaders"], "parsers\u{2026}");
            let list = take(&bound.parser_editors.list, "parsers");
            list.invoke_row_clicked(0, false, false);
            list.invoke_action("edit".into());
            let page = take(&bound.parser_editors.page, "a parser");
            (
                store,
                Box::new(move || {
                    let _ = (&dir, &ui, &list);
                    let b = &bound;
                    page.invoke_action("converter".into());
                    opened(
                        take(
                            &b.parser_editors.formula.strings.converter,
                            "the string converter",
                        ),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("conversion", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let slots = hydrus_gui::string_processor_window::Slots::default();
            slots.set_store(&store);
            let converter = hydrus_gui::string_processor_window::open_converter(
                &hydrus_core::url::strings::StringConverter::default(),
                Some("example".into()),
                &slots,
                Rc::new(|_| {}),
            )
            .unwrap();
            *slots.converter.borrow_mut() = Some(converter.clone_strong());
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    converter.invoke_add();
                    opened(
                        take(&slots.conversion, "a conversion"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("formula", || {
            // (a content parser's "formula")
            parser_case(|b, content| {
                content.invoke_action("formula".into());
                opened(
                    take(&b.parser_editors.formula.formula, "the formula"),
                    |w| w.invoke_apply(),
                    |w| w.invoke_cancel(),
                )
            })
        }),
        ("formula rule", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            let slots = hydrus_gui::formula_window::Slots::default();
            let formula = hydrus_gui::formula_window::open(
                &store,
                &hydrus_gui::formula_editors::new_formula(false),
                hydrus_gui::formula_window::FormulaTestData::default(),
                &slots,
                Rc::new(|_| {}),
            )
            .unwrap();
            *slots.formula.borrow_mut() = Some(formula.clone_strong());
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    formula.invoke_row_clicked(0, false, false);
                    formula.invoke_edit();
                    opened(
                        take(&slots.rule, "a formula rule"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("edit parser", || {
            let (dir, store, _) = crate::parser_editors::setup();
            let (ui, bound) = client(&store);
            crate::common::main_menu::choose(&ui, &["network", "downloaders"], "parsers\u{2026}");
            let list = take(&bound.parser_editors.list, "parsers");
            (
                store,
                Box::new(move || {
                    let _ = &dir;
                    list.invoke_row_clicked(0, false, false);
                    list.invoke_action("edit".into());
                    opened(
                        take(&bound.parser_editors.page, "a parser"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                }),
            )
        }),
        ("select parser", || {
            let (dir, store, _) = crate::parser_editors::setup();
            let (ui, bound) = client(&store);
            crate::common::main_menu::choose(
                &ui,
                &["network", "downloaders"],
                "url class links\u{2026}",
            );
            let links = take(&bound.parser_editors.links, "url class links");
            (
                store,
                Box::new(move || {
                    let _ = (&dir, &ui);
                    links.invoke_row_clicked(0, false, false);
                    links.invoke_action("pick-parser".into());
                    opened(
                        take(&bound.parser_editors.picker, "the parser picker"),
                        |w| w.invoke_answered(true),
                        |w| w.invoke_answered(false),
                    )
                }),
            )
        }),
        ("login script", || {
            let (dir, store, _) = crate::login_workflows::store();
            let (ui, bound) = client(&store);
            crate::common::main_menu::choose(&ui, &["network", "logins"], "login scripts\u{2026}");
            let list = take(&bound.login_workflows.scripts, "login scripts");
            (
                store,
                Box::new(move || {
                    let _ = (&dir, &ui);
                    list.invoke_row_clicked(0, false, false);
                    list.invoke_action("edit".into());
                    opened(
                        take(&bound.login_workflows.script, "a login script"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                }),
            )
        }),
        ("login credential definition", || {
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
                    let _ = (&dir, &list, &ui);
                    script.invoke_credential_clicked(0, false, false);
                    script.invoke_action("edit-credential".into());
                    opened(
                        take(&bound.login_workflows.definition, "a credential definition"),
                        |w| w.invoke_action("apply".into()),
                        |w| w.invoke_action("cancel".into()),
                    )
                }),
            )
        }),
        ("delete files", || {
            let (dirs, store) = crate::options_window::store();
            store
                .write(|ctx| {
                    let mut prefs: hydrus_store::settings::DeletionPreferences =
                        hydrus_store::settings::get(ctx.conn())?;
                    prefs.advanced = true;
                    hydrus_store::settings::set(ctx.conn(), &prefs)
                })
                .unwrap();
            let (ui, bound) = client(&store);
            let fixture = hydrus_testkit::fixture_json("files_trash.json");
            let files = crate::options_remember_action::pair(
                &store,
                fixture["advanced"][0]["hash"].as_str().unwrap(),
            );
            let opener = store.clone();
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    opened(
                        crate::options_remember_action::open(&opener, &files),
                        |w| w.invoke_accept_deletion(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("frame location", || {
            let (dirs, store) = crate::options_window::store();
            let (ui, bound) = client(&store);
            let options = options_page(&ui, &bound, "gui");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    let rows = options.get_frame_rows();
                    let at = (0..rows.row_count())
                        .position(|i| {
                            rows.row_data(i).unwrap().cells.row_data(0).unwrap() == "media_viewer"
                        })
                        .unwrap();
                    options.invoke_frame_clicked(at as i32, false, false);
                    options.invoke_frame_action("edit".into());
                    opened(
                        take(&bound.options_frame_child, "the frame location editor"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("manage ratings", || {
            let (dirs, store) = crate::options_window::store();
            let (ui, bound) = client(&store);
            ui.invoke_search_edited("system:everything".into());
            ui.invoke_search_accepted();
            (
                store,
                Box::new(move || {
                    let _ = &dirs;
                    ui.invoke_thumbnail_clicked(0, false, false);
                    opened(
                        crate::manage_ratings::open(&ui, &bound, 0),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("manage api keys", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            (
                store,
                Box::new(move || {
                    let _ = &dirs;
                    let (_review, keys) = api_keys(&ui, &bound);
                    opened(
                        keys,
                        |w| w.invoke_apply_clicked(),
                        |w| w.invoke_cancel_clicked(),
                    )
                }),
            )
        }),
        // (batch 6: the last of the ordinary dialogs)
        ("edit datetime", || {
            let (dirs, store) = crate::options_window::store();
            let (ui, bound) = client(&store);
            ui.invoke_search_edited("system:everything".into());
            ui.invoke_search_accepted();
            ui.invoke_thumbnail_clicked(0, false, false);
            let times = crate::manage_times::open(&ui, &bound, 0);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    times.invoke_time_clicked(0);
                    opened(
                        crate::manage_times::editor(&bound),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
        ("edit bandwidth rules", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            crate::common::main_menu::choose(
                &ui,
                &["network", "data"],
                "review bandwidth usage and edit rules",
            );
            let review = hydrus_gui::network_data_window::last_bandwidth().expect("bandwidth");
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    let rules = (0..250)
                        .find_map(|_| {
                            if !review.get_busy() {
                                review.invoke_default_clicked();
                            }
                            spin(std::time::Duration::from_millis(20));
                            hydrus_gui::network_data_window::last_rules()
                        })
                        .expect("a rules editor");
                    // (its apply saves the rules on the review's worker,
                    // then closes it)
                    opened(
                        rules,
                        |w| {
                            w.invoke_apply_clicked();
                            until("the rules are saved", || !w.window().is_visible());
                        },
                        |w| w.invoke_cancel_clicked(),
                    )
                }),
            )
        }),
        ("export url classes", || {
            menu_dialog(&["network", "downloaders"], "url classes\u{2026}", |b| {
                let list = take(&b.downloader_definitions.classes, "url classes");
                list.invoke_row_clicked(0, false, false);
                list.invoke_action("export".into());
                // (`DialogNullipotent`: its close button is its OK)
                opened(
                    take(&b.downloader_definitions.class_exchange.0, "the export"),
                    |w| w.invoke_action("cancel".into()),
                    |w| w.invoke_escape(),
                )
            })
        }),
        ("input tags", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            ui.invoke_new_page();
            ui.invoke_chooser_pressed(4);
            ui.invoke_chooser_pressed(8);
            let editor = crate::import_additional_tags::open_tags_page(&ui, &bound);
            let mine = crate::import_additional_tags::my_tags_row(&editor);
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui, &bound);
                    editor.invoke_edit_additional_tags(mine);
                    opened(
                        hydrus_gui::write_tag_window::last_opened().expect("input tags"),
                        |w| w.invoke_apply(),
                        |w| w.invoke_cancel(),
                    )
                }),
            )
        }),
    ]
}

/// parsers > a parser > its first content parser, kept open: `open` opens a
/// window from the parser or that content parser, as a user does.
fn parser_case(open: fn(&Bound, &hydrus_gui::ParserEditWindow) -> Opened) -> Case {
    let (dir, store, _) = crate::parser_editors::setup();
    let (ui, bound) = client(&store);
    crate::common::main_menu::choose(&ui, &["network", "downloaders"], "parsers\u{2026}");
    let list = take(&bound.parser_editors.list, "parsers");
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = take(&bound.parser_editors.page, "a parser");
    page.invoke_row_clicked(0, false, false);
    page.invoke_action("edit-content".into());
    let content = take(&bound.parser_editors.content, "a content parser");
    (
        store,
        Box::new(move || {
            let _ = (&dir, &ui, &list, &page);
            open(&bound, &content)
        }),
    )
}

/// The content parser's formula, opened by its "formula" button.
fn formula_of(bound: &Bound, content: &hydrus_gui::ParserEditWindow) -> hydrus_gui::FormulaWindow {
    let open = bound.parser_editors.formula.formula.borrow().is_none();
    if open {
        content.invoke_action("formula".into());
    }
    take(&bound.parser_editors.formula.formula, "the formula")
}

/// A client on the `basic` fixture opening a window by `open` on its store.
fn keyed_case(open: fn(&Arc<Store>, &Bound) -> Opened) -> Case {
    let (dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    let opener = store.clone();
    (
        store,
        Box::new(move || {
            let _ = (&dirs, &ui);
            open(&opener, &bound)
        }),
    )
}

/// A client on the `basic` fixture opening a dialog from the main menu.
fn menu_dialog(
    path: &'static [&'static str],
    line: &'static str,
    fetch: fn(&Bound) -> Opened,
) -> Case {
    let (dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    (
        store,
        Box::new(move || {
            let _ = &dirs;
            crate::common::main_menu::choose(&ui, path, line);
            fetch(&bound)
        }),
    )
}

/// services > review, the client api's "manage api keys": the review
/// frame and the keys window it opens (kept open for their children).
fn api_keys(
    ui: &MainWindow,
    bound: &Bound,
) -> (
    hydrus_gui::ServicesReviewWindow,
    hydrus_gui::ClientApiKeysWindow,
) {
    crate::common::main_menu::choose(ui, &["services"], "review");
    let review = take(&bound.services_review, "review services");
    let at = review
        .get_services()
        .iter()
        .position(|s| s.ends_with(": client api"))
        .expect("a client api service");
    review.invoke_selected_service(i32::try_from(at).unwrap());
    review.invoke_manage_api_keys();
    let keys = hydrus_gui::client_api_admin_window::last_opened().expect("api keys");
    (review, keys)
}

/// The `basic` fixture with a second storage location, so database
/// locations can set a location's max size (its directories kept alive).
type LocationDirs = ([tempfile::TempDir; 2], tempfile::TempDir);
fn database_locations_store() -> (LocationDirs, Arc<Store>) {
    let (dirs, store) = crate::subscriptions::store();
    std::fs::create_dir_all(store.dir().join("client_files")).unwrap();
    let other = tempfile::tempdir().unwrap();
    let path = other.path().to_path_buf();
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::storage_locations::add_location(ctx.conn(), &path)
        })
        .unwrap();
    ((dirs, other), store)
}

/// manage subscriptions > edit a subscription > edit its query "blue eyes"
/// (with a file in its log): the edit subscription dialog, its query editor
/// open, kept for the logs it opens.
fn subscription_query(
    store: &Arc<Store>,
    ui: &MainWindow,
    bound: &Bound,
) -> hydrus_gui::EditSubscriptionWindow {
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedType};
    let now = crate::subscriptions::now();
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let settings = hydrus_core::subscriptions::SubscriptionSettings {
                gug_name: "example tag search".into(),
                ..Default::default()
            };
            let id =
                hydrus_store::subscriptions::create_subscription(conn, "zz artist", &settings)?
                    .unwrap();
            let state = hydrus_core::subscriptions::QueryState::new("blue eyes");
            let blue = hydrus_store::subscriptions::add_query(conn, id, &state, now)?;
            let seed = NewFileSeed {
                seed_type: SeedType::Url,
                data: "https://booru.example/post/1".into(),
                data_for_comparison: "https://booru.example/post/1".into(),
                source_time: None,
                referral_url: None,
                meta: FileSeedMeta::default(),
            };
            queues::add_file_seeds(conn, blue, &[seed], false, now)?;
            Ok(())
        })
        .unwrap();
    let list = crate::subscriptions::open_dialog(ui, bound);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let dialog = take(&bound.edit_subscription, "edit subscription");
    // ("not found: example tag search": the basic fixture has no downloaders)
    dialog.invoke_chosen(0);
    let rows = dialog.get_rows();
    let blue = (0..rows.row_count())
        .position(|r| rows.row_data(r).unwrap().cells.row_data(0).unwrap() == "blue eyes")
        .expect("the query");
    dialog.invoke_row_clicked(i32::try_from(blue).unwrap(), false, false);
    dialog.invoke_edit_query();
    assert!(dialog.get_query_has_logs());
    dialog
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
    frame_of(store, "regular_dialog")
}

fn frame_of(store: &Store, key: &str) -> FrameLocation {
    store
        .read(hydrus_store::settings::get::<WindowSettings>)
        .unwrap()
        .frame(key)
        .unwrap()
        .clone()
}

fn keep(store: &Store, frame: FrameLocation) {
    keep_as(store, "regular_dialog", frame);
}

fn keep_as(store: &Store, key: &'static str, frame: FrameLocation) {
    store
        .write(move |ctx| {
            let mut settings: WindowSettings = hydrus_store::settings::get(ctx.conn())?;
            settings.set_frame(key, frame);
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
}

/// The cases the reference opens by a frame key of their own: their
/// `DialogEdit` keeps that frame, as `regular_dialog`'s keeps its own.
const KEYED: &[(&str, &str)] = &[
    ("string processor", "deeply_nested_dialog"),
    ("string match", "deeply_nested_dialog"),
    ("string converter", "deeply_nested_dialog"),
    ("conversion", "deeply_nested_dialog"),
    ("formula", "deeply_nested_dialog"),
    ("formula rule", "deeply_nested_dialog"),
    ("edit parser", "deeply_nested_dialog"),
    ("login script", "deeply_nested_dialog"),
    ("login credential definition", "deeply_nested_dialog"),
    ("select parser", "quick_select_dialog"),
    ("delete files", "quick_select_dialog"),
    ("frame location", "quick_entry_dialog"),
];

/// The cases the reference doesn't size or place (an old
/// `ClientGUIDialogs.Dialog`; a panel of review services): whatever ends
/// them keeps nothing, as the recording's cancel.
const PLAIN: &[&str] = &["manage ratings", "manage api keys", "input tags"];

fn key_of(name: &str) -> &'static str {
    KEYED
        .iter()
        .find(|(n, _)| *n == name)
        .map_or("regular_dialog", |(_, key)| key)
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

/// The `DialogNullipotent` cases (their "apply" is the close button).
const NULLIPOTENT: &[&str] = &[
    "export to png",
    "export to pngs",
    "waiting for API access permissions request",
    "database locations",
    "a subscription query's file log",
    "a subscription query's search log",
    "export url classes",
];

/// The cases whose clients set up the headless platform themselves.
const INITS_ITSELF: &[&str] = &[
    "merge options",
    "media view options",
    "tag filter",
    "edit rules",
    "edit rule",
    "edit comparator",
];

/// Each window, opened afresh three times: Escape and cancel keep nothing,
/// apply keeps the size it was given.
// leaf: audit-options-geometry
#[test]
fn every_helper_dialog_keeps_its_frame_only_when_applied() {
    each_case(|name, (store, open)| {
        let recorded = hydrus_testkit::fixture_json("regular_dialog.json");
        let baseline = recorded_frame(&recorded["baseline"]);
        let moved = pair(&recorded["moved"]["size"]).unwrap();
        // (a `DialogNullipotent` replays the recording's: its close button
        // is its OK, and Escape, as its `keyPressEvent`, is its window's
        // close)
        let nullipotent = NULLIPOTENT.contains(&name);
        let plain = PLAIN.contains(&name);
        let key = key_of(name);
        let case = |action: &str| {
            let (kind, action) = match (nullipotent, action) {
                (false, _) if plain => ("edit", "cancel"),
                (false, action) => ("edit", action),
                (true, "apply") => ("nullipotent", "close_button"),
                (true, _) => ("nullipotent", "window_close"),
            };
            recorded["lifecycle"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["kind"] == kind && c["action"] == action)
                .unwrap()
                .clone()
        };
        for (how, recorded) in [
            ("escape", case("escape")),
            ("cancel", case("cancel")),
            ("apply", case("apply")),
        ] {
            keep_as(&store, key, baseline.clone());
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
            assert_eq!(frame_of(&store, key), after, "{name}: {how} ({key})");
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
        // (nor does the window's own close end it under its child)
        merge
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(merge.window().is_visible(), "merge: its close waits too");
        // (and once the child has gone, Escape reaches it again)
        filter.invoke_cancel();
        escape(merge.window());
        assert!(
            !merge.window().is_visible(),
            "merge: Escape again cancels it"
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
    step.invoke_action("edit-content".into());
    assert!(step.get_child_open(), "login step: a content parser opens");
    escape(step.window());
    assert!(step.window().is_visible(), "login step: waits on its child");

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
    escape(edit.window());
    assert!(
        !edit.window().is_visible(),
        "edit service: Escape again cancels it"
    );
    escape(manage.window());
    assert!(
        !manage.window().is_visible(),
        "manage services: once its child is gone"
    );

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
    formulae.invoke_row_clicked(0, false, false);
    formulae.invoke_exchange(1);
    let png = hydrus_gui::png_export_window::last().expect("export to png opens");
    escape(formulae.window());
    assert!(
        formulae.window().is_visible() && png.window().is_visible(),
        "formulae: waits on its export"
    );
    escape(png.window());
    assert!(!png.window().is_visible(), "the export: Escape cancels it");
    escape(formulae.window());
    assert!(
        !formulae.window().is_visible(),
        "formulae: Escape again cancels it"
    );

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
    predicate.invoke_cancel();
    edit.invoke_or_action(3);
    let or = take(&bound.favourites.search_or, "its OR editor");
    escape(edit.window());
    assert!(
        edit.window().is_visible() && or.window().is_visible(),
        "favourite: waits on its OR editor"
    );
    or.invoke_cancel();
    escape(edit.window());
    assert!(
        !edit.window().is_visible(),
        "favourite: Escape again cancels it"
    );
    escape(favourites.window());
    assert!(
        !favourites.window().is_visible(),
        "favourites: once its editor is gone"
    );

    // the OR editor: a nested OR editor is its modal child
    let (_dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    ui.invoke_search_edited("parent draft".into());
    ui.invoke_search_or_action(3);
    let or = take(&bound.search_or, "the OR editor");
    // (laid out, as a shown native window is: its search input is made then)
    widgets::lay_out(or.window(), 600.0, 400.0);
    or.invoke_or_action(3);
    let nested_slot = bound.search_or.child().expect("a nested OR editor");
    let nested = take(&nested_slot, "a nested OR editor");
    escape(or.window());
    assert!(
        or.window().is_visible() && nested.window().is_visible(),
        "OR: waits on its nested editor"
    );
    nested.invoke_cancel();
    escape(or.window());
    assert!(!or.window().is_visible(), "OR: Escape again cancels it");
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

/// A frame opened, and how its close button ends it (`None`: it has none).
struct Framed {
    window: Box<dyn crate::frame_geometry::Shown>,
    close: Option<Box<dyn Fn()>>,
}

fn framed<C: ComponentHandle + 'static>(window: C, close: impl Fn(&C) + 'static) -> Framed {
    let c = window.clone_strong();
    Framed {
        window: Box::new(window),
        close: Some(Box::new(move || close(&c))),
    }
}

/// The store a client shows, and how a user opens the frame in it.
type FrameCase = (Arc<Store>, Box<dyn Fn() -> Framed>);
type NamedFrame = (&'static str, fn() -> FrameCase);

/// A client on the `basic` fixture opening a frame from the main menu.
fn menu_frame(
    path: &'static [&'static str],
    line: &'static str,
    fetch: fn(&Bound) -> Framed,
) -> FrameCase {
    let (dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    (
        store,
        Box::new(move || {
            let _ = &dirs;
            crate::common::main_menu::choose(&ui, path, line);
            fetch(&bound)
        }),
    )
}

/// Every `FrameThatTakesScrollablePanel` on the helper.
fn frames() -> Vec<NamedFrame> {
    vec![
        ("import url classes", || {
            menu_frame(&["network", "downloaders"], "url classes\u{2026}", |b| {
                take(&b.downloader_definitions.classes, "url classes")
                    .invoke_action("import".into());
                framed(
                    take(&b.downloader_definitions.class_exchange.0, "the import"),
                    |w| w.invoke_action("cancel".into()),
                )
            })
        }),
        ("tag display sync", || {
            menu_frame(
                &["tags", "sync"],
                "review current sibling/parent sync",
                |_| {
                    framed(
                        hydrus_gui::tag_sync_review_opened().expect("tag sync opens"),
                        |w| w.invoke_close_clicked(),
                    )
                },
            )
        }),
        ("migrate tags", || {
            menu_frame(&["tags"], "migrate\u{2026}", |b| {
                framed(take(&b.tag_migration, "tag migration"), |w| {
                    w.invoke_close_clicked();
                })
            })
        }),
        ("file maintenance", || {
            menu_frame(
                &["database", "file maintenance"],
                "manage scheduled jobs\u{2026}",
                |b| {
                    let slot = b.file_maintenance.as_ref().unwrap().slot();
                    framed(take(&slot, "file maintenance"), |w| {
                        w.invoke_close_clicked();
                    })
                },
            )
        }),
        ("bandwidth", || {
            menu_frame(
                &["network", "data"],
                "review bandwidth usage and edit rules",
                |_| {
                    framed(
                        hydrus_gui::network_data_window::last_bandwidth().expect("bandwidth"),
                        |w| w.invoke_close_clicked(),
                    )
                },
            )
        }),
        ("network jobs", || {
            menu_frame(&["network", "data"], "review current network jobs", |_| {
                framed(
                    hydrus_gui::network_data_window::last_jobs().expect("network jobs"),
                    |w| w.invoke_close_clicked(),
                )
            })
        }),
        ("vacuum review", || {
            menu_frame(
                &["database", "db maintenance"],
                "review vacuum data\u{2026}",
                |b| {
                    framed(b.vacuum_review().expect("vacuum review"), |w| {
                        w.invoke_close_clicked();
                    })
                },
            )
        }),
        ("review files to import", || {
            menu_frame(&["file"], "import files\u{2026}", |b| {
                let window = b
                    .review_imports
                    .borrow()
                    .as_ref()
                    .map(|(w, _)| w.clone_strong())
                    .expect("review imports");
                framed(window, |w| w.invoke_cancel())
            })
        }),
        ("what is this object", || {
            menu_frame(&["help", "debug"], "what is this object?", |_| {
                let window = hydrus_gui::what_is_this_window::windows()
                    .last()
                    .expect("what is this")
                    .clone_strong();
                framed(window, |w| w.invoke_close_clicked())
            })
        }),
        ("detailed file metadata", || {
            let (dirs, store) = crate::subscriptions::store();
            let ui = MainWindow::new().unwrap();
            ui.show().unwrap();
            let bound = Rc::new(bind(&ui, Pages::single(SearchPage::new(store.clone()))));
            ui.invoke_search_edited("system:everything".into());
            ui.invoke_search_accepted();
            (
                store,
                Box::new(move || {
                    let _ = &dirs;
                    ui.invoke_thumbnail_clicked(0, false, false);
                    ui.invoke_thumbnail_menu_requested(0);
                    let menu = ui.get_thumbnail_menu();
                    assert!(menu.info_is_menu && menu.info_metadata >= 0);
                    ui.invoke_menu_chosen(menu.info_metadata);
                    framed(take(&bound.embedded_metadata, "metadata"), |w| {
                        w.invoke_dismissed();
                    })
                }),
            )
        }),
        ("auto-resolution review", || {
            let crate::auto_resolution_review::Opened {
                _legacy: legacy,
                _dir: dir,
                store,
                ui,
                bound,
                ..
            } = crate::auto_resolution_review::opened();
            let name = hydrus_testkit::fixture_json("auto_resolution_review.json")["reviewed"]
                .as_str()
                .unwrap()
                .to_owned();
            (
                store,
                Box::new(move || {
                    let _ = (&legacy, &dir);
                    framed(
                        crate::auto_resolution_review::review(&ui, &bound, &name),
                        |w| w.invoke_close_window(),
                    )
                }),
            )
        }),
        ("login test result", || {
            let (dir, store, _) = crate::login_workflows::store();
            let (ui, bound) = client(&store);
            let slot = hydrus_gui::login_test_window::ResultSlot::default();
            (
                store,
                Box::new(move || {
                    let _ = (&dir, &ui, &bound);
                    let fixture = hydrus_testkit::fixture_json("login_execution.json");
                    let result = crate::login_workflows::recorded_result(&fixture[0]["results"][0]);
                    framed(
                        hydrus_gui::login_test_window::open_result(&result, &slot).unwrap(),
                        |w| w.invoke_action("close".into()),
                    )
                }),
            )
        }),
        ("review session cookies", || {
            menu_frame(&["network", "data"], "review session cookies", |b| {
                framed(take(&b.network_sessions.browser, "session cookies"), |w| {
                    w.invoke_cancel_clicked();
                })
            })
        }),
        ("review network session", || {
            let (dirs, store) = crate::subscriptions::store();
            let (ui, bound) = client(&store);
            crate::common::main_menu::choose(&ui, &["network", "data"], "review session cookies");
            let browser = take(&bound.network_sessions.browser, "session cookies");
            browser.invoke_add_clicked();
            let edit = hydrus_gui::network_sessions_window::last_edit_opened().unwrap();
            edit.set_domain("example.com".into());
            edit.invoke_apply_clicked();
            (
                store,
                Box::new(move || {
                    let _ = (&dirs, &ui);
                    browser.invoke_row_clicked(0, false, false);
                    browser.invoke_edit_clicked();
                    framed(
                        take(&bound.network_sessions.cookies, "network session"),
                        |w| {
                            w.invoke_cancel_clicked();
                        },
                    )
                }),
            )
        }),
    ]
}

fn spin(time: std::time::Duration) {
    let until = std::time::Instant::now() + time;
    while std::time::Instant::now() < until {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Each frame, as the recording's `FrameThatTakesScrollablePanel` cases: a
/// moment after the user resizes it, its frame is kept; its close button,
/// the window's close and a real Escape close it and keep nothing more.
// leaf: audit-options-geometry
#[test]
fn every_helper_frame_keeps_its_frame_as_it_is_resized_and_nothing_as_it_closes() {
    let recorded = hydrus_testkit::fixture_json("regular_dialog.json");
    let lifecycle = recorded["lifecycle"].as_array().unwrap().clone();
    for (name, make) in frames() {
        let lifecycle = lifecycle.clone();
        let (baseline, moved) = (
            recorded_frame(&recorded["baseline"]),
            pair(&recorded["moved"]["size"]).unwrap(),
        );
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let _windows = headless::init();
                let (store, open) = make();
                let case = |action: &str| {
                    lifecycle
                        .iter()
                        .find(|c| c["kind"] == "frame" && c["action"] == action)
                        .unwrap()
                        .clone()
                };
                for (how, recorded) in [
                    ("close button", case("close_button")),
                    ("window close", case("window_close")),
                    // (the recording has no frame Escape: the reference's
                    // `keyPressEvent` closes the frame, `self.close()`, as its
                    // window's close does, so Escape replays that case)
                    ("escape", case("window_close")),
                ] {
                    keep(&store, baseline.clone());
                    let shown = open();
                    let window = shown.window.win();
                    assert!(window.is_visible(), "{name}: opened");
                    // the user resizes it; a moment later it is kept
                    window.set_size(slint::LogicalSize::new(moved.0 as f32, moved.1 as f32));
                    let mut after_move = recorded_frame(&recorded["after_move"]);
                    after_move.last_position = Some(windows::state(window).position);
                    // (kept a moment after: waited for, however busy the machine)
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                    while frame(&store) != after_move && std::time::Instant::now() < deadline {
                        spin(std::time::Duration::from_millis(20));
                    }
                    assert_eq!(frame(&store), after_move, "{name}: kept as it was resized");
                    // as it closes, nothing more
                    keep(&store, baseline.clone());
                    match (how, &shown.close) {
                        ("close button", Some(close)) => close(),
                        ("escape", _) => {
                            widgets::lay_out(window, 600.0, 400.0);
                            widgets::key(window, Key::Escape);
                        }
                        _ => window.dispatch_event(slint::platform::WindowEvent::CloseRequested),
                    }
                    assert!(!window.is_visible(), "{name}: {how} closes it");
                    spin(std::time::Duration::from_millis(700));
                    assert_eq!(
                        frame(&store),
                        recorded_frame(&recorded["after"]),
                        "{name}: {how} keeps nothing"
                    );
                }
            })
            .unwrap()
            .join()
            .unwrap_or_else(|_| panic!("{name} failed"));
    }
}

/// A frame's own question takes Escape first (answered no) and its modal
/// child holds it, as their own modal dialogs do in the reference.
#[test]
fn escape_answers_a_frames_question_or_waits_on_its_child_first() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);

    // migrate tags: "Go!" asks first
    crate::common::main_menu::choose(&ui, &["tags"], "migrate\u{2026}");
    let migration = take(&bound.tag_migration, "tag migration");
    migration.invoke_go();
    assert!(!migration.get_question().is_empty());
    escape(migration.window());
    assert!(
        migration.get_question().is_empty(),
        "migrate tags: the question answered no"
    );
    assert!(migration.window().is_visible(), "migrate tags: still open");
    // (its tag filter editor is a modal child)
    migration.invoke_edit_filter(false);
    let filter = hydrus_gui::tag_filter_window::last_opened().expect("a tag filter");
    escape(migration.window());
    migration
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        migration.window().is_visible() && filter.window().is_visible(),
        "migrate tags: waits on its tag filter"
    );
    filter.invoke_cancel();
    escape(migration.window());
    assert!(
        !migration.window().is_visible(),
        "migrate tags: then closed"
    );

    // review vacuum data: "vacuum" asks first
    crate::common::main_menu::choose(
        &ui,
        &["database", "db maintenance"],
        "review vacuum data\u{2026}",
    );
    let vacuum = bound.vacuum_review().expect("vacuum review");
    vacuum.invoke_clicked(0, false, false);
    assert!(vacuum.get_can_vacuum());
    vacuum.invoke_vacuum_clicked();
    assert!(!vacuum.get_question().is_empty());
    escape(vacuum.window());
    assert!(
        vacuum.get_question().is_empty(),
        "vacuum: the question answered no"
    );
    assert!(vacuum.window().is_visible(), "vacuum: still open");
    escape(vacuum.window());
    assert!(!vacuum.window().is_visible(), "vacuum: then closed");

    // bandwidth: a rules editor is its modal child
    crate::common::main_menu::choose(
        &ui,
        &["network", "data"],
        "review bandwidth usage and edit rules",
    );
    let review = hydrus_gui::network_data_window::last_bandwidth().expect("bandwidth");
    // ("reset default bandwidth rules" asks first)
    until("bandwidth loads", || !review.get_busy());
    review.invoke_reset_clicked();
    assert!(!review.get_question().is_empty());
    escape(review.window());
    assert!(
        review.get_question().is_empty(),
        "bandwidth: the question answered no"
    );
    assert!(review.window().is_visible(), "bandwidth: still open");
    let editor = (0..250)
        .find_map(|_| {
            if !review.get_busy() {
                review.invoke_default_clicked();
            }
            spin(std::time::Duration::from_millis(20));
            hydrus_gui::network_data_window::last_rules()
        })
        .expect("a rules editor");
    escape(review.window());
    assert!(
        review.window().is_visible() && editor.window().is_visible(),
        "bandwidth: waits on its rules editor"
    );
    review
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        review.window().is_visible(),
        "bandwidth: its close waits too"
    );
    editor.invoke_cancel_clicked();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while hydrus_gui::network_data_window::last_rules().is_some() {
        assert!(std::time::Instant::now() < until, "the rules editor closes");
        spin(std::time::Duration::from_millis(20));
    }
    escape(review.window());
    assert!(!review.window().is_visible(), "bandwidth: then closed");
}

/// Spin the event loop until `ready`, for at most five seconds.
fn until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ready() {
        assert!(std::time::Instant::now() < deadline, "{what}");
        spin(std::time::Duration::from_millis(20));
    }
}

/// Network jobs: the rules editor its cog opens is a modal child, and once
/// a job's menu is dismissed by a click, Escape reaches the window again.
#[test]
fn network_jobs_wait_on_their_rules_editor_and_take_escape_after_a_menu() {
    let _windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut snapshot = crate::network_job_control::snapshot();
    snapshot.jobs[0].wait = hydrus_store::network_runtime::WaitReason::ServerBandwidth;
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &snapshot))
        .unwrap();
    let slots = hydrus_gui::network_data_window::Slots::default();
    let jobs = hydrus_gui::network_data_window::open_jobs(store.clone(), &slots).unwrap();
    let win = jobs.window();
    widgets::lay_out(win, 1100.0, 590.0);
    until("a job", || jobs.get_rows().row_count() == 1);
    jobs.invoke_row_clicked(0, false, false);
    jobs.invoke_control_menu();
    // (the cog's "edit bandwidth rules")
    jobs.invoke_control_action(100);
    until("a rules editor", || {
        hydrus_gui::network_data_window::last_rules().is_some()
    });
    escape(win);
    win.dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(win.is_visible(), "network jobs: waits on its rules editor");
    hydrus_gui::network_data_window::last_rules()
        .unwrap()
        .invoke_cancel_clicked();
    until("the rules editor closes", || {
        hydrus_gui::network_data_window::last_rules().is_none()
    });
    // a job's menu, dismissed by a click beside it
    jobs.invoke_row_menu(0, 300.0, 200.0);
    assert!(
        jobs.get_menu_panes().row_count() > 0,
        "the job's menu opens"
    );
    widgets::click_at(
        win,
        slint::LogicalPosition::new(20.0, 560.0),
        slint::platform::PointerEventButton::Left,
    );
    until("the menu is dismissed", || {
        jobs.get_menu_panes().row_count() == 0
    });
    assert_eq!(
        jobs.get_menu_panes().row_count(),
        0,
        "the menu is dismissed"
    );
    escape(win);
    assert!(
        !win.is_visible(),
        "network jobs: Escape closes it after the menu"
    );
}

/// Review files to import: "Remove them?" is answered no by Escape, and
/// its import options editor and filename tagging (modal children) hold
/// Escape and the window's close; auto-resolution's "Are you sure?" takes
/// Escape first.
#[test]
fn review_imports_and_auto_resolution_take_escape_for_their_questions_first() {
    std::thread::spawn(|| {
        let _windows = headless::init();
        let (_dirs, store) = crate::subscriptions::store();
        let (ui, bound) = client(&store);
        let work = tempfile::tempdir().unwrap();
        let file = work.path().join("a.png");
        std::fs::copy(hydrus_testkit::fixture_path("media/png_1x1.png"), &file).unwrap();
        let review = || {
            crate::common::main_menu::choose(&ui, &["file"], "import files\u{2026}");
            let review = bound
                .review_imports
                .borrow()
                .as_ref()
                .map(|(w, _)| w.clone_strong())
                .expect("review imports");
            widgets::lay_out(review.window(), 900.0, 700.0);
            review.invoke_path_entered(file.display().to_string().into());
            until("parsed", || {
                !review.get_working() && review.get_rows().row_count() > 0
            });
            review
        };
        // "Remove them?"
        let first = review();
        first.invoke_row_clicked(0, false, false);
        widgets::key(first.window(), Key::Delete);
        assert!(first.get_asking_remove(), "review imports: asks to remove");
        escape(first.window());
        assert!(!first.get_asking_remove(), "review imports: answered no");
        assert_eq!(first.get_rows().row_count(), 1, "nothing removed");
        assert!(first.window().is_visible(), "review imports: still open");
        // its import options editor
        first.invoke_import_options();
        assert!(bound.review_import_options.borrow().is_some());
        escape(first.window());
        first
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(
            first.window().is_visible(),
            "review imports: waits on its import options"
        );
        // (its cancel ends both)
        first.invoke_cancel();
        assert!(!first.window().is_visible());
        // its filename tagging
        let second = review();
        second.invoke_add_tags();
        assert!(bound.filename_tagging.borrow().is_some());
        escape(second.window());
        second
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(
            second.window().is_visible(),
            "review imports: waits on its filename tagging"
        );
        second.invoke_cancel();
        assert!(!second.window().is_visible());
    })
    .join()
    .unwrap();

    let _windows = headless::init();
    let crate::auto_resolution_review::Opened { ui, bound, .. } =
        crate::auto_resolution_review::opened();
    let name = hydrus_testkit::fixture_json("auto_resolution_review.json")["reviewed"]
        .as_str()
        .unwrap()
        .to_owned();
    let window = crate::auto_resolution_review::review(&ui, &bound, &name);
    // approve one, then undo it from the actioned tab: "Are you sure?"
    window.invoke_row_clicked(0, false, false);
    window.invoke_approve();
    crate::auto_resolution_review::settle(&window);
    window.invoke_tab_chosen(1);
    window.invoke_row_clicked(0, false, false);
    window.invoke_undo();
    assert!(window.get_asking(), "auto-resolution: asks first");
    escape(window.window());
    assert!(!window.get_asking(), "auto-resolution: answered no");
    assert!(window.window().is_visible(), "auto-resolution: still open");
    escape(window.window());
    assert!(
        !window.window().is_visible(),
        "auto-resolution: then closed"
    );
}

/// The downloader, login, network and API dialogs: a question or inline
/// editor within takes Escape first, and a child the reference opens
/// modally holds Escape and the window's close.
#[test]
fn escape_answers_the_downloader_login_and_api_dialogs_sub_states_first() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    let close = |w: &slint::Window| w.dispatch_event(slint::platform::WindowEvent::CloseRequested);

    // manage url classes: its definition editor is a modal child; once a
    // class is added, a cancel or Escape asks before discarding it
    crate::common::main_menu::choose(&ui, &["network", "downloaders"], "url classes\u{2026}");
    let list = take(&bound.downloader_definitions.classes, "url classes");
    list.invoke_action("add".into());
    let edit = take(&bound.downloader_definitions.class_edit, "edit url class");
    escape(list.window());
    close(list.window());
    assert!(
        list.window().is_visible() && edit.window().is_visible(),
        "url classes: waits on its editor"
    );
    edit.invoke_action("apply".into());
    assert!(!edit.window().is_visible());
    escape(list.window());
    assert!(
        !list.get_question().is_empty(),
        "url classes: asks before discarding"
    );
    assert!(list.window().is_visible());
    escape(list.window());
    assert!(list.get_question().is_empty(), "url classes: answered no");
    assert!(list.window().is_visible(), "url classes: still open");
    close(list.window());
    assert!(
        !list.get_question().is_empty(),
        "url classes: its close asks too"
    );
    list.invoke_answered(true);
    assert!(
        !list.window().is_visible(),
        "url classes: discarded and closed"
    );

    // manage tag display: its tag filter is a modal child
    crate::common::main_menu::choose(&ui, &["tags"], "display/search\u{2026}");
    let display = take(&bound.tag_display, "tag display");
    display.invoke_filter(false);
    assert!(display.get_child_open());
    escape(display.window());
    close(display.window());
    assert!(
        display.window().is_visible(),
        "tag display: waits on its filter"
    );
    display.invoke_cancel();
    assert!(!display.window().is_visible());

    // manage http headers: its value editor is a modal child
    crate::common::main_menu::choose(&ui, &["network", "data"], "manage http headers\u{2026}");
    let headers = take(&bound.network_sessions.headers, "http headers");
    headers.invoke_row_clicked(0, false, false);
    headers.invoke_edit_clicked();
    let header = hydrus_gui::network_sessions_window::last_edit_opened().expect("edit header");
    escape(headers.window());
    close(headers.window());
    assert!(
        headers.window().is_visible() && header.window().is_visible(),
        "http headers: waits on its editor"
    );
    headers.invoke_cancel_clicked();
    assert!(!headers.window().is_visible() && !header.window().is_visible());

    // edit api access permissions: "change key" takes Escape first; its
    // tag filter is a modal child
    let (_review, keys) = api_keys(&ui, &bound);
    keys.invoke_add_clicked();
    let permissions =
        hydrus_gui::client_api_admin_window::last_edit_opened().expect("api permissions");
    permissions.invoke_change_key();
    assert!(permissions.get_changing_key());
    escape(permissions.window());
    assert!(
        !permissions.get_changing_key(),
        "api permissions: key editing cancelled"
    );
    assert!(
        permissions.window().is_visible(),
        "api permissions: still open"
    );
    // (the tag filter is for "search for and fetch files" alone)
    permissions.set_permits_everything(false);
    permissions.invoke_full_toggled();
    permissions.invoke_permission_toggled(3, true);
    permissions.invoke_edit_filter();
    assert!(permissions.get_filtering());
    escape(permissions.window());
    close(permissions.window());
    assert!(
        permissions.window().is_visible(),
        "api permissions: waits on its filter"
    );
    permissions.invoke_cancel_clicked();
    assert!(!permissions.window().is_visible());

    // manage regex favourites: an inline row editor takes Escape first
    let (_odirs, ostore) = crate::options_window::store();
    let (oui, obound) = client(&ostore);
    let options = options_page(&oui, &obound, "regex favourites");
    options.invoke_regex_favourites_clicked();
    let regex = hydrus_gui::regex_favourites_window::last_opened().expect("regex favourites");
    regex.invoke_action("add".into());
    assert!(regex.get_editing());
    escape(regex.window());
    assert!(
        !regex.get_editing(),
        "regex favourites: the row editor cancelled"
    );
    assert!(regex.window().is_visible(), "regex favourites: still open");
    escape(regex.window());
    assert!(
        !regex.window().is_visible(),
        "regex favourites: then cancelled"
    );

    // manage login scripts: "delete" asks first; its script editor is a
    // modal child
    let (_ldir, lstore, _) = crate::login_workflows::store();
    let (lui, lbound) = client(&lstore);
    crate::common::main_menu::choose(&lui, &["network", "logins"], "login scripts\u{2026}");
    let scripts = take(&lbound.login_workflows.scripts, "login scripts");
    scripts.invoke_row_clicked(0, false, false);
    scripts.invoke_action("delete".into());
    assert!(scripts.get_deleting());
    escape(scripts.window());
    assert!(!scripts.get_deleting(), "login scripts: delete answered no");
    assert!(scripts.window().is_visible(), "login scripts: still open");
    scripts.invoke_action("add".into());
    let script = take(&lbound.login_workflows.script, "a login script");
    escape(scripts.window());
    close(scripts.window());
    assert!(
        scripts.window().is_visible() && script.window().is_visible(),
        "login scripts: waits on its editor"
    );
    scripts.invoke_action("cancel".into());
    assert!(!scripts.window().is_visible() && !script.window().is_visible());

    // manage logins: its credentials editor is a modal child
    crate::common::main_menu::choose(&lui, &["network", "logins"], "logins\u{2026}");
    let domains = take(&lbound.login_workflows.domains.domains, "manage logins");
    domains.invoke_row_clicked(0, false, false);
    domains.invoke_action("credentials".into());
    let credentials = take(&lbound.login_workflows.domains.credentials, "edit login");
    escape(domains.window());
    close(domains.window());
    assert!(
        domains.window().is_visible() && credentials.window().is_visible(),
        "manage logins: waits on its credentials"
    );
    domains.invoke_action("cancel".into());
    assert!(!domains.window().is_visible() && !credentials.window().is_visible());
}

/// The window's own close answers a question or closes an inline editor up
/// within it first, as Escape does (the reference's are modal dialogs of
/// their own, which the parent's close can't pass), and only the next close
/// ends the window.
#[test]
fn a_windows_close_answers_its_question_or_inline_editor_first() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    let close = |w: &slint::Window| w.dispatch_event(slint::platform::WindowEvent::CloseRequested);

    // review vacuum data: its "vacuum" question
    crate::common::main_menu::choose(
        &ui,
        &["database", "db maintenance"],
        "review vacuum data\u{2026}",
    );
    let vacuum = bound.vacuum_review().expect("vacuum review");
    vacuum.invoke_clicked(0, false, false);
    vacuum.invoke_vacuum_clicked();
    assert!(!vacuum.get_question().is_empty());
    close(vacuum.window());
    assert!(
        vacuum.get_question().is_empty(),
        "vacuum: its close answers no"
    );
    assert!(vacuum.window().is_visible(), "vacuum: still open");
    close(vacuum.window());
    assert!(!vacuum.window().is_visible(), "vacuum: then closed");

    // manage url classes: applied after a change, nothing is asked; a
    // cancel with a class added asks, and the close answers that no
    crate::common::main_menu::choose(&ui, &["network", "downloaders"], "url classes\u{2026}");
    let list = take(&bound.downloader_definitions.classes, "url classes");
    list.invoke_action("add".into());
    take(&bound.downloader_definitions.class_edit, "edit url class").invoke_action("apply".into());
    list.invoke_action("apply".into());
    assert!(!list.window().is_visible(), "url classes: apply never asks");
    crate::common::main_menu::choose(&ui, &["network", "downloaders"], "url classes\u{2026}");
    let list = take(&bound.downloader_definitions.classes, "url classes");
    list.invoke_action("add".into());
    take(&bound.downloader_definitions.class_edit, "edit url class").invoke_action("apply".into());
    list.invoke_action("cancel".into());
    assert!(!list.get_question().is_empty(), "url classes: cancel asks");
    close(list.window());
    assert!(
        list.get_question().is_empty(),
        "url classes: its close answers no"
    );
    assert!(list.window().is_visible());
    list.invoke_action("cancel".into());
    list.invoke_answered(true);
    assert!(!list.window().is_visible());

    // manage gallery url generators: a generator added, its cancel doesn't
    // ask (the reference's `EditGUGsPanel` has no `UserIsOKToCancel`); the
    // nested editor's members chooser (or its "No remaining gugs" notice)
    // is closed by Escape and by the window's close first
    crate::common::main_menu::choose(
        &ui,
        &["network", "downloaders"],
        "gallery url generators\u{2026}",
    );
    let gugs = take(&bound.downloader_definitions.gugs, "gallery url generators");
    gugs.invoke_tab_chosen(1);
    gugs.invoke_action("add".into());
    let nested = take(&bound.downloader_definitions.gug_edit, "a nested generator");
    let chooser_or_notice = |w: &hydrus_gui::DownloaderDefinitionEditWindow| {
        !w.get_chooser_title().is_empty() || !w.get_notice().is_empty()
    };
    nested.invoke_member_action("add".into());
    assert!(
        chooser_or_notice(&nested),
        "nested: the chooser or its notice"
    );
    escape(nested.window());
    assert!(
        !chooser_or_notice(&nested),
        "nested: Escape closes it first"
    );
    assert!(nested.window().is_visible(), "nested: still open");
    nested.invoke_member_action("add".into());
    assert!(chooser_or_notice(&nested));
    close(nested.window());
    assert!(
        !chooser_or_notice(&nested),
        "nested: its close closes it first"
    );
    assert!(nested.window().is_visible(), "nested: still open");
    nested.invoke_action("cancel".into());
    gugs.invoke_action("add".into());
    take(&bound.downloader_definitions.gug_edit, "a generator").invoke_action("apply".into());
    gugs.invoke_action("cancel".into());
    assert!(!gugs.window().is_visible(), "gugs: a cancel never asks");

    // a session's cookies: changed, a cancel, Escape or the close asks
    // before discarding them, Escape answering no
    crate::common::main_menu::choose(&ui, &["network", "data"], "review session cookies");
    let browser = take(&bound.network_sessions.browser, "session cookies");
    browser.invoke_add_clicked();
    let edit = hydrus_gui::network_sessions_window::last_edit_opened().unwrap();
    edit.set_domain("example.com".into());
    edit.invoke_apply_clicked();
    browser.invoke_row_clicked(0, false, false);
    browser.invoke_edit_clicked();
    let cookies = take(&bound.network_sessions.cookies, "network session");
    cookies.invoke_add_clicked();
    let cookie = hydrus_gui::network_sessions_window::last_edit_opened().unwrap();
    cookie.set_name("sid".into());
    cookie.set_value("token".into());
    cookie.invoke_apply_clicked();
    escape(cookies.window());
    assert!(!cookies.get_question().is_empty(), "cookies: Escape asks");
    escape(cookies.window());
    assert!(cookies.get_question().is_empty(), "cookies: answered no");
    close(cookies.window());
    assert!(
        !cookies.get_question().is_empty(),
        "cookies: its close asks"
    );
    cookies.invoke_answered(true);
    assert!(
        !cookies.window().is_visible(),
        "cookies: discarded and closed"
    );

    // manage logins' credentials: the "values are invalid" warning
    let (_ldir, lstore, _) = crate::login_workflows::store();
    let (lui, lbound) = client(&lstore);
    crate::common::main_menu::choose(&lui, &["network", "logins"], "logins\u{2026}");
    let domains = take(&lbound.login_workflows.domains.domains, "manage logins");
    domains.invoke_row_clicked(0, false, false);
    domains.invoke_action("credentials".into());
    let credentials = take(&lbound.login_workflows.domains.credentials, "edit login");
    credentials.invoke_edited(0, "1?!".into());
    credentials.invoke_action("apply".into());
    assert!(
        !credentials.get_question().is_empty(),
        "credentials: warned"
    );
    close(credentials.window());
    assert!(
        credentials.get_question().is_empty(),
        "credentials: its close goes back"
    );
    assert!(credentials.window().is_visible());
    credentials.invoke_action("cancel".into());
    // and manage logins' delete question
    domains.invoke_row_clicked(0, false, false);
    domains.invoke_action("delete".into());
    assert!(
        !domains.get_question().is_empty(),
        "manage logins: asks to delete"
    );
    close(domains.window());
    assert!(
        domains.get_question().is_empty(),
        "manage logins: its close answers no"
    );
    assert!(domains.window().is_visible());
    domains.invoke_action("cancel".into());

    // manage regex favourites: after its favourites menu is dismissed by a
    // click, Escape still reaches the row editor
    let (_odirs, ostore) = crate::options_window::store();
    let (oui, obound) = client(&ostore);
    let options = options_page(&oui, &obound, "regex favourites");
    options.invoke_regex_favourites_clicked();
    let regex = hydrus_gui::regex_favourites_window::last_opened().expect("regex favourites");
    widgets::lay_out(regex.window(), 700.0, 500.0);
    regex.invoke_action("add".into());
    widgets::click(regex.window(), "favourites \u{25be}");
    assert!(
        regex.get_favourite_panes().row_count() > 0,
        "regex: its menu opens"
    );
    widgets::click_at(
        regex.window(),
        slint::LogicalPosition::new(5.0, 495.0),
        slint::platform::PointerEventButton::Left,
    );
    assert_eq!(
        regex.get_favourite_panes().row_count(),
        0,
        "regex: the menu dismissed"
    );
    escape(regex.window());
    assert!(
        !regex.get_editing(),
        "regex: Escape cancels the row after the menu"
    );
    assert!(regex.window().is_visible());
    close(regex.window());
    assert!(!regex.window().is_visible());
}

/// Filename tagging: its regex menu, opened from within the "configure
/// quick namespace" editor, is closed by Escape first, the editor and its
/// text kept; the next Escape cancels the editor; the window's close then
/// takes the same order.
#[test]
fn filename_tagging_escape_closes_its_regex_menu_before_the_quick_namespace_editor() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    let watched = tempfile::tempdir().unwrap();
    crate::folders::open(&ui, "manage import folders\u{2026}");
    let list = take(&bound.folders.import_list, "import folders");
    list.invoke_add();
    let folder = take(&bound.folders.import_edit, "an import folder");
    folder.set_path(watched.path().to_string_lossy().into_owned().into());
    let mine = (0..folder.get_tagging_choices().row_count())
        .find(|&i| folder.get_tagging_choices().row_data(i).unwrap() == "my tags")
        .unwrap();
    folder.set_tagging_choice(i32::try_from(mine).unwrap());
    folder.invoke_tagging_add();
    let dialog = take(&bound.folders.filename_tagging, "filename tagging");
    dialog.set_advanced(true);
    let win = dialog.window();
    widgets::lay_out(win, 900.0, 700.0);
    let open_menu = || {
        dialog.invoke_rule_action("quick_add".into());
        assert!(dialog.get_rule_child(), "the quick namespace editor opens");
        dialog.set_rule_namespace("series".into());
        widgets::click(win, ".*");
        assert!(
            dialog.get_regex_panes().row_count() > 0,
            "its regex menu opens"
        );
    };
    open_menu();
    escape(win);
    assert_eq!(
        dialog.get_regex_panes().row_count(),
        0,
        "Escape: the menu first"
    );
    assert!(dialog.get_rule_child(), "Escape: the editor kept");
    assert_eq!(
        dialog.get_rule_namespace(),
        "series",
        "Escape: its text kept"
    );
    escape(win);
    assert!(!dialog.get_rule_child(), "Escape: then the editor");
    assert!(win.is_visible(), "Escape: the dialog still open");
    open_menu();
    win.dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert_eq!(
        dialog.get_regex_panes().row_count(),
        0,
        "close: the menu first"
    );
    assert!(dialog.get_rule_child(), "close: the editor kept");
    win.dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!dialog.get_rule_child(), "close: then the editor");
    assert!(win.is_visible(), "close: the dialog still open");
    dialog.invoke_cancel();
}

/// The folder, sidecar, database and rules dialogs: a question or chooser
/// up within takes Escape and the window's close first, and a child the
/// reference opens modally holds both.
#[test]
fn escape_and_close_answer_the_folder_database_and_rules_dialogs_sub_states_first() {
    let close = |w: &slint::Window| w.dispatch_event(slint::platform::WindowEvent::CloseRequested);
    std::thread::spawn(move || {
        let _windows = headless::init();
        let (_dirs, store) = crate::subscriptions::store();
        let (ui, bound) = client(&store);
        // edit import folder: its apply with no path is refused by a notice
        crate::folders::open(&ui, "manage import folders\u{2026}");
        let list = crate::folders::import_list(&bound);
        list.invoke_add();
        let edit = take(&bound.folders.import_edit, "an import folder");
        edit.invoke_apply();
        assert!(edit.get_asking(), "import folder: no path, refused");
        escape(edit.window());
        assert!(
            !edit.get_asking(),
            "import folder: Escape closes the notice"
        );
        assert!(edit.window().is_visible());
        edit.invoke_apply();
        close(edit.window());
        assert!(
            !edit.get_asking(),
            "import folder: its close closes the notice"
        );
        assert!(edit.window().is_visible());
        // the folder list waits on the folder; the folder on its file log
        escape(list.window());
        close(list.window());
        assert!(
            list.window().is_visible(),
            "import folders: waits on its folder"
        );
        edit.invoke_file_log();
        let log = take(&bound.folders.log, "the file log");
        escape(edit.window());
        close(edit.window());
        assert!(
            edit.window().is_visible() && log.window().is_visible(),
            "import folder: waits on its file log"
        );
        log.invoke_close_window();
        // the sidecars: routers wait on a router, a router on its destination
        let watched = tempfile::tempdir().unwrap();
        edit.set_path(watched.path().to_string_lossy().into_owned().into());
        edit.invoke_edit_sidecars();
        let routers = take(&bound.folders.sidecars.routers, "routers");
        escape(edit.window());
        assert!(
            edit.window().is_visible(),
            "import folder: waits on its sidecars"
        );
        routers.invoke_add();
        let router = take(&bound.folders.sidecars.router, "a router");
        escape(routers.window());
        close(routers.window());
        assert!(routers.window().is_visible(), "routers: wait on a router");
        router.invoke_edit_destination();
        let node = take(&bound.folders.sidecars.node, "a destination");
        escape(router.window());
        close(router.window());
        assert!(
            router.window().is_visible() && node.window().is_visible(),
            "router: waits on its destination"
        );
        node.invoke_cancel();
        router.invoke_cancel();
        routers.invoke_cancel();
        edit.invoke_cancel();
        list.invoke_cancel();
        assert!(!list.window().is_visible());
    })
    .join()
    .unwrap();

    std::thread::spawn(move || {
        // edit rules: its "add suggested" chooser is closed first; it waits
        // on a rule's editor, and the rule on a comparator's
        let crate::duplicates_lane_page::Opened {
            _windows,
            _dir,
            ui,
            bound,
            ..
        } = crate::duplicates_lane_page::opened();
        ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
        let list = crate::duplicates_lane_page::list_of(&bound);
        list.invoke_add_suggested();
        assert!(list.get_choosing_suggested());
        escape(list.window());
        assert!(
            !list.get_choosing_suggested(),
            "rules: Escape closes the chooser"
        );
        assert!(list.window().is_visible());
        list.invoke_add_suggested();
        close(list.window());
        assert!(
            !list.get_choosing_suggested(),
            "rules: the close closes it too"
        );
        assert!(list.window().is_visible());
        list.invoke_add();
        let rule = crate::duplicates_lane_page::rule_window(&bound);
        escape(list.window());
        close(list.window());
        assert!(list.window().is_visible(), "rules: wait on a rule");
        rule.set_comparator_kind(crate::duplicates_lane_page::kind_index(
            &rule,
            "test A against B using file info",
        ));
        rule.invoke_comparator_add();
        let comparator = crate::duplicates_lane_page::newest_comparator(&bound);
        escape(rule.window());
        close(rule.window());
        assert!(
            rule.window().is_visible() && comparator.window().is_visible(),
            "rule: waits on its comparator"
        );
        comparator.invoke_cancel();
        rule.invoke_cancel();
        list.invoke_cancel();
        assert!(!list.window().is_visible());
    })
    .join()
    .unwrap();

    // database locations: waits on its max size editor
    let _windows = headless::init();
    let (_dirs, store) = database_locations_store();
    let (ui, _bound) = client(&store);
    crate::database_menu_jobs::from_menu_path(&ui, &[], "locations\u{2026}");
    let window = hydrus_gui::database_locations_window::opened().expect("locations");
    window.invoke_clicked(0, false, false);
    window.invoke_action("max".into());
    let max = hydrus_gui::database_locations_window::max_size_window().expect("edit max size");
    escape(window.window());
    close(window.window());
    assert!(
        window.window().is_visible() && max.window().is_visible(),
        "database locations: waits on its max size editor"
    );
    max.invoke_cancel();
    escape(window.window());
    assert!(
        !window.window().is_visible(),
        "database locations: then cancelled"
    );
}

/// The questions and messages a dialog opens of its own (`GetYesNo`,
/// `ShowWarning`) and its other sub-states, as the #333 review listed them:
/// each holds Escape and the window's close (or takes them first).
#[test]
fn a_dialogs_own_questions_and_messages_hold_it_as_the_references_modal_ones() {
    let close = |w: &slint::Window| w.dispatch_event(slint::platform::WindowEvent::CloseRequested);
    // database locations: "remove" asks, a warning says why not
    std::thread::spawn(move || {
        let _windows = headless::init();
        let (_dirs, store) = database_locations_store();
        let (ui, _bound) = client(&store);
        crate::database_menu_jobs::from_menu_path(&ui, &[], "locations\u{2026}");
        let window = hydrus_gui::database_locations_window::opened().expect("locations");
        window.invoke_clicked(0, false, false);
        window.invoke_action("remove".into());
        let question = hydrus_gui::dialog_question().expect("remove asks");
        escape(window.window());
        close(window.window());
        assert!(
            window.window().is_visible() && question.window().is_visible(),
            "locations: waits on its question"
        );
        // (its own close button, its OK, closes the question with it,
        // answering nothing: the location stays)
        let locations = || {
            store
                .read(hydrus_store::storage_locations::review)
                .unwrap()
                .locations
                .len()
        };
        let before = locations();
        window.invoke_close_window();
        assert!(
            !window.window().is_visible() && !question.window().is_visible(),
            "locations: its close ends its question too"
        );
        question.invoke_chosen(0);
        assert_eq!(locations(), before, "locations: nothing removed after");
        crate::database_menu_jobs::from_menu_path(&ui, &[], "locations\u{2026}");
        let window = hydrus_gui::database_locations_window::opened().expect("locations again");
        // ("set time to run": Escape and cancel keep nothing, the locations
        // waiting on it)
        window.invoke_action("rebalance".into());
        hydrus_gui::database_locations_window::runtime_question()
            .expect("how long")
            .invoke_chosen(3);
        let custom = hydrus_gui::database_locations_window::custom_runtime_window()
            .expect("set time to run");
        escape(window.window());
        assert!(
            window.window().is_visible(),
            "locations: waits on set time to run"
        );
        let before = frame(&store);
        custom
            .window()
            .set_size(slint::LogicalSize::new(612.0, 478.0));
        escape(custom.window());
        assert!(
            !custom.window().is_visible(),
            "set time to run: Escape cancels"
        );
        assert_eq!(frame(&store), before, "set time to run: nothing kept");
        escape(window.window());
        assert!(!window.window().is_visible(), "locations: then cancelled");
    })
    .join()
    .unwrap();

    // a rule's editor: its import's problem message
    std::thread::spawn(move || {
        let crate::duplicates_lane_page::Opened {
            _windows,
            _dir,
            ui,
            bound,
            ..
        } = crate::duplicates_lane_page::opened();
        ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
        let list = crate::duplicates_lane_page::list_of(&bound);
        list.invoke_add();
        let rule = crate::duplicates_lane_page::rule_window(&bound);
        hydrus_gui::set_paster(|| "nonsense".into());
        rule.invoke_comparator_exchange(2);
        let message = hydrus_gui::dialog_question().expect("a problem message");
        escape(rule.window());
        close(rule.window());
        assert!(
            rule.window().is_visible() && message.window().is_visible(),
            "rule: waits on its message"
        );
        message.invoke_escape();
        escape(rule.window());
        assert!(!rule.window().is_visible(), "rule: then cancelled");
        list.invoke_cancel();
    })
    .join()
    .unwrap();

    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    // edit export folder: "delete from the client" asks first
    crate::folders::open(&ui, "manage export folders\u{2026}");
    let folders = take(&bound.folders.export_list, "export folders");
    folders.invoke_add();
    let export = take(&bound.folders.export_edit, "an export folder");
    export.set_path("/tmp/exported".into());
    export.set_delete_from_client(true);
    export.invoke_apply();
    assert!(export.get_asking(), "export folder: asks to delete");
    escape(export.window());
    assert!(!export.get_asking(), "export folder: Escape answers no");
    export.invoke_apply();
    close(export.window());
    assert!(!export.get_asking(), "export folder: its close answers no");
    assert!(export.window().is_visible());
    export.invoke_cancel();
    folders.invoke_cancel();

    // a subscription query's file log: its menu takes Escape first, and once
    // dismissed by a click, Escape reaches the log again
    let query = subscription_query(&store, &ui, &bound);
    query.invoke_query_log(false);
    let log = take(&bound.folders.log, "the file log");
    widgets::lay_out(log.window(), 900.0, 600.0);
    log.invoke_row_clicked(0, false, false);
    log.invoke_row_menu(0, 50.0, 50.0);
    assert!(log.get_menu_panes().row_count() > 0, "file log: its menu");
    escape(log.window());
    assert_eq!(
        log.get_menu_panes().row_count(),
        0,
        "file log: Escape closes the menu"
    );
    assert!(log.window().is_visible());
    log.invoke_row_menu(0, 50.0, 50.0);
    widgets::click_at(
        log.window(),
        slint::LogicalPosition::new(890.0, 590.0),
        slint::platform::PointerEventButton::Left,
    );
    assert_eq!(
        log.get_menu_panes().row_count(),
        0,
        "file log: the menu dismissed"
    );
    escape(log.window());
    assert!(
        !log.window().is_visible(),
        "file log: Escape after the menu closes it"
    );
    query.invoke_query_cancel();
    query.invoke_cancel();

    // edit import options: its tag filter is a modal child
    let s = crate::import_options_fields::setup();
    let editor = s.editor();
    editor.invoke_kind_clicked(2);
    editor.set_custom_index(1);
    editor.invoke_changed();
    editor.invoke_edit_blacklist();
    let filter = hydrus_gui::tag_filter_window::last_opened().expect("a tag filter");
    escape(editor.window());
    close(editor.window());
    assert!(
        editor.window().is_visible() && filter.window().is_visible(),
        "import options: waits on its tag filter"
    );
    filter.invoke_cancel();
    // (its "paste" menu, a popup: once closed, Escape reaches the dialog)
    widgets::lay_out(editor.window(), 900.0, 620.0);
    widgets::click(editor.window(), "paste");
    assert!(
        widgets::popup_open(editor.window()),
        "import options: its paste menu"
    );
    widgets::close_popups(editor.window());
    escape(editor.window());
    assert!(
        !editor.window().is_visible(),
        "import options: Escape after its menu"
    );

    // the predicate editor (system:hash): its notice holds it, its
    // question takes Escape and the close first
    let page_store = store.clone();
    let pui = MainWindow::new().unwrap();
    pui.show().unwrap();
    let pbound = bind(&pui, Pages::single(SearchPage::new(page_store)));
    pui.invoke_search_edited("".into());
    let i = pui
        .get_suggestions()
        .iter()
        .position(|s| s.text == "system:hash")
        .unwrap();
    pui.invoke_suggestion_chosen(i32::try_from(i).unwrap());
    let hash = take(&pbound.predicate_editor, "the hash editor");
    hash.invoke_text_edited(0, 2, "d41d8cd98f00b204e9800998ecf8427e\nnot a hash".into());
    hash.invoke_pressed(0, 4);
    assert!(
        !hash.get_question().is_empty(),
        "hash: asks to remove what isn't"
    );
    escape(hash.window());
    assert!(hash.get_question().is_empty(), "hash: Escape answers no");
    hash.invoke_pressed(0, 4);
    close(hash.window());
    assert!(hash.get_question().is_empty(), "hash: its close answers no");
    assert!(hash.window().is_visible());
    hash.invoke_pressed(0, 3);
    assert!(hash.get_notice_open(), "hash: a notice");
    escape(hash.window());
    close(hash.window());
    assert!(hash.window().is_visible(), "hash: waits on its notice");
    hash.invoke_cancel();
    assert!(!hash.window().is_visible());
}

/// The deeply nested editors (batch 5): a question or choice up within one
/// takes Escape first, answered no; a child editor it opened holds it, as
/// the reference's modal child dialogs do; a parser changed asks before
/// it is discarded, and the window's close answers that no.
#[test]
fn the_deeply_nested_editors_take_escape_for_their_questions_and_children_first() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let (_ui, _bound) = client(&store);
    let shown = |w: &slint::Window| w.is_visible();

    // the string processor: "add"'s choice, then a step editor open
    let slots = hydrus_gui::string_processor_window::Slots::default();
    let processor = hydrus_gui::string_processor_window::open(
        &store,
        &hydrus_core::url::strings::StringProcessor::default(),
        vec!["example".into()],
        &slots,
        Rc::new(|_| {}),
    )
    .unwrap();
    *slots.processor.borrow_mut() = Some(processor.clone_strong());
    processor.invoke_add();
    assert!(processor.get_asking());
    escape(processor.window());
    assert!(
        !processor.get_asking(),
        "processor: Escape answers its choice"
    );
    assert!(shown(processor.window()), "processor: still open");
    processor.invoke_add();
    processor.invoke_chosen(0);
    let step = take(&slots.step, "a processing step");
    escape(processor.window());
    assert!(
        shown(processor.window()),
        "processor: its step editor holds it"
    );
    assert!(shown(step.window()));
    escape(step.window());
    assert!(!shown(step.window()), "the step: Escape cancels it");
    escape(processor.window());
    assert!(
        !shown(processor.window()),
        "processor: then Escape cancels it"
    );

    // the string converter: its delete question
    let slots = hydrus_gui::string_processor_window::Slots::default();
    slots.set_store(&store);
    let converter = hydrus_gui::string_processor_window::open_converter(
        &hydrus_core::url::strings::StringConverter::default(),
        Some("example".into()),
        &slots,
        Rc::new(|_| {}),
    )
    .unwrap();
    *slots.converter.borrow_mut() = Some(converter.clone_strong());
    converter.invoke_add();
    let conversion = take(&slots.conversion, "a conversion");
    escape(converter.window());
    assert!(
        shown(converter.window()),
        "converter: its conversion holds it"
    );
    conversion.invoke_apply();
    assert!(!shown(conversion.window()));
    converter.invoke_row_clicked(0, false, false);
    converter.invoke_delete();
    assert!(converter.get_asking());
    escape(converter.window());
    assert!(!converter.get_asking(), "converter: Escape answers no");
    assert!(shown(converter.window()));
    assert_eq!(converter.get_rows().row_count(), 1, "nothing deleted");
    escape(converter.window());
    assert!(!shown(converter.window()));

    // a formula: its remove question, then its rule editor open
    let slots = hydrus_gui::formula_window::Slots::default();
    let formula = hydrus_gui::formula_window::open(
        &store,
        &hydrus_gui::formula_editors::new_formula(false),
        hydrus_gui::formula_window::FormulaTestData::default(),
        &slots,
        Rc::new(|_| {}),
    )
    .unwrap();
    *slots.formula.borrow_mut() = Some(formula.clone_strong());
    formula.invoke_row_clicked(0, false, false);
    formula.invoke_delete();
    assert!(formula.get_deleting());
    escape(formula.window());
    assert!(!formula.get_deleting(), "formula: Escape answers no");
    assert!(shown(formula.window()));
    formula.invoke_edit();
    let rule = take(&slots.rule, "a rule");
    escape(formula.window());
    assert!(shown(formula.window()), "formula: its rule editor holds it");
    escape(rule.window());
    assert!(!shown(rule.window()));
    escape(formula.window());
    assert!(!shown(formula.window()));

    // a parser, changed: cancel asks; Escape and the window's close answer no
    let (_dir, store, slots) = crate::parser_editors::setup();
    let list = hydrus_gui::parser_editors_window::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = take(&slots.page, "a parser");
    page.invoke_text_edited(0, "edited page".into());
    escape(page.window());
    assert!(!page.get_question().is_empty(), "parser: Escape asks first");
    assert!(shown(page.window()));
    escape(page.window());
    assert!(page.get_question().is_empty(), "parser: Escape answers no");
    page.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(
        !page.get_question().is_empty(),
        "parser: its close asks too"
    );
    page.invoke_answered(true);
    assert!(!shown(page.window()), "parser: discarded on yes");
    assert!(shown(list.window()));

    // a login script: its remove question
    let (_dir, store, _) = crate::login_workflows::store();
    let slots = hydrus_gui::login_workflows_window::Slots::default();
    let scripts = hydrus_gui::login_workflows_window::open_scripts(&store, &slots).unwrap();
    scripts.invoke_row_clicked(0, false, false);
    scripts.invoke_action("edit".into());
    let script = take(&slots.script, "a login script");
    script.invoke_credential_clicked(0, false, false);
    script.invoke_action("delete-credential".into());
    assert!(!script.get_question().is_empty());
    escape(script.window());
    assert!(
        script.get_question().is_empty(),
        "login script: Escape answers no"
    );
    assert!(shown(script.window()));
    script.invoke_action("edit-credential".into());
    let definition = take(&slots.definition, "a credential definition");
    escape(script.window());
    assert!(
        shown(script.window()),
        "login script: its definition holds it"
    );
    escape(definition.window());
    assert!(!shown(definition.window()));
    escape(script.window());
    assert!(!shown(script.window()));
}

/// A menu opened in a dialog takes Escape first and closes, the dialog
/// staying open; once a menu is dismissed by a click, the keys come back,
/// so the next Escape reaches the dialog. Driven with real clicks and keys
/// in each helper window with a favourites menu.
#[test]
fn a_dialogs_menu_takes_escape_first_and_gives_the_keys_back_once_closed() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let (_ui, _bound) = client(&store);
    let check = |name: &str,
                 win: &slint::Window,
                 open: &dyn Fn(),
                 panes: &dyn Fn() -> usize,
                 inner_escape: &dyn Fn()| {
        widgets::lay_out(win, 900.0, 700.0);
        open();
        assert!(panes() > 0, "{name}: the menu opens");
        escape(win);
        assert_eq!(panes(), 0, "{name}: Escape closes the menu first");
        assert!(win.is_visible(), "{name}: the dialog stays open");
        open();
        // (a click away from the menu dismisses it)
        widgets::click_at(
            win,
            slint::LogicalPosition::new(890.0, 690.0),
            slint::platform::PointerEventButton::Left,
        );
        assert_eq!(panes(), 0, "{name}: a click dismisses it");
        inner_escape();
        escape(win);
        assert!(!win.is_visible(), "{name}: then Escape reaches the dialog");
    };

    // a string match's "favourites ▾"
    let slots = hydrus_gui::string_processor_window::Slots::default();
    hydrus_gui::string_processor_window::open_match(
        &store,
        &hydrus_core::url::strings::StringMatch::any(),
        &slots,
        Rc::new(|_| {}),
    );
    let matcher = take(&slots.step, "a string match");
    // (its regex controls show for a regex match)
    matcher.set_match_type(3);
    matcher.invoke_changed();
    check(
        "string match",
        matcher.window(),
        &|| widgets::click(matcher.window(), "favourites ▾"),
        &|| matcher.get_favourite_panes().row_count(),
        &|| {},
    );
    // the input tags window's tag menu (a tag's context menu)
    let (_dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let editor = crate::import_additional_tags::open_tags_page(&ui, &bound);
    editor.invoke_edit_additional_tags(crate::import_additional_tags::my_tags_row(&editor));
    let tags = hydrus_gui::write_tag_window::last_opened().expect("input tags");
    tags.invoke_edited("menu tag".into());
    tags.invoke_entered();
    check(
        "input tags",
        tags.window(),
        &|| tags.invoke_context_menu(0, 20.0, 20.0),
        &|| tags.get_tag_menu_panes().row_count(),
        &|| {},
    );
}

/// Each frame the options' frame locations list offers, and the tests that
/// drive the windows opening by it against the reference's recordings:
/// `audit-options-geometry`'s scope, frame by frame.
const FRAME_COVERAGE: &[(&str, &str)] = &[
    (
        "main_gui",
        "shell_geometry.rs (save_geometry.json, frame_state.json)",
    ),
    (
        "media_viewer",
        "viewer_window.rs (viewer_close_geometry.json), options_gui_frames.rs",
    ),
    (
        "manage_options_dialog",
        "frame_locations.rs (options_geometry_lifecycle.json)",
    ),
    (
        "regular_dialog",
        "dialog_sweep.rs: every helper dialog and frame (regular_dialog.json); frame_geometry.rs: other windows",
    ),
    (
        "deeply_nested_dialog",
        "dialog_sweep.rs: KEYED cases (regular_dialog.json's DialogEdit cases on their key)",
    ),
    ("quick_entry_dialog", "dialog_sweep.rs: KEYED cases"),
    (
        "quick_select_dialog",
        "dialog_sweep.rs: KEYED cases; frame_geometry.rs: the choice dialogs",
    ),
    (
        "quick_yesno_dialog",
        "frame_geometry.rs: the choice dialogs",
    ),
    (
        "review_services",
        "frame_geometry.rs: named windows (frame_state.json)",
    ),
    ("mr_bones", "frame_geometry.rs: named windows"),
    ("file_history_chart", "frame_geometry.rs: named windows"),
    ("export_files_frame", "frame_geometry.rs: named windows"),
    (
        "manage_subscriptions_dialog",
        "frame_geometry.rs: named windows",
    ),
    (
        "edit_subscription_dialog",
        "frame_geometry.rs: named windows",
    ),
    ("manage_tags_dialog", "frame_geometry.rs: named windows"),
    ("manage_tags_frame", "frame_geometry.rs: named windows"),
    ("manage_urls_dialog", "frame_geometry.rs: named windows"),
    ("manage_times_dialog", "frame_geometry.rs: named windows"),
    ("manage_notes_dialog", "frame_geometry.rs: named windows"),
    ("file_import_status", "frame_geometry.rs: named windows"),
    ("gallery_import_log", "frame_geometry.rs: named windows"),
    (
        "local_import_filename_tagging",
        "frame_geometry.rs: named windows",
    ),
];

/// The frame locations Options lists are exactly the frames this file's
/// coverage table names, so a frame added to the list without a test that
/// drives its windows against the recording fails here; and every other key
/// the sweep replays on is one of them.
// leaf: audit-options-geometry
#[test]
fn every_frame_the_options_list_offers_is_driven_against_the_recording() {
    let _windows = headless::init();
    let (_dirs, store) = crate::options_window::store();
    let (ui, bound) = client(&store);
    let options = options_page(&ui, &bound, "gui");
    let rows = options.get_frame_rows();
    let offered: std::collections::BTreeSet<String> = (0..rows.row_count())
        .map(|i| {
            rows.row_data(i)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect();
    let covered: std::collections::BTreeSet<String> = FRAME_COVERAGE
        .iter()
        .map(|(key, _)| (*key).to_owned())
        .collect();
    assert_eq!(offered, covered, "every listed frame, and only those");
    for (name, key) in KEYED {
        assert!(covered.contains(*key), "{name}: {key}");
    }
    options.invoke_cancel();
}

/// A held job's modal dialog (`DialogNullipotent`, `PopupMessageDialogPanel`)
/// ends through the helper, replaying `regular_dialog.json`'s nullipotent
/// cases: its close button (once the job is cancelled, on yes) and its job
/// finishing are its OK and keep `regular_dialog`; Escape (asking the same
/// question, answered yes) keeps nothing. While it asks, the question holds
/// it, and the dialog's own warning is its own.
// leaf: audit-options-geometry
#[test]
fn a_modal_popups_dialog_keeps_its_frame_by_its_ok_only() {
    use hydrus_store::popups::{self, Job};
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let (_ui, bound) = client(&store);
    let recorded = hydrus_testkit::fixture_json("regular_dialog.json");
    let baseline = recorded_frame(&recorded["baseline"]);
    let moved = pair(&recorded["moved"]["size"]).unwrap();
    let case = |action: &str| {
        recorded["lifecycle"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "nullipotent" && c["action"] == action)
            .unwrap()
            .clone()
    };
    let now = || hydrus_core::time::TimestampMs::now().millis() / 1000;
    let controller = bound.popup_modal.clone();
    let publish = |cancellable: bool| {
        let mut job = Job::new(!cancellable, cancellable, 0.0);
        job.status_title = Some("a long job".into());
        job.held_by_modal = true;
        let key = job.key;
        store
            .write(move |ctx| popups::add(ctx.conn(), &job, now()))
            .unwrap();
        until("the dialog opens", || controller.dialog().is_some());
        let dialog = controller.dialog().unwrap();
        dialog
            .window()
            .set_size(slint::LogicalSize::new(moved.0 as f32, moved.1 as f32));
        (key, dialog)
    };
    let kept = |how: &str, recorded: &Value, window: &slint::Window| {
        let mut after = recorded_frame(&recorded["after"]);
        if after != baseline {
            after.last_position = Some(windows::state(window).position);
        }
        assert_eq!(frame(&store), after, "{how}");
    };
    for how in ["close button", "escape", "job done"] {
        keep(&store, baseline.clone());
        let (key, dialog) = publish(true);
        match how {
            "close button" => {
                dialog.invoke_close_clicked();
                let question = controller.question().expect("it asks");
                // (the question holds it: Escape and its close wait)
                escape(dialog.window());
                assert!(dialog.window().is_visible(), "{how}: held by its question");
                question.invoke_chosen(0);
            }
            "escape" => {
                widgets::lay_out(dialog.window(), 400.0, 200.0);
                escape(dialog.window());
                controller.question().expect("it asks").invoke_chosen(0);
            }
            _ => {
                store
                    .write(move |ctx| {
                        popups::update(ctx.conn(), &key, now(), Job::finish).map(|_| ())
                    })
                    .unwrap();
            }
        }
        until("it closes", || !dialog.window().is_visible());
        let recorded = case(if how == "escape" {
            "window_close"
        } else {
            "close_button"
        });
        kept(how, &recorded, dialog.window());
    }

    // a job that can't be cancelled: Escape warns, in a message of its own
    let (key, dialog) = publish(false);
    widgets::lay_out(dialog.window(), 400.0, 200.0);
    escape(dialog.window());
    let warning = hydrus_gui::dialog_question().expect("it warns");
    assert!(warning.window().is_visible());
    escape(dialog.window());
    assert!(dialog.window().is_visible(), "held by its warning");
    warning.invoke_cancelled();
    store
        .write(move |ctx| popups::update(ctx.conn(), &key, now(), Job::finish).map(|_| ()))
        .unwrap();
    until("it closes itself when done", || {
        !dialog.window().is_visible()
    });
}

/// The last ordinary dialogs' sub-states (batch 6) take Escape first: a
/// downloader import's notice, then the import itself.
#[test]
fn the_last_dialogs_take_escape_for_their_questions_and_children_first() {
    let _windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let (ui, bound) = client(&store);

    // import url classes: a bad paste's notice is answered by Escape first
    crate::common::main_menu::choose(&ui, &["network", "downloaders"], "url classes\u{2026}");
    let list = take(&bound.downloader_definitions.classes, "url classes");
    list.invoke_action("import".into());
    let import = take(&bound.downloader_definitions.class_exchange.0, "the import");
    import.set_text("not a package".into());
    import.invoke_action("review".into());
    widgets::lay_out(import.window(), 700.0, 500.0);
    if !import.get_question().is_empty() {
        escape(import.window());
        assert!(import.get_question().is_empty(), "import: its notice first");
        assert!(import.window().is_visible());
    }
    escape(import.window());
    assert!(
        !import.window().is_visible(),
        "import: then Escape closes it"
    );
    list.invoke_action("cancel".into());
}
