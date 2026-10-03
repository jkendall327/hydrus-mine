//! The "review actions" window for a duplicates auto-resolution rule,
//! bound (`ui/auto_resolution_review.slint`, its words hydrus-gui-model's
//! [`auto_resolution_review`](crate::auto_resolution_review)): its three
//! tabs' pairs read from the store, approving and denying pending pairs,
//! and undoing actions taken (dissolving the files' groups, undeleting
//! them) and denials (queueing the pairs to be searched again), as the
//! reference's `ReviewActionsPanel` does.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::{DuplicateType, HashId};
use hydrus_store::Store;
use hydrus_store::duplicates::auto::{
    self, DeniedPair, GroupPair, OperationMode, PairStatus, Rule,
};

use crate::auto_resolution_review::{
    BOXES, COLUMNS, FETCHING, FULLY_AUTOMATIC, TABS, TITLE, actioned_cell, approve_question,
    denied_cell, deny_question, found, pending_cell, remaining, reselect, start_tab,
    undo_actioned_question, undo_denied_question,
};
use crate::list_selection::ListSelection;
use crate::{AutoResolutionReviewWindow, PairRow};

const PENDING: usize = 0;
const ACTIONED: usize = 1;
const DENIED: usize = 2;

/// What the window's panel asks, and what is done on yes.
#[derive(Debug, Clone)]
enum Asking {
    Approve(Vec<usize>),
    Deny(Vec<usize>),
    UndoActioned(Vec<HashId>),
    UndoDenied(Vec<GroupPair>),
}

struct State {
    store: Arc<Store>,
    rule_id: i64,
    rule: Rule,
    tab: usize,
    pending: Vec<(GroupPair, HashId, HashId)>,
    actioned: Vec<(HashId, HashId, DuplicateType, i64)>,
    denied: Vec<DeniedPair>,
    labels: [String; 3],
    /// Fetched since opening (a tab not yet shown isn't).
    fetched: [bool; 3],
    /// To fetch again when next shown (`_we_have_done_...`).
    stale: [bool; 3],
    /// "only sample this many" (`None`: fetch all).
    limits: [Option<usize>; 3],
    selections: [ListSelection<usize>; 3],
    asking: Option<Asking>,
    thumbs: HashMap<HashId, slint::Image>,
    /// Each pending pair's "action" cell, once worked out.
    pending_texts: HashMap<(HashId, HashId), String>,
}

fn now() -> i64 {
    hydrus_core::TimestampMs::now().millis() / 1000
}

impl State {
    fn len(&self, tab: usize) -> usize {
        match tab {
            PENDING => self.pending.len(),
            ACTIONED => self.actioned.len(),
            _ => self.denied.len(),
        }
    }

    fn order(&self, tab: usize) -> Vec<usize> {
        (0..self.len(tab)).collect()
    }

    fn selected(&self, tab: usize) -> Vec<usize> {
        self.selections[tab].in_order(&self.order(tab))
    }

    fn files(&self, tab: usize, row: usize) -> (HashId, HashId) {
        match tab {
            PENDING => (self.pending[row].1, self.pending[row].2),
            ACTIONED => (self.actioned[row].0, self.actioned[row].1),
            _ => self.denied[row].kings,
        }
    }

    /// Read a tab's pairs from the store afresh.
    fn fetch(&mut self, tab: usize) {
        self.selections[tab] = ListSelection::default();
        self.fetched[tab] = true;
        self.stale[tab] = false;
        if tab == PENDING {
            self.pending_texts.clear();
        }
        let (id, limit) = (self.rule_id, self.limits[tab]);
        let read = match tab {
            PENDING => {
                if self.rule.mode == OperationMode::FullyAutomatic {
                    self.pending.clear();
                    FULLY_AUTOMATIC.clone_into(&mut self.labels[tab]);
                    return;
                }
                self.store
                    .read(|c| auto::pending_pairs(c, id, limit))
                    .map(|rows| self.pending = rows)
            }
            ACTIONED => self
                .store
                .read(|c| auto::actioned(c, id, limit))
                .map(|rows| self.actioned = rows),
            _ => self
                .store
                .read(|c| auto::denied_pairs(c, id, limit))
                .map(|rows| self.denied = rows),
        };
        self.labels[tab] = match read {
            Ok(()) => found(self.len(tab)),
            Err(e) => format!("could not read the pairs: {e}"),
        };
    }

    /// Show `tab`, fetching it if it has nothing or is stale
    /// (`_CurrentPageChanged`).
    fn choose_tab(&mut self, tab: usize) {
        self.tab = tab;
        if !self.fetched[tab] || self.len(tab) == 0 || self.stale[tab] {
            self.fetch(tab);
        }
    }

