//! Non-blocking native network reviews, using the daemon's typed store IPC.

use crate::{BandwidthWindow, EditBandwidthRulesWindow, NetworkJobsWindow, TableColumn, TableRow};
use crossbeam_channel::{Receiver, Sender};
use hydrus_core::{bandwidth::BandwidthType, network::NetworkContext, time::TimestampMs};
use hydrus_gui_model::{
    list_selection::ListSelection,
    network_data::{self as model, Review, RulesDraft},
};
use hydrus_store::{
    Store,
    network_runtime::{self, Command, JobAction},
};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

enum Operation {
    Refresh,
    Save(RulesDraft, bool),
    Reset,
    Job(Command),
    Stop,
}
enum Event {
    Review(Result<Review, String>),
    Written(Result<(), String>),
}
struct Worker {
    send: Sender<Operation>,
    receive: Receiver<Event>,
}
impl Worker {
    fn start(store: Arc<Store>) -> Self {
        let (send, requests) = crossbeam_channel::unbounded();
        let (events, receive) = crossbeam_channel::unbounded();
        std::thread::spawn(move || {
            loop {
                let operation = requests.recv_timeout(Duration::from_millis(500));
                let result = match operation {
                    Ok(Operation::Stop)
                    | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    Ok(Operation::Save(draft, revert)) => Some(draft.apply(&store, revert)),
                    Ok(Operation::Reset) => Some(model::reset_defaults(&store)),
                    Ok(Operation::Job(command)) => Some(
                        store
                            .write(move |ctx| network_runtime::send(ctx.conn(), command))
                            .map_err(|e| e.to_string()),
                    ),
                    Ok(Operation::Refresh) | Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        None
                    }
                };
                if let Some(result) = result {
                    let _ = events.send(Event::Written(result));
                }
                if events
                    .send(Event::Review(Review::load(&store, now())))
                    .is_err()
                {
                    break;
                }
            }
        });
        let _ = send.send(Operation::Refresh);
        Self { send, receive }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.send.send(Operation::Stop);
    }
}
fn now() -> i64 {
    TimestampMs::now().millis() / 1000
}
fn columns(titles: &[(&str, f32)]) -> ModelRc<TableColumn> {
    ModelRc::new(VecModel::from(
        titles
            .iter()
            .map(|(title, width)| TableColumn {
                title: (*title).into(),
                width: *width,
                stretch: *width >= 250.0,
            })
            .collect::<Vec<_>>(),
    ))
}
fn row(cells: impl IntoIterator<Item = String>, selected: bool) -> TableRow {
    TableRow {
        cells: ModelRc::new(VecModel::from(
            cells
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )),
        selected,
    }
}

/// Network review owners retained by the main-window binding.
#[derive(Clone, Default)]
pub struct Slots {
    bandwidth: Rc<RefCell<Option<Rc<Bandwidth>>>>,
    jobs: Rc<RefCell<Option<Rc<Jobs>>>>,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkDataSlots").finish_non_exhaustive()
    }
}
impl Slots {
    /// Close both reviews and any detached rules draft.
    pub fn close(&self) {
        let bandwidth = self.bandwidth.borrow().clone();
        let jobs = self.jobs.borrow().clone();
        if let Some(bandwidth) = bandwidth {
            bandwidth.close();
        }
        if let Some(jobs) = jobs {
            jobs.close();
        }
    }
}
impl Drop for Slots {
    fn drop(&mut self) {
        if Rc::strong_count(&self.bandwidth) == 1 && Rc::strong_count(&self.jobs) == 1 {
            self.close();
        }
    }
}
thread_local! {
    static LAST_BANDWIDTH: RefCell<Option<slint::Weak<BandwidthWindow>>> = const { RefCell::new(None) };
    static LAST_RULES: RefCell<Option<slint::Weak<EditBandwidthRulesWindow>>> = const { RefCell::new(None) };
    static LAST_JOBS: RefCell<Option<slint::Weak<NetworkJobsWindow>>> = const { RefCell::new(None) };
}
/// Visible bandwidth review, for interaction tests.
pub fn last_bandwidth() -> Option<BandwidthWindow> {
    LAST_BANDWIDTH
        .with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}
