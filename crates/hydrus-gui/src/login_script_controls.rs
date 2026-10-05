//! The script editor's isolated NetworkJobControl and owned completion notices.
use crate::{LoginScriptWindow, SessionDialog, login_test_window::RunSlot};
use hydrus_gui_model::{login_workflows::TestControl, network_job_control::Action};
use hydrus_store::Store;
use slint::{ComponentHandle, Timer, TimerMode};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

type NoticeSlot = Rc<RefCell<Option<SessionDialog>>>;
fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}
fn cancel_notice(slot: &NoticeSlot) {
    let notice = slot.borrow().as_ref().map(ComponentHandle::clone_strong);
    if let Some(notice) = notice {
        notice.invoke_force_close();
    }
}
fn notice(
    slot: &NoticeSlot,
    title: &str,
    text: &str,
    done: Rc<dyn Fn(bool)>,
    allowed: Rc<dyn Fn() -> bool>,
) -> Result<(), String> {
    cancel_notice(slot);
    let window = SessionDialog::new().map_err(|error| error.to_string())?;
    window.set_window_title(title.into());
    window.set_notice_only(true);
    window.set_notice_ok_label("OK".into());
    window.set_message(text.into());
    let alive = Rc::new(Cell::new(true));
    let close: Rc<dyn Fn(bool)> = Rc::new({
        let slot = Rc::downgrade(slot);
        let weak = window.as_weak();
        move |acknowledged| {
            if !alive.replace(false) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            done(acknowledged);
        }
    });
    window.on_cancelled({
        let close = close.clone();
        let allowed = allowed.clone();
        move || {
            if allowed() {
                close(true);
            }
        }
    });
    window.on_force_close({
        let close = close.clone();
        move || close(false)
    });
    window.window().on_close_requested(move || {
        if allowed() {
            close(true);
            slint::CloseRequestResponse::HideWindow
        } else {
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    *slot.borrow_mut() = Some(window.clone_strong());
    if let Err(error) = window.show() {
        window.invoke_force_close();
        return Err(error.to_string());
    }
    Ok(())
}
struct State {
    window: slint::Weak<LoginScriptWindow>,
    active: Rc<Cell<bool>>,
    run: RunSlot,
    store: Arc<Store>,
    formatting: hydrus_store::settings::GuiFormatting,
    model: RefCell<TestControl>,
    actions: RefCell<HashMap<i32, Action>>,
    owner: u64,
    rules: crate::network_data_window::Slots,
    information: NoticeSlot,
    error: NoticeSlot,
    other_children: Rc<dyn Fn() -> bool>,
    timer: Timer,
}
impl State {
    fn valid(&self) -> bool {
        self.active.get()
            && self
                .window
                .upgrade()
                .is_some_and(|window| window.window().is_visible())
    }
    fn children(&self) {
        if let Some(window) = self.window.upgrade() {
            window.set_child_open(
                self.information.borrow().is_some()
                    || self.error.borrow().is_some()
                    || self.rules.has_open()
                    || (self.other_children)(),
            );
        }
    }
    fn poll(&self) {
        if !self.valid() {
            return;
        }
        self.children();
        let review = self.run.review();
        if let Some(review) = review.as_ref()
            && let Some(command) =
                self.model
                    .borrow_mut()
                    .auto_command(&review.runtime, now(), self.owner)
        {
            self.run.command(&command);
        }
        if let Some(window) = self.window.upgrade() {
            let line = review
                .as_ref()
                .and_then(|review| review.runtime.jobs.first())
                .map(|job| {
                    hydrus_store::live::JobLive {
                        url: job.url.clone(),
                        status: job.status.clone(),
                        speed: job.speed,
                        bytes_read: job.bytes_read,
                        bytes_to_read: job.bytes_total,
                        done: false,
                        error: false,
                    }
                    .line_with_figures(self.formatting.figures)
                })
                .unwrap_or_default();
            window.set_test_download(crate::download_line(&line));
            let model = self.model.borrow();
            let mut menu = window.get_test_cog();
            menu.auto_override = model.retained.auto_override;
            menu.has_error = model.retained.error().is_some();
            window.set_test_cog(menu);
        }
    }
    fn menu(&self) {
        if !self.valid() || self.has_children() || (self.other_children)() {
            return;
        }
        self.poll();
        let review = self.run.review();
        let menu = self.model.borrow_mut().menu(review.as_ref(), now());
        let (menu, actions) = crate::network_job_control::menu_data(
            menu,
            self.model.borrow().retained.error().is_some(),
        );
        *self.actions.borrow_mut() = actions;
        if let Some(window) = self.window.upgrade() {
            window.set_test_cog(menu);
        }
    }
    fn has_children(&self) -> bool {
        self.information.borrow().is_some()
            || self.error.borrow().is_some()
            || self.rules.has_open()
    }
    fn action(self: &Rc<Self>, id: i32) {
        if !self.valid() || self.has_children() || (self.other_children)() {
            return;
        }
        if id == 7 {
            self.model.borrow_mut().retained.flip_auto_override();
            self.poll();
            self.menu();
            return;
        }
        if matches!(id, 8 | 9) {
            let text = self.model.borrow().retained.error().map(str::to_owned);
            if let Some(text) = text {
                if id == 9 {
                    crate::copy_to_clipboard(&text);
                } else {
                    let done = Rc::new({
                        let state = Rc::downgrade(self);
                        move |_| {
                            if let Some(state) = state.upgrade() {
                                state.children();
                            }
                        }
                    });
                    if let Err(error) = notice(
                        &self.error,
                        "Network Error",
                        &text,
                        done,
                        Rc::new({
                            let state = Rc::downgrade(self);
                            move || state.upgrade().is_some_and(|state| state.valid())
                        }),
                    ) {
                        if let Some(window) = self.window.upgrade() {
                            window.set_error(error.into());
                        }
                    }
                    self.children();
                }
            }
            return;
        }
        let action = self.actions.borrow().get(&id).cloned();
        match action {
            Some(Action::CopyUrl(url)) => crate::copy_to_clipboard(&url),
            Some(Action::Rules(context)) => {
                if let Err(error) =
                    crate::network_data_window::open_rules(self.store.clone(), &self.rules, context)
                    && let Some(window) = self.window.upgrade()
                {
                    window.set_error(error.into());
                }
            }
            Some(Action::Job(action)) => {
                if let Some(review) = self.run.review()
                    && let Some(command) =
                        self.model.borrow().command(&review.runtime, now(), action)
                {
                    self.run.command(&command);
                }
            }
            _ => {}
        }
    }
}
/// This editor's controls, whose jobs belong to its isolated engine rather than the daemon.
#[derive(Clone)]
pub struct Binding(Rc<State>);
impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginScriptControls")
            .finish_non_exhaustive()
    }
}
impl Binding {
    pub fn poll(&self) {
        self.0.poll();
    }
    pub fn children_open(&self) -> bool {
        self.0.has_children()
    }
    pub fn information_window(&self) -> Option<SessionDialog> {
        self.0
            .information
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
    }
    pub fn error_window(&self) -> Option<SessionDialog> {
        self.0
            .error
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
    }
    /// Qt does not infer this error from a failed request; the owner supplies it.
    pub fn set_error(&self, text: String) {
        if self.0.valid() {
            self.0.model.borrow_mut().retained.set_error(text);
            self.0.poll();
        }
    }
    pub fn clear_error(&self) {
        if self.0.valid() {
            self.0.model.borrow_mut().retained.clear_error();
            self.0.poll();
        }
    }
    pub fn cancel(&self) {
        self.0.timer.stop();
        cancel_notice(&self.0.information);
        cancel_notice(&self.0.error);
        self.0.rules.close();
    }
    pub fn information(&self, text: &str, acknowledged: Rc<dyn Fn()>) -> Result<(), String> {
        if !self.0.valid() {
            return Err("The login script editor is no longer visible.".into());
        }
        let done = Rc::new({
            let state = Rc::downgrade(&self.0);
            move |ok| {
                if let Some(state) = state.upgrade() {
                    state.children();
                    if ok && state.valid() {
                        acknowledged();
                    }
                }
            }
        });
        notice(
            &self.0.information,
            "Information",
            text,
            done,
            Rc::new({
                let state = Rc::downgrade(&self.0);
                move || state.upgrade().is_some_and(|state| state.valid())
            }),
        )?;
        self.0.children();
        Ok(())
    }
}
pub(crate) fn bind(
    window: &LoginScriptWindow,
    store: &Arc<Store>,
    run: &RunSlot,
    active: &Rc<Cell<bool>>,
    other_children: Rc<dyn Fn() -> bool>,
) -> Binding {
    let state = Rc::new(State {
        window: window.as_weak(),
        store: store.clone(),
        run: run.clone(),
        active: active.clone(),
        formatting: hydrus_gui_model::gui_format::preferences(store),
        model: RefCell::default(),
        actions: RefCell::default(),
        owner: crate::network_job_control::new_control_owner(),
        rules: crate::network_data_window::Slots::default(),
        information: Rc::default(),
        error: Rc::default(),
        other_children,
        timer: Timer::default(),
    });
    window.on_test_control_menu({
        let state = state.clone();
        move || state.menu()
    });
    window.on_test_control_action({
        let state = state.clone();
        move |id| state.action(id)
    });
    state.timer.start(
        TimerMode::Repeated,
        std::time::Duration::from_millis(100),
        {
            let state = Rc::downgrade(&state);
            move || {
                if let Some(state) = state.upgrade() {
                    state.poll();
                }
            }
        },
    );
    state.poll();
    state.menu();
    Binding(state)
}
/// The existing OS-launch route opens the repository's local login-script help.
pub(crate) fn help() -> Result<(), String> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/downloader_login.md");
    let path = path
        .canonicalize()
        .map_err(|_| "The local login scripts help is unavailable.".to_owned())?;
    crate::launch(&path.display().to_string());
    Ok(())
}