    fn thumb(&mut self, id: HashId) -> slint::Image {
        if let Some(image) = self.thumbs.get(&id) {
            return image.clone();
        }
        let image = crate::thumbnails::thumbnail(&self.store, id)
            .map(|raster| crate::thumbnails::Pixels::new(&raster).image())
            .unwrap_or_default();
        self.thumbs.insert(id, image.clone());
        image
    }

    fn pending_text(&mut self, a: HashId, b: HashId) -> String {
        if let Some(text) = self.pending_texts.get(&(a, b)) {
            return text.clone();
        }
        let merge = crate::auto_resolution_review::pending_summary(&self.store, &self.rule, a, b);
        if let Err(e) = &merge {
            eprintln!("could not summarise the duplicate merge: {e}");
        }
        let text = pending_cell(&self.rule, merge.ok().as_deref());
        self.pending_texts.insert((a, b), text.clone());
        text
    }

    /// Approve or deny the pending rows `rows`, as one decision each.
    fn decide(&mut self, rows: &[usize], approve: bool) {
        let pairs: Vec<(HashId, HashId)> = rows
            .iter()
            .filter_map(|&r| self.pending.get(r).map(|p| (p.1, p.2)))
            .collect();
        let done = if approve {
            hydrus_duplicates::engine::approve(&self.store, self.rule_id, &pairs)
        } else {
            hydrus_duplicates::engine::deny(&self.store, self.rule_id, &pairs)
        };
        if let Err(e) = done {
            eprintln!("could not record the decisions: {e}");
        }
        let earliest = rows.iter().copied().min().unwrap_or(0);
        let taken: BTreeSet<usize> = rows.iter().copied().collect();
        let mut row = 0;
        self.pending.retain(|_| {
            let keep = !taken.contains(&row);
            row += 1;
            keep
        });
        self.labels[PENDING] = remaining(self.pending.len());
        self.selections[PENDING] = ListSelection::default();
        match reselect(earliest, self.pending.len()) {
            Some(row) => self.selections[PENDING].select_many(&[row]),
            None => self.fetch(PENDING),
        }
        self.stale[if approve { ACTIONED } else { DENIED }] = true;
    }

    fn undo_actioned(&mut self, files: &[HashId]) {
        let store = self.store.clone();
        let files_ = files.to_vec();
        let done = crate::media_actions::undelete(&store, files).and_then(|()| {
            store.write_content(move |w| {
                let local = w.roles().local_file_storage;
                hydrus_store::duplicates::write::RelationshipWriter::new(w.conn(), local)
                    .dissolve_groups_of(&files_)
            })
        });
        if let Err(e) = done {
            eprintln!("could not undo the actions: {e}");
        }
        // (the log keeps its entries)
        self.stale[PENDING] = true;
    }

    fn undo_denied(&mut self, pairs: &[GroupPair]) {
        let (id, pairs) = (self.rule_id, pairs.to_vec());
        let done = self
            .store
            .write(move |ctx| auto::set_status(ctx.conn(), id, &pairs, PairStatus::NotSearched));
        if let Err(e) = done {
            eprintln!("could not undo the denials: {e}");
        }
        self.stale[PENDING] = true;
        self.fetch(DENIED);
    }
}

fn strings(items: &[&str]) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.iter().map(|&s| s.into()).collect();
    ModelRc::new(VecModel::from(items))
}

