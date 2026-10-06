//! Tags > sibling/parent sync > "review current sibling/parent sync"
//! (hydrus-gui-model's `tag_sync_review`): a tab per tag service, opened on
//! the default tag service's, remembering the tab chosen when the tag
//! dialogs do.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_gui_model::tag_sync_review::{self as model, Tab};
use hydrus_store::Store;
use hydrus_store::settings::{self, BackgroundWork};
use hydrus_store::tag_editing::TagEditingSettings;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::TagSyncReviewWindow;

thread_local! {
    static OPEN: RefCell<Option<TagSyncReviewWindow>> = const { RefCell::new(None) };
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
    if let Some(window) = OPEN.with_borrow(|w| w.as_ref().map(slint::ComponentHandle::clone_strong))
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
    let window = match TagSyncReviewWindow::new() {
        Ok(window) => window,
        Err(e) => {
            eprintln!("could not open the tag display sync: {e}");
            return;
        }
    };
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
        let state = state.clone();
        let weak = window.as_weak();
        move |i| {
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
            if let Some(window) = weak.upgrade() {
                paint(&window, &state);
            }
        }
    });
    window.on_refresh_clicked({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            let mut state = state.borrow_mut();
            reload(&mut state);
            if let Some(window) = weak.upgrade() {
                paint(&window, &state);
            }
        }
    });
    let close = || {
        if let Some(window) = OPEN.with_borrow_mut(Option::take) {
            let _ = window.hide();
        }
    };
    window.on_close_clicked(close);
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    if let Err(e) = window.show() {
        eprintln!("could not show the tag display sync: {e}");
        return;
    }
    OPEN.set(Some(window));
}
