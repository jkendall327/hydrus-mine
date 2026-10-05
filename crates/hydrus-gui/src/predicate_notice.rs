//! A hash predicate's warning, owned by its editor rather than by a global slot.
use crate::{PredicateEditorWindow, SessionDialog};
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Notice {
    _window: SessionDialog,
    close: Rc<dyn Fn()>,
}
/// The parent callbacks retain this slot; the child only refers back weakly.
pub(crate) struct Notices {
    child: Rc<RefCell<Option<Notice>>>,
    parent: slint::Weak<PredicateEditorWindow>,
    valid: Rc<dyn Fn() -> bool>,
}
impl Notices {
    pub fn new(parent: &PredicateEditorWindow, valid: Rc<dyn Fn() -> bool>) -> Self {
        Self {
            child: Rc::default(),
            parent: parent.as_weak(),
            valid,
        }
    }
    pub fn cancel(&self) {
        let close = self
            .child
            .borrow()
            .as_ref()
            .map(|notice| notice.close.clone());
        if let Some(close) = close {
            close();
        }
    }
    pub fn show(&self, message: &str) -> Result<(), String> {
        if self.child.borrow().is_some() || !(self.valid)() {
            return Ok(());
        }
        let Some(parent) = self.parent.upgrade() else {
            return Ok(());
        };
        if !parent.window().is_visible() {
            return Ok(());
        }
        let window = SessionDialog::new().map_err(|error| error.to_string())?;
        window.set_window_title("Warning".into());
        window.set_message(message.into());
        window.set_notice_only(true);
        window.set_notice_ok_label("OK".into());
        let alive = Rc::new(Cell::new(true));
        let close: Rc<dyn Fn()> = Rc::new({
            let alive = alive.clone();
            let child = Rc::downgrade(&self.child);
            let weak = window.as_weak();
            let parent = self.parent.clone();
            move || {
                if !alive.replace(false) {
                    return;
                }
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
                if let Some(child) = child.upgrade() {
                    child.borrow_mut().take();
                }
                if let Some(parent) = parent.upgrade() {
                    parent.set_notice_open(false);
                    parent.set_notice_message("".into());
                }
            }
        });
        let acknowledge: Rc<dyn Fn()> = Rc::new({
            let weak = window.as_weak();
            let parent = self.parent.clone();
            let valid = self.valid.clone();
            let close = close.clone();
            move || {
                if valid()
                    && parent.upgrade().is_some_and(|p| p.window().is_visible())
                    && weak.upgrade().is_some_and(|w| w.window().is_visible())
                {
                    close();
                }
            }
        });
        window.on_cancelled({
            let acknowledge = acknowledge.clone();
            move || acknowledge()
        });
        window.window().on_close_requested(move || {
            acknowledge();
            slint::CloseRequestResponse::KeepWindowShown
        });
        parent.set_notice_open(true);
        parent.set_notice_message(message.into());
        *self.child.borrow_mut() = Some(Notice {
            _window: window.clone_strong(),
            close: close.clone(),
        });
        if let Err(error) = window.show() {
            close();
            return Err(error.to_string());
        }
        Ok(())
    }
}
impl Drop for Notices {
    fn drop(&mut self) {
        self.cancel();
    }
}