/// Visible detached rules editor, for interaction tests.
pub fn last_rules() -> Option<EditBandwidthRulesWindow> {
    LAST_RULES
        .with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}
/// Visible current-jobs review, for interaction tests.
pub fn last_jobs() -> Option<NetworkJobsWindow> {
    LAST_JOBS
        .with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}

struct Edit {
    window: EditBandwidthRulesWindow,
}
struct Bandwidth {
    window: BandwidthWindow,
    owner: std::rc::Weak<RefCell<Option<Rc<Bandwidth>>>>,
    worker: Worker,
    timer: Timer,
    active: Cell<bool>,
    review: RefCell<Option<Review>>,
    order: RefCell<Vec<NetworkContext>>,
    selected: RefCell<Option<NetworkContext>>,
    edit: RefCell<Option<Edit>>,
    pending: RefCell<Option<Operation>>,
    error: RefCell<String>,
}
impl Bandwidth {
    fn show(&self) {
        let review = self.review.borrow();
        let Some(review) = review.as_ref() else {
            return;
        };
        let history = match self.window.get_history() {
            1 => Some(86400),
            2 | 3 => None,
            _ => Some(604_800),
        };
        let column = usize::try_from(self.window.get_sort_column())
            .unwrap_or(0)
            .min(7);
        let numeric_column = if column == 4 && self.window.get_history() == 2 {
            5
        } else {
            column
        };
        let mut order = review.contexts();
        let mut rows: Vec<_> = order
            .drain(..)
            .map(|c| {
                let mut cells = review.row(&c, history, now());
                if self.window.get_history() == 2 {
                    let (history, month) = cells.split_at_mut(5);
                    history[4].clone_from(&month[0]);
                }
                let key = review.sort_key(&c, numeric_column, history, now());
                (c, cells, key)
            })
            .collect();
        rows.sort_by(|a, b| a.2.cmp(&b.2));
        if !self.window.get_ascending() {
            rows.reverse();
        }
        let selected = self.selected.borrow();
        *self.order.borrow_mut() = rows.iter().map(|(c, _, _)| c.clone()).collect();
        self.window.set_rows(ModelRc::new(VecModel::from(
            rows.into_iter()
                .map(|(c, cells, _)| row(cells, selected.as_ref() == Some(&c)))
                .collect::<Vec<_>>(),
        )));
        self.window.set_can_edit(selected.is_some());
        self.window.set_can_revert(
            selected
                .as_ref()
                .is_some_and(|c| !c.is_default() && c.kind != 0 && !review.inherits(c)),
        );
        if let Some(context) = selected.as_ref() {
            let mut tracker = review.tracker(context, now());
            let rules = review
                .rules(context)
                .rules()
                .iter()
                .map(|r| {
                    let cells = model::rule_row(*r);
                    let used = tracker.usage(r.kind, r.time_delta, now());
                    format!(
                        "{} every {} ({} used)",
                        cells[0],
                        cells[1],
                        if r.kind == BandwidthType::Data {
                            hydrus_core::numbers::human_bytes(used)
                        } else {
                            hydrus_core::numbers::human_int(used)
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            self.window.set_detail(
                format!(
                    "{} — {}\nAll time usage: {}\n{}",
                    context.to_human_string(),
                    if review.inherits(context) {
                        "uses default rules"
                    } else {
                        "has its own rules"
                    },
                    model::all_usage_text(&tracker),
                    rules
                )
                .into(),
            );
        }
        self.window.set_status(
            if !self.error.borrow().is_empty() {
                self.error.borrow().clone()
            } else if review.live {
                "Live usage from the running daemon.".into()
            } else {
                "Daemon offline: showing saved bandwidth history.".into()
            }
            .into(),
        );
    }
    fn poll(self: &Rc<Self>) {
        for event in self.worker.receive.try_iter() {
            match event {
                Event::Review(Ok(review)) => {
                    *self.review.borrow_mut() = Some(review);
                    self.show();
                }
                Event::Review(Err(error)) => self.window.set_status(error.into()),
                Event::Written(result) => {
                    self.window.set_busy(false);
                    if let Some(edit) = self.edit.borrow().as_ref() {
                        edit.window.set_busy(false);
                    }
                    match result {
                        Ok(()) => {
                            self.error.borrow_mut().clear();
                            self.close_edit();
                        }
                        Err(error) => {
                            self.error.borrow_mut().clone_from(&error);
                            self.window.set_status(error.as_str().into());
                            if let Some(edit) = self.edit.borrow().as_ref() {
                                edit.window.set_error(error.into());
                            }
                        }
                    }
                }
            }
        }
    }
    fn close_edit(&self) {
        if let Some(edit) = self.edit.borrow_mut().take() {
            let _ = edit.window.hide();
        }
    }
    fn edit(self: &Rc<Self>, context: NetworkContext) {
        if !self.active.get() {
            return;
        }
        if let Some(edit) = self.edit.borrow().as_ref() {
            let _ = edit.window.show();
            return;
        }
        let review = self.review.borrow();
        let Some(review) = review.as_ref() else {
            return;
        };
        let Ok(window) = EditBandwidthRulesWindow::new() else {
            return;
        };
        window.set_window_title(
            format!("edit bandwidth rules for {}", context.to_human_string()).into(),
        );
        window.set_columns(columns(&[("max allowed", 280.0), ("every", 200.0)]));
        let draft = Rc::new(RefCell::new(RulesDraft::new(review, context)));
        let selection = Rc::new(RefCell::new(ListSelection::default()));
        let show: Rc<dyn Fn()> = Rc::new({
            let weak = window.as_weak();
            let draft = draft.clone();
            let selection = selection.clone();
            move || {
                if let Some(w) = weak.upgrade() {
                    w.set_rows(ModelRc::new(VecModel::from(
                        draft
                            .borrow()
                            .rules
                            .iter()
                            .enumerate()
                            .map(|(i, r)| {
                                row(model::rule_row(*r), selection.borrow().is_selected(i))
                            })
                            .collect::<Vec<_>>(),
                    )));
                }
            }
        });
        window.on_row_clicked({
            let draft = draft.clone();
            let selection = selection.clone();
            let show = show.clone();
            let weak = window.as_weak();
            move |r, c, s| {
                let Ok(index) = usize::try_from(r) else {
                    return;
                };
                selection.borrow_mut().click(
                    &(0..draft.borrow().rules.len()).collect::<Vec<_>>(),
                    index,
                    c,
                    s,
                );
                if let (Some(rule), Some(w)) = (draft.borrow().rules.get(index), weak.upgrade()) {
                    w.set_requests(rule.kind == BandwidthType::Requests);
                    w.set_monthly(rule.time_delta.is_none());
                    w.set_amount(rule.max_allowed.to_string().into());
                    w.set_seconds(rule.time_delta.unwrap_or(86400).to_string().into());
                }
                show();
            }
        });
        for replace in [false, true] {
            let draft = draft.clone();
            let selection = selection.clone();
            let show = show.clone();
            let weak = window.as_weak();
            let owner = Rc::downgrade(self);
            let callback = move || {
                let (Some(w), Some(owner)) = (weak.upgrade(), owner.upgrade()) else {
                    return;
                };
                if !owner.active.get() || owner.window.get_busy() {
                    return;
                }
                match model::parse_rule(
                    w.get_requests(),
                    w.get_amount().as_str(),
                    w.get_seconds().as_str(),
                    w.get_monthly(),
                ) {
                    Ok(rule) => {
                        let mut draft = draft.borrow_mut();
                        if replace {
                            let Some(i) =
                                selection.borrow().one().filter(|i| *i < draft.rules.len())
                            else {
                                w.set_error("Select one rule to replace.".into());
                                return;
                            };
                            draft.rules[i] = rule;
                        } else if !draft.rules.contains(&rule) {
                            draft.rules.push(rule);
                        }
                        w.set_error("".into());
                        drop(draft);
                        show();
                    }
                    Err(error) => w.set_error(error.into()),
                }
            };
            if replace {
                window.on_replace_rule(callback);
            } else {
                window.on_add_rule(callback);
            }
        }
        window.on_delete_rule({
            let draft = draft.clone();
            let selection = selection.clone();
            let show = show.clone();
            let owner = Rc::downgrade(self);
            move || {
                if !owner
                    .upgrade()
                    .is_some_and(|o| o.active.get() && !o.window.get_busy())
                {
                    return;
                }
                let mut index = 0;
                draft.borrow_mut().rules.retain(|_| {
                    let keep = !selection.borrow().is_selected(index);
                    index += 1;
                    keep
                });
                selection.borrow_mut().select_only(None);
                show();
            }
        });
        window.on_apply_clicked({
            let owner = Rc::downgrade(self);
            let draft = draft.clone();
            let weak = window.as_weak();
            move || {
                if let (Some(owner), Some(w)) = (owner.upgrade(), weak.upgrade())
                    && owner.active.get()
                    && !owner.window.get_busy()
                    && w.window().is_visible()
                {
                    owner.window.set_busy(true);
                    w.set_busy(true);
                    let _ = owner
                        .worker
                        .send
                        .send(Operation::Save(draft.borrow().clone(), false));
                }
            }
        });
        let close = Rc::new({
            let owner = Rc::downgrade(self);
            move || {
                if let Some(owner) = owner.upgrade()
                    && !owner.window.get_busy()
                {
                    owner.close_edit();
                }
            }
        });
        window.on_cancel_clicked({
            let close = close.clone();
            move || close()
        });
        window.window().on_close_requested(move || {
            close();
            slint::CloseRequestResponse::KeepWindowShown
        });
        show();
        LAST_RULES.with(|s| *s.borrow_mut() = Some(window.as_weak()));
        let _ = window.show();
        *self.edit.borrow_mut() = Some(Edit { window });
    }
    fn close(&self) {
        self.active.set(false);
        self.close_edit();
        self.timer.stop();
        let _ = self.worker.send.send(Operation::Stop);
        let _ = self.window.hide();
        if let Some(owner) = self.owner.upgrade() {
            owner.borrow_mut().take();
        }
    }
}

/// Open the bandwidth review once; every database operation runs on its worker.
pub fn open_bandwidth(store: Arc<Store>, slots: &Slots) -> Result<BandwidthWindow, String> {
    if let Some(w) = slots
        .bandwidth
        .borrow()
        .as_ref()
        .map(|s| s.window.clone_strong())
    {
        w.show().map_err(|e| e.to_string())?;
        return Ok(w);
    }
    let window = BandwidthWindow::new().map_err(|e| e.to_string())?;
    window.set_columns(columns(&[
        ("network context", 250.0),
        ("type", 105.0),
        ("current", 80.0),
        ("past day", 150.0),
        ("history", 150.0),
        ("this month", 150.0),
        ("own rules", 75.0),
        ("blocked", 100.0),
    ]));
    window.set_status("Loading bandwidth usage…".into());
    let state = Rc::new(Bandwidth {
        window: window.clone_strong(),
        owner: Rc::downgrade(&slots.bandwidth),
        worker: Worker::start(store),
        timer: Timer::default(),
        active: Cell::new(true),
        review: RefCell::default(),
        order: RefCell::default(),
        selected: RefCell::default(),
        edit: RefCell::default(),
        pending: RefCell::default(),
        error: RefCell::default(),
    });
    window.on_row_clicked({
        let state = Rc::downgrade(&state);
        move |r, _, _| {
            if let Some(s) = state.upgrade() {
                *s.selected.borrow_mut() = usize::try_from(r)
                    .ok()
                    .and_then(|i| s.order.borrow().get(i).cloned());
                s.show();
            }
        }
    });
    window.on_sort({
        let state = Rc::downgrade(&state);
        move |c, a| {
            if let Some(s) = state.upgrade() {
                s.window.set_sort_column(c);
                s.window.set_ascending(a);
                s.show();
            }
        }
    });
    window.on_refresh({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                s.show();
                let _ = s.worker.send.send(Operation::Refresh);
            }
        }
    });
    window.on_edit_clicked({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                let context = s.selected.borrow().clone();
                if let Some(c) = context {
                    s.edit(c);
                }
            }
        }
    });
    window.on_default_clicked({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                let kind = [0, 2, 1, 4, 5, 6][usize::try_from(s.window.get_default_kind())
                    .unwrap_or(0)
                    .min(5)];
                s.edit(NetworkContext::default_of_kind(kind));
            }
        }
    });
    window.on_domain_clicked({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                let domain = s.window.get_domain().trim().to_lowercase();
                if domain.contains('.') && !domain.contains(['/', ':', ' ']) {
                    s.edit(NetworkContext::domain(domain));
                } else {
                    *s.error.borrow_mut() = "Enter a domain without a scheme or path.".into();
                    s.window
                        .set_status("Enter a domain without a scheme or path.".into());
                }
            }
        }
    });
    window.on_revert_clicked({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade()
                && let (Some(c), Some(review)) =
                    (s.selected.borrow().clone(), s.review.borrow().as_ref())
                && !review.inherits(&c)
                && !c.is_default()
                && c.kind != 0
            {
                *s.pending.borrow_mut() = Some(Operation::Save(RulesDraft::new(review, c), true));
                s.window.set_question(model::REVERT_QUESTION.into());
            }
        }
    });
    window.on_reset_clicked({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                *s.pending.borrow_mut() = Some(Operation::Reset);
                s.window.set_question(model::RESET_QUESTION.into());
            }
        }
    });
    window.on_answer({
        let state = Rc::downgrade(&state);
        move |yes| {
            if let Some(s) = state.upgrade() {
                s.window.set_question("".into());
                if let Some(op) = s.pending.borrow_mut().take()
                    && yes
                {
                    s.window.set_busy(true);
                    let _ = s.worker.send.send(op);
                }
            }
        }
    });
    window.on_close_clicked({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                s.close();
            }
        }
    });
    window.window().on_close_requested({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                s.close();
            }
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    state
        .timer
        .start(TimerMode::Repeated, Duration::from_millis(100), {
            let state = Rc::downgrade(&state);
            move || {
                if let Some(s) = state.upgrade() {
                    s.poll();
                }
            }
        });
    LAST_BANDWIDTH.with(|s| *s.borrow_mut() = Some(window.as_weak()));
    *slots.bandwidth.borrow_mut() = Some(state);
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}

