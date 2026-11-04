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
    ApiRequestWindow,
    AutoResolutionReviewWindow,
    AutoResolutionRuleWindow,
    AutoResolutionRulesWindow,
    BandwidthWindow,
    CheckerOptionsWindow,
    ChoiceButtonsWindow,
    ClientApiKeysWindow,
    ComparatorWindow,
    ConversionWindow,
    DatabaseLocationsWindow,
    DeleteFilesWindow,
    DownloaderDefinitionEditWindow,
    DownloaderDefinitionsWindow,
    DownloaderDisplayWindow,
    EditApiPermissionsWindow,
    EditNetworkValueWindow,
    EditServiceWindow,
    EditSubscriptionWindow,
    EditValueWindow,
    EmbeddedMetadataWindow,
    ExportFolderWindow,
    ExternalCallWindow,
    ExternalCommandWindow,
    ExternalProgramEntryWindow,
    FavouriteEditWindow,
    FavouritesWindow,
    FileLogWindow,
    FileMaintenanceWindow,
    FilenameTaggingWindow,
    FoldersWindow,
    ForceFiletypeWindow,
    FormulaRuleWindow,
    FormulaWindow,
    FrameLocationWindow,
    GranularityWindow,
    ImportFolderWindow,
    ImportOptionsOverwriteWindow,
    ImportOptionsWindow,
    IncrementalTaggingWindow,
    LocationMaxSizeWindow,
    LocationsWindow,
    LoginCredentialDefinitionWindow,
    LoginCredentialsWindow,
    LoginDomainsWindow,
    LoginScriptWindow,
    LoginScriptsWindow,
    LoginStepWindow,
    LoginTestResultWindow,
    ManageNotesWindow,
    ManageRatingsWindow,
    ManageTagsWindow,
    ManageTimesWindow,
    ManageUrlsWindow,
    MediaViewWindow,
    MergeOptionsWindow,
    NetworkDataWindow,
    NetworkJobsWindow,
    OpenFileCallsWindow,
    ParserEditWindow,
    ParserListWindow,
    ParserPickerWindow,
    PngExportWindow,
    PngsExportWindow,
    PredicateEditorWindow,
    RegexFavouritesWindow,
    ReviewImportsWindow,
    SearchOrWindow,
    ServicesEditorWindow,
    ServicesReviewWindow,
    ShortcutCommandWindow,
    ShortcutSetWindow,
    SidecarNodeWindow,
    SidecarRouterWindow,
    SidecarRoutersWindow,
    SimpleFormulaeWindow,
    StringConverterWindow,
    StringProcessorWindow,
    StringStepWindow,
    SubscriptionsWindow,
    TagBannerWindow,
    TagDisplayWindow,
    TagFilterWindow,
    TagMigrationWindow,
    TagRelationshipsWindow,
    TagSyncReviewWindow,
    VacuumReviewWindow,
    WhatIsThisWindow,
);

/// A dialog on the helper, by its window: whether it is still about, and
/// how it is ended as its owner goes ([`Dialog::cancel_confirmed`]).
struct Helped {
    window: *const slint::Window,
    alive: Box<dyn Fn() -> bool>,
    end: Rc<dyn Fn()>,
}

thread_local! {
    /// Every dialog on the helper, so a window held on is ended as a dialog
    /// when it is one ([`Dialog::hold`]).
    static HELPED: RefCell<Vec<Helped>> = const { RefCell::new(Vec::new()) };
    /// The message or question a dialog last opened of its own (for tests).
    static LAST_OWN: RefCell<Option<slint::Weak<crate::ChoiceButtonsWindow>>> =
        const { RefCell::new(None) };
}

/// The message or question a dialog last opened of its own
/// ([`Dialog::message`], [`Dialog::ask`]), while it is shown (for tests).
#[doc(hidden)]
pub fn last_own_question() -> Option<crate::ChoiceButtonsWindow> {
    LAST_OWN
        .with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}

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
    escape_first: RefCell<Vec<Box<dyn Fn() -> bool>>>,
    /// Whether a child the reference opens modally is open, which neither
    /// Escape nor the window's own close reach past.
    modal_child: RefCell<Vec<Box<dyn Fn() -> bool>>>,
    /// A frame's keeping of its geometry as it moves.
    autosave: RefCell<Option<Rc<slint::Timer>>>,
    /// The store its frame is kept in.
    store: Option<std::sync::Arc<hydrus_store::Store>>,
    /// A frame with a frame key of its own, kept as it closes.
    save_on_close: Cell<bool>,
    /// The windows it opened modally and holds on (see [`Dialog::hold`]),
    /// each while it is shown: whether it is, and how it is closed.
    holds: RefCell<Vec<Held>>,
    /// Its own messages and questions, closed with it.
    own: RefCell<Vec<slint::Weak<crate::ChoiceButtonsWindow>>>,
}

/// A window held on: whether it is shown, and its close.
struct Held {
    open: Box<dyn Fn() -> bool>,
    close: Box<dyn Fn()>,
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

