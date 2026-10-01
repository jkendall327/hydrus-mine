//! The archive/delete filter's window, bound to its model
//! ([`ArchiveDeleteFilter`]): files play as in the media viewer, and
//! finishing (or stopping, with anything decided) asks to commit.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::archive_delete::ArchiveDeleteFilter;
use crate::{ArchiveDeleteWindow, ListText, Removed, animation, list_text, playback};

/// Open the filter's window; it forgets itself from `slot` when closed,
/// and tells `removed` of files committing deleted out of the page.
pub(crate) fn open(
    model: ArchiveDeleteFilter,
    location: hydrus_search::LocationContext,
    slot: &Rc<RefCell<Option<ArchiveDeleteWindow>>>,
    removed: Removed,
) -> Result<ArchiveDeleteWindow, slint::PlatformError> {
    let window = ArchiveDeleteWindow::new()?;
    let model = Rc::new(RefCell::new(model));
    let playback = playback::Playback::new(model.borrow().store().dir().join("mpv.conf"));
    let animator = animation::Animator::new();
    let settings: hydrus_core::media_viewer::MediaViewerSettings = model
        .borrow()
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let zoomed = crate::zoom_window!(window, settings);
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let playback = playback.clone();
        let animator = animator.clone();
        move || {
            playback.close();
            animator.stop();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    // show the file to decide on; with none left, ask
    let show = {
        let model = model.clone();
        let weak = window.as_weak();
        let playback = playback.clone();
        let animator = animator.clone();
        let zoomed = zoomed.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            window.set_caption(model.caption().into());
            let Some(file) = model.current() else {
                playback.stop();
                animator.stop();
                window.set_question(model.question().into());
                return;
            };
            let store = model.store();
            let (shape, media) = (
                crate::viewer::shape(store, file),
                crate::viewer::still(store, file).map(std::sync::Arc::new),
            );
            let (playable, animation) = (
                crate::viewer::playable(store, file),
                crate::viewer::animation(store, file),
            );
            window.set_media(media.as_deref().map(crate::image).unwrap_or_default());
            let still = playable.is_none() && animation.is_none();
            zoomed.set_still(crate::viewer::still_of(media, shape, still));
            zoomed.show(shape);
            let tags: Vec<ListText> = crate::viewer::hover_tags(store, file)
                .iter()
                .map(|(row, rgb)| list_text(row, *rgb))
                .collect();
            window.set_tags(ModelRc::new(VecModel::from(tags)));
            let (size, frame) = (weak.clone(), weak.clone());
            let zoomed = zoomed.clone();
            playback.play(
                playable.as_deref(),
                move || {
                    // (rendered at the size shown)
                    zoomed.render_size().or_else(|| {
                        let size = size.upgrade()?.window().size();
                        Some((size.width, size.height))
                    })
                },
                move |image| {
                    if let Some(window) = frame.upgrade() {
                        window.set_media(image);
                    }
                },
            );
            let frame = weak.clone();
            animator.play(animation, move |image| {
                if let Some(window) = frame.upgrade() {
                    window.set_media(image);
                }
            });
        }
    };
    // after a decision: the next file, or, when done, ask (or, with
    // nothing to commit, close)
    let decided = {
        let model = model.clone();
        let show = show.clone();
        let close = close.clone();
        move |decide: fn(&mut ArchiveDeleteFilter)| {
            decide(&mut model.borrow_mut());
            let (done, anything) = {
                let model = model.borrow();
                (model.is_done(), model.has_decisions())
            };
            if done && !anything {
                close();
            } else {
                show();
            }
        }
    };
    window.on_keep({
        let decided = decided.clone();
        move || decided(ArchiveDeleteFilter::keep)
    });
    window.on_delete({
        let decided = decided.clone();
        move || decided(ArchiveDeleteFilter::delete)
    });
    window.on_skip(move || decided(ArchiveDeleteFilter::skip));
    window.on_back({
        let model = model.clone();
        let show = show.clone();
        move || {
            model.borrow_mut().back();
            show();
        }
    });
    window.on_close_requested({
        let model = model.clone();
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            let model = model.borrow();
            match weak.upgrade() {
                Some(window) if model.has_decisions() => {
                    window.set_question(model.question().into());
                }
                _ => close(),
            }
        }
    });
    window.on_resume({
        let model = model.clone();
        let weak = window.as_weak();
        let show = show.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                window.set_question(SharedString::new());
            }
            // (finished, back to the last file)
            if model.borrow().is_done() {
                model.borrow_mut().back();
            }
            show();
        }
    });
    window.on_forget({
        let close = close.clone();
        move || close()
    });
    window.on_commit({
        let model = model.clone();
        let close = close.clone();
        move || {
            let model = model.borrow();
            if let Err(e) = model.commit() {
                eprintln!("could not commit the archive/delete filter: {e}");
                return;
            }
            let deleted = model.deleted();
            let still = crate::media_actions::still_in(model.store(), &location, &deleted);
            let gone: Vec<_> = deleted.into_iter().filter(|d| !still.contains(d)).collect();
            drop(model);
            if !gone.is_empty() {
                removed(&gone);
            }
            close();
        }
    });
    window.on_toggle_pause(move || {
        playback.toggle_pause();
        animator.toggle_pause();
    });
    crate::bind_zoom!(window, zoomed);
    show();
    window.show()?;
    Ok(window)
}