struct Jobs {
    window: NetworkJobsWindow,
    owner: std::rc::Weak<RefCell<Option<Rc<Jobs>>>>,
    worker: Worker,
    timer: Timer,
    review: RefCell<Option<Review>>,
    order: RefCell<Vec<u64>>,
    selection: RefCell<ListSelection<u64>>,
    manual: Cell<bool>,
    error: RefCell<String>,
}
impl Jobs {
    fn show(&self) {
        let review = self.review.borrow();
        let Some(review) = review.as_ref() else {
            return;
        };
        let mut jobs = if review.live {
            review.runtime.jobs.clone()
        } else {
            Vec::new()
        };
        let column = usize::try_from(self.window.get_sort_column())
            .unwrap_or(0)
            .min(4);
        jobs.sort_by_key(|job| model::job_sort_key(job, column));
        if !self.window.get_ascending() {
            jobs.reverse();
        }
        let order: Vec<_> = jobs.iter().map(|j| j.id).collect();
        let old = self.order.replace(order.clone());
        for id in old {
            if !order.contains(&id) {
                self.selection.borrow_mut().forget(id);
            }
        }
        self.window.set_rows(ModelRc::new(VecModel::from(
            jobs.iter()
                .map(|j| row(model::job_row(j), self.selection.borrow().is_selected(j.id)))
                .collect::<Vec<_>>(),
        )));
        self.window
            .set_can_act(!self.selection.borrow().is_empty() && review.runtime.fresh(now()));
        let selected = self
            .selection
            .borrow()
            .one()
            .and_then(|id| jobs.iter().find(|j| j.id == id));
        self.window.set_detail(
            selected
                .map_or_else(String::new, |job| {
                    format!(
                        "{}\nnetwork contexts: {}\nobeys bandwidth: {}",
                        job.url,
                        job.contexts
                            .iter()
                            .map(NetworkContext::to_human_string)
                            .collect::<Vec<_>>()
                            .join(", "),
                        job.obeys_bandwidth
                    )
                })
                .into(),
        );
        self.window.set_status(
            if !self.error.borrow().is_empty() {
                self.error.borrow().clone()
            } else if review.live {
                format!("{} current network jobs", jobs.len())
            } else {
                "Daemon offline: no current network jobs.".into()
            }
            .into(),
        );
    }
    fn poll(&self) {
        for event in self.worker.receive.try_iter() {
            match event {
                Event::Review(Ok(review)) => {
                    if self.manual.replace(false) || self.window.get_auto_refresh() {
                        if self
                            .review
                            .borrow()
                            .as_ref()
                            .is_some_and(|old| old.runtime.epoch != review.runtime.epoch)
                        {
                            self.selection.borrow_mut().select_only(None);
                        }
                        *self.review.borrow_mut() = Some(review);
                        self.show();
                    }
                }
                Event::Review(Err(e)) | Event::Written(Err(e)) => {
                    self.error.borrow_mut().clone_from(&e);
                    self.window.set_status(e.into());
                }
                Event::Written(Ok(())) => {
                    self.error.borrow_mut().clear();
                    self.window.set_status("Action sent to the daemon.".into());
                }
            }
        }
        if self
            .review
            .borrow()
            .as_ref()
            .is_some_and(|r| !r.runtime.fresh(now()))
        {
            self.window.set_can_act(false);
        }
    }
    fn action(&self, action: JobAction) {
        let review = self.review.borrow();
        let Some(review) = review.as_ref().filter(|r| r.runtime.fresh(now())) else {
            return;
        };
        for id in self.selection.borrow().in_order(&self.order.borrow()) {
            let _ = self.worker.send.send(Operation::Job(Command {
                epoch: review.runtime.epoch.clone(),
                job: id,
                action,
            }));
        }
        self.manual.set(true);
    }
    fn close(&self) {
        self.timer.stop();
        let _ = self.worker.send.send(Operation::Stop);
        let _ = self.window.hide();
        if let Some(owner) = self.owner.upgrade() {
            owner.borrow_mut().take();
        }
    }
}
/// Open the full engine's current requests, without requiring an API listener.
pub fn open_jobs(store: Arc<Store>, slots: &Slots) -> Result<NetworkJobsWindow, String> {
    if let Some(w) = slots
        .jobs
        .borrow()
        .as_ref()
        .map(|s| s.window.clone_strong())
    {
        w.show().map_err(|e| e.to_string())?;
        return Ok(w);
    }
    let window = NetworkJobsWindow::new().map_err(|e| e.to_string())?;
    window.set_columns(columns(&[
        ("position", 150.0),
        ("url", 350.0),
        ("status", 290.0),
        ("speed", 95.0),
        ("progress", 110.0),
    ]));
    let state = Rc::new(Jobs {
        window: window.clone_strong(),
        owner: Rc::downgrade(&slots.jobs),
        worker: Worker::start(store),
        timer: Timer::default(),
        review: RefCell::default(),
        order: RefCell::default(),
        selection: RefCell::default(),
        manual: Cell::new(true),
        error: RefCell::default(),
    });
    window.on_row_clicked({
        let state = Rc::downgrade(&state);
        move |r, c, s| {
            if let (Some(state), Ok(r)) = (state.upgrade(), usize::try_from(r)) {
                state
                    .selection
                    .borrow_mut()
                    .click(&state.order.borrow(), r, c, s);
                state.show();
            }
        }
    });
    window.on_sort({
        let state = Rc::downgrade(&state);
        move |c, a| {
            if let Some(s) = state.upgrade() {
                s.window.set_sort_column(c);
                s.window.set_ascending(a);
                s.show();
            }
        }
    });
    window.on_refresh({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                s.manual.set(true);
                let _ = s.worker.send.send(Operation::Refresh);
            }
        }
    });
    window.on_cancel_jobs({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                s.action(JobAction::Cancel);
            }
        }
    });
    window.on_override_jobs({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                s.action(JobAction::OverrideBandwidth);
            }
        }
    });
    window.on_close_clicked({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                s.close();
            }
        }
    });
    window.window().on_close_requested({
        let state = Rc::downgrade(&state);
        move || {
            if let Some(s) = state.upgrade() {
                s.close();
            }
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    state
        .timer
        .start(TimerMode::Repeated, Duration::from_millis(100), {
            let state = Rc::downgrade(&state);
            move || {
                if let Some(s) = state.upgrade() {
                    s.poll();
                }
            }
        });
    LAST_JOBS.with(|s| *s.borrow_mut() = Some(window.as_weak()));
    *slots.jobs.borrow_mut() = Some(state);
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
