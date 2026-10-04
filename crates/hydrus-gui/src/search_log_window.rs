//! A gallery search's or watcher's search log window (a watcher's "check
//! log"), bound: `ui/file_log.slint`'s window with the search log's
//! columns, its pages read from the store and listed as the reference
//! lists them (hydrus-gui-model's [`search_log`](crate::search_log)), and
//! the right-click and whole log menus, whose actions change the store at
//! once and nudge the daemon to work the queue again.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_store::Store;
use hydrus_store::queues::{
    self, GallerySeed, NewGallerySeed, QueueKind, SeedStatus, StatusCounts,
};

use crate::file_log::Entry;
use crate::file_log_window::columns;
use crate::list_selection::ListSelection;
use crate::main_menu::{self, PopupNode};
use crate::popup_menu::{Chosen, Popup};
use crate::search_log::{Action, LogFacts, delete_question, log_menu, row, row_menu};
use crate::{FileLogWindow, TableRow};

struct State {
    queue: i64,
    /// "search", or a watcher's "check".
    kind: &'static str,
    read_only: bool,
    can_generate_more_pages: bool,
    seeds: Vec<GallerySeed>,
    selection: ListSelection<i64>,
    /// Deleting this status's entries, asked.
    asking: Option<SeedStatus>,
    exports: crate::png_export_window::Slots,
    imports: crate::search_log_import_window::Slots,
    exchange_closed: Rc<dyn Fn()>,
}

impl State {
    fn ids(&self) -> Vec<i64> {
        self.seeds.iter().map(|s| s.id).collect()
    }

    fn selected(&self) -> Vec<&GallerySeed> {
        self.seeds
            .iter()
            .filter(|s| self.selection.is_selected(s.id))
            .collect()
    }

    fn counts(&self) -> StatusCounts {
        let mut counts = StatusCounts::new();
        for s in &self.seeds {
            *counts.entry(s.status).or_default() += 1;
        }
        counts
    }

    fn facts(&self) -> LogFacts {
        LogFacts {
            counts: self.counts(),
            len: self.seeds.len(),
            last_failed: self
                .seeds
                .last()
                .is_some_and(|s| s.status == SeedStatus::Error),
            read_only: self.read_only,
            can_generate_more_pages: self.can_generate_more_pages,
        }
    }
}

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

fn node(entry: &Entry<Action>) -> PopupNode<'_, Entry<Action>, Action> {
    match entry {
        Entry::Item(label, action) => PopupNode::Item(label, action),
        Entry::Label(label) => PopupNode::Label(label),
        Entry::Separator => PopupNode::Separator,
        Entry::Menu(label, entries) => PopupNode::Menu(label, entries),
    }
}

fn read(store: &Store, state: &mut State) {
    match store.read(|c| queues::gallery_seeds(c, state.queue)) {
        Ok(seeds) => state.seeds = seeds,
        Err(e) => eprintln!("could not read the search log: {e}"),
    }
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
    window.set_status(queues::search_log_status(&state.counts()).0.into());
    window.set_asking(state.asking.is_some());
    window.set_busy(state.exports.has_open() || state.imports.has_open());
    if let Some(status) = state.asking {
        window.set_asking_title("Are you sure?".into());
        window.set_asking_message(delete_question(status, state.kind).into());
        let choices: Vec<SharedString> = vec!["yes".into(), "no".into()];
        window.set_asking_choices(ModelRc::new(VecModel::from(choices)));
    }
}

/// A page read again, after `seed` (`GenerateRestartedDuplicate`): a new
/// run, so the log takes it though its URL is there already.
fn restarted(seed: &GallerySeed, can_generate_more_pages: bool) -> NewGallerySeed {
    let mut meta = seed.meta.clone();
    meta.run_token = hex::encode(rand::random::<[u8; 32]>());
    meta.force_next_page_url_generation = can_generate_more_pages;
    NewGallerySeed {
        url: seed.url.clone(),
        can_generate_more_pages,
        referral_url: seed.referral_url.clone(),
        meta,
    }
}

type StoreChange = Box<dyn FnOnce(&rusqlite::Connection) -> hydrus_store::Result<()> + Send>;

fn blocked(state: &State) -> bool {
    state.asking.is_some() || state.exports.has_open() || state.imports.has_open()
}

fn report_error(store: &Arc<Store>, state: &State, title: &str, error: String) {
    if let Err(error) = crate::search_log_import_window::error(
        &state.imports,
        store,
        title,
        error,
        state.exchange_closed.clone(),
    ) {
        eprintln!("could not show search log import error: {error}");
    }
}

