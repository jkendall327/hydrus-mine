//! Tags > sibling/parent sync > "review current sibling/parent sync"
//! (hydrus-gui-model's `tag_sync_review`): a tab per tag service, opened on
//! the default tag service's, remembering the tab chosen when the tag
//! dialogs do.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use hydrus_gui_model::tag_sync_review::{self as model, Tab};
use hydrus_store::Store;
use hydrus_store::settings::{self, BackgroundWork};
use hydrus_store::tag_editing::TagEditingSettings;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::TagSyncReviewWindow;

thread_local! {
    static OPEN: RefCell<Option<Owner>> = const { RefCell::new(None) };
}

struct Owner {
    window: TagSyncReviewWindow,
    active: Rc<Cell<bool>>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.active.set(false);
        let _ = self.window.hide();
    }
}

fn admitted(
    active: &Rc<Cell<bool>>,
    weak: &slint::Weak<TagSyncReviewWindow>,
) -> Option<TagSyncReviewWindow> {
    let current = OPEN.with_borrow(|open| {
        open.as_ref()
            .is_some_and(|owner| Rc::ptr_eq(&owner.active, active))
    });
    if !active.get() || !current {
        return None;
    }
    weak.upgrade().filter(|window| window.window().is_visible())
}

struct State {
    store: Arc<Store>,
    tabs: Vec<Tab>,
    index: usize,
}

fn paint(window: &TagSyncReviewWindow, state: &State) {
    let work = state
        .store
        .read(settings::get::<BackgroundWork>)
        .unwrap_or_default();
    let (status, valid) =
        model::status(work.tag_display_during_idle, work.tag_display_during_active);
    window.set_status(status.into());
    window.set_status_valid(valid);
    window.set_service_names(ModelRc::new(VecModel::from(
        state
            .tabs
            .iter()
            .map(|t| SharedString::from(t.name.as_str()))
            .collect::<Vec<_>>(),
    )));
    window.set_service_index(i32::try_from(state.index).unwrap_or(0));
    let progress = state.tabs.get(state.index).map(|t| t.progress.clone());
    window.set_summary(model::SYNCED.into());
    window.set_progress(progress.unwrap_or_default().into());
}

fn reload(state: &mut State) {
    match model::tabs(&state.store) {
        Ok((tabs, _)) => {
            state.index = state.index.min(tabs.len().saturating_sub(1));
            state.tabs = tabs;
        }
        Err(e) => eprintln!("could not read the tag display sync: {e}"),
    }
}

