//! An importer's file log window, bound (`ui/file_log.slint`): the queue's
//! seeds read from the store, listed as the reference lists them
//! (hydrus-gui-model's [`file_log`](crate::file_log)), selected as its
//! lists select, and the right-click and whole log menus, whose actions
//! change the store at once (as the reference's file log window, a frame,
//! edits its log in place) and read it again.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::HashId;
use hydrus_store::Store;
use hydrus_store::queues::{self, FileSeed, SeedStatus, SeedType, StatusCounts};

use crate::file_log::{
    Action, Entry, LogFacts, OPEN_MANY_QUESTION, delete_question, log_menu, row, row_menu,
};
use crate::list_selection::ListSelection;
use crate::main_menu::{PopupNode, popup};
use crate::popup_menu::{Chosen, Popup};
use crate::{FileLogWindow, TableRow};

/// What the question panel waits on.
enum Asking {
    /// Deleting the selected.
    Delete(Vec<i64>),
    /// Opening many selected sources.
    OpenMany(Vec<String>),
}

struct State {
    queue: i64,
    seeds: Vec<FileSeed>,
    selection: ListSelection<i64>,
    asking: Option<Asking>,
}

impl State {
    fn ids(&self) -> Vec<i64> {
        self.seeds.iter().map(|s| s.id).collect()
    }

    fn selected(&self) -> Vec<&FileSeed> {
        let ids = self.selection.in_order(&self.ids());
        self.seeds.iter().filter(|s| ids.contains(&s.id)).collect()
    }

    fn facts(&self) -> LogFacts {
        let mut counts = StatusCounts::new();
        for s in &self.seeds {
            *counts.entry(s.status).or_default() += 1;
        }
        LogFacts {
            counts,
            len: self.seeds.len(),
            urls: self
                .seeds
                .first()
                .is_none_or(|s| s.seed_type == SeedType::Url),
        }
    }
}

/// A change to the store.
type StoreChange = Box<dyn FnOnce(&rusqlite::Connection) -> hydrus_store::Result<()> + Send>;

/// A log's columns, as the window shows them: titles and widths, the
/// widest stretching.
pub(crate) fn columns(columns: &[(&str, f32)]) -> ModelRc<crate::TableColumn> {
    let widest = columns.iter().map(|c| c.1).fold(0.0, f32::max);
    let columns: Vec<crate::TableColumn> = columns
        .iter()
        .map(|&(title, width)| crate::TableColumn {
            title: title.into(),
            width,
            stretch: width >= widest,
        })
        .collect();
    ModelRc::new(VecModel::from(columns))
}

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

/// A menu's tree, for the popup.
fn node(entry: &Entry) -> PopupNode<'_, Entry, Action> {
    match entry {
        Entry::Item(label, Action::NotYet | Action::ImportFromClipboard | Action::SearchUrls) => {
            PopupNode::Disabled(label)
        }
        Entry::Item(label, action) => PopupNode::Item(label, action),
        Entry::Label(label) => PopupNode::Label(label),
        Entry::Separator => PopupNode::Separator,
        Entry::Menu(label, entries) => PopupNode::Menu(label, entries),
    }
}

fn read(store: &Store, state: &mut State) {
    match store.read(|c| queues::file_seeds(c, state.queue)) {
        Ok(seeds) => state.seeds = seeds,
        Err(e) => eprintln!("could not read the file log: {e}"),
    }
    // (the selection keeps only what is still there)
    let kept = state.selection.in_order(&state.ids());
    state.selection.select_many(&kept);
}

