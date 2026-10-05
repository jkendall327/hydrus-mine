//! Live command palette, with one snapshot worker per owner and query-ticket
//! gating. Closing invalidates launches and pending results before releasing
//! the window slot; callbacks retained by an old window cannot affect a reopen.
use crate::{CommandPaletteWindow, MainWindow, PaletteRow};
use hydrus_gui_model::command_palette::{
    self as palette, Action, CommandPaletteSettings, Provider, Results, Snapshot, Suggestion,
    Ticket,
};
use slint::winit_030::WinitWindowAccessor as _;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

/// The palette held strongly by its main-window owner while open.
pub type Slot = Rc<RefCell<Option<CommandPaletteWindow>>>;
pub(crate) type MainDispatcher = Rc<RefCell<Option<Rc<dyn Fn(crate::main_menu::Command)>>>>;
pub(crate) type Context = (Snapshot, Rc<dyn Fn(Action)>);
pub(crate) type Snapshotter = Rc<dyn Fn() -> Context>;
type Reply = (Ticket, Provider, Vec<Suggestion>);
struct Request {
    ticket: Ticket,
    query: String,
    settings: CommandPaletteSettings,
    snapshot: Snapshot,
    cancelled: Arc<AtomicBool>,
}
struct Owner {
    active: Cell<bool>,
    parent: slint::Weak<MainWindow>,
    focus: RefCell<Option<crate::session_autosave::FocusCallback>>,
    results: RefCell<Results>,
    shown: RefCell<Vec<Option<(Provider, Suggestion)>>>,
    selected: RefCell<Option<(Provider, Suggestion)>>,
    launch: RefCell<Rc<dyn Fn(Action)>>,
    receiver: mpsc::Receiver<Reply>,
    sender: mpsc::Sender<Option<Request>>,
    cancelled: RefCell<Option<Arc<AtomicBool>>>,
    timer: slint::Timer,
}

fn display(window: &CommandPaletteWindow, owner: &Owner) {
    let results = owner.results.borrow();
    let mut rows = Vec::new();
    let mut shown = Vec::new();
    let mut previous = None;
    let mut selected = -1;
    for (provider, suggestion) in results.ordered() {
        if previous != Some(provider) && !provider.title().is_empty() {
            let count = results
                .ordered()
                .iter()
                .filter(|(p, _)| *p == provider)
                .count();
            rows.push(PaletteRow {
                primary: provider.title().into(),
                secondary: count.to_string().into(),
                heading: true,
                enabled: true,
                checked: -1,
            });
            shown.push(None);
        }
        let item = (provider, suggestion.clone());
        if owner.selected.borrow().as_ref() == Some(&item) {
            selected = i32::try_from(shown.len()).unwrap_or(-1);
        }
        rows.push(PaletteRow {
            primary: suggestion.primary.as_str().into(),
            secondary: suggestion.secondary.as_str().into(),
            heading: false,
            enabled: suggestion.action.is_some(),
            checked: suggestion.checked.map_or(-1, i32::from),
        });
        shown.push(Some(item));
        previous = Some(provider);
    }
    *owner.shown.borrow_mut() = shown;
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_selected(selected);
    let height = 44.0 + (owner.shown.borrow().len().min(16) as f32) * 36.0;
    window
        .window()
        .set_size(slint::LogicalSize::new(800.0, height));
    if let Some(parent) = owner.parent.upgrade() {
        let position = parent.window().position();
        let scale = parent.window().scale_factor();
        let size = parent.window().size();
        window.window().set_position(slint::LogicalPosition::new(
            position.x as f32 / scale + (size.width as f32 / scale - 800.0) / 2.0,
            position.y as f32 / scale + (size.height as f32 / scale - height) / 2.0,
        ));
    }
}

fn poll(window: &CommandPaletteWindow, owner: &Owner) {
    if !owner.active.get() {
        return;
    }
    let mut changed = false;
    while let Ok((ticket, provider, rows)) = owner.receiver.try_recv() {
        changed |= owner.results.borrow_mut().accept(ticket, provider, rows);
    }
    if changed {
        display(window, owner);
    }
}