/// Do a menu's action.
fn act(store: &Arc<Store>, state: &mut State, action: &Action) {
    let queue = state.queue;
    let now = now();
    let write = |f: StoreChange| {
        if let Err(e) = store.write(move |ctx| {
            f(ctx.conn())?;
            queues::nudge(ctx.conn(), queue)
        }) {
            eprintln!("could not change the search log: {e}");
        }
    };
    match action {
        Action::DeleteStatus(status) => state.asking = Some(*status),
        Action::RestartFailedSearch => {
            if let Some(last) = state.seeds.last().filter(|s| s.status == SeedStatus::Error) {
                let new = restarted(last, true);
                write(Box::new(move |c| {
                    queues::add_gallery_seeds(c, queue, &[new], None, now).map(|_| ())
                }));
            }
        }
        Action::ExportToClipboard => {
            let all: Vec<&str> = state.seeds.iter().map(|s| s.url.as_str()).collect();
            crate::copy_to_clipboard(&all.join("\n"));
        }
        Action::CopyUrls => {
            let urls: Vec<&str> = state.selected().iter().map(|s| s.url.as_str()).collect();
            if !urls.is_empty() {
                crate::copy_to_clipboard(&urls.join("\n"));
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
        Action::OpenUrls => {
            for seed in state.selected() {
                crate::launch(&seed.url);
            }
        }
        Action::TryAgain(more) => {
            let pages: Vec<(GallerySeed, NewGallerySeed)> = state
                .selected()
                .into_iter()
                .map(|s| (s.clone(), restarted(s, *more)))
                .collect();
            write(Box::new(move |c| {
                for (after, new) in pages {
                    queues::add_gallery_seeds(c, queue, &[new], Some(&after), now)?;
                }
                Ok(())
            }));
        }
        Action::Skip => {
            let pages: Vec<GallerySeed> = state.selected().into_iter().cloned().collect();
            write(Box::new(move |c| {
                for mut seed in pages {
                    seed.status = SeedStatus::Skipped;
                    seed.modified = now;
                    queues::update_gallery_seed(c, &seed)?;
                }
                Ok(())
            }));
        }
        Action::ExportObjects => match crate::search_log::export_objects(&state.selected()) {
            Ok(text) => crate::copy_to_clipboard(&text),
            Err(error) => report_error(store, state, "Could not export!", error),
        },
        Action::ExportToPng => {
            let payload = state
                .seeds
                .iter()
                .map(|s| s.url.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if let Err(error) = crate::png_export_window::open(
                &state.exports,
                store,
                payload,
                state.exchange_closed.clone(),
            ) {
                report_error(store, state, "Could not export!", error);
            }
        }
        Action::ImportFromClipboard | Action::ImportFromPng => {
            if state.read_only {
                return;
            }
            let title = if *action == Action::ImportFromPng {
                "Could not import!"
            } else {
                "Problem importing from clipboard!"
            };
            let result = if *action == Action::ImportFromPng {
                crate::png_export_window::import_text_with_title("select the png with the urls")
            } else {
                crate::from_clipboard().map(Some)
            };
            match result {
                Ok(Some(text)) => {
                    if let Err(error) = crate::search_log_import_window::open(
                        &state.imports,
                        store,
                        queue,
                        &text,
                        state.can_generate_more_pages,
                        state.exchange_closed.clone(),
                    ) {
                        report_error(store, state, "Could not import!", error);
                    }
                }
                Err(error) => report_error(store, state, title, error),
                Ok(None) => {}
            }
        }
    }
}

/// Do a whole log menu's action on `queue`'s log, with no window open (a
/// downloader list's menu's); deleting entries of a status, without
/// asking (the caller asks).
pub(crate) fn act_on_queue(
    store: &Arc<Store>,
    queue: i64,
    action: &Action,
    exchange: &crate::file_log_window::OpenFiles,
) {
    let mut state = State {
        queue,
        kind: "search",
        read_only: false,
        can_generate_more_pages: true,
        seeds: Vec::new(),
        selection: ListSelection::default(),
        asking: None,
        exports: exchange.2.clone(),
        imports: exchange.3.clone(),
        exchange_closed: Rc::new(|| {}),
    };
    read(store, &mut state);
    if let Action::DeleteStatus(status) = action {
        let ids: Vec<i64> = state
            .seeds
            .iter()
            .filter(|s| s.status == *status)
            .map(|s| s.id)
            .collect();
        if let Err(e) = store.write(move |ctx| queues::remove_gallery_seeds_by_id(ctx.conn(), &ids))
        {
            eprintln!("could not change the search log: {e}");
        }
        return;
    }
    act(store, &mut state, action);
}

/// Open the search log of `queue` (its "check log", a watcher's). It
/// forgets itself from `slot` when closed.
pub(crate) fn open(
    store: &Arc<Store>,
    queue: i64,
    slot: &Rc<RefCell<Option<FileLogWindow>>>,
) -> Result<FileLogWindow, String> {
    let kind = store
        .read(|c| queues::queue(c, queue))
        .map_err(|e| e.to_string())?
        .map(|q| q.kind);
    let watcher = kind == Some(QueueKind::Watcher);
    let window = FileLogWindow::new().map_err(|e| e.to_string())?;
    let alive = Rc::new(Cell::new(true));
    window.set_window_title(if watcher { "check log" } else { "search log" }.into());
    let state = Rc::new(RefCell::new(State {
        queue,
        kind: if watcher { "check" } else { "search" },
        read_only: watcher || kind == Some(QueueKind::Subscription),
        can_generate_more_pages: !watcher,
        seeds: Vec::new(),
        selection: ListSelection::default(),
        asking: None,
        exports: crate::png_export_window::Slots::default(),
        imports: crate::search_log_import_window::Slots::default(),
        exchange_closed: Rc::new(|| {}),
    }));
    state.borrow_mut().exchange_closed = Rc::new({
        let weak_state = Rc::downgrade(&state);
        let weak_window = window.as_weak();
        let alive = alive.clone();
        let store = store.clone();
        move || {
            if !alive.get() {
                return;
            }
            if let (Some(state), Some(w)) = (weak_state.upgrade(), weak_window.upgrade()) {
                read(&store, &mut state.borrow_mut());
                show(&w, &state.borrow());
            }
        }
    });
    read(store, &mut state.borrow_mut());
    // (the reference's widths, in characters)
    window.set_columns(columns(&[
        ("#", 30.0),
        ("url", 360.0),
        ("status", 100.0),
        ("added", 150.0),
        ("last modified", 150.0),
        ("note", 200.0),
    ]));
    let popup: Rc<Popup<Action>> = Popup::new();
    window.set_menu_panes(popup.model());
    let refresh: Rc<dyn Fn(bool)> = Rc::new({
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
    });
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let state = state.clone();
        let alive = alive.clone();
        move || {
            if !alive.replace(false) {
                return;
            }
            state.borrow().imports.cancel();
            state.borrow().exports.cancel();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    window.on_row_clicked({
        let alive = alive.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        move |r, ctrl, shift| {
            if !alive.get() || blocked(&state.borrow()) {
                return;
            }
            if let Ok(r) = usize::try_from(r) {
                let mut state = state.borrow_mut();
                let ids = state.ids();
                state.selection.click(&ids, r, ctrl, shift);
            }
            refresh(false);
        }
    });
    // (a double-click opens the page in the browser)
    window.on_row_activated({
        let alive = alive.clone();
        let state = state.clone();
        move |_| {
            if !alive.get() || blocked(&state.borrow()) {
                return;
            }
            for seed in state.borrow().selected() {
                crate::launch(&seed.url);
            }
        }
    });
    window.on_row_menu({
        let alive = alive.clone();
        let state = state.clone();
        let popup = popup.clone();
        let refresh = refresh.clone();
        move |r, x, y| {
            if !alive.get() || blocked(&state.borrow()) {
                return;
            }
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
            let (entries, actions) = main_menu::popup(&entries, &node);
            popup.open(entries, actions, x, y);
        }
    });
    window.on_log_menu({
        let alive = alive.clone();
        let state = state.clone();
        let popup = popup.clone();
        move |x, y| {
            if !alive.get() || blocked(&state.borrow()) {
                return;
            }
            let state = state.borrow();
            let entries = log_menu(&state.facts(), !state.selection.is_empty());
            let (entries, actions) = main_menu::popup(&entries, &node);
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
        let alive = alive.clone();
        let popup = popup.clone();
        let state = state.clone();
        let store = store.clone();
        let refresh = refresh.clone();
        move |p, l, right, top, left| {
            if !alive.get() || blocked(&state.borrow()) {
                return;
            }
            match popup.click(p, l, right, top, left) {
                Some(Chosen::Action(action)) => act(&store, &mut state.borrow_mut(), &action),
                Some(Chosen::Copy(text)) => crate::copy_to_clipboard(&text),
                None => return,
            }
            refresh(true);
        }
    });
    // (the reference's search log has no delete key)
    window.on_delete_pressed(|| {});
    window.on_chosen({
        let alive = alive.clone();
        let state = state.clone();
        let store = store.clone();
        let refresh = refresh.clone();
        move |index| {
            if !alive.get() {
                return;
            }
            let (asking, ids) = {
                let mut state = state.borrow_mut();
                let asking = state.asking.take();
                let ids: Vec<i64> = state
                    .seeds
                    .iter()
                    .filter(|s| Some(s.status) == asking)
                    .map(|s| s.id)
                    .collect();
                (asking, ids)
            };
            if index == 0
                && asking.is_some()
                && let Err(e) =
                    store.write(move |ctx| queues::remove_gallery_seeds_by_id(ctx.conn(), &ids))
            {
                eprintln!("could not change the search log: {e}");
            }
            refresh(true);
        }
    });
    window.on_cancelled({
        let alive = alive.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            if !alive.get() {
                return;
            }
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