fn show(window: &AutoResolutionReviewWindow, state: &mut State) {
    let tab = state.tab;
    window.set_tab(i32::try_from(tab).unwrap_or(0));
    window.set_box_title(BOXES[tab].into());
    window.set_columns(strings(&COLUMNS[tab]));
    window.set_label(state.labels[tab].as_str().into());
    let now = now();
    let rows: Vec<PairRow> = (0..state.len(tab))
        .map(|r| {
            let (a, b) = state.files(tab, r);
            let text = match tab {
                PENDING => state.pending_text(a, b),
                ACTIONED => {
                    let (_, _, t, when) = state.actioned[r];
                    actioned_cell(t, when, now)
                }
                _ => denied_cell(state.denied[r].timestamp_ms, now),
            };
            PairRow {
                a: state.thumb(a),
                b: state.thumb(b),
                text: text.into(),
                selected: state.selections[tab].is_selected(r),
            }
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    let limit = state.limits[tab];
    window.set_fetch_all(limit.is_none());
    if let Some(limit) = limit {
        window.set_fetch_limit(i32::try_from(limit).unwrap_or(i32::MAX));
    }
    let selected = state.selected(tab).len();
    window.set_can_act(selected > 0);
    window.set_can_select_all(selected < state.len(tab));
    window.set_can_undo(selected > 0);
    window.set_asking(state.asking.is_some());
    let message = match &state.asking {
        None => return,
        Some(Asking::Approve(rows)) => approve_question(rows.len()).unwrap_or_default(),
        Some(Asking::Deny(rows)) => deny_question(rows.len()).unwrap_or_default(),
        Some(Asking::UndoActioned(files)) => undo_actioned_question(files.len()),
        Some(Asking::UndoDenied(pairs)) => undo_denied_question(pairs.len()),
    };
    window.set_asking_title("Are you sure?".into());
    window.set_asking_message(message.into());
    window.set_asking_choices(strings(&["yes", "no"]));
}

/// Opens the media viewer on files, from the one at an index.
pub(crate) type OpenViewer = Rc<dyn Fn(Vec<HashId>, usize)>;

/// The open review windows, each with a number of its own.
pub(crate) type Windows = Rc<RefCell<Vec<(u64, AutoResolutionReviewWindow)>>>;

/// Close every review window (as the reference does when the rules are
/// edited).
pub(crate) fn close_all(windows: &Windows) {
    let open: Vec<_> = windows.borrow_mut().drain(..).collect();
    for (_, window) in open {
        let _ = window.hide();
    }
}

/// Open a window on rule `rule_id`, kept in `windows` until closed.
pub(crate) fn open(
    store: &Arc<Store>,
    rule_id: i64,
    rule: Rule,
    windows: &Windows,
    filter: &Rc<RefCell<Option<crate::DuplicateFilterWindow>>>,
    open_viewer: Option<OpenViewer>,
) -> Result<(), String> {
    let window = AutoResolutionReviewWindow::new().map_err(|e| e.to_string())?;
    window.set_window_title(TITLE.into());
    window.set_rule_name(rule.name.as_str().into());
    window.set_tabs(strings(&TABS));
    let tab = start_tab(&rule);
    let state = Rc::new(RefCell::new(State {
        store: store.clone(),
        rule_id,
        rule,
        tab,
        pending: Vec::new(),
        actioned: Vec::new(),
        denied: Vec::new(),
        labels: Default::default(),
        fetched: [false; 3],
        stale: [false; 3],
        limits: [Some(crate::auto_resolution_review::DEFAULT_FETCH); 3],
        selections: Default::default(),
        asking: None,
        thumbs: HashMap::new(),
        pending_texts: HashMap::new(),
    }));
    {
        // (pending and denied are fetched as it opens, and the tab shown)
        let mut state = state.borrow_mut();
        state.labels = [
            FETCHING.to_owned(),
            FETCHING.to_owned(),
            FETCHING.to_owned(),
        ];
        state.fetch(PENDING);
        state.fetch(DENIED);
        state.choose_tab(tab);
    }
    let refresh = {
        let state = state.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                show(&window, &mut state.borrow_mut());
            }
        })
    };
    window.on_tab_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        move |tab| {
            if let Ok(tab) = usize::try_from(tab)
                && tab < 3
            {
                state.borrow_mut().choose_tab(tab);
            }
            refresh();
        }
    });
    window.on_refresh({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let mut state = state.borrow_mut();
            let tab = state.tab;
            state.fetch(tab);
            drop(state);
            refresh();
        }
    });
    window.on_fetch_changed({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            let tab = state.tab;
            state.limits[tab] = if window.get_fetch_all() {
                None
            } else {
                usize::try_from(window.get_fetch_limit()).ok()
            };
        }
    });
    window.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                let mut state = state.borrow_mut();
                let tab = state.tab;
                let order = state.order(tab);
                state.selections[tab].click(&order, row, ctrl, shift);
            }
            refresh();
        }
    });
    // a pending pair: the duplicate filter on the pending pairs from it,
    // round to the start, to approve or deny as well as decide on; every
    // tab is fetched again if it did any work. An actioned or denied pair:
    // the media viewer on its files still stored, from A.
    window.on_row_activated({
        let state = state.clone();
        let refresh = refresh.clone();
        let filter = filter.clone();
        move |row| {
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            let (store, rule_id, rule, pairs) = {
                let state = state.borrow();
                if row >= state.len(state.tab) {
                    return;
                }
                if state.tab != PENDING {
                    let (a, b) = state.files(state.tab, row);
                    let store = state.store.clone();
                    drop(state);
                    let snapshot = store.snapshot();
                    let local = snapshot
                        .services
                        .builtin(hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE);
                    let stored = local.map_err(anyhow::Error::from).and_then(|local| {
                        let local = local.id;
                        store
                            .read(|c| hydrus_store::media::current_in(c, local, &[a, b]))
                            .map_err(anyhow::Error::from)
                    });
                    let files: Vec<HashId> = match stored {
                        Ok(stored) => [a, b].into_iter().filter(|f| stored.contains(f)).collect(),
                        Err(e) => {
                            eprintln!("{e}");
                            return;
                        }
                    };
                    if files.is_empty() {
                        eprintln!("{}", crate::auto_resolution_review::NOTHING_LOCAL_IN_VIEWER);
                    } else if let Some(open_viewer) = &open_viewer {
                        open_viewer(files, 0);
                    }
                    return;
                }
                let pairs: Vec<(HashId, HashId)> = state.pending[row..]
                    .iter()
                    .chain(&state.pending[..row])
                    .map(|&(_, a, b)| (a, b))
                    .collect();
                (
                    state.store.clone(),
                    state.rule_id,
                    state.rule.clone(),
                    pairs,
                )
            };
            let exited: Rc<dyn Fn()> = {
                let state = state.clone();
                let refresh = refresh.clone();
                Rc::new(move || {
                    let mut state = state.borrow_mut();
                    for tab in [PENDING, ACTIONED, DENIED] {
                        state.fetch(tab);
                    }
                    drop(state);
                    refresh();
                })
            };
            let opened =
                crate::duplicate_filter::DuplicateFilter::for_review(store, rule_id, rule, pairs)
                    .and_then(|mut model| {
                        let step = model.load_batch();
                        if matches!(step, Ok(crate::duplicate_filter::Step::Finished)) {
                            return Err(anyhow::anyhow!(
                                crate::auto_resolution_review::NOTHING_LOCAL_IN_FILTER
                            ));
                        }
                        crate::filter_window::open_filter(model, step, &filter, Some(exited))
                            .map_err(anyhow::Error::from)
                    });
            match opened {
                Ok(window) => *filter.borrow_mut() = Some(window),
                Err(e) => eprintln!("{e}"),
            }
        }
    });
    window.on_select_all({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let mut state = state.borrow_mut();
            let order = state.order(PENDING);
            state.selections[PENDING].select_many(&order);
            drop(state);
            refresh();
        }
    });
    let decide = {
        let state = state.clone();
        let refresh = refresh.clone();
        Rc::new(move |approve: bool| {
            let mut state = state.borrow_mut();
            let rows = state.selected(PENDING);
            if rows.is_empty() {
                return;
            }
            let asks = if approve {
                approve_question(rows.len())
            } else {
                deny_question(rows.len())
            };
            if asks.is_some() {
                state.asking = Some(if approve {
                    Asking::Approve(rows)
                } else {
                    Asking::Deny(rows)
                });
            } else {
                state.decide(&rows, approve);
            }
            drop(state);
            refresh();
        })
    };
    window.on_approve({
        let decide = decide.clone();
        move || decide(true)
    });
    window.on_deny({
        let decide = decide.clone();
        move || decide(false)
    });
    window.on_undo({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let mut state = state.borrow_mut();
            let tab = state.tab;
            let rows = state.selected(tab);
            if rows.is_empty() {
                return;
            }
            state.asking = Some(if tab == ACTIONED {
                let files: BTreeSet<HashId> = rows
                    .iter()
                    .flat_map(|&r| {
                        let (a, b) = state.files(ACTIONED, r);
                        [a, b]
                    })
                    .collect();
                Asking::UndoActioned(files.into_iter().collect())
            } else {
                let pairs: BTreeSet<GroupPair> =
                    rows.iter().map(|&r| state.denied[r].pair).collect();
                Asking::UndoDenied(pairs.into_iter().collect())
            });
            drop(state);
            refresh();
        }
    });
    window.on_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        move |index| {
            let mut state = state.borrow_mut();
            let asking = state.asking.take();
            if index == 0 {
                match asking {
                    Some(Asking::Approve(rows)) => state.decide(&rows, true),
                    Some(Asking::Deny(rows)) => state.decide(&rows, false),
                    Some(Asking::UndoActioned(files)) => state.undo_actioned(&files),
                    Some(Asking::UndoDenied(pairs)) => state.undo_denied(&pairs),
                    None => {}
                }
            }
            drop(state);
            refresh();
        }
    });
    window.on_cancelled({
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            state.borrow_mut().asking = None;
            refresh();
        }
    });
    let number = windows
        .borrow()
        .iter()
        .map(|(n, _)| n + 1)
        .max()
        .unwrap_or(0);
    let close = {
        let windows = windows.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            windows.borrow_mut().retain(|(n, _)| *n != number);
        })
    };
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
    show(&window, &mut state.borrow_mut());
    window.show().map_err(|e| e.to_string())?;
    windows.borrow_mut().push((number, window));
    Ok(())
}
