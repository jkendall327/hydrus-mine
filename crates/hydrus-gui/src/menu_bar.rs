//! The main window's menu bar in the window: its menus as [`main_menu`]
//! builds them from what the store and the pages say, opened, moved
//! through and chosen from as Qt's are, and what each entry does.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_core::numbers::human_int;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::main_menu::{self, Command, Facts, LineKind, MenuKey, OpenMenus};
use crate::pages::Pages;
use crate::{ChangePages, MainWindow, MenuLine, MenuPane, MenuTitle};

/// Ask a question, then (on yes) do this.
pub(crate) type Ask = Rc<dyn Fn(String, Rc<dyn Fn()>)>;

/// The menus' lines as last shown, and the models showing them.
type ShownLines = Rc<RefCell<Vec<(Vec<MenuLine>, ModelRc<MenuLine>)>>>;

/// What the menu bar works with.
pub(crate) struct Hooks {
    pub debug_long_popup: crate::debug_long_popup::Control,
    pub debug_session_reload: crate::debug_session_reload::Control,
    pub debug_fetch: crate::debug_fetch::Control,
    pub force_idle: crate::force_idle::Control,
    pub quick_export_directory: crate::quick_export_directory::Control,
    pub darkmode: Rc<dyn Fn()>,
    pub sidebar_layout: Rc<dyn Fn(hydrus_gui_model::page_layout::Action)>,
    /// Open the siblings or parents editor.
    pub tag_display: Rc<dyn Fn(bool)>,
    pub tag_relationships: Rc<dyn Fn(hydrus_store::display::RelationKind)>,
    /// Open tag migration without a selected-file restriction.
    pub tag_migrate: Rc<dyn Fn()>,
    pub pages: Rc<RefCell<Pages>>,
    pub network_data: crate::network_data_window::Slots,
    pub change_pages: ChangePages,
    pub ask: Ask,
    /// Show the page shown again, its files changed.
    pub reshow: Rc<dyn Fn()>,
    /// Open the options window.
    pub options: Rc<dyn Fn()>,
    /// Open the manage subscriptions dialog.
    pub manage_subscriptions: Rc<dyn Fn()>,
    /// Open URL class or gallery URL generator definition editors.
    pub manage_downloader_definitions: Rc<dyn Fn(bool)>,
    pub manage_login_scripts: Rc<dyn Fn()>,
    pub manage_logins: Rc<dyn Fn()>,
    pub manage_downloader_display: Rc<dyn Fn()>,
    /// Open native parser definitions or URL-class links.
    pub manage_parsers: Rc<dyn Fn(bool)>,
    pub manage_network_sessions: Rc<dyn Fn(bool)>,
    /// Open reference downloader bundle interchange.
    pub exchange_downloaders: Rc<dyn Fn(bool)>,
    /// Open the manage import folders (`true`) or export folders dialog.
    pub manage_folders: Rc<dyn Fn(bool)>,
    /// Open the "review files to import" window.
    pub import_files: Rc<dyn Fn()>,
    /// Save the open pages as this session, or a new one (asking).
    pub save_session: Rc<dyn Fn(Option<String>, crate::session_saving::Scope)>,
    /// The page shown's file (0) or tag (1) domain button's menu: none
    /// for a page without a search.
    pub domain_menu: Rc<dyn Fn(i32) -> Vec<main_menu::Entry>>,
    /// Search the domain chosen from one.
    pub search_domain: Rc<dyn Fn(crate::domains::Choice)>,
    /// The page shown's star button's menu: none for a page without a
    /// search.
    pub favourites_menu: Rc<dyn Fn() -> Vec<main_menu::Entry>>,
    /// Do what was chosen from it.
    pub favourite: Rc<dyn Fn(crate::favourites::Action)>,
    /// A downloader page's list's menu, on a right press on a row (its
    /// entries' `Command::Popup`s go to the window's
    /// `importer-list-action`).
    pub importer_menu: Rc<dyn Fn(i32) -> Vec<main_menu::Entry>>,
    /// Open the about window.
    pub about: Rc<dyn Fn()>,
    /// Open service review.
    pub review_services: Rc<dyn Fn()>,
    /// Open staged service management.
    pub manage_services: Rc<dyn Fn()>,
    pub repair_archive_times: Rc<dyn Fn()>,
    pub viewing_maintenance: Rc<dyn Fn(bool)>,
    /// Ask about, then run, a Database menu maintenance job.
    pub database_maintenance: Rc<dyn Fn(hydrus_gui_model::database_maintenance::Job)>,
    pub set_password: Rc<dyn Fn()>,
    pub how_boned: Rc<dyn Fn()>,
    pub clear_thumbnail_cache: Rc<dyn Fn()>,
    /// Run a Help > debug action.
    pub debug: Rc<dyn Fn(hydrus_gui_model::debug_actions::Action)>,
    /// Run a Database > backup entry.
    pub backup: Rc<dyn Fn(hydrus_gui_model::database_backup::Action)>,
    /// Open Database > locations.
    pub locations: Rc<dyn Fn()>,
    pub file_history: Rc<dyn Fn()>,
    pub file_maintenance: Rc<dyn Fn()>,
    /// Toggle watcher or other recognised clipboard URL imports.
    pub watch_clipboard: Rc<dyn Fn(bool)>,
}

