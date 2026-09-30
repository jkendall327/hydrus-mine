//! The hydrus-rs desktop client.
//!
//! Behaviour lives in plain Rust types ([`SearchPage`]) that tests drive
//! directly; the Slint files in `ui/` only lay out and bind (GUI.md).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ModelRc, SharedPixelBuffer, SharedString, VecModel};

/// The UI compiled from `ui/` (generated code).
#[allow(missing_debug_implementations)]
mod ui {
    slint::include_modules!();
}

pub use ui::*;

pub mod autocomplete;
mod grid;
pub mod headless;
mod page;

pub use grid::ThumbnailRows;
pub use page::SearchPage;

/// A decoded image as Slint shows it.
pub fn image(raster: &hydrus_media::Raster) -> slint::Image {
    let (width, height) = (raster.width(), raster.height());
    match raster.channels() {
        4 => slint::Image::from_rgba8(SharedPixelBuffer::clone_from_slice(
            raster.data(),
            width,
            height,
        )),
        3 => slint::Image::from_rgb8(SharedPixelBuffer::clone_from_slice(
            raster.data(),
            width,
            height,
        )),
        _ => {
            let rgb: Vec<u8> = raster.data().iter().flat_map(|&v| [v, v, v]).collect();
            slint::Image::from_rgb8(SharedPixelBuffer::clone_from_slice(&rgb, width, height))
        }
    }
}

/// Show `page` in `window`, and let the window change it. Returns the
/// grid's rows.
pub fn bind(window: &MainWindow, page: Rc<RefCell<SearchPage>>) -> Rc<ThumbnailRows> {
    let rows = Rc::new(ThumbnailRows::new(page.clone()));
    window.set_thumbnail_rows(ModelRc::from(rows.clone()));
    rows.set_columns(usize::try_from(window.get_grid_columns()).unwrap_or(1));
    refresh(window, &page.borrow());

    // after a change to the page, show it; `true` if its files changed
    let shown = {
        let page = page.clone();
        let weak = window.as_weak();
        let rows = rows.clone();
        move |files: bool| {
            if let Some(window) = weak.upgrade() {
                refresh(&window, &page.borrow());
                if files {
                    rows.reset();
                }
            }
        }
    };
    window.on_search_edited({
        let page = page.clone();
        let shown = shown.clone();
        move |text| {
            page.borrow_mut().type_text(&text);
            shown(false);
        }
    });
    window.on_search_accepted({
        let page = page.clone();
        let shown = shown.clone();
        move || {
            page.borrow_mut().enter();
            shown(true);
        }
    });
    window.on_move_highlight({
        let page = page.clone();
        let shown = shown.clone();
        move |by| {
            page.borrow_mut().move_highlight(by as isize);
            shown(false);
        }
    });
    window.on_suggestion_chosen({
        let page = page.clone();
        let shown = shown.clone();
        move |index| {
            page.borrow_mut()
                .choose(usize::try_from(index).unwrap_or(usize::MAX));
            shown(true);
        }
    });
    window.on_remove_predicate({
        let page = page.clone();
        move |index| {
            page.borrow_mut()
                .remove_predicate(usize::try_from(index).unwrap_or(usize::MAX));
            shown(true);
        }
    });
    window.on_columns_changed({
        let rows = rows.clone();
        move |columns| rows.set_columns(usize::try_from(columns).unwrap_or(1))
    });
    window.on_thumbnail_clicked({
        let rows = rows.clone();
        move |index| {
            let before = page.borrow().selected();
            page.borrow_mut()
                .select(usize::try_from(index).unwrap_or(usize::MAX));
            let after = page.borrow().selected();
            for changed in [before, after].into_iter().flatten() {
                rows.file_changed(changed);
            }
        }
    });
    rows
}

/// Show the page's search: the box's text and suggestions, the predicates,
/// any error, and the status bar.
fn refresh(window: &MainWindow, page: &SearchPage) {
    let autocomplete = page.autocomplete();
    window.set_search_text(autocomplete.text().into());
    let suggestions: Vec<SharedString> = autocomplete
        .suggestions()
        .iter()
        .map(|s| s.label.as_str().into())
        .collect();
    window.set_suggestions(ModelRc::new(VecModel::from(suggestions)));
    window.set_highlighted(
        autocomplete
            .highlighted()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1),
    );
    let predicates: Vec<SharedString> = page.predicates().iter().map(Into::into).collect();
    window.set_predicates(ModelRc::new(VecModel::from(predicates)));
    window.set_error(page.error().unwrap_or_default().into());
    window.set_status(page.status().into());
}