/// Open the review, or raise it if it is open.
pub(crate) fn open(store: &Arc<Store>) {
    if let Some(window) =
        OPEN.with_borrow(|open| open.as_ref().map(|owner| owner.window.clone_strong()))
    {
        let _ = window.show();
        return;
    }
    let (tabs, index) = match model::tabs(store) {
        Ok(found) => found,
        Err(e) => {
            crate::debug_actions::message("Error", &e.to_string());
            return;
        }
    };
    let window = match crate::app_title::new::<crate::TagSyncReviewWindow>() {
        Ok(window) => window,
        Err(e) => {
            eprintln!("could not open the tag display sync: {e}");
            return;
        }
    };
    let active = Rc::new(Cell::new(true));
    let listbook = store
        .read(settings::get::<TagEditingSettings>)
        .unwrap_or_default()
        .use_listbook;
    window.set_use_listbook(listbook);
    window.set_message(model::MESSAGE.into());
    let state = Rc::new(RefCell::new(State {
        store: store.clone(),
        tabs,
        index,
    }));
    paint(&window, &state.borrow());
    window.on_service_chosen({
        let active = active.clone();
        let state = state.clone();
        let weak = window.as_weak();
        move |i| {
            let Some(window) = admitted(&active, &weak) else {
                return;
            };
            let Ok(i) = usize::try_from(i) else { return };
            let mut state = state.borrow_mut();
            let Some(key) = state.tabs.get(i).map(|t| t.key.clone()) else {
                return;
            };
            state.index = i;
            let remembered = state.store.write(move |ctx| {
                let mut editing: TagEditingSettings = settings::get(ctx.conn())?;
                if editing.remember_service {
                    editing.default_service = key;
                    settings::set(ctx.conn(), &editing)?;
                }
                Ok(())
            });
            if let Err(e) = remembered {
                eprintln!("could not remember the tag service tab: {e}");
            }
            paint(&window, &state);
        }
    });
    window.on_refresh_clicked({
        let active = active.clone();
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = admitted(&active, &weak) else {
                return;
            };
            let mut state = state.borrow_mut();
            reload(&mut state);
            paint(&window, &state);
        }
    });
    let close = Rc::new({
        let active = active.clone();
        let weak = window.as_weak();
        move || {
            active.set(false);
            let retired = OPEN.with_borrow_mut(|open| {
                if open
                    .as_ref()
                    .is_some_and(|owner| Rc::ptr_eq(&owner.active, &active))
                {
                    open.take()
                } else {
                    None
                }
            });
            drop(retired);
            // A retained predecessor can hide itself, never the current owner.
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
        }
    });
    window.on_close_clicked({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    if let Err(e) = window.show() {
        eprintln!("could not show the tag display sync: {e}");
        return;
    }
    OPEN.set(Some(Owner { window, active }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydrus_core::ServiceKey;
    use hydrus_store::services::{self, ServiceKind};
    use slint::Model as _;

    fn current() -> (TagSyncReviewWindow, Rc<Cell<bool>>) {
        OPEN.with_borrow(|open| {
            let owner = open.as_ref().unwrap();
            (owner.window.clone_strong(), owner.active.clone())
        })
    }
    fn editing(store: &Store) -> TagEditingSettings {
        store.read(settings::get).unwrap()
    }
    fn close(window: &TagSyncReviewWindow, x: bool) {
        if x {
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        } else {
            window.invoke_close_clicked();
        }
        assert!(!window.window().is_visible());
    }

    #[test]
    fn retained_predecessors_cannot_remember_refresh_or_close_the_current_review() {
        let _windows = crate::headless::init();
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let first = store
            .snapshot()
            .services
            .tag_services()
            .next()
            .unwrap()
            .key
            .clone();
        let second = ServiceKey::new(vec![42; 32]);
        let added = second.clone();
        store
            .write_and_refresh(move |ctx| {
                services::insert(ctx.conn(), &added, "second tags", &ServiceKind::LocalTags)?;
                Ok(())
            })
            .unwrap();
        let service_rows: Vec<_> = store
            .snapshot()
            .services
            .tag_services()
            .map(|service| (service.key.clone(), service.name.clone()))
            .collect();
        let first_row =
            i32::try_from(service_rows.iter().position(|row| row.0 == first).unwrap()).unwrap();
        let second_row =
            i32::try_from(service_rows.iter().position(|row| row.0 == second).unwrap()).unwrap();
        for listbook in [false, true] {
            let mut original = editing(&store);
            original.remember_service = true;
            original.default_service = first.clone();
            original.use_listbook = listbook;
            let seeded = original.clone();
            store
                .write(move |ctx| settings::set(ctx.conn(), &seeded))
                .unwrap();
            open(&store);
            let (old, old_active) = current();
            assert_eq!(old.get_use_listbook(), listbook);
            assert_eq!(old.get_service_index(), first_row);
            assert_eq!(
                old.get_service_names()
                    .iter()
                    .map(|name| name.to_string())
                    .collect::<Vec<_>>(),
                service_rows
                    .iter()
                    .map(|row| row.1.clone())
                    .collect::<Vec<_>>()
            );
            old.invoke_service_chosen(second_row);
            let mut remembered = original.clone();
            remembered.default_service = second.clone();
            assert_eq!(editing(&store), remembered);
            close(&old, listbook);
            assert!(!old_active.get());
            assert!(OPEN.with_borrow(Option::is_none));

            open(&store);
            let (successor, successor_active) = current();
            assert_eq!(successor.get_service_index(), second_row);
            old.set_status("retired status".into());
            old.set_progress("retired progress".into());
            old.show().unwrap();
            old.invoke_service_chosen(first_row);
            old.invoke_refresh_clicked();
            assert_eq!(old.get_status().as_str(), "retired status");
            assert_eq!(old.get_progress().as_str(), "retired progress");
            assert_eq!(editing(&store), remembered);
            // Both stale closing paths must leave OPEN and the successor intact.
            close(&old, false);
            assert!(successor.window().is_visible());
            old.show().unwrap();
            close(&old, true);
            assert!(successor.window().is_visible());
            assert!(OPEN.with_borrow(|open| {
                open.as_ref()
                    .is_some_and(|owner| Rc::ptr_eq(&owner.active, &successor_active))
            }));
            assert!(successor_active.get());

            store
                .write(|ctx| {
                    let mut work: BackgroundWork = settings::get(ctx.conn())?;
                    work.tag_display_during_idle = false;
                    work.tag_display_during_active = false;
                    settings::set(ctx.conn(), &work)
                })
                .unwrap();
            successor.set_status("hidden status".into());
            successor.hide().unwrap();
            successor.invoke_service_chosen(first_row);
            successor.invoke_refresh_clicked();
            assert_eq!(editing(&store), remembered);
            assert_eq!(successor.get_status().as_str(), "hidden status");
            open(&store); // Raising a current hidden review remains legitimate.
            assert!(successor.window().is_visible());
            successor.invoke_refresh_clicked();
            assert!(successor.get_status().starts_with(
                "Siblings and parents are not set to sync in the background at any time."
            ));
            successor.invoke_service_chosen(first_row);
            assert_eq!(editing(&store), original);

            // The existing live remember preference is read on each current change.
            let mut no_remember = original.clone();
            no_remember.remember_service = false;
            let saved = no_remember.clone();
            store
                .write(move |ctx| settings::set(ctx.conn(), &saved))
                .unwrap();
            successor.invoke_service_chosen(second_row);
            assert_eq!(successor.get_service_index(), second_row);
            assert_eq!(editing(&store), no_remember);
            close(&successor, !listbook);
            assert!(OPEN.with_borrow(Option::is_none));
            open(&store);
            let (reopened, _) = current();
            assert_eq!(reopened.get_service_index(), first_row);
            close(&reopened, false);
            assert_eq!(editing(&Store::open(home.path()).unwrap()), no_remember);
        }
    }
}