/// What the menus show now: the store's facts and the pages'.
pub(crate) fn facts(pages: &RefCell<Pages>, weigh: bool) -> Facts {
    let mut pages = pages.borrow_mut();
    let mut facts = match Facts::from_store(pages.store()) {
        Ok(facts) => facts,
        Err(e) => {
            eprintln!("could not read the menus' facts: {e}");
            Facts::default()
        }
    };
    facts.page_count = pages.page_count();
    // (only the pages menu says it, and it counts every download)
    if weigh {
        facts.session_weight = pages.session_weight();
    }
    facts.history = Some(pages.history().to_vec());
    facts.closed_pages = pages.closed_names();
    let history = pages.predicate_history();
    let mut context = pages.current().borrow().text_context();
    // FrameGUI uses Predicate.ToString() without render_for_user.
    context.presentation = None;
    let labelled = |predicates: Vec<hydrus_search::Predicate>| {
        predicates
            .into_iter()
            .map(|predicate| {
                let label = hydrus_search::predicate_text(&predicate, &context);
                (predicate, label)
            })
            .collect()
    };
    facts.search_added = labelled(history.added);
    facts.search_removed = labelled(history.removed);
    facts
}

fn owned_facts(hooks: &Hooks, weigh: bool) -> Facts {
    let mut facts = facts(&hooks.pages, weigh);
    facts.force_idle = hooks.force_idle.enabled();
    facts
}

