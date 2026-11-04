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
//! closes; a cancel, Escape or the window's close keeps nothing. A [`Dialog::frame`]
//! (a window with only a close button, which Escape also closes) keeps its
//! frame a moment after each move or resize instead, as `FrameThatResizes` does, and nothing as it
//! closes (recorded in `oracle/fixtures/regular_dialog.json`).
//!
//! Where it opens is decided as it is made (`app_title::new` places every
//! window by `regular_dialog`); the helper also has it fitted to the screens
//! once shown (`windows::fit_after_show`), as only the windows the reference
//! opens by that frame are.
//!
//! The helper owns the window's close request (`on_close_requested`): a
//! window that ends through it must not set its own afterwards, which would
//! silently replace the helper's (Slint keeps one); what it needs done as it
//! closes goes in [`Dialog::on_closed`], and a reason to stay open in
//! [`Dialog::refuse_cancel`].
//!
//! A dialog with a frame key of its own ([`Dialog::named`]) is placed by
//! that frame, with its own off-screen rescue as it opens, rather than
//! fitted after.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::ComponentHandle;

/// A window the helper can end by Escape: its `.slint` has a `callback
/// escape()` its key handler calls (the reference's `keyPressEvent` on
/// Escape, from a widget within it that did not take the key).
pub(crate) trait Escapes: ComponentHandle {
    fn on_escape(&self, then: impl Fn() + 'static);
}

macro_rules! escapes {
    ($($window:ident),* $(,)?) => {
        // (a window without its own `escape` callback would call this
        // itself: refused)
        $(#[deny(unconditional_recursion)]
        impl Escapes for crate::$window {
            fn on_escape(&self, then: impl Fn() + 'static) {
                crate::$window::on_escape(self, then);
            }
        })*
    };
}

escapes!(
    AboutWindow,
    CheckerOptionsWindow,
    ChoiceButtonsWindow,
    EditServiceWindow,
    EditSubscriptionWindow,
    EditValueWindow,
    ExternalCallWindow,
    ExternalCommandWindow,
    FavouriteEditWindow,
    FavouritesWindow,
    FilenameTaggingWindow,
    ForceFiletypeWindow,
    ImportOptionsOverwriteWindow,
    IncrementalTaggingWindow,
    LocationsWindow,
    LoginStepWindow,
    ManageNotesWindow,
    ManageTagsWindow,
    ManageTimesWindow,
    ManageUrlsWindow,
    MediaViewWindow,
    MergeOptionsWindow,
    OpenFileCallsWindow,
    PngExportWindow,
    PngsExportWindow,
    SearchOrWindow,
    ServicesEditorWindow,
    ShortcutCommandWindow,
    ShortcutSetWindow,
    SimpleFormulaeWindow,
    SubscriptionsWindow,
    TagBannerWindow,
    TagFilterWindow,
    TagRelationshipsWindow,
);

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
    /// Whether it may be cancelled or closed now (`UserIsOKToCancel`).
    may_cancel: RefCell<Option<Box<dyn Fn() -> bool>>>,
    /// What Escape does first (a sub-state closed, a modal child waited on).
    escape_first: RefCell<Option<Box<dyn Fn() -> bool>>>,
    /// Whether a child the reference opens modally is open, which neither
    /// Escape nor the window's own close reach past.
    modal_child: RefCell<Option<Box<dyn Fn() -> bool>>>,
    /// A frame's keeping of its geometry as it moves.
    autosave: RefCell<Option<Rc<slint::Timer>>>,
    /// The store its frame is kept in.
    store: Option<std::sync::Arc<hydrus_store::Store>>,
    /// A frame with a frame key of its own, kept as it closes.
    save_on_close: Cell<bool>,
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

impl<C: Escapes + 'static> Dialog<C> {
    /// An ordinary dialog (`DialogEdit`, `DialogManage`,
    /// `DialogNullipotent`): kept by `regular_dialog` when applied. Its
    /// window's own close cancels it.
    pub(crate) fn new(component: &C) -> Self {
        let dialog = Self::make(
            component,
            "regular_dialog",
            Kind::Dialog,
            crate::windows::store(),
        );
        dialog.fit(component);
        dialog
    }

