//! The advanced-mode quality menu reads durable histories off the UI thread.
//! Closing its editor cancels publication and stops the retained poll timer.
use crate::{
    EditSubscriptionWindow,
    main_menu::PopupNode,
    popup_menu::{Chosen, Popup},
};
use hydrus_gui_model::subscription_quality::{self as model, Action};
use hydrus_store::{Store, subscription_quality::QueryQuality};
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

type Selected = Rc<dyn Fn() -> Vec<(i64, String)>>;
type Message = Rc<dyn Fn(String)>;
struct Pending {
    action: Action,
    receiver: mpsc::Receiver<Result<Vec<QueryQuality>, String>>,
}
struct Inner {
    alive: Cell<bool>,
    cancel: Arc<AtomicBool>,
    timer: slint::Timer,
    pending: RefCell<Option<Pending>>,
    popup: Rc<Popup<Action>>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.timer.stop();
    }
}
#[derive(Clone)]
pub(crate) struct Control(Rc<Inner>);
impl Control {
    pub fn cancel(&self) {
        self.0.alive.set(false);
        self.0.cancel.store(true, Ordering::Relaxed);
        self.0.timer.stop();
        self.0.pending.borrow_mut().take();
        self.0.popup.close();
    }
}

fn node<'a>(item: &'a (&'static str, Action)) -> PopupNode<'a, (&'static str, Action), Action> {
    PopupNode::Item(item.0, &item.1)
}

pub(crate) fn bind(
    window: &EditSubscriptionWindow,
    store: &Arc<Store>,
    advanced: bool,
    selected: Selected,
    message: Message,
) -> Control {
    let popup = Popup::new();
    window.set_quality_panes(popup.model());
    let inner = Rc::new(Inner {
        alive: Cell::new(true),
        cancel: Arc::new(AtomicBool::new(false)),
        timer: slint::Timer::default(),
        pending: RefCell::new(None),
        popup: popup.clone(),
    });
    window.set_quality_visible(advanced);
    window.on_quality_menu({
        let inner = inner.clone();
        let selected = selected.clone();
        let weak = window.as_weak();
        move |x, y| {
            if !inner.alive.get()
                || !advanced
                || inner.pending.borrow().is_some()
                || selected().is_empty()
            {
                return;
            }
            if let Some(w) = weak.upgrade()
                && !w.get_asking()
                && !w.get_editing_query()
            {
                let (entries, actions) = crate::main_menu::popup(&model::MENU, &node);
                inner.popup.open(entries, actions, x, y);
            }
        }
    });
    window.on_quality_line_hovered({
        let popup = popup.clone();
        move |p, l, r, t, left| popup.hover(p, l, r, t, left)
    });
    window.on_quality_placed({
        let popup = popup.clone();
        move |p, x, y, w| popup.placed(p, x, y, w)
    });
    window.on_quality_dismissed({
        let popup = popup.clone();
        move || popup.close()
    });
    window.on_quality_line_clicked({
        let inner = inner.clone();
        let selected = selected.clone();
        let weak = window.as_weak();
        let store = store.clone();
        let message = message.clone();
        move |p, l, r, t, left| {
            if !inner.alive.get() || inner.pending.borrow().is_some() {
                return;
            }
            let Some(Chosen::Action(action)) = inner.popup.click(p, l, r, t, left) else {
                return;
            };
            let selected = selected();
            if selected.is_empty() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            if w.get_asking() || w.get_editing_query() {
                return;
            }
            let (sender, receiver) = mpsc::channel();
            let store = store.clone();
            let cancel = inner.cancel.clone();
            match std::thread::Builder::new()
                .name("subscription quality".into())
                .spawn(move || {
                    let result = store
                        .read(|conn| {
                            hydrus_store::subscription_quality::read(conn, &selected, &cancel)
                        })
                        .map_err(|e| e.to_string());
                    let _ = sender.send(result);
                }) {
                Ok(_) => {
                    *inner.pending.borrow_mut() = Some(Pending { action, receiver });
                    w.set_quality_working(true);
                }
                Err(error) => {
                    message(format!("Could not read query quality: {error}"));
                    return;
                }
            }
            inner
                .timer
                .start(slint::TimerMode::Repeated, Duration::from_millis(20), {
                    let inner = Rc::downgrade(&inner);
                    let weak = w.as_weak();
                    let message = message.clone();
                    move || {
                        let Some(inner) = inner.upgrade() else {
                            return;
                        };
                        if !inner.alive.get() {
                            inner.timer.stop();
                            return;
                        }
                        let result = {
                            let pending = inner.pending.borrow();
                            let Some(pending) = pending.as_ref() else {
                                return;
                            };
                            match pending.receiver.try_recv() {
                                Ok(result) => Some((pending.action, result)),
                                Err(mpsc::TryRecvError::Empty) => None,
                                Err(mpsc::TryRecvError::Disconnected) => Some((
                                    pending.action,
                                    Err("Quality worker stopped unexpectedly".into()),
                                )),
                            }
                        };
                        let Some((action, result)) = result else {
                            return;
                        };
                        inner.timer.stop();
                        inner.pending.borrow_mut().take();
                        let Some(w) = weak.upgrade() else {
                            return;
                        };
                        w.set_quality_working(false);
                        match result {
                            Ok(rows) => match action {
                                Action::Show => message(model::information(&rows)),
                                Action::CopyCsv => crate::copy_to_clipboard(&model::csv(&rows)),
                            },
                            Err(error) => message(format!("Could not read query quality: {error}")),
                        }
                    }
                });
        }
    });
    Control(inner)
}
