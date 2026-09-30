//! The hydrus-rs desktop client.
//!
//! Behaviour lives in plain Rust types ([`SearchPage`]) that tests drive
//! directly; the Slint files in `ui/` only lay out and bind (GUI.md).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Model as _, ModelRc, SharedPixelBuffer, SharedString, VecModel};

/// The UI compiled from `ui/` (generated code).
#[allow(missing_debug_implementations)]
mod ui {
    slint::include_modules!();
}

pub use ui::*;

pub mod headless;
mod page;

pub use page::SearchPage;

/// The most thumbnails a page shows, until the grid loads them as they
/// scroll into view.
pub const SHOWN_LIMIT: usize = 1000;

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

/// Show `page` in `window`, and let the window change it.
pub fn bind(window: &MainWindow, page: Rc<RefCell<SearchPage>>) {
    let thumbnails: Rc<VecModel<Thumbnail>> = Rc::new(VecModel::default());
    window.set_thumbnails(ModelRc::from(thumbnails.clone()));
    refresh(window, &page.borrow(), &thumbnails);

    let changed = {
        let page = page.clone();
        let weak = window.as_weak();
        let thumbnails = thumbnails.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                refresh(&window, &page.borrow(), &thumbnails);
            }
        }
    };
    window.on_add_predicate({
        let page = page.clone();
        let changed = changed.clone();
        move |text| {
            page.borrow_mut().add_predicate(&text);
            changed();
        }
    });
    window.on_remove_predicate({
        let page = page.clone();
        move |index| {
            page.borrow_mut()
                .remove_predicate(usize::try_from(index).unwrap_or(usize::MAX));
            changed();
        }
    });
    window.on_thumbnail_clicked(move |index| {
        let mut page = page.borrow_mut();
        page.select(usize::try_from(index).unwrap_or(usize::MAX));
        for row in 0..thumbnails.row_count() {
            if let Some(mut thumbnail) = thumbnails.row_data(row) {
                let selected = page.selected() == Some(row);
                if thumbnail.selected != selected {
                    thumbnail.selected = selected;
                    thumbnails.set_row_data(row, thumbnail);
                }
            }
        }
    });
}

fn refresh(window: &MainWindow, page: &SearchPage, thumbnails: &VecModel<Thumbnail>) {
    let predicates: Vec<SharedString> = page.predicates().iter().map(Into::into).collect();
    window.set_predicates(ModelRc::new(VecModel::from(predicates)));
    let shown: Vec<Thumbnail> = page
        .results()
        .iter()
        .take(SHOWN_LIMIT)
        .enumerate()
        .map(|(i, &id)| Thumbnail {
            image: page.thumbnail(id).as_ref().map(image).unwrap_or_default(),
            selected: page.selected() == Some(i),
        })
        .collect();
    thumbnails.set_vec(shown);
    window.set_status(page.status().into());
    window.set_error(page.error().unwrap_or_default().into());
}
