//! Frozen originating page identity and weak main owner for viewer close.
use crate::{ChangePages, MainWindow, SearchPage};
use hydrus_core::{HashId, pages::PageKey};
use hydrus_gui_model::viewer_closing::{Action, actions};
use hydrus_store::settings::{self, ViewerClosingSettings};
use slint::ComponentHandle as _;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// Observe real activation attempts in headless replay; never replaces activation.
pub type ActivationObserver = Rc<dyn Fn(&MainWindow, Action)>;
thread_local! {
    static ACTIVATION_OBSERVER: RefCell<Option<ActivationObserver>> = const { RefCell::new(None) };
}
pub fn set_activation_observer(observer: Option<ActivationObserver>) {
    ACTIVATION_OBSERVER.with(|slot| *slot.borrow_mut() = observer);
}

fn activate(window: &MainWindow, reason: Action) {
    use slint::winit_030::WinitWindowAccessor as _;
    let observer = ACTIVATION_OBSERVER.with(|slot| slot.borrow().clone());
    if let Some(observer) = observer {
        observer(window, reason);
    }
    // Real native focus/raise; Slint receives the OS activation normally.
    let _ = window
        .window()
        .with_winit_window(|native| native.focus_window());
}

pub(crate) struct Owner {
    original: Option<(PageKey, Weak<RefCell<SearchPage>>)>,
    main: slint::Weak<MainWindow>,
    change_pages: ChangePages,
    reveal_exit: Rc<dyn Fn(PageKey, HashId)>,
}
impl std::fmt::Debug for Owner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Owner")
            .field("original_key", &self.original.as_ref().map(|(key, _)| key))
            .finish_non_exhaustive()
    }
}
impl Owner {
    pub(crate) fn new(
        original: Option<(PageKey, Weak<RefCell<SearchPage>>)>,
        main: slint::Weak<MainWindow>,
        change_pages: ChangePages,
        reveal_exit: Rc<dyn Fn(PageKey, HashId)>,
    ) -> Rc<Self> {
        Rc::new(Self {
            original,
            main,
            change_pages,
            reveal_exit,
        })
    }

    pub(crate) fn closed(&self, store: &hydrus_store::Store, exit: Option<HashId>) {
        let Some(main) = self.main.upgrade() else {
            return;
        };
        let policy: ViewerClosingSettings = store.read(settings::get).unwrap_or_default();
        let original = self
            .original
            .as_ref()
            .and_then(|(key, page)| page.upgrade().map(|page| (*key, page)));
        let requested = actions(&policy, original.is_some() && exit.is_some());
        if let Some((key, page)) = original
            && let Some(exit) = exit
            && requested
                .iter()
                .any(|action| matches!(action, Action::ReselectPage | Action::SelectExitMedia))
        {
            (self.change_pages)(&|pages| {
                for action in &requested {
                    match action {
                        Action::ReselectPage => {
                            pages.show(&key);
                        }
                        Action::SelectExitMedia => {
                            let mut page = page.borrow_mut();
                            let index = page
                                .results()
                                .iter()
                                .position(|&item| page.files_of(item).contains(&exit));
                            if let Some(index) = index {
                                // A normal hit focuses the collection containing the exit
                                // file, preserving multiple selection if already selected.
                                page.select(index);
                                (self.reveal_exit)(key, exit);
                            }
                        }
                        Action::ActivateFocusing | Action::ActivateDebug => {}
                    }
                }
                Ok(())
            });
        }
        for action in requested {
            if matches!(action, Action::ActivateFocusing | Action::ActivateDebug) {
                activate(&main, action);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unowned_and_destroyed_originals_only_deliver_debug_to_a_live_main_owner() {
        let legacy = hydrus_testkit::legacy_fixture("basic");
        let native = tempfile::tempdir().unwrap();
        hydrus_store::import::import_legacy(
            legacy.path(),
            &native.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store = hydrus_store::Store::open(native.path()).unwrap();
        store
            .write(|conn| {
                settings::set(
                    conn,
                    &ViewerClosingSettings {
                        reselect_page: true,
                        select_exit_media: true,
                        activate_focusing: true,
                        activate_always: true,
                    },
                )
            })
            .unwrap();
        let _windows = crate::headless::init();
        let main = MainWindow::new().unwrap();
        let calls = Rc::new(RefCell::new(Vec::new()));
        set_activation_observer(Some(Rc::new({
            let calls = calls.clone();
            move |_, reason| calls.borrow_mut().push(reason)
        })));
        let change: ChangePages =
            Rc::new(|_| panic!("dead and unowned sources must not redirect another page"));
        let reveal = Rc::new(|_, _| panic!("no surviving original exit selection"));
        let unowned = Owner::new(None, main.as_weak(), change.clone(), reveal.clone());
        unowned.closed(&store, Some(HashId(1)));
        assert_eq!(calls.borrow().as_slice(), [Action::ActivateDebug]);
        calls.borrow_mut().clear();
        let page = Rc::new(RefCell::new(SearchPage::new(store.clone())));
        let weak = Rc::downgrade(&page);
        let destroyed = Owner::new(
            Some((PageKey::random(), weak.clone())),
            main.as_weak(),
            change,
            reveal,
        );
        drop(page);
        assert!(
            weak.upgrade().is_none(),
            "the closing owner retains no source page"
        );
        destroyed.closed(&store, Some(HashId(1)));
        assert_eq!(calls.borrow().as_slice(), [Action::ActivateDebug]);
        calls.borrow_mut().clear();
        drop(main);
        destroyed.closed(&store, Some(HashId(1)));
        assert!(
            calls.borrow().is_empty(),
            "the closing owner retains no main component"
        );
        set_activation_observer(None);
    }
}