    /// [`Dialog::named`] in the main window's store (the reference's
    /// `deeply_nested_dialog`, `quick_select_dialog`, `quick_entry_dialog`
    /// dialogs); without one (no main window), kept nowhere.
    pub(crate) fn keyed(component: &C, name: &'static str) -> Self {
        match crate::windows::store() {
            Some(store) => Self::named(component, &store, name),
            None => Self::make(component, name, Kind::Dialog, None),
        }
    }

    /// A dialog the reference doesn't size or place (an old
    /// `ClientGUIDialogs.Dialog`, a panel hydrus-rs opens as a window of
    /// its own): it ends as the ordinary dialogs do, keeping no frame.
    pub(crate) fn plain(component: &C) -> Self {
        Self::make(component, "regular_dialog", Kind::Dialog, None)
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

    /// Fitted to the screens again, once its contents have changed what it
    /// wants (a waiting window grown into its list).
    pub(crate) fn refit(&self) {
        if let Some(component) = self.inner.window.upgrade() {
            self.fit(&component);
        }
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
                holds: RefCell::default(),
                own: RefCell::default(),
            }),
        };
        // (known by its window, for an owner that holds it)
        HELPED.with(|helped| {
            let mut helped = helped.borrow_mut();
            helped.retain(|h| (h.alive)());
            let (alive, end) = (Rc::downgrade(&dialog.inner), Rc::downgrade(&dialog.inner));
            helped.push(Helped {
                window: component.window(),
                alive: Box::new(move || alive.upgrade().is_some_and(|inner| inner.open.get())),
                end: Rc::new(move || {
                    if let Some(inner) = end.upgrade() {
                        Self { inner }.cancel_confirmed();
                    }
                }),
            });
        });
        // (the window holds it for as long as the window lives; it holds
        // the window only weakly)
        component.window().on_close_requested({
            let dialog = dialog.clone();
            move || {
                // (a modal child open: the window isn't the user's to close
                // under it, as Qt's modality has it)
                // (and a question or inline editor up within it is answered
                // no or closed first, as Escape does: the reference's are
                // modal dialogs of their own, which its close can't pass)
                if !dialog.held() && !dialog.first() && dialog.cancel() {
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
                    if !dialog.first() {
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
    /// (Several are tried in the order given; the window's own close runs
    /// them too.)
    pub(crate) fn escape_first(&self, first: impl Fn() -> bool + 'static) {
        self.inner.escape_first.borrow_mut().push(Box::new(first));
    }

    /// Whether something within took Escape (or the window's close) first.
    fn first(&self) -> bool {
        self.inner.escape_first.borrow().iter().any(|first| first())
    }

    /// Whether a child window the reference opens modally is open: while it
    /// is, Escape and the window's own close do nothing (its own buttons
    /// still end it, cancelling the child with it, as before).
    /// (Several may be given; any one open holds it.)
    pub(crate) fn modal_child(&self, open: impl Fn() -> bool + 'static) {
        self.inner.modal_child.borrow_mut().push(Box::new(open));
    }

    /// Hold on `child`, a window it opened modally, for as long as that is
    /// shown. Ended by its own buttons while `child` is still shown, it
    /// ends `child` with it, so nothing it held outlives it: a dialog on
    /// the helper is ended asking nothing, as Qt takes a modal child with
    /// its parent (its own questions and held windows with it); any other
    /// window is closed as its own close would, or hidden if it refuses.
    pub(crate) fn hold<W: ComponentHandle + 'static>(&self, child: &W) {
        let at: *const slint::Window = child.window();
        let helped = HELPED.with(|helped| {
            helped
                .borrow()
                .iter()
                .find(|h| std::ptr::eq(h.window, at) && (h.alive)())
                .map(|h| h.end.clone())
        });
        let shown = child.as_weak();
        let open: Box<dyn Fn() -> bool> =
            Box::new(move || shown.upgrade().is_some_and(|w| w.window().is_visible()));
        if let Some(end) = helped {
            self.inner.holds.borrow_mut().push(Held {
                open,
                close: Box::new(move || end()),
            });
            return;
        }
        let closed = child.as_weak();
        self.inner.holds.borrow_mut().push(Held {
            open,
            close: Box::new(move || {
                let Some(w) = closed.upgrade() else {
                    return;
                };
                // (a question up within it takes its first close, as
                // Escape does; one that still refuses is hidden all the
                // same, its owner gone)
                for _ in 0..3 {
                    if !w.window().is_visible() {
                        return;
                    }
                    w.window()
                        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
                }
                let _ = w.hide();
            }),
        });
    }

    /// A message of its own (`ShowWarning`, `ShowInformation`): an "ok"
    /// message window, which it waits on as the reference's modal message
    /// box keeps it waiting.
    pub(crate) fn message(&self, title: &str, text: &str) {
        if let Some(window) = crate::debug_actions::open_message(title, text, Box::new(|| {})) {
            self.hold_own(&window);
        }
    }

    /// Hold on a message or question of its own, kept for the tests too.
    fn hold_own(&self, window: &crate::ChoiceButtonsWindow) {
        LAST_OWN.with(|last| *last.borrow_mut() = Some(window.as_weak()));
        let mut own = self.inner.own.borrow_mut();
        own.retain(|w| w.upgrade().is_some_and(|w| w.window().is_visible()));
        own.push(window.as_weak());
        drop(own);
        self.hold(window);
    }

    /// Messages (title, text) of its own, one after another, each shown
    /// once the one before is dismissed, and each waited on.
    pub(crate) fn messages(&self, messages: Vec<(String, String)>) {
        // (none once it has ended)
        if !self.is_open() {
            return;
        }
        let mut messages = messages.into_iter();
        let Some((title, text)) = messages.next() else {
            return;
        };
        let rest: Vec<_> = messages.collect();
        let next = self.clone();
        if let Some(window) =
            crate::debug_actions::open_message(&title, &text, Box::new(move || next.messages(rest)))
        {
            self.hold_own(&window);
        }
    }

    /// A yes/no question of its own (`GetYesNo`): `yes` runs on "yes"; it
    /// waits on the question, as on the reference's modal one.
    pub(crate) fn ask(&self, message: &str, yes: impl FnOnce() + 'static) {
        let ask = crate::choice_buttons::Ask {
            title: "Are you sure?",
            message,
            choices: vec!["yes".into()],
            no_label: "no",
        };
        // ("yes" counts only while it is open: a question it closed with
        // itself answers nothing)
        let alive = Rc::downgrade(&self.inner);
        match crate::choice_buttons::open(&ask, move |answer| {
            if answer == Some(0) && alive.upgrade().is_some_and(|inner| inner.open.get()) {
                yes();
            }
        }) {
            Ok(Some(window)) => self.hold_own(&window),
            Ok(None) => {}
            Err(e) => eprintln!("could not ask: {e}"),
        }
    }

    fn held(&self) -> bool {
        // (a held window once closed is let go)
        self.inner.holds.borrow_mut().retain(|held| (held.open)());
        !self.inner.holds.borrow().is_empty()
            || self.inner.modal_child.borrow().iter().any(|open| open())
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
            // (a window already ended, shown again, is only hidden)
            if let Some(component) = self.inner.window.upgrade() {
                let _ = component.hide();
            }
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
        // (its own messages and questions close with it, answering nothing)
        let own = std::mem::take(&mut *self.inner.own.borrow_mut());
        for window in own.iter().filter_map(slint::Weak::upgrade) {
            if window.window().is_visible() {
                window.invoke_cancelled();
            }
        }
        // (and so does every other window it held)
        let held = std::mem::take(&mut *self.inner.holds.borrow_mut());
        for held in held {
            (held.close)();
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

    /// A dialog ended by its own buttons while it holds a child closes the
    /// child with it (the child's own close: cancelled, if it is a dialog
    /// on the helper), even one that refuses its close, so no child it
    /// held outlives it.
    #[test]
    fn ending_a_dialog_closes_the_windows_it_holds() {
        let _windows = crate::headless::init();
        let parent = crate::AboutWindow::new().unwrap();
        parent.show().unwrap();
        let dialog = Dialog::new(&parent);
        let child = crate::AboutWindow::new().unwrap();
        child.show().unwrap();
        let child_dialog = Dialog::new(&child);
        let child_closed = Rc::new(Cell::new(false));
        child_dialog.on_closed({
            let child_closed = child_closed.clone();
            move || child_closed.set(true)
        });
        // (a child with changes: its cancel asks, as `UserIsOKToCancel`)
        let stubborn = crate::AboutWindow::new().unwrap();
        stubborn.show().unwrap();
        let stubborn_dialog = Dialog::new(&stubborn);
        let stubborn_closed = Rc::new(Cell::new(false));
        stubborn_dialog.on_closed({
            let stubborn_closed = stubborn_closed.clone();
            move || stubborn_closed.set(true)
        });
        stubborn_dialog.refuse_cancel({
            let asker = Rc::downgrade(&stubborn_dialog.inner);
            move || {
                if let Some(inner) = asker.upgrade() {
                    Dialog { inner }.ask("Sure you are ok to cancel?", || {});
                }
                false
            }
        });
        // (and a window not on the helper)
        let plain = crate::AboutWindow::new().unwrap();
        plain.show().unwrap();
        dialog.hold(&child);
        dialog.hold(&stubborn);
        dialog.hold(&plain);
        // (held: its own close waits)
        parent
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert!(parent.window().is_visible() && dialog.is_open());
        // the child with changes asks
        stubborn
            .window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        let question = last_own_question().expect("it asks");
        assert!(stubborn.window().is_visible());
        dialog.accept();
        assert!(!parent.window().is_visible());
        assert!(!child.window().is_visible(), "the child closes with it");
        assert!(child_closed.get() && !child_dialog.is_open(), "cancelled");
        assert!(
            !stubborn.window().is_visible() && !stubborn_dialog.is_open() && stubborn_closed.get(),
            "the child with changes is ended, asking nothing more"
        );
        assert!(!question.window().is_visible(), "and its question with it");
        assert!(!plain.window().is_visible(), "a plain window is closed");
    }
}