/// Search an opened palette. Snapshot capture stays on the GUI thread; filtering
/// and calculator evaluation run on its dedicated, cancellation-aware worker.
pub(crate) fn open(
    parent: &MainWindow,
    slot: &Slot,
    settings: CommandPaletteSettings,
    snapshotter: Snapshotter,
) -> Result<CommandPaletteWindow, slint::PlatformError> {
    let window = CommandPaletteWindow::new()?;
    let (sender, requests) = mpsc::channel::<Option<Request>>();
    let (replies, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(Some(mut request)) = requests.recv() {
            // Typing can outrun filtering; skip queued intermediate snapshots.
            while let Ok(next) = requests.try_recv() {
                let Some(next) = next else {
                    return;
                };
                request = next;
            }
            for provider in &request.settings.provider_order {
                if request.cancelled.load(Ordering::Acquire) {
                    break;
                }
                let rows = palette::query(
                    *provider,
                    &request.query,
                    &request.settings,
                    &request.snapshot,
                );
                if replies.send((request.ticket, *provider, rows)).is_err() {
                    return;
                }
            }
        }
    });
    let owner = Rc::new(Owner {
        active: Cell::new(true),
        parent: parent.as_weak(),
        focus: RefCell::default(),
        results: RefCell::default(),
        shown: RefCell::default(),
        selected: RefCell::default(),
        launch: RefCell::new(Rc::new(|_| {})),
        receiver,
        sender,
        cancelled: RefCell::default(),
        timer: slint::Timer::default(),
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let parent = parent.as_weak();
        let slot = Rc::downgrade(slot);
        let owner = owner.clone();
        move || {
            if !owner.active.replace(false) {
                return;
            }
            owner.results.borrow_mut().close();
            owner.timer.stop();
            if let Some(cancelled) = owner.cancelled.borrow_mut().take() {
                cancelled.store(true, Ordering::Release);
            }
            let _ = owner.sender.send(None);
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            if let Some(parent) = parent.upgrade() {
                parent
                    .set_search_focus_requests(parent.get_search_focus_requests().wrapping_add(1));
                let _ = parent
                    .window()
                    .with_winit_window(slint::winit_030::winit::window::Window::focus_window);
            }
        }
    });
    window.on_native_focus({
        let close = close.clone();
        move |focused| {
            if !focused {
                close();
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    let query: Rc<dyn Fn(String)> = Rc::new({
        let weak = window.as_weak();
        let owner = owner.clone();
        move |text| {
            if !owner.active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if let Some(cancelled) = owner.cancelled.borrow_mut().take() {
                cancelled.store(true, Ordering::Release);
            }
            let cancelled = Arc::new(AtomicBool::new(false));
            *owner.cancelled.borrow_mut() = Some(cancelled.clone());
            window.set_query(text.clone().into());
            let (snapshot, launch) = snapshotter();
            *owner.launch.borrow_mut() = launch;
            let ticket = owner.results.borrow_mut().begin(&settings.provider_order);
            owner.selected.borrow_mut().take();
            display(&window, &owner);
            let _ = owner.sender.send(Some(Request {
                ticket,
                query: text,
                settings: settings.clone(),
                snapshot,
                cancelled,
            }));
        }
    });
    window.on_query_edited({
        let query = query.clone();
        move |text| query(text.to_string())
    });
    window.on_poll({
        let weak = window.as_weak();
        let owner = owner.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                poll(&window, &owner);
            }
        }
    });
    window.on_move_selection({
        let weak = window.as_weak();
        let owner = owner.clone();
        move |direction| {
            if !owner.active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let shown = owner.shown.borrow();
            let indices: Vec<_> = shown
                .iter()
                .enumerate()
                .filter_map(|(i, row)| row.is_some().then_some(i))
                .collect();
            if indices.is_empty() {
                return;
            }
            let current = usize::try_from(window.get_selected()).ok();
            let target = match direction.as_str() {
                "home" => indices.first().copied(),
                "end" => indices.last().copied(),
                "up" => current.map_or_else(
                    || indices.last().copied(),
                    |current| indices.iter().rev().find(|&&i| i < current).copied(),
                ),
                "down" => current
                    .and_then(|current| indices.iter().find(|&&i| i > current).copied())
                    .or_else(|| indices.first().copied()),
                "page-up" => {
                    let target = current.unwrap_or(0).saturating_sub(16);
                    indices
                        .iter()
                        .rev()
                        .find(|&&i| i <= target)
                        .copied()
                        .or_else(|| indices.first().copied())
                }
                "page-down" => {
                    let target = current.unwrap_or(0) + 16;
                    indices
                        .iter()
                        .find(|&&i| i >= target)
                        .copied()
                        .or_else(|| indices.last().copied())
                }
                _ => return,
            };
            *owner.selected.borrow_mut() = target.and_then(|index| shown[index].clone());
            window.set_selected(target.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
        }
    });
    window.on_hover({
        let owner = owner.clone();
        let weak = window.as_weak();
        move |index| {
            if !owner.active.get() {
                return;
            }
            if let Some(item) = usize::try_from(index)
                .ok()
                .and_then(|index| owner.shown.borrow().get(index).cloned())
                .flatten()
            {
                *owner.selected.borrow_mut() = Some(item);
                if let Some(window) = weak.upgrade() {
                    window.set_selected(index);
                }
            }
        }
    });
    window.on_activate({
        let owner = owner.clone();
        let close = close.clone();
        move |index| {
            if !owner.active.get() {
                return;
            }
            let item = usize::try_from(index)
                .ok()
                .and_then(|index| owner.shown.borrow().get(index).cloned())
                .flatten();
            let Some((_, suggestion)) = item else {
                return;
            };
            if suggestion.action == Some(Action::Calculator) {
                return;
            }
            let launch = owner.launch.borrow().clone();
            close();
            if let Some(action) = suggestion.action {
                launch(action);
            }
        }
    });
    owner
        .timer
        .start(slint::TimerMode::Repeated, Duration::from_millis(16), {
            let weak = window.as_weak();
            let owner = Rc::downgrade(&owner);
            move || {
                if let (Some(window), Some(owner)) = (weak.upgrade(), owner.upgrade()) {
                    poll(&window, &owner);
                }
            }
        });
    window.show()?;
    let focus: crate::session_autosave::FocusCallback = Rc::new({
        let weak = window.as_weak();
        move |focused| {
            if let Some(window) = weak.upgrade() {
                window.invoke_native_focus(focused);
            }
        }
    });
    crate::session_autosave::watch_native_focus(window.window(), &focus);
    *owner.focus.borrow_mut() = Some(focus);
    *slot.borrow_mut() = Some(window.clone_strong());
    query(String::new());
    Ok(window)
}

/// Build media suggestions from the real thumbnail menu's leaves, retaining the
/// native action IDs rather than deriving commands from displayed labels.
pub(crate) fn media_menu_items(
    entries: &[crate::thumbnail_menu::Entry],
    actions: &[(crate::thumbnail_menu::Action, String)],
) -> Vec<palette::MenuItem> {
    fn walk(
        entries: &[crate::thumbnail_menu::Entry],
        parent: &str,
        actions: &[(crate::thumbnail_menu::Action, String)],
        out: &mut Vec<palette::MenuItem>,
    ) {
        use crate::thumbnail_menu::Entry;
        for entry in entries {
            let (label, action, checked) = match entry {
                Entry::Menu(label, children) => {
                    let path = if parent.is_empty() {
                        label.clone()
                    } else {
                        format!("{parent} | {label}")
                    };
                    walk(children, &path, actions, out);
                    continue;
                }
                Entry::Item(label, action) => (label, *action, None),
                Entry::Check(label, action, checked) => (label, *action, Some(*checked)),
                Entry::Label(label) => (label, crate::thumbnail_menu::Action::Copy, None),
                Entry::Separator => continue,
            };
            let id = actions
                .iter()
                .position(|(a, name)| *a == action && name == label);
            out.push(palette::MenuItem {
                label: label.clone(),
                parent: parent.to_owned(),
                checked,
                action: id.map(Action::MediaMenu),
            });
        }
    }
    let mut out = Vec::new();
    walk(entries, "", actions, &mut out);
    out
}

/// Install the reachable native route and snapshots of actual supported menus.
#[allow(clippy::too_many_arguments)] // existing main-owner stores and dispatchers are passed explicitly
pub(crate) fn bind(
    window: &MainWindow,
    slot: &Slot,
    pages: &Rc<RefCell<crate::Pages>>,
    current: &Rc<RefCell<Rc<RefCell<crate::SearchPage>>>>,
    change_pages: &crate::ChangePages,
    dispatcher: &MainDispatcher,
    menu_state: &crate::MenuState,
    media_items: &Rc<RefCell<Vec<palette::MenuItem>>>,
) {
    let snapshotter: Snapshotter = Rc::new({
        let weak = window.as_weak();
        let pages = pages.clone();
        let current = current.clone();
        let change_pages = change_pages.clone();
        let dispatcher = dispatcher.clone();
        let menu_state = menu_state.clone();
        let media_items = media_items.clone();
        move || {
            let store = pages.borrow().store().clone();
            let settings: CommandPaletteSettings =
                store.read(hydrus_store::settings::get).unwrap_or_default();
            let favourites: hydrus_store::settings::FavouriteSearches =
                store.read(hydrus_store::settings::get).unwrap_or_default();
            let mut snapshot = Snapshot {
                pages: pages.borrow().command_palette_pages(),
                history: pages.borrow().history().to_vec(),
                favourites: favourites.0,
                ..Default::default()
            };
            if settings.show_main_menu {
                snapshot.main_menu = palette::main_menu_items(&crate::main_menu::menubar(
                    &crate::menu_bar::facts(&pages, false),
                ));
            }
            let key = pages.borrow().shown().key;
            let media_context = if settings.show_media_menu
                && !matches!(
                    pages.borrow().shown().content,
                    hydrus_core::pages::PageContent::Pages(_)
                ) {
                weak.upgrade().map(|window| {
                    window.invoke_thumbnail_menu_requested(-1);
                    snapshot.media_menu.clone_from(&media_items.borrow());
                    let page = current.borrow().clone();
                    let page = page.borrow();
                    (
                        menu_state.borrow().clone(),
                        page.selected_files(),
                        page.focused().map(|i| page.results()[i]),
                    )
                })
            } else {
                None
            };
            let launch: Rc<dyn Fn(Action)> = Rc::new({
                let weak = weak.clone();
                let pages = pages.clone();
                let current = current.clone();
                let change_pages = change_pages.clone();
                let dispatcher = dispatcher.clone();
                let menu_state = menu_state.clone();
                move |action| {
                    let Some(window) = weak.upgrade() else {
                        return;
                    };
                    match action {
                        Action::Page(key) => change_pages(&|pages| {
                            pages.show(&key);
                            Ok(())
                        }),
                        Action::Favourite(favourite) => {
                            let settings: CommandPaletteSettings = pages
                                .borrow()
                                .store()
                                .read(hydrus_store::settings::get)
                                .unwrap_or_default();
                            change_pages(&|pages| {
                                pages.command_palette_favourite(
                                    &favourite,
                                    settings.favourites_new_page,
                                );
                                Ok(())
                            });
                        }
                        Action::MainMenu(command) => {
                            let launch = dispatcher.borrow().clone();
                            if let Some(launch) = launch {
                                launch(command);
                            }
                        }
                        Action::MediaMenu(index) => {
                            let Some((frozen, selected, focused)) = &media_context else {
                                return;
                            };
                            if pages.borrow().shown().key != key {
                                return;
                            }
                            let page = current.borrow().clone();
                            let valid = {
                                let page = page.borrow();
                                page.selected_files() == *selected
                                    && page.focused().map(|i| page.results()[i]) == *focused
                            };
                            if valid && index < frozen.0.len() {
                                *menu_state.borrow_mut() = frozen.clone();
                                if let Ok(index) = i32::try_from(index) {
                                    window.invoke_menu_chosen(index);
                                }
                            }
                        }
                        Action::Calculator => {}
                    }
                }
            });
            (snapshot, launch)
        }
    });
    window.on_command_palette_requested({
        let weak = window.as_weak();
        let slot = slot.clone();
        let pages = pages.clone();
        move || {
            let Some(parent) = weak.upgrade() else {
                return;
            };
            let existing = slot
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(existing) = existing {
                existing.invoke_cancel();
            }
            let settings = pages
                .borrow()
                .store()
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            if let Err(error) = open(&parent, &slot, settings, snapshotter.clone()) {
                eprintln!("could not open command palette: {error}");
            }
        }
    });
}