    /// A dialog the reference opens by a frame of its own
    /// (`manage_urls_dialog` and the like), kept in `store`: placed by that
    /// frame now (`windows::place_frame`, with its own off-screen rescue as
    /// it opens), and kept by it when applied.
    pub(crate) fn named(
        component: &C,
        store: &std::sync::Arc<hydrus_store::Store>,
        name: &'static str,
    ) -> Self {
        let dialog = Self::make(component, name, Kind::Dialog, Some(store.clone()));
        crate::windows::place_frame(component, store, name);
        dialog
    }

    /// A frame the reference opens by a frame key of its own (the viewer's
    /// `manage_tags_frame`), kept in `store`: placed by that frame now, and
    /// kept by it as it closes, however it is closed (as the other windows
    /// with frames of their own are, `frame_state.json`).
    pub(crate) fn named_frame(
        component: &C,
        store: &std::sync::Arc<hydrus_store::Store>,
        name: &'static str,
    ) -> Self {
        let dialog = Self::make(component, name, Kind::Frame, Some(store.clone()));
        dialog.inner.save_on_close.set(true);
        crate::windows::place_frame(component, store, name);
        dialog
    }

    /// An ordinary frame (`FrameThatTakesScrollablePanel`): kept by
    /// `regular_dialog` a moment after each move and resize while open.
    /// Its window's own close closes it.
    pub(crate) fn frame(component: &C) -> Self {
        let dialog = Self::make(
            component,
            "regular_dialog",
            Kind::Frame,
            crate::windows::store(),
        );
        dialog.fit(component);
        if let Some(store) = dialog.inner.store.clone() {
            *dialog.inner.autosave.borrow_mut() = Some(crate::windows::autosave(
                component.as_weak(),
                store,
                dialog.inner.name,
            ));
        }
        dialog
    }

    /// Fitted to the screens once shown (`windows::fit_after_show`).
    fn fit(&self, component: &C) {
        if let Some(store) = &self.inner.store {
            crate::windows::fit_after_show(component, store, self.inner.name);
        }
    }

    fn make(
        component: &C,
        name: &'static str,
        kind: Kind,
        store: Option<std::sync::Arc<hydrus_store::Store>>,
    ) -> Self {
        let dialog = Self {
            inner: Rc::new(Inner {
                window: component.as_weak(),
                name,
                kind,
                open: Cell::new(true),
                closed: RefCell::default(),
                may_cancel: RefCell::default(),
                escape_first: RefCell::default(),
                modal_child: RefCell::default(),
                autosave: RefCell::default(),
                store,
                save_on_close: Cell::new(false),
            }),
        };
        // (the window holds it for as long as the window lives; it holds
        // the window only weakly)
        component.window().on_close_requested({
            let dialog = dialog.clone();
            move || {
                // (a modal child open: the window isn't the user's to close
                // under it, as Qt's modality has it)
                if !dialog.held() && dialog.cancel() {
                    slint::CloseRequestResponse::HideWindow
                } else {
                    slint::CloseRequestResponse::KeepWindowShown
                }
            }
        });
        // (Escape cancels a dialog and closes a frame, both refusable)
        component.on_escape({
            let dialog = Rc::downgrade(&dialog.inner);
            move || {
                if let Some(inner) = dialog.upgrade() {
                    let dialog = Self { inner };
                    if dialog.held() {
                        return;
                    }
                    let taken = dialog
                        .inner
                        .escape_first
                        .borrow()
                        .as_ref()
                        .is_some_and(|first| first());
                    if !taken {
                        dialog.cancel();
                    }
                }
            }
        });
        dialog
    }

    /// Run `then` once it has closed, however it was ended.
    pub(crate) fn on_closed(&self, then: impl FnOnce() + 'static) {
        self.inner.closed.borrow_mut().push(Box::new(then));
    }