/// Bind the menu bar; returns what shows its titles again (as what they
/// say changes: the undo menu with pages to reopen, the pending menu).
pub(crate) fn bind(
    window: &MainWindow,
    hooks: Hooks,
    palette_dispatcher: &crate::command_palette_window::MainDispatcher,
) -> Rc<dyn Fn()> {
    let hooks = Rc::new(hooks);
    let open: Rc<RefCell<OpenMenus>> = Rc::default();
    // where the bar's titles are, for menus opened by key
    let title_x: Rc<RefCell<Vec<f32>>> = Rc::default();
    // (the window's models kept, and changed in place: lines made anew
    // under the pointer would lose its press)
    let panes_model: Rc<VecModel<MenuPane>> = Rc::new(VecModel::default());
    window.set_menu_panes(ModelRc::from(panes_model.clone()));
    let show = {
        let open = open.clone();
        let weak = window.as_weak();
        let panes_model = panes_model.clone();
        let shown_lines: ShownLines = Rc::default();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let open = open.borrow();
            // (-2: a menu opened from a button)
            window.set_menu_open(match open.top() {
                Some(top) => i32::try_from(top).unwrap_or(-1),
                None if open.is_open() => -2,
                None => -1,
            });
            let view = open.view();
            let mut shown_lines = shown_lines.borrow_mut();
            shown_lines.truncate(view.len());
            for (p, pane) in view.into_iter().enumerate() {
                let lines: Vec<MenuLine> = pane
                    .lines
                    .into_iter()
                    .map(|(label, kind, usable, checked)| MenuLine {
                        label: label.into(),
                        kind: match kind {
                            LineKind::Item => 0,
                            LineKind::Check => 1,
                            LineKind::Separator => 2,
                            LineKind::Menu => 3,
                        },
                        usable,
                        checked,
                    })
                    .collect();
                let model = match shown_lines.get(p) {
                    Some((shown, model)) if *shown == lines => model.clone(),
                    _ => {
                        let model = ModelRc::new(VecModel::from(lines.clone()));
                        shown_lines.truncate(p);
                        shown_lines.push((lines, model.clone()));
                        model
                    }
                };
                let row = MenuPane {
                    x: pane.x,
                    y: pane.y,
                    left: pane.left,
                    lines: model,
                    current: pane
                        .current
                        .and_then(|c| i32::try_from(c).ok())
                        .unwrap_or(-1),
                };
                if p < panes_model.row_count() {
                    if panes_model.row_data(p).as_ref() != Some(&row) {
                        panes_model.set_row_data(p, row);
                    }
                } else {
                    panes_model.push(row);
                }
            }
            while panes_model.row_count() > shown_lines.len() {
                panes_model.remove(panes_model.row_count() - 1);
            }
        }
    };
    let titles: Rc<dyn Fn()> = {
        let hooks = hooks.clone();
        let titles_model: Rc<VecModel<MenuTitle>> = Rc::new(VecModel::default());
        window.set_menu_titles(ModelRc::from(titles_model.clone()));
        Rc::new(move || {
            let titles: Vec<MenuTitle> = main_menu::menubar(&owned_facts(&hooks, false))
                .iter()
                .map(|menu| MenuTitle {
                    label: main_menu::title(menu.label()).into(),
                    usable: menu.usable(),
                })
                .collect();
            // (as the panes, only what changed)
            for (i, title) in titles.iter().enumerate() {
                if i < titles_model.row_count() {
                    if titles_model.row_data(i).as_ref() != Some(title) {
                        titles_model.set_row_data(i, title.clone());
                    }
                } else {
                    titles_model.push(title.clone());
                }
            }
            while titles_model.row_count() > titles.len() {
                titles_model.remove(titles_model.row_count() - 1);
            }
        })
    };
    titles();
    // open the bar's menu `top`, its menus as they are now
    let open_menu = {
        let open = open.clone();
        let hooks = hooks.clone();
        let titles = titles.clone();
        move |top: usize, x: f32, y: f32| {
            let menus = main_menu::menubar(&owned_facts(&hooks, true));
            open.borrow_mut().open(menus, top, x, y);
            titles();
        }
    };
    // after a choice: do it, and show the menus (closed) and titles again
    let chosen = {
        let hooks = hooks.clone();
        let weak = window.as_weak();
        let titles = titles.clone();
        let show = show.clone();
        move |command: Option<Command>| {
            show();
            if let (Some(command), Some(window)) = (command, weak.upgrade()) {
                run(&window, &hooks, command);
                titles();
            }
        }
    };
    *palette_dispatcher.borrow_mut() = Some(Rc::new({
        let chosen = chosen.clone();
        let open = open.clone();
        move |command| {
            open.borrow_mut().close();
            chosen(Some(command));
        }
    }));
    window.on_menu_title_pressed({
        let open = open.clone();
        let open_menu = open_menu.clone();
        let show = show.clone();
        move |top, x, y| {
            let Ok(top) = usize::try_from(top) else {
                return;
            };
            if open.borrow().top() == Some(top) {
                open.borrow_mut().close();
            } else {
                open_menu(top, x, y);
            }
            show();
        }
    });
    window.on_tab_menu_requested({
        let open = open.clone();
        let hooks = hooks.clone();
        let show = show.clone();
        move |depth, index, x, y| {
            let Ok(depth) = usize::try_from(depth) else {
                return;
            };
            let entries = if index == -1 {
                hooks.pages.borrow().tab_space_menu(depth)
            } else if let Ok(index) = usize::try_from(index) {
                hooks.pages.borrow().tab_menu(depth, index)
            } else {
                return;
            };
            if !entries.is_empty() {
                open.borrow_mut().open_popup(entries, x, y);
                show();
            }
        }
    });
    // a search page's domain buttons: their menus, below them
    window.on_domain_menu_requested({
        let open = open.clone();
        let hooks = hooks.clone();
        let show = show.clone();
        move |which, x, y| {
            let entries = (hooks.domain_menu)(which);
            if !entries.is_empty() {
                open.borrow_mut().open_popup(entries, x, y);
                show();
            }
        }
    });
    // a downloader page's list's
    window.on_importer_list_menu({
        let open = open.clone();
        let hooks = hooks.clone();
        let show = show.clone();
        move |row, x, y| {
            let entries = (hooks.importer_menu)(row);
            if !entries.is_empty() {
                open.borrow_mut().open_popup(entries, x, y);
                show();
            }
        }
    });
    // and its star button's
    window.on_favourites_menu_requested({
        let open = open.clone();
        let hooks = hooks.clone();
        let show = show.clone();
        move |x, y| {
            let entries = (hooks.favourites_menu)();
            if !entries.is_empty() {
                open.borrow_mut().open_popup(entries, x, y);
                show();
            }
        }
    });
    window.on_menu_title_hovered({
        let open = open.clone();
        let open_menu = open_menu.clone();
        let show = show.clone();
        move |top, x, y| {
            let Ok(top) = usize::try_from(top) else {
                return;
            };
            let switching = open.borrow().top().is_some_and(|t| t != top);
            if switching {
                open_menu(top, x, y);
                show();
            }
        }
    });
    window.on_menu_title_placed({
        let title_x = title_x.clone();
        move |i, x| {
            let Ok(i) = usize::try_from(i) else { return };
            let mut title_x = title_x.borrow_mut();
            if title_x.len() <= i {
                title_x.resize(i + 1, 0.0);
            }
            title_x[i] = x;
        }
    });
    window.on_menu_line_hovered({
        let open = open.clone();
        let show = show.clone();
        move |pane, line, right, top, left| {
            if let (Ok(pane), Ok(line)) = (usize::try_from(pane), usize::try_from(line)) {
                open.borrow_mut().hover(pane, line, right, top, left);
                show();
            }
        }
    });
    window.on_menu_line_clicked({
        let open = open.clone();
        let chosen = chosen.clone();
        move |pane, line, right, top, left| {
            if let (Ok(pane), Ok(line)) = (usize::try_from(pane), usize::try_from(line)) {
                let command = open.borrow_mut().click(pane, line, right, top, left);
                chosen(command);
            }
        }
    });
    window.on_menu_placed({
        let open = open.clone();
        move |pane, x, y, width| {
            if let Ok(pane) = usize::try_from(pane) {
                open.borrow_mut().placed(pane, x, y, width);
            }
        }
    });
    window.on_menu_dismissed({
        let open = open.clone();
        let show = show.clone();
        move || {
            open.borrow_mut().close();
            show();
        }
    });
    // keys while a menu is open, and alt and a title's letter
    window.on_menu_key({
        let open = open.clone();
        let hooks = hooks.clone();
        let open_menu = open_menu.clone();
        let show = show.clone();
        let chosen = chosen.clone();
        move |text, alt| {
            use slint::platform::Key;
            let by_key = |top: usize| {
                let x = title_x.borrow().get(top).copied().unwrap_or(0.0);
                open_menu(top, x, main_menu::LINE_HEIGHT);
                open.borrow_mut().key(MenuKey::Down);
            };
            if alt {
                let menus = main_menu::menubar(&owned_facts(&hooks, false));
                if let Some(top) = main_menu::mnemonic(&menus, &text) {
                    by_key(top);
                    show();
                    return true;
                }
            }
            if !open.borrow().is_open() {
                return false;
            }
            let key = |k: Key| SharedString::from(k) == text;
            let key = if key(Key::UpArrow) {
                MenuKey::Up
            } else if key(Key::DownArrow) {
                MenuKey::Down
            } else if key(Key::LeftArrow) {
                MenuKey::Left
            } else if key(Key::RightArrow) {
                MenuKey::Right
            } else if key(Key::Return) || text == "\n" {
                MenuKey::Enter
            } else if key(Key::Escape) {
                MenuKey::Escape
            } else {
                return true;
            };
            let (command, switch) = open.borrow_mut().key(key);
            if let Some(top) = switch {
                by_key(top);
            }
            chosen(command);
            true
        }
    });
    titles
}

