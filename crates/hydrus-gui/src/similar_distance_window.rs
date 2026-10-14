//! "similar files in a new page > custom": the `choose distance` dialog
//! (a spin box from 10, in steps of 2, 0 to 99) of the thumbnails' open menu
//! and the media viewer's.

use std::cell::RefCell;
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_store::Store;
use slint::ComponentHandle as _;

use crate::{EditValueWindow, Pages, app_title, thumbnail_menu};

thread_local! {
    static EDITOR: RefCell<Option<EditValueWindow>> = const { RefCell::new(None) };
}

/// The dialog while one is open.
pub fn editor() -> Option<EditValueWindow> {
    EDITOR.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    })
}

fn close() {
    if let Some(editor) = EDITOR.with(|slot| slot.borrow_mut().take()) {
        let _ = editor.hide();
    }
}

/// Ask for a distance, then open a page of the files like `selected`.
pub(crate) fn open(
    store: Arc<Store>,
    selected: Vec<HashId>,
    location: hydrus_search::LocationContext,
    change_pages: impl Fn(&dyn Fn(&mut Pages) -> Result<(), String>) + 'static,
) {
    close();
    let Ok(window) = app_title::new::<EditValueWindow>() else {
        return;
    };
    window.set_window_title("choose distance".into());
    window.set_minimum(0);
    window.set_maximum(99);
    window.set_step_size(2);
    window.set_value(10);
    window.on_apply({
        let weak = window.as_weak();
        move || {
            let distance = weak
                .upgrade()
                .map_or(0, |w| u64::try_from(w.get_value()).unwrap_or(0));
            close();
            if let Some(search) = thumbnail_menu::similar_search(&store, &selected, distance) {
                change_pages(&|pages| {
                    pages.open_search(location.clone(), vec![search.clone()], "files");
                    Ok(())
                });
            }
        }
    });
    window.on_cancel(close);
    if window.show().is_ok() {
        EDITOR.with(|slot| *slot.borrow_mut() = Some(window));
    }
}