    /// Ask `may` before it is cancelled or closed (`UserIsOKToCancel`): if
    /// it says no, it stays open (the dialog says why itself, as by asking
    /// whether to discard its changes).
    pub(crate) fn refuse_cancel(&self, may: impl Fn() -> bool + 'static) {
        *self.inner.may_cancel.borrow_mut() = Some(Box::new(may));
    }

    /// What Escape does first, before it cancels or closes the window:
    /// `first` closes whatever the window has open within it (a question, an
    /// inline editor: back, as its own modal dialog's Escape would take it),
    /// and says whether it took it.
    pub(crate) fn escape_first(&self, first: impl Fn() -> bool + 'static) {
        *self.inner.escape_first.borrow_mut() = Some(Box::new(first));
    }

    /// Whether a child window the reference opens modally is open: while it
    /// is, Escape and the window's own close do nothing (its own buttons
    /// still end it, cancelling the child with it, as before).
    pub(crate) fn modal_child(&self, open: impl Fn() -> bool + 'static) {
        *self.inner.modal_child.borrow_mut() = Some(Box::new(open));
    }

    fn held(&self) -> bool {
        self.inner
            .modal_child
            .borrow()
            .as_ref()
            .is_some_and(|open| open())
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
            if let (Some(component), Some(store)) = (self.inner.window.upgrade(), &self.inner.store)
            {
                crate::windows::save_named(component.window(), store, self.inner.name);
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

    /// Its cancel, or its window's own close: closed, nothing kept, unless
    /// [`Dialog::refuse_cancel`] says it stays. Whether it closed (or was
    /// already closed).
    pub(crate) fn cancel(&self) -> bool {
        if !self.is_open() {
            return true;
        }
        let may = self
            .inner
            .may_cancel
            .borrow()
            .as_ref()
            .is_none_or(|may| may());
        if may {
            self.end();
        }
        may
    }

    /// Cancelled once the user has said yes to what
    /// [`Dialog::refuse_cancel`] asked (discard the changes): closed,
    /// nothing kept, without asking again.
    pub(crate) fn cancel_confirmed(&self) {
        self.end();
    }

    /// A frame's close button: closed (what it kept as it moved stays),
    /// unless [`Dialog::refuse_cancel`] says it stays.
    pub(crate) fn close(&self) -> bool {
        self.cancel()
    }

    fn end(&self) {
        if !self.inner.open.replace(false) {
            return;
        }
        if let Some(timer) = self.inner.autosave.borrow_mut().take() {
            timer.stop();
        }
        if let Some(component) = self.inner.window.upgrade() {
            if self.inner.save_on_close.get()
                && let Some(store) = &self.inner.store
            {
                crate::windows::save_named(component.window(), store, self.inner.name);
            }
            let _ = component.hide();
        }
        let closed = std::mem::take(&mut *self.inner.closed.borrow_mut());
        for then in closed {
            then();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dialog that may not be cancelled yet (`UserIsOKToCancel` saying
    /// no) stays open through its cancel and its window's close, and closes
    /// once it may.
    #[test]
    fn a_refused_cancel_or_close_leaves_the_dialog_open() {
        let _windows = crate::headless::init();
        let window = crate::AboutWindow::new().unwrap();
        window.show().unwrap();
        let dialog = Dialog::new(&window);
        let may = Rc::new(Cell::new(false));
        dialog.refuse_cancel({
            let may = may.clone();
            move || may.get()
        });
        let closed = Rc::new(Cell::new(0));
        dialog.on_closed({
            let closed = closed.clone();
            move || closed.set(closed.get() + 1)
        });
        assert!(!dialog.cancel());
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(window.window().is_visible() && dialog.is_open());
        assert_eq!(closed.get(), 0);
        may.set(true);
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(!window.window().is_visible() && !dialog.is_open());
        assert!(dialog.cancel(), "already closed");
        assert_eq!(closed.get(), 1, "its closing ran once");
    }
}