/// Flip a setting in the store.
fn flip<S: hydrus_store::settings::Setting + Send + 'static>(
    store: &hydrus_store::Store,
    change: impl FnOnce(&mut S) + Send + 'static,
) {
    let done = store.write(move |ctx| {
        let mut setting: S = hydrus_store::settings::get(ctx.conn())?;
        change(&mut setting);
        hydrus_store::settings::set(ctx.conn(), &setting)
    });
    if let Err(e) = done {
        eprintln!("could not change the setting: {e}");
    }
}

/// Do what an entry does.
fn run(window: &MainWindow, hooks: &Hooks, command: Command) {
    let store = hooks.pages.borrow().store().clone();
    let change_pages = &hooks.change_pages;
    match command {
        Command::Sidebar(action) => (hooks.sidebar_layout)(action),
        Command::DuplicateTab { depth, index } => {
            change_pages(&|pages| pages.duplicate_tab(depth, index));
        }
        Command::CollapseTabs {
            depth,
            index,
            scope,
        } => {
            let harvest = hooks
                .pages
                .borrow_mut()
                .collapse_tabs_question(depth, index, scope);
            match harvest {
                Ok(Some((keys, files, question))) => {
                    let change = hooks.change_pages.clone();
                    (hooks.ask)(
                        question,
                        Rc::new(move || {
                            change(&|pages| pages.collapse_tab_keys(&keys, &files));
                        }),
                    );
                }
                Ok(None) => {}
                Err(error) => eprintln!("could not harvest pages: {error}"),
            }
        }
        Command::RenameTab { depth, index } => {
            if let (Ok(depth), Ok(index)) = (i32::try_from(depth), i32::try_from(index)) {
                window.invoke_tab_rename_requested(depth, index);
            }
        }
        Command::SendTabs {
            depth,
            index,
            scope,
        } => {
            let keys = hooks.pages.borrow().send_tab_targets(depth, index, scope);
            if keys.is_empty() {
                return;
            }
            let change = hooks.change_pages.clone();
            let weak = window.as_weak();
            let store = store.clone();
            let send: Rc<dyn Fn()> = Rc::new(move || {
                let created = std::cell::Cell::new(None);
                change(&|pages| {
                    created
                        .set(pages.send_tab_keys(&keys, scope == crate::tab_context::Send::This));
                    Ok(())
                });
                let settings: hydrus_store::sessions::NotebookSettings =
                    store.read(hydrus_store::settings::get).unwrap_or_default();
                if settings.rename_sent_notebooks
                    && let (Some(key), Some(window)) = (created.get(), weak.upgrade())
                {
                    window.invoke_notebook_rename_requested(key.to_hex().into());
                }
            });
            if scope == crate::tab_context::Send::This {
                send();
            } else {
                (hooks.ask)(
                    "Send all pages to the right to a new page of pages?".into(),
                    send,
                );
            }
        }
        Command::CloseTab { depth, index } => {
            if let (Ok(depth), Ok(index)) = (i32::try_from(depth), i32::try_from(index)) {
                window.invoke_close_tab(depth, index);
            }
        }
        Command::CloseTabs { depth, index, side } => {
            let prepared = hooks
                .pages
                .borrow_mut()
                .close_tabs_question(depth, index, side);
            if let Some((keys, question)) = prepared {
                let change_pages = hooks.change_pages.clone();
                (hooks.ask)(
                    question,
                    Rc::new(move || change_pages(&|pages| pages.close_tab_keys(&keys))),
                );
            }
        }
        Command::NavigateTabs { depth, movement } => change_pages(&|pages| {
            pages.navigate_tabs(depth, movement, std::time::Instant::now());
            Ok(())
        }),
        Command::SortTabs {
            depth,
            by,
            ascending,
        } => change_pages(&|pages| pages.sort_tabs(depth, by, ascending)),
        Command::MoveTab {
            depth,
            index,
            movement,
        } => change_pages(&|pages| {
            pages.move_tab(depth, index, movement);
            Ok(())
        }),
        Command::Copy(text) => crate::copy_to_clipboard(&text),
        Command::NetworkBootPause => {
            let done = store.write(|ctx| {
                let conn = ctx.conn();
                let mut boot: hydrus_store::settings::NetworkBootPause =
                    hydrus_store::settings::get(conn)?;
                boot.0 = !boot.0;
                hydrus_store::settings::set(conn, &boot)
            });
            if let Err(error) = done {
                eprintln!("could not save the network boot preference: {error}");
            }
        }
        Command::Pause(pause) => {
            let done = store.write(move |ctx| {
                let conn = ctx.conn();
                let mut pauses: hydrus_store::settings::Pauses = hydrus_store::settings::get(conn)?;
                let mut folders: hydrus_store::settings::FolderSettings =
                    hydrus_store::settings::get(conn)?;
                let field = pause.field(&mut pauses, &mut folders);
                *field = !*field;
                hydrus_store::settings::set(conn, &pauses)?;
                hydrus_store::settings::set(conn, &folders)
            });
            if let Err(e) = done {
                eprintln!("could not pause: {e}");
            }
        }
        Command::CheckImportFolder(name) => {
            if let Err(e) = hydrus_gui_model::folder_runs::check_import_folders(&store, name) {
                eprintln!("could not check the import folders: {e}");
            }
        }
        Command::RunExportFolder(name) => {
            if let Err(e) = hydrus_gui_model::folder_runs::run_export_folders(&store, name) {
                eprintln!("could not run the export folders: {e}");
            }
        }
        Command::OpenInstallDirectory => match std::env::current_exe() {
            Ok(exe) => {
                if let Some(dir) = exe.parent() {
                    crate::launch(&dir.to_string_lossy());
                }
            }
            Err(e) => eprintln!("could not find the installation directory: {e}"),
        },
        Command::OpenDatabaseDirectory => crate::launch(&store.dir().to_string_lossy()),
        Command::OpenQuickExportDirectory => hooks.quick_export_directory.open(),
        Command::Exit | Command::Restart | Command::ExitForceMaintenance => {
            crate::client_exit::set_mode(match command {
                Command::Restart => hydrus_gui_model::shutdown_work::ExitMode::Restart,
                Command::ExitForceMaintenance => {
                    hydrus_gui_model::shutdown_work::ExitMode::ForceMaintenance
                }
                _ => hydrus_gui_model::shutdown_work::ExitMode::Exit,
            });
            let _ = window
                .window()
                .dispatch_event_with_result(slint::platform::WindowEvent::CloseRequested);
        }
        Command::UndoContent | Command::RedoContent => {
            let store = hooks.pages.borrow().store().clone();
            let done = if command == Command::UndoContent {
                store.undo()
            } else {
                store.redo()
            };
            match done {
                Ok(_) => (hooks.reshow)(),
                Err(error) => eprintln!("could not undo or redo: {error}"),
            }
        }
        Command::ClearClosedPages => {
            let count = hooks.pages.borrow_mut().closed_names().len();
            let pages = hooks.pages.clone();
            (hooks.ask)(
                format!("Clear the {} closed pages?", human_int(count as u64)),
                Rc::new(move || pages.borrow_mut().forget_closed()),
            );
        }
        Command::UndoSearch { kind, predicate } => change_pages(&|pages| {
            pages.undo_search_predicate(kind, &predicate);
            Ok(())
        }),
        Command::ClearSearchHistory => {
            let pages = hooks.pages.clone();
            let reshow = hooks.reshow.clone();
            (hooks.ask)(
                "Clear the entire search predicate history? This cannot be undone.".into(),
                Rc::new(move || {
                    pages.borrow_mut().clear_predicate_history();
                    reshow();
                }),
            );
        }
        Command::Unclose(index) => change_pages(&|pages| {
            pages.unclose_at(index);
            Ok(())
        }),
        Command::ShowPage(key) => change_pages(&|pages| {
            pages.show(&key);
            Ok(())
        }),
        Command::ClearHistory => hooks.pages.borrow_mut().clear_history(),
        Command::Refresh => window.invoke_refresh_page(),
        Command::AppendSessionBackup { name, timestamp } => {
            change_pages(&|pages| pages.append_session_backup(&name, timestamp));
        }
        Command::AppendSession(name) => change_pages(&|pages| pages.append_session(&name)),
        // asked first, then (any page objecting) asked again, as the
        // reference's `LoadGUISession` asks
        Command::DebugReloadSession => hooks.debug_session_reload.start(),
        Command::ClearAndLoadSession(name) => {
            let pages = hooks.pages.clone();
            let change_pages = hooks.change_pages.clone();
            let ask = hooks.ask.clone();
            (hooks.ask)(
                crate::session_saving::clear_and_load_question(&name),
                Rc::new(move || {
                    let load: Rc<dyn Fn()> = {
                        let change_pages = change_pages.clone();
                        let name = name.clone();
                        Rc::new(move || change_pages(&|pages| pages.clear_and_load(&name)))
                    };
                    let vetoes = pages.borrow_mut().session_close_vetoes();
                    match crate::session_saving::close_all_question(&vetoes) {
                        Some(question) => ask(question, load),
                        None => load(),
                    }
                }),
            );
        }
        Command::RefreshTab(key) => {
            change_pages(&|pages| {
                pages.refresh_tab_tree(key);
                Ok(())
            });
        }
        Command::ChooseNotebookPage { parent, before } => {
            window.invoke_tab_new_page_requested(
                parent.map_or_else(String::new, |key| key.to_hex()).into(),
                before.map_or_else(String::new, |key| key.to_hex()).into(),
            );
        }
        Command::SaveSession(name) => (hooks.save_session)(name, crate::session_saving::Scope::All),
        Command::SaveNotebookSession {
            key,
            name,
            suggested_name,
        } => {
            (hooks.save_session)(
                name,
                crate::session_saving::Scope::Notebook {
                    key,
                    suggested_name,
                },
            );
        }
        Command::AppendNotebookSession { notebook, name } => {
            change_pages(&|pages| pages.append_session_to_notebook(notebook, &name));
        }
        Command::DeleteSession(name) => {
            let store = store.clone();
            let deleted = name.clone();
            (hooks.ask)(
                format!("Delete session \"{name}\"?"),
                Rc::new(move || {
                    let name = deleted.clone();
                    let done =
                        store.write(move |ctx| hydrus_store::sessions::delete(ctx.conn(), &name));
                    if let Err(e) = done {
                        eprintln!("could not delete the session: {e}");
                    }
                }),
            );
        }
        Command::ChooseNewPage => window.invoke_new_page(),
        Command::NewPage(page) => change_pages(&|pages| pages.new_page(&page)),
        Command::WatchClipboard(watchers) => (hooks.watch_clipboard)(watchers),
        Command::ClearWatcherHighlights => {
            hooks.pages.borrow_mut().clear_watcher_highlights();
            (hooks.reshow)();
        }
        Command::FileHistory => (hooks.file_history)(),
        Command::ManageFileMaintenance => (hooks.file_maintenance)(),
        Command::RepairArchiveTimes => (hooks.repair_archive_times)(),
        Command::ClearThumbnailCache => (hooks.clear_thumbnail_cache)(),
        Command::Debug(action) => (hooks.debug)(action),
        Command::Backup(action) => (hooks.backup)(action),
        Command::Locations => (hooks.locations)(),
        Command::ClearOrphanFiles => crate::orphan_files_window::open(&store),
        Command::DebugFetchUrl => hooks.debug_fetch.open(),
        Command::DebugLongTextPopup => hooks.debug_long_popup.start(),
        Command::DebugForceIdleMode => {
            hooks.force_idle.toggle();
        }
        Command::DebugDelayedTextPopup => hooks.debug_long_popup.start_delayed_popup(),
        Command::DebugDelayedNewPage(location) => {
            hooks.debug_long_popup.start_delayed_page(location);
        }
        Command::ClearViewingStatistics => (hooks.viewing_maintenance)(false),
        Command::CullViewingStatistics => (hooks.viewing_maintenance)(true),
        Command::DatabaseMaintenance(job) => (hooks.database_maintenance)(job),
        Command::SetPassword => (hooks.set_password)(),
        Command::HowBoned => (hooks.how_boned)(),
        Command::DeferredDelete(idle) => {
            flip::<hydrus_store::settings::BackgroundWork>(&store, move |w| {
                let field = if idle {
                    &mut w.deferred_delete_during_idle
                } else {
                    &mut w.deferred_delete_during_active
                };
                *field = !*field;
            });
        }
        Command::TagDisplaySync(idle) => {
            flip::<hydrus_store::settings::BackgroundWork>(&store, move |w| {
                let field = if idle {
                    &mut w.tag_display_during_idle
                } else {
                    &mut w.tag_display_during_active
                };
                *field = !*field;
            });
        }
        // (hydrus-rs applies siblings and parents as it writes: there is
        // never work left)
        Command::SessionWeightReport => {
            let report = hooks.pages.borrow().weight_report();
            crate::debug_actions::message("Information", &report);
        }
        Command::TagSyncReview => crate::tag_sync_review_window::open(&store),
        Command::TagDisplaySyncNow => {
            let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
            #[allow(clippy::cast_precision_loss)] // (seconds)
            let popup = hydrus_store::popups::Job::text(
                "Seems like we are all synced already!",
                now as f64,
            );
            if let Err(e) =
                store.write(move |ctx| hydrus_store::popups::add(ctx.conn(), &popup, now))
            {
                eprintln!("could not say so: {e}");
            }
        }
        Command::FileMaintenance(idle) => {
            flip::<hydrus_store::file_maintenance::FileMaintenanceSettings>(&store, move |m| {
                let field = if idle {
                    &mut m.during_idle
                } else {
                    &mut m.during_active
                };
                *field = !*field;
            });
        }
        Command::ForgetPending(key) => {
            let Ok(service) = store.snapshot().services.by_key(&key).cloned() else {
                return;
            };
            let store = store.clone();
            (hooks.ask)(
                format!(
                    "Are you sure you want to delete the pending data for {}?",
                    service.name
                ),
                Rc::new(move || {
                    if let Err(e) = hydrus_store::pending::forget(&store, service.id) {
                        eprintln!("could not forget the pending data: {e}");
                    }
                }),
            );
        }
        Command::OpenUrl(url) => crate::launch(url),
        Command::AdvancedMode => {
            flip::<hydrus_store::settings::AdvancedMode>(&store, |a| a.0 = !a.0);
        }
        Command::Options => (hooks.options)(),
        // (the only popup the bar opens with these: a downloader list's)
        Command::Popup(i) => {
            if let Ok(i) = i32::try_from(i) {
                window.invoke_importer_list_action(i);
            }
        }
        Command::NudgeSubscriptions => {
            if let Err(e) = store.write(|ctx| {
                hydrus_store::queues::nudge(ctx.conn(), hydrus_store::queues::SUBSCRIPTIONS_NUDGE)
            }) {
                eprintln!("could not nudge the subscriptions: {e}");
            }
        }
        Command::TagDisplay(application) => (hooks.tag_display)(application),
        Command::TagRelationships(kind) => (hooks.tag_relationships)(kind),
        Command::TagMigrate => (hooks.tag_migrate)(),
        Command::ManageLoginScripts => (hooks.manage_login_scripts)(),
        Command::ManageLogins => (hooks.manage_logins)(),
        Command::ManageDownloaderDisplay => (hooks.manage_downloader_display)(),
        Command::ManageDownloaderDefinitions(classes) => {
            (hooks.manage_downloader_definitions)(classes);
        }
        Command::ManageNetworkSessions(headers) => (hooks.manage_network_sessions)(headers),
        Command::ManageParsers(links) => (hooks.manage_parsers)(links),
        Command::NetworkData(bandwidth) => {
            let result = if bandwidth {
                crate::network_data_window::open_bandwidth(store, &hooks.network_data).map(|_| ())
            } else {
                crate::network_data_window::open_jobs(store, &hooks.network_data).map(|_| ())
            };
            if let Err(e) = result {
                eprintln!("could not open network review: {e}");
            }
        }
        Command::ExchangeDownloaders(importing) => (hooks.exchange_downloaders)(importing),
        Command::ManageSubscriptions => (hooks.manage_subscriptions)(),
        Command::ManageImportFolders => (hooks.manage_folders)(true),
        Command::ManageExportFolders => (hooks.manage_folders)(false),
        Command::ImportFiles => (hooks.import_files)(),
        Command::SearchDomain(choice) => (hooks.search_domain)(choice),
        Command::Favourite(action) => (hooks.favourite)(action),
        Command::Darkmode => (hooks.darkmode)(),
        Command::About => (hooks.about)(),
        Command::ReviewServices => (hooks.review_services)(),
        Command::ManageServices => (hooks.manage_services)(),
    }
}
