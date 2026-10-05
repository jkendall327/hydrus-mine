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
    guard: Rc<dyn Fn() -> bool>,
    return_to: Rc<dyn Fn(hydrus_core::HashId)>,
) -> Result<ArchiveDeleteWindow, slint::PlatformError> {
    let window = ArchiveDeleteWindow::new()?;
    let guard: Rc<dyn Fn() -> bool> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        move || {
            guard()
                && weak.upgrade().is_some_and(|window| {
                    window.window().is_visible()
                        && slot.upgrade().is_some_and(|slot| {
                            slot.borrow().as_ref().is_some_and(|current| {
                                std::ptr::eq(current.window(), window.window())
                            })
                        })
                })
        }
    });
    let presentation: hydrus_core::tag_presentation::TagPresentation = model
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let tag_display_type = presentation.viewer_display_type;
    let viewing_stats = crate::viewing_tracking::CanvasTracker::new(
        model.store().clone(),
        hydrus_core::CanvasType::ArchiveDeleteFilter,
    );
    crate::gui_colours::bind(
        window.global::<crate::Theme<'_>>(),
        model.store(),
        viewing_stats.active_flag(),
    );
    let model = Rc::new(RefCell::new(model));
    let playback = playback::Playback::for_store(model.borrow().store().clone());
    let animator = animation::Animator::for_store(model.borrow().store().clone());
    let settings: hydrus_core::media_viewer::MediaViewerSettings = model
        .borrow()
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let zoomed = crate::zoom_window!(window, settings);
    let colour_watch = crate::image_colour_watch::Watch::new(model.borrow().store().clone());
    let close = {
        let viewing_stats = viewing_stats.clone();
        let colour_watch = colour_watch.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        let playback = playback.clone();
        let animator = animator.clone();
        let zoomed = zoomed.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            viewing_stats.close();
            colour_watch.close();
            playback.close();
            animator.stop();
            zoomed.close();
            let _ = window.hide();
            if !slot
                .borrow()
                .as_ref()
                .is_some_and(|current| std::ptr::eq(current.window(), window.window()))
            {
                return;
            }
            slot.borrow_mut().take();
        }
    };
    // show the file to decide on; with none left, ask
    let show = {
        let viewing_stats = viewing_stats.clone();
        let model = model.clone();
        let weak = window.as_weak();
        let playback = playback.clone();
        let animator = animator.clone();
        let zoomed = zoomed.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !viewing_stats.active() {
                return;
            }
            let model = model.borrow();
            viewing_stats.show(model.current());
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
                crate::viewer::animation_owned(store, file),
            );
            window.set_media(media.as_deref().map(crate::image).unwrap_or_default());
            let still = playable.is_none() && animation.is_none();
            zoomed.set_still(crate::viewer::still_of(media, shape, still));
            zoomed.show(shape);
            window.set_info_line(crate::viewer::shown(store, file).line.into());
            let tags: Vec<ListText> = crate::viewer::hover_tags(store, file, tag_display_type)
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
    colour_watch.start(
        Rc::new({
            let weak = window.as_weak();
            let slot = Rc::downgrade(slot);
            let guard = guard.clone();
            let viewing_stats = viewing_stats.clone();
            move || {
                guard()
                    && viewing_stats.active()
                    && weak.upgrade().is_some_and(|window| {
                        window.window().is_visible()
                            && slot.upgrade().is_some_and(|slot| {
                                slot.borrow().as_ref().is_some_and(|current| {
                                    std::ptr::eq(current.window(), window.window())
                                })
                            })
                    })
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            let model = model.clone();
            let zoomed = zoomed.clone();
            move || {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                let model = model.borrow();
                let Some(file) = model.current() else {
                    return;
                };
                let store = model.store();
                let shape = crate::viewer::shape(store, file);
                if crate::viewer::playable(store, file).is_some()
                    || shape.is_some_and(|shape| hydrus_media::animation::Frames::plays(shape.0))
                {
                    return;
                }
                let media = crate::viewer::still(store, file).map(std::sync::Arc::new);
                window.set_media(media.as_deref().map(crate::image).unwrap_or_default());
                zoomed.refresh_still(crate::viewer::still_of(media, shape, true));
            }
        }),
    );
    // after a decision: the next file, or, when done, ask (or, with
    // nothing to commit, close)
    let decided = {
        let guard = guard.clone();
        let viewing_stats = viewing_stats.clone();
        let model = model.clone();
        let show = show.clone();
        let close = close.clone();
        move |decide: fn(&mut ArchiveDeleteFilter)| {
            if !viewing_stats.active() || !guard() {
                return;
            }
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
        let guard = guard.clone();
        let viewing_stats = viewing_stats.clone();
        let model = model.clone();
        let show = show.clone();
        move || {
            if !viewing_stats.active() || !guard() {
                return;
            }
            model.borrow_mut().back();
            show();
        }
    });
    window.on_close_requested({
        let viewing_stats = viewing_stats.clone();
        let model = model.clone();
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            if !viewing_stats.active() {
                return;
            }
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
        let guard = guard.clone();
        let viewing_stats = viewing_stats.clone();
        let model = model.clone();
        let weak = window.as_weak();
        let show = show.clone();
        move || {
            if !viewing_stats.active() || !guard() {
                return;
            }
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
        let weak = window.as_weak();
        let viewing_stats = viewing_stats.clone();
        let model = model.clone();
        let close = close.clone();
        move || {
            if !viewing_stats.active()
                || !guard()
                || weak
                    .upgrade()
                    .is_none_or(|window| !window.window().is_visible())
            {
                return;
            }
            let model = model.borrow();
            let affected = match model.commit_changed() {
                Ok(affected) => affected,
                Err(e) => {
                    eprintln!("could not commit the archive/delete filter: {e}");
                    return;
                }
            };
            let mut gone = model.removed_from_view();
            gone.extend(hydrus_gui_model::file_view_removal::deleted(
                model.store(),
                &location,
                &affected,
                &crate::media_actions::Deletion::ToTrash,
            ));
            gone.sort();
            gone.dedup();
            let returned = model.return_file();
            drop(model);
            removed(&gone);
            if let Some(file) = returned {
                return_to(file);
            }
            close();
        }
    });
    window.on_toggle_pause(move || {
        playback.toggle_pause();
        animator.toggle_pause();
    });
    window.window().on_close_requested({
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                window.invoke_close_requested();
            }
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    crate::bind_zoom!(window, zoomed);
    show();
    window.show()?;
    Ok(window)
}