fn show(window: &FileLogWindow, state: &State) {
    let now = now();
    let rows: Vec<TableRow> = state
        .seeds
        .iter()
        .enumerate()
        .map(|(i, seed)| {
            let cells: Vec<SharedString> = row(seed, i, now).into_iter().map(Into::into).collect();
            TableRow {
                cells: ModelRc::new(VecModel::from(cells)),
                selected: state.selection.is_selected(seed.id),
            }
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_status(queues::file_log_status(&state.facts().counts).into());
    let question = match &state.asking {
        None => None,
        Some(Asking::Delete(ids)) => Some(delete_question(ids.len())),
        Some(Asking::OpenMany(_)) => Some(OPEN_MANY_QUESTION.to_owned()),
    };
    window.set_asking(question.is_some());
    if let Some(message) = question {
        window.set_asking_title("Are you sure?".into());
        window.set_asking_message(message.into());
        let choices: Vec<SharedString> = vec!["yes".into(), "no".into()];
        window.set_asking_choices(ModelRc::new(VecModel::from(choices)));
    }
}

/// The selected seeds' files, as hash ids.
fn hash_ids(store: &Store, seeds: &[&FileSeed]) -> Vec<HashId> {
    let hashes: Vec<hydrus_core::Sha256> = seeds
        .iter()
        .filter_map(|s| s.meta.hash("sha256")?.parse().ok())
        .collect();
    let found = store
        .read(|c| hydrus_store::master::hash_ids(c, &hashes))
        .unwrap_or_default();
    hashes
        .iter()
        .filter_map(|h| found.get(h).copied())
        .collect()
}

/// Do a menu's action.
fn act(store: &Store, state: &mut State, action: &Action, open_files: &dyn Fn(Vec<HashId>)) {
    let queue = state.queue;
    let now = now();
    // (each change nudges the daemon, which works the queue)
    let write = |f: StoreChange| {
        if let Err(e) = store.write(move |ctx| {
            f(ctx.conn())?;
            queues::nudge(ctx.conn(), queue)
        }) {
            eprintln!("could not change the file log: {e}");
        }
    };
    let selected_ids: Vec<i64> = state.selected().iter().map(|s| s.id).collect();
    match action {
        Action::RetryFailed => write(Box::new(move |c| {
            queues::retry_file_seeds(c, queue, &[SeedStatus::Error], now).map(|_| ())
        })),
        Action::RetryIgnored => write(Box::new(move |c| {
            queues::retry_file_seeds(c, queue, &[SeedStatus::Vetoed], now).map(|_| ())
        })),
        Action::DeleteStatuses(statuses) => {
            let statuses = statuses.clone();
            write(Box::new(move |c| {
                queues::remove_file_seeds(c, queue, &statuses).map(|_| ())
            }));
        }
        Action::SkipUnknown => {
            let unknown: Vec<i64> = state
                .seeds
                .iter()
                .filter(|s| s.status == SeedStatus::Unknown)
                .map(|s| s.id)
                .collect();
            write(Box::new(move |c| {
                queues::set_file_seed_statuses(c, &unknown, SeedStatus::Skipped, now)
            }));
        }
        Action::ShowFiles { new_only } => {
            let seeds: Vec<&FileSeed> = state
                .seeds
                .iter()
                .filter(|s| {
                    if *new_only {
                        s.status == SeedStatus::SuccessfulAndNew
                    } else {
                        s.status.is_successful()
                    }
                })
                .collect();
            let files = hash_ids(store, &seeds);
            if !files.is_empty() {
                open_files(files);
            }
        }
        Action::Reverse => write(Box::new(move |c| queues::reverse_file_seeds(c, queue))),
        Action::ExportToClipboard => {
            let all: Vec<&str> = state.seeds.iter().map(|s| s.data.as_str()).collect();
            crate::copy_to_clipboard(&all.join("\n"));
        }
        Action::OpenSelectedFiles => {
            let files = hash_ids(store, &state.selected());
            if !files.is_empty() {
                open_files(files);
            }
        }
        Action::CopySources => {
            let sources: Vec<&str> = state.selected().iter().map(|s| s.data.as_str()).collect();
            if !sources.is_empty() {
                crate::copy_to_clipboard(&sources.join("\n"));
            }
        }
        Action::CopyNotes => {
            let notes: Vec<&str> = state
                .selected()
                .iter()
                .map(|s| s.note.as_str())
                .filter(|n| !n.is_empty())
                .collect();
            if !notes.is_empty() {
                crate::copy_to_clipboard(&notes.join("\n\n"));
            }
        }
        Action::OpenSources => {
            let sources: Vec<String> = state.selected().iter().map(|s| s.data.clone()).collect();
            if sources.len() > 10 {
                state.asking = Some(Asking::OpenMany(sources));
            } else {
                open_sources(&sources);
            }
        }
        Action::TryAgain | Action::Skip => {
            let status = if *action == Action::TryAgain {
                SeedStatus::Unknown
            } else {
                SeedStatus::Skipped
            };
            write(Box::new(move |c| {
                queues::set_file_seed_statuses(c, &selected_ids, status, now)
            }));
        }
        Action::DeleteSelected => {
            if !selected_ids.is_empty() {
                state.asking = Some(Asking::Delete(selected_ids));
            }
        }
        Action::ImportFromClipboard | Action::SearchUrls | Action::NotYet => {}
    }
}

/// Open URLs in the browser, or paths' folders.
fn open_sources(sources: &[String]) {
    for source in sources {
        if source.starts_with("http") {
            crate::launch(source);
        } else if let Some(dir) = std::path::Path::new(source).parent() {
            crate::launch(&dir.to_string_lossy());
        }
    }
}

/// Open the file log of `queue`; `open_files` shows files in a new page.
/// It forgets itself from `slot` when closed.
pub(crate) fn open(
    store: &Arc<Store>,
    queue: i64,
    slot: &Rc<RefCell<Option<FileLogWindow>>>,
    open_files: &Rc<dyn Fn(Vec<HashId>)>,
) -> Result<FileLogWindow, String> {
    let window = FileLogWindow::new().map_err(|e| e.to_string())?;
    let state = Rc::new(RefCell::new(State {
        queue,
        seeds: Vec::new(),
        selection: ListSelection::default(),
        asking: None,
    }));
    read(store, &mut state.borrow_mut());
    // (the reference's widths, in characters)
    window.set_columns(columns(&[
        ("#", 40.0),
        ("source", 300.0),
        ("status", 100.0),
        ("added", 150.0),
        ("last modified", 150.0),
        ("source time", 150.0),
        ("note", 160.0),
    ]));
    let popup: Rc<Popup<Action>> = Popup::new();
    window.set_menu_panes(popup.model());
    let refresh = {
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        move |reread: bool| {
            if reread {
                read(&store, &mut state.borrow_mut());
            }
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow());
            }
        }
    };
    let refresh: Rc<dyn Fn(bool)> = Rc::new(refresh);
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    window.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |r, ctrl, shift| {
            if let Ok(r) = usize::try_from(r) {
                let mut state = state.borrow_mut();
                let ids = state.ids();
                state.selection.click(&ids, r, ctrl, shift);
            }
            refresh(false);
        }
    });
    window.on_row_activated({
        let state = state.clone();
        let store = store.clone();
        let open_files = open_files.clone();
        move |_| {
            let files = hash_ids(&store, &state.borrow().selected());
            if !files.is_empty() {
                open_files(files);
            }
        }
    });
    // the right-click menu: on the row pressed (selecting it, if it isn't)
    window.on_row_menu({
        let state = state.clone();
        let popup = popup.clone();
        let refresh = refresh.clone();
        move |r, x, y| {
            {
                let mut state = state.borrow_mut();
                let ids = state.ids();
                if let Some(&id) = usize::try_from(r).ok().and_then(|r| ids.get(r))
                    && !state.selection.is_selected(id)
                {
                    state.selection.select_many(&[id]);
                }
            }
            refresh(false);
            let state = state.borrow();
            let entries = row_menu(&state.selected(), &state.facts());
            let (entries, actions) = popup_entries(&entries);
            popup.open(entries, actions, x, y);
        }
    });
    window.on_log_menu({
        let state = state.clone();
        let popup = popup.clone();
        move |x, y| {
            let state = state.borrow();
            let entries = log_menu(&state.facts(), !state.selection.is_empty());
            let (entries, actions) = popup_entries(&entries);
            popup.open(entries, actions, x, y);
        }
    });
    window.on_menu_line_hovered({
        let popup = popup.clone();
        move |p, l, right, top, left| popup.hover(p, l, right, top, left)
    });
    window.on_menu_placed({
        let popup = popup.clone();
        move |p, x, y, w| popup.placed(p, x, y, w)
    });
    window.on_menu_dismissed({
        let popup = popup.clone();
        move || popup.close()
    });
    window.on_menu_line_clicked({
        let popup = popup.clone();
        let state = state.clone();
        let store = store.clone();
        let refresh = refresh.clone();
        let open_files = open_files.clone();
        move |p, l, right, top, left| {
            match popup.click(p, l, right, top, left) {
                Some(Chosen::Action(action)) => {
                    act(&store, &mut state.borrow_mut(), &action, &*open_files);
                }
                Some(Chosen::Copy(text)) => crate::copy_to_clipboard(&text),
                None => return,
            }
            refresh(true);
        }
    });
    window.on_delete_pressed({
        let state = state.clone();
        let store = store.clone();
        let refresh = refresh.clone();
        let open_files = open_files.clone();
        move || {
            act(
                &store,
                &mut state.borrow_mut(),
                &Action::DeleteSelected,
                &*open_files,
            );
            refresh(false);
        }
    });
    window.on_chosen({
        let state = state.clone();
        let store = store.clone();
        let refresh = refresh.clone();
        move |index| {
            let asking = state.borrow_mut().asking.take();
            if index == 0 {
                match asking {
                    Some(Asking::Delete(ids)) => {
                        if let Err(e) = store
                            .write(move |ctx| queues::remove_file_seeds_by_id(ctx.conn(), &ids))
                        {
                            eprintln!("could not change the file log: {e}");
                        }
                    }
                    Some(Asking::OpenMany(sources)) => open_sources(&sources),
                    None => {}
                }
            }
            refresh(true);
        }
    });
    window.on_cancelled({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().asking = None;
            refresh(false);
        }
    });
    window.on_close_window({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    show(&window, &state.borrow());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}

/// A file log menu as the popup's entries and actions.
fn popup_entries(entries: &[Entry]) -> (Vec<crate::main_menu::Entry>, Vec<Action>) {
    popup(entries, &node)
}
