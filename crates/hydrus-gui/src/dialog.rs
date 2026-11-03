//! The reference's ordinary dialogs and frames, as one helper that ends them
//! (`ClientGUITopLevelWindows`' `NewDialog` and `DialogThatResizes`, with
//! `ClientGUITopLevelWindowsPanels`' `DialogEdit`, `DialogManage`,
//! `DialogNullipotent` and `FrameThatTakesScrollablePanel`).
//!
//! A [`Dialog`] is ended by its apply (or its close button, for a dialog
//! that only shows things), by its cancel, or by its window's own close. As
//! `NewDialog._TryEndModal`: an apply whose dialog is not valid leaves it
//! open; a valid one keeps its frame (`SaveTLWSizeAndPosition` on its frame
//! key, `regular_dialog` unless it has one of its own), then commits, then
//! closes; a cancel or the window's close keeps nothing. A [`Dialog::frame`]
//! (a window with only a close button) keeps its frame a moment after each
//! move or resize instead, as `FrameThatResizes` does, and nothing as it
//! closes (recorded in `oracle/fixtures/regular_dialog.json`).
//!
//! Where it opens is decided as it is made (`app_title::new` places every
//! window by `regular_dialog`, fitted to the screens once shown), so the
//! helper only ends it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::ComponentHandle;

/// Whether it is a dialog (kept when applied) or a frame (kept as it moves).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dialog,
    Frame,
}

struct Inner<C: ComponentHandle + 'static> {
    window: slint::Weak<C>,
    name: &'static str,
    kind: Kind,
    open: Cell<bool>,
    /// What runs once it has closed, however it was ended.
    closed: RefCell<Vec<Box<dyn FnOnce()>>>,
    /// A frame's keeping of its geometry as it moves.
    autosave: RefCell<Option<Rc<slint::Timer>>>,
}

/// An ordinary dialog or frame, until it is ended. Clones are the same one.
pub(crate) struct Dialog<C: ComponentHandle + 'static> {
    inner: Rc<Inner<C>>,
}

impl<C: ComponentHandle + 'static> Clone for Dialog<C> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<C: ComponentHandle + 'static> Dialog<C> {
    /// An ordinary dialog (`DialogEdit`, `DialogManage`,
    /// `DialogNullipotent`): kept by `regular_dialog` when applied. Its
    /// window's own close cancels it.
    pub(crate) fn new(component: &C) -> Self {
        Self::make(component, "regular_dialog", Kind::Dialog)
    }

    /// A dialog the reference opens by a frame of its own.
    #[allow(dead_code)]
    pub(crate) fn named(component: &C, name: &'static str) -> Self {
        Self::make(component, name, Kind::Dialog)
    }

    /// An ordinary frame (`FrameThatTakesScrollablePanel`): kept by
    /// `regular_dialog` a moment after each move and resize while open.
    /// Its window's own close closes it.
    pub(crate) fn frame(component: &C) -> Self {
        let dialog = Self::make(component, "regular_dialog", Kind::Frame);
        if let Some(store) = crate::windows::store() {
            *dialog.inner.autosave.borrow_mut() = Some(crate::windows::autosave(
                component.as_weak(),
                store,
                dialog.inner.name,
            ));
        }
        dialog
    }

    fn make(component: &C, name: &'static str, kind: Kind) -> Self {
        let dialog = Self {
            inner: Rc::new(Inner {
                window: component.as_weak(),
                name,
                kind,
                open: Cell::new(true),
                closed: RefCell::default(),
                autosave: RefCell::default(),
            }),
        };
        // (the window holds it for as long as the window lives; it holds
        // the window only weakly)
        component.window().on_close_requested({
            let dialog = dialog.clone();
            move || {
                dialog.end();
                slint::CloseRequestResponse::HideWindow
            }
        });
        dialog
    }

    /// Run `then` once it has closed, however it was ended.
    pub(crate) fn on_closed(&self, then: impl FnOnce() + 'static) {
        self.inner.closed.borrow_mut().push(Box::new(then));
    }

    /// Whether it has not been ended yet.
    pub(crate) fn is_open(&self) -> bool {
        self.inner.open.get()
    }

    /// Its apply (a frame has none: closed): unless `valid` says otherwise
    /// (the dialog says why itself), its frame is kept, `commit` runs, and
    /// it closes.
    pub(crate) fn apply(&self, valid: impl FnOnce() -> bool, commit: impl FnOnce()) {
        if !self.is_open() {
            return;
        }
        if self.inner.kind == Kind::Dialog {
            if !valid() {
                return;
            }
            if let (Some(component), Some(store)) =
                (self.inner.window.upgrade(), crate::windows::store())
            {
                crate::windows::save_named(component.window(), &store, self.inner.name);
            }
        }
        commit();
        self.end();
    }

    /// Accepted, with nothing to check or commit (a close button that is
    /// the dialog's OK).
    pub(crate) fn accept(&self) {
        self.apply(|| true, || {});
    }

    /// Its cancel, or its window's own close: closed, nothing kept.
    pub(crate) fn cancel(&self) {
        self.end();
    }

    /// A frame's close button: closed (what it kept as it moved stays).
    pub(crate) fn close(&self) {
        self.end();
    }

    fn end(&self) {
        if !self.inner.open.replace(false) {
            return;
        }
        if let Some(timer) = self.inner.autosave.borrow_mut().take() {
            timer.stop();
        }
        if let Some(component) = self.inner.window.upgrade() {
            let _ = component.hide();
        }
        let closed = std::mem::take(&mut *self.inner.closed.borrow_mut());
        for then in closed {
            then();
        }
    }
}
