//! Options-owned URL queues, MIME routes and detached registered-call children.
use crate::{
    ExternalRoutingChoiceWindow, OpenFileCallsWindow, OptionsWindow, SessionDialog, TableRow,
};
use hydrus_core::{Mime, external_calls::Pipeline};
use hydrus_gui_model::{
    open_externally::{self as model, Queue},
    options::{Editor, Kind, Row},
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Default)]
pub struct Slots {
    pub files: Rc<RefCell<Option<OpenFileCallsWindow>>>,
    pub choice: Rc<RefCell<Option<ExternalRoutingChoiceWindow>>>,
    pub question: Rc<RefCell<Option<SessionDialog>>>,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalRoutingSlots")
            .field("open", &self.has_open())
            .finish()
    }
}
impl Slots {
    pub fn has_open(&self) -> bool {
        self.files.borrow().is_some()
            || self.choice.borrow().is_some()
            || self.question.borrow().is_some()
    }
    pub fn cancel(&self) {
        let choice = self
            .choice
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong);
        if let Some(window) = choice {
            window.invoke_cancel();
        }
        let question = self
            .question
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong);
        if let Some(window) = question {
            window.invoke_force_close();
        }
        let files = self
            .files
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong);
        if let Some(window) = files {
            window.invoke_cancel();
        }
    }
}
pub(crate) struct Binding {
    pub show: Rc<dyn Fn()>,
    pub cancel: Rc<dyn Fn()>,
    pub has_open: Rc<dyn Fn() -> bool>,
}
fn rows(queue: &Queue) -> ModelRc<TableRow> {
    ModelRc::new(VecModel::from(
        queue
            .rows
            .iter()
            .map(|(id, value)| TableRow {
                cells: ModelRc::new(VecModel::from(vec![value.name.as_str().into()])),
                selected: queue.selection.is_selected(*id),
            })
            .collect::<Vec<_>>(),
    ))
}
struct State {
    window: slint::Weak<OptionsWindow>,
    active: Rc<Cell<bool>>,
    editor: Rc<RefCell<Editor>>,
    model: RefCell<model::Editor>,
    urls: Rc<RefCell<Queue>>,
    slots: Slots,
    prepared: Cell<bool>,
    other_open: Rc<dyn Fn() -> bool>,
}
impl State {
    fn valid(&self) -> bool {
        self.active.get()
            && !(self.other_open)()
            && self
                .window
                .upgrade()
                .is_some_and(|window| window.window().is_visible())
    }
    fn visible_page(&self) -> bool {
        self.editor.borrow().rows().iter().any(
            |row| matches!(row,Row::Opt {option,..} if matches!(option.kind,Kind::OpenExternally)),
        )
    }
    fn error(&self, text: String) {
        if let Some(window) = self.window.upgrade() {
            window.set_routing_error(text.into());
        }
    }
    fn show(&self) {
        if let Some(window) = self.slots.files.borrow().as_ref() {
            window.set_child_open(
                self.slots.choice.borrow().is_some() || self.slots.question.borrow().is_some(),
            );
        }
        if !self.prepared.get() && self.visible_page() {
            let mut manager = self.editor.borrow().edited_external_calls();
            let url = manager.ensure_os(Pipeline::Url);
            let file = manager.ensure_os(Pipeline::File);
            self.editor.borrow_mut().set_external_calls(manager);
            if self.urls.borrow().rows.is_empty() {
                self.urls.borrow_mut().put(None, url);
            }
            for values in self.model.borrow_mut().routing.files.values_mut() {
                if values.is_empty() {
                    values.push(file.clone());
                }
            }
            self.model
                .borrow_mut()
                .routing
                .files
                .entry(Mime::GeneralFile)
                .or_insert_with(|| vec![file]);
            self.prepared.set(true);
            self.changed();
            return;
        }
        if let Some(window) = self.window.upgrade() {
            window.set_routing_prepared(self.prepared.get());
            window.set_routing_url_rows(rows(&self.urls.borrow()));
            window.set_routing_url_selected(!self.urls.borrow().selection.is_empty());
            let model = self.model.borrow();
            window.set_routing_file_rows(ModelRc::new(VecModel::from(
                model
                    .rows()
                    .into_iter()
                    .map(|(mime, values)| TableRow {
                        cells: ModelRc::new(VecModel::from(vec![
                            mime.human_name().into(),
                            model::summary(&values).into(),
                        ])),
                        selected: model.selection.is_selected(mime),
                    })
                    .collect::<Vec<_>>(),
            )));
            window.set_routing_file_single(model.selection.one().is_some());
            let selected = model.selected();
            window.set_routing_file_delete(
                !selected.is_empty() && !selected.contains(&Mime::GeneralFile),
            );
            window.set_routing_child_open(self.slots.has_open());
        }
    }
    fn changed(&self) {
        let mut routing = self.model.borrow().routing.clone();
        routing.urls = self.urls.borrow().values();
        self.editor.borrow_mut().set_open_externally(routing);
        self.show();
    }
    fn parent_allowed(self: &Rc<Self>) -> Rc<dyn Fn() -> bool> {
        let weak = Rc::downgrade(self);
        Rc::new(move || weak.upgrade().is_some_and(|state| state.valid()))
    }
    fn choice(
        self: &Rc<Self>,
        title: &str,
        labels: Vec<String>,
        description: &str,
        allowed: Rc<dyn Fn() -> bool>,
        accepted: Rc<dyn Fn(usize)>,
    ) -> Result<(), String> {
        let window = ExternalRoutingChoiceWindow::new().map_err(|error| error.to_string())?;
        window.set_window_title(title.into());
        window.set_choices(ModelRc::new(VecModel::from(
            labels
                .into_iter()
                .map(Into::into)
                .collect::<Vec<slint::SharedString>>(),
        )));
        window.set_choice_description(description.into());
        let alive = Rc::new(Cell::new(true));
        let close: Rc<dyn Fn()> = Rc::new({
            let weak = window.as_weak();
            let state = Rc::downgrade(self);
            let alive = alive.clone();
            move || {
                if !alive.replace(false) {
                    return;
                }
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
                if let Some(state) = state.upgrade() {
                    state.slots.choice.borrow_mut().take();
                    state.show();
                }
            }
        });
        window.on_chosen({
            let close = close.clone();
            let alive = alive.clone();
            let weak = window.as_weak();
            move |index| {
                if !alive.get()
                    || !allowed()
                    || !weak
                        .upgrade()
                        .is_some_and(|window| window.window().is_visible())
                {
                    return;
                }
                if let Ok(index) = usize::try_from(index) {
                    close();
                    accepted(index);
                }
            }
        });
        window.on_cancel({
            let close = close.clone();
            move || close()
        });
        window.window().on_close_requested(move || {
            close();
            slint::CloseRequestResponse::HideWindow
        });
        *self.slots.choice.borrow_mut() = Some(window.clone_strong());
        if let Err(error) = window.show() {
            window.invoke_cancel();
            return Err(error.to_string());
        }
        self.show();
        Ok(())
    }
    fn question(
        self: &Rc<Self>,
        title: &str,
        text: &str,
        notice: bool,
        allowed: Rc<dyn Fn() -> bool>,
        accepted: Rc<dyn Fn()>,
    ) -> Result<(), String> {
        let window = SessionDialog::new().map_err(|error| error.to_string())?;
        window.set_window_title(title.into());
        window.set_message(text.into());
        window.set_notice_only(notice);
        window.set_notice_ok_label("OK".into());
        let alive = Rc::new(Cell::new(true));
        let close: Rc<dyn Fn()> = Rc::new({
            let weak = window.as_weak();
            let state = Rc::downgrade(self);
            let alive = alive.clone();
            move || {
                if !alive.replace(false) {
                    return;
                }
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
                if let Some(state) = state.upgrade() {
                    state.slots.question.borrow_mut().take();
                    state.show();
                }
            }
        });
        window.on_answered({
            let close = close.clone();
            let alive = alive.clone();
            let allowed = allowed.clone();
            let weak = window.as_weak();
            move |yes| {
                if !alive.get()
                    || !allowed()
                    || !weak
                        .upgrade()
                        .is_some_and(|window| window.window().is_visible())
                {
                    return;
                }
                close();
                if yes {
                    accepted();
                }
            }
        });
        window.on_cancelled({
            let close = close.clone();
            let allowed = allowed.clone();
            move || {
                if allowed() {
                    close();
                }
            }
        });
        window.on_force_close({
            let close = close.clone();
            move || close()
        });
        window.window().on_close_requested(move || {
            if allowed() {
                close();
                slint::CloseRequestResponse::HideWindow
            } else {
                slint::CloseRequestResponse::KeepWindowShown
            }
        });
        *self.slots.question.borrow_mut() = Some(window.clone_strong());
        if let Err(error) = window.show() {
            window.invoke_force_close();
            return Err(error.to_string());
        }
        self.show();
        Ok(())
    }
    fn queue_action(
        self: &Rc<Self>,
        queue: &Rc<RefCell<Queue>>,
        pipeline: Pipeline,
        action: &str,
        allowed: Rc<dyn Fn() -> bool>,
        changed: Rc<dyn Fn()>,
    ) {
        if !allowed()
            || self.slots.choice.borrow().is_some()
            || self.slots.question.borrow().is_some()
        {
            return;
        }
        if matches!(action, "up" | "down") {
            queue.borrow_mut().move_selected(action == "down");
            changed();
            return;
        }
        if action == "delete" {
            let ids = queue.borrow().selected();
            if ids.is_empty() {
                return;
            }
            let count = hydrus_core::numbers::human_int(ids.len() as u64);
            let queue = queue.clone();
            if let Err(error) = self.question(
                "Question",
                &format!("Remove {count} selected?"),
                false,
                allowed,
                Rc::new(move || {
                    queue.borrow_mut().delete(&ids);
                    changed();
                }),
            ) {
                self.error(error);
            }
            return;
        }
        let editing = if action == "edit" {
            let Some(id) = queue.borrow().editing() else {
                return;
            };
            Some(id)
        } else if action == "add" {
            None
        } else {
            return;
        };
        let choices = model::choices(
            &self.editor.borrow().edited_external_calls(),
            pipeline,
            &queue.borrow().values(),
        );
        if choices.is_empty() {
            if let Err(error) = self.question(
                "Information",
                &model::exhausted(pipeline),
                true,
                allowed,
                Rc::new(|| {}),
            ) {
                self.error(error);
            }
            return;
        }
        let labels = choices.iter().map(|value| value.name.clone()).collect();
        let queue = queue.clone();
        if let Err(error) = self.choice(
            model::CALL_TITLE,
            labels,
            "Select this call.",
            allowed,
            Rc::new(move |index| {
                if let Some(value) = choices.get(index) {
                    queue.borrow_mut().put(editing, value.clone());
                    changed();
                }
            }),
        ) {
            self.error(error);
        }
    }
    fn open_files(self: &Rc<Self>, mime: Mime, new: bool) -> Result<(), String> {
        let values = self
            .model
            .borrow()
            .routing
            .files
            .get(&mime)
            .cloned()
            .unwrap_or_default();
        let queue = Rc::new(RefCell::new(Queue::new(&values)));
        let window = OpenFileCallsWindow::new().map_err(|error| error.to_string())?;
        window.set_window_title(
            if new {
                "edit calls"
            } else {
                "edit launch path"
            }
            .into(),
        );
        let alive = Rc::new(Cell::new(true));
        let allowed: Rc<dyn Fn() -> bool> = Rc::new({
            let state = Rc::downgrade(self);
            let weak = window.as_weak();
            let alive = alive.clone();
            move || {
                alive.get()
                    && state.upgrade().is_some_and(|state| state.valid())
                    && weak
                        .upgrade()
                        .is_some_and(|window| window.window().is_visible())
            }
        });
        let show: Rc<dyn Fn()> = Rc::new({
            let weak = window.as_weak();
            let queue = queue.clone();
            let state = Rc::downgrade(self);
            move || {
                if let Some(window) = weak.upgrade() {
                    window.set_rows(rows(&queue.borrow()));
                    window.set_selected(!queue.borrow().selection.is_empty());
                    if let Some(state) = state.upgrade() {
                        window.set_child_open(
                            state.slots.choice.borrow().is_some()
                                || state.slots.question.borrow().is_some(),
                        );
                    }
                }
            }
        });
        let close: Rc<dyn Fn()> = Rc::new({
            let state = Rc::downgrade(self);
            let weak = window.as_weak();
            let alive = alive.clone();
            move || {
                if !alive.replace(false) {
                    return;
                }
                if let Some(state) = state.upgrade() {
                    let choice = state
                        .slots
                        .choice
                        .borrow()
                        .as_ref()
                        .map(ComponentHandle::clone_strong);
                    if let Some(window) = choice {
                        window.invoke_cancel();
                    }
                    let question = state
                        .slots
                        .question
                        .borrow()
                        .as_ref()
                        .map(ComponentHandle::clone_strong);
                    if let Some(window) = question {
                        window.invoke_force_close();
                    }
                    state.slots.files.borrow_mut().take();
                    state.show();
                }
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
            }
        });
        window.on_clicked({
            let queue = queue.clone();
            let show = show.clone();
            let allowed = allowed.clone();
            let state = Rc::downgrade(self);
            move |index, ctrl, shift| {
                if allowed()
                    && state.upgrade().is_some_and(|state| {
                        state.slots.choice.borrow().is_none()
                            && state.slots.question.borrow().is_none()
                    })
                    && let Ok(index) = usize::try_from(index)
                {
                    queue.borrow_mut().click(index, ctrl, shift);
                    show();
                }
            }
        });
        window.on_activated({
            let weak = window.as_weak();
            move |index| {
                if let Some(window) = weak.upgrade() {
                    window.invoke_clicked(index, false, false);
                    window.invoke_action("edit".into());
                }
            }
        });
        window.on_action({
            let queue = queue.clone();
            let show = show.clone();
            let allowed = allowed.clone();
            let state = Rc::downgrade(self);
            move |action| {
                if let Some(state) = state.upgrade() {
                    state.queue_action(
                        &queue,
                        Pipeline::File,
                        &action,
                        allowed.clone(),
                        show.clone(),
                    );
                    show();
                }
            }
        });
        window.on_apply({
            let state = Rc::downgrade(self);
            let queue = queue.clone();
            let allowed = allowed.clone();
            let close = close.clone();
            move || {
                if !allowed() {
                    return;
                }
                if let Some(state) = state.upgrade() {
                    if state.slots.choice.borrow().is_some()
                        || state.slots.question.borrow().is_some()
                    {
                        return;
                    }
                    state
                        .model
                        .borrow_mut()
                        .put(mime, queue.borrow().values(), new);
                    close();
                    state.changed();
                }
            }
        });
        window.on_cancel({
            let close = close.clone();
            move || close()
        });
        window.window().on_close_requested(move || {
            close();
            slint::CloseRequestResponse::HideWindow
        });
        *self.slots.files.borrow_mut() = Some(window.clone_strong());
        show();
        if let Err(error) = window.show() {
            window.invoke_cancel();
            return Err(error.to_string());
        }
        self.show();
        Ok(())
    }
    fn file_action(self: &Rc<Self>, action: &str) {
        if !self.valid() || !self.visible_page() || self.slots.has_open() {
            return;
        }
        if action == "edit" {
            if let Some(mime) = self.model.borrow().selection.one()
                && let Err(error) = self.open_files(mime, false)
            {
                self.error(error);
            }
            return;
        }
        if action == "delete" {
            let selected = self.model.borrow().selected();
            if selected.is_empty() || selected.contains(&Mime::GeneralFile) {
                return;
            }
            let count = selected.len();
            let total = self.model.borrow().routing.files.len();
            let message = if count == total {
                "Remove all selected?".into()
            } else {
                format!(
                    "Remove {} selected?",
                    hydrus_core::numbers::human_int(count as u64)
                )
            };
            let state = Rc::downgrade(self);
            if let Err(error) = self.question(
                "Question",
                &message,
                false,
                self.parent_allowed(),
                Rc::new(move || {
                    if let Some(state) = state.upgrade() {
                        state.model.borrow_mut().delete(&selected);
                        state.changed();
                    }
                }),
            ) {
                self.error(error);
            }
            return;
        }
        if action != "add" {
            return;
        }
        let choices = model::mime_choices()
            .into_iter()
            .filter(|mime| !self.model.borrow().routing.files.contains_key(mime))
            .collect::<Vec<_>>();
        if choices.is_empty() {
            if let Err(error) = self.question(
                "Warning",
                model::MIME_EXHAUSTED,
                true,
                self.parent_allowed(),
                Rc::new(|| {}),
            ) {
                self.error(error);
            }
            return;
        }
        let labels = choices
            .iter()
            .map(|mime| mime.mimetype().to_owned())
            .collect();
        let state = Rc::downgrade(self);
        if let Err(error) = self.choice(
            "which filetype?",
            labels,
            "",
            self.parent_allowed(),
            Rc::new(move |index| {
                if let Some(state) = state.upgrade()
                    && let Some(mime) = choices.get(index)
                    && let Err(error) = state.open_files(*mime, true)
                {
                    state.error(error);
                }
            }),
        ) {
            self.error(error);
        }
    }
}
pub(crate) fn bind(
    window: &OptionsWindow,
    editor: &Rc<RefCell<Editor>>,
    active: &Rc<Cell<bool>>,
    slots: &Slots,
    other_open: Rc<dyn Fn() -> bool>,
) -> Binding {
    let routing = editor.borrow().edited_open_externally();
    let state = Rc::new(State {
        window: window.as_weak(),
        active: active.clone(),
        editor: editor.clone(),
        urls: Rc::new(RefCell::new(Queue::new(&routing.urls))),
        model: RefCell::new(model::Editor::new(routing)),
        slots: slots.clone(),
        prepared: Cell::new(false),
        other_open,
    });
    window.on_routing_url_clicked({
        let state = state.clone();
        move |index, ctrl, shift| {
            if state.valid()
                && state.visible_page()
                && !state.slots.has_open()
                && let Ok(index) = usize::try_from(index)
            {
                state.urls.borrow_mut().click(index, ctrl, shift);
                state.show();
            }
        }
    });
    window.on_routing_url_activated({
        let weak = window.as_weak();
        move |index| {
            if let Some(window) = weak.upgrade() {
                window.invoke_routing_url_clicked(index, false, false);
                window.invoke_routing_url_action("edit".into());
            }
        }
    });
    window.on_routing_url_action({
        let state = state.clone();
        move |action| {
            if state.valid() && state.visible_page() && !state.slots.has_open() {
                let changed = Rc::new({
                    let weak = Rc::downgrade(&state);
                    move || {
                        if let Some(state) = weak.upgrade() {
                            state.changed();
                        }
                    }
                });
                state.queue_action(
                    &state.urls,
                    Pipeline::Url,
                    &action,
                    state.parent_allowed(),
                    changed,
                );
            }
        }
    });
    window.on_routing_file_clicked({
        let state = state.clone();
        move |index, ctrl, shift| {
            if state.valid()
                && state.visible_page()
                && !state.slots.has_open()
                && let Ok(index) = usize::try_from(index)
            {
                state.model.borrow_mut().click(index, ctrl, shift);
                state.show();
            }
        }
    });
    window.on_routing_file_activated({
        let weak = window.as_weak();
        move |index| {
            if let Some(window) = weak.upgrade() {
                window.invoke_routing_file_clicked(index, false, false);
                window.invoke_routing_file_action("edit".into());
            }
        }
    });
    window.on_routing_file_action({
        let state = state.clone();
        move |action| state.file_action(&action)
    });
    Binding {
        show: Rc::new({
            let state = state.clone();
            move || state.show()
        }),
        cancel: Rc::new({
            let slots = slots.clone();
            move || slots.cancel()
        }),
        has_open: Rc::new({
            let slots = slots.clone();
            move || slots.has_open()
        }),
    }
}
