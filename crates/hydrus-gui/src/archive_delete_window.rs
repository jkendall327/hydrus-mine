//! The archive/delete filter's window, bound to its model
//! ([`ArchiveDeleteFilter`]): files play as in the media viewer, and
//! finishing (or stopping, with anything decided) asks to commit.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::archive_delete::ArchiveDeleteFilter;
use crate::{ArchiveDeleteWindow, ListText, Removed, animation, list_text, playback};

// Slint timer instants have millisecond precision; admission keeps the full delay.
const COMMIT_DELAY: Duration = Duration::from_millis(1200);

fn commit_delay_remaining(deadline: Option<Instant>, now: Instant) -> Duration {
    deadline.map_or(Duration::ZERO, |deadline| {
        deadline.saturating_duration_since(now)
    })
}

fn timer_interval(remaining: Duration) -> Duration {
    Duration::from_millis(
        u64::try_from(remaining.as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX),
    )
}

/// Open the filter's window; it forgets itself from `slot` when closed,
/// and tells `removed` of files committing deleted out of the page.
pub(crate) fn open(
    model: ArchiveDeleteFilter,
    location: hydrus_search::LocationContext,
    slot: &Rc<RefCell<Option<ArchiveDeleteWindow>>>,
    removed: Removed,
    guard: Rc<dyn Fn() -> bool>,
    return_to: Rc<dyn Fn(hydrus_core::HashId)>,
    image_cache: crate::image_cache::Handle,
) -> Result<ArchiveDeleteWindow, slint::PlatformError> {
    let window = ArchiveDeleteWindow::new()?;
    let parent_guard = guard.clone();
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
    let warm_valid: Rc<dyn Fn() -> bool> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        let active = viewing_stats.active_flag();
        let parent = parent_guard.clone();
        move || {
            active.get()
                && parent()
                && weak.upgrade().is_some_and(|window| {
                    slot.upgrade().is_some_and(|slot| {
                        slot.borrow()
                            .as_ref()
                            .is_some_and(|current| std::ptr::eq(current.window(), window.window()))
                    })
                })
        }
    });
    let warm = crate::viewer_prefetch::Control::new(
        model.borrow().store(),
        image_cache.clone(),
        warm_valid.clone(),
        Rc::new({
            let model = Rc::downgrade(&model);
            move |preferences| {
                model
                    .upgrade()
                    .map_or_else(Vec::new, |model| model.borrow().prefetch_files(preferences))
            }
        }),
    );
    let playback = playback::Playback::for_store(model.borrow().store().clone());
    let animator = animation::Animator::for_store(model.borrow().store().clone());
    let settings: hydrus_core::media_viewer::MediaViewerSettings = model
        .borrow()
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let zoomed = crate::zoom_window!(window, settings);
    crate::archive_delete_playback::seek_options(&window, model.borrow().store());
    let controls = crate::archive_delete_playback::Controls::new(
        &window,
        model.borrow().store().clone(),
        playback.clone(),
        animator.clone(),
    );
    let colour_watch = crate::image_colour_watch::Watch::new(model.borrow().store().clone());
    let finish_choices = Rc::new(RefCell::new(
        None::<(Vec<crate::archive_delete::DeletionChoice>, Option<Instant>)>,
    ));
    let finish_timer = Rc::new(slint::Timer::default());
    let ask_finish: Rc<dyn Fn()> = Rc::new({
        let model = model.clone();
        let weak = window.as_weak();
        let choices = finish_choices.clone();
        let timer = finish_timer.clone();
        let slot = Rc::downgrade(slot);
        let parent_guard = parent_guard.clone();
        let viewing_stats = viewing_stats.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !viewing_stats.active() {
                return;
            }
            timer.stop();
            let model = model.borrow();
            let options = model.deletion_choices();
            let delayed = model.delay_multiple_choices();
            let labels = if options.len() > 1 {
                options
                    .iter()
                    .map(|choice| format!("{}?", choice.label).into())
                    .collect()
            } else {
                vec![model.question().into()]
            };
            window.set_commit_labels(ModelRc::new(VecModel::from(labels)));
            window.set_finish_header(if options.len() > 1 {
                model
                    .kept_label()
                    .map_or_else(SharedString::new, |label| format!("{label}\n-and-").into())
            } else {
                SharedString::new()
            });
            window.set_question(model.question().into());
            window.set_forget_question(false);
            window.set_commit_ready(!delayed);
            let deadline = delayed.then(|| Instant::now() + COMMIT_DELAY);
            *choices.borrow_mut() = Some((options, deadline));
            if delayed {
                let weak = weak.clone();
                let slot = slot.clone();
                let parent_guard = parent_guard.clone();
                let viewing_stats = viewing_stats.clone();
                let choices = choices.clone();
                let weak_timer = Rc::downgrade(&timer);
                timer.start(slint::TimerMode::Repeated, COMMIT_DELAY, move || {
                    let Some(timer) = weak_timer.upgrade() else {
                        return;
                    };
                    let Some(window) = weak.upgrade() else {
                        timer.stop();
                        return;
                    };
                    if !viewing_stats.active()
                        || !parent_guard()
                        || !slot.upgrade().is_some_and(|slot| {
                            slot.borrow().as_ref().is_some_and(|current| {
                                std::ptr::eq(current.window(), window.window())
                            })
                        })
                    {
                        timer.stop();
                        return;
                    }
                    let remaining = choices
                        .borrow()
                        .as_ref()
                        .map(|(_, deadline)| commit_delay_remaining(*deadline, Instant::now()));
                    let Some(remaining) = remaining else {
                        timer.stop();
                        return;
                    };
                    if remaining.is_zero() {
                        timer.stop();
                        window.set_commit_ready(true);
                    } else {
                        // An early rounded Slint tick cannot enable a refused button.
                        timer.set_interval(timer_interval(remaining));
                    }
                });
            }
        }
    });
    let close = {
        let warm = warm.clone();
        let viewing_stats = viewing_stats.clone();
        let colour_watch = colour_watch.clone();
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        let finish_choices = finish_choices.clone();
        let finish_timer = finish_timer.clone();
        let playback = playback.clone();
        let animator = animator.clone();
        let zoomed = zoomed.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            finish_timer.stop();
            finish_choices.borrow_mut().take();
            warm.retire();
            viewing_stats.close();
            colour_watch.close();
            playback.close();
            animator.stop();
            zoomed.close();
            let _ = window.hide();
            let Some(slot) = slot.upgrade() else {
                return;
            };
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
    window.on_owner_valid({
        let parent_guard = parent_guard.clone();
        let viewing_stats = viewing_stats.clone();
        move || viewing_stats.active() && parent_guard()
    });
    window.on_retire({
        let close = close.clone();
        move || close()
    });
    // show the file to decide on; with none left, ask
    let show = {
        let warm = warm.clone();
        let warm_valid = warm_valid.clone();
        let viewing_stats = viewing_stats.clone();
        let model = model.clone();
        let weak = window.as_weak();
        let ask_finish = ask_finish.clone();
        let playback = playback.clone();
        let animator = animator.clone();
        let zoomed = zoomed.clone();
        let image_cache = image_cache.clone();
        let controls = controls.clone();
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
                controls.file_shown(false, None, None, None, false);
                drop(model);
                ask_finish();
                return;
            };
            let store = model.store();
            let (shape, media) = (
                crate::viewer::shape(store, file),
                image_cache.load_current_saved(store, file),
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
            let (duration_ms, num_frames) = crate::viewer::timing(store, file);
            controls.file_shown(
                playable.is_some(),
                animation.as_ref().map(|f| (f.len(), f.total_ms())),
                duration_ms,
                num_frames,
                crate::viewer::has_audio_of(store, file),
            );
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
            animator.play_with_metadata(animation, num_frames, false, move |image| {
                if let Some(window) = frame.upgrade() {
                    window.set_media(image);
                }
            });
            if warm_valid() {
                warm.refresh();
            }
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
                let media = image_cache.load_current_saved(store, file);
                window.set_media(media.as_deref().map(crate::image).unwrap_or_default());
                zoomed.refresh_still(crate::viewer::still_of(media, shape, true));
            }
        }),
    );
    // after a decision: the next file, or, when done, ask (or, with
    // nothing to commit, close)
    let decided = {
        let weak = window.as_weak();
        let guard = guard.clone();
        let viewing_stats = viewing_stats.clone();
        let model = model.clone();
        let show = show.clone();
        let close = close.clone();
        move |decide: fn(&mut ArchiveDeleteFilter)| {
            if !viewing_stats.active()
                || !guard()
                || weak
                    .upgrade()
                    .is_none_or(|window| !window.get_question().is_empty())
            {
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
        let weak = window.as_weak();
        let guard = guard.clone();
        let viewing_stats = viewing_stats.clone();
        let model = model.clone();
        let show = show.clone();
        move || {
            if !viewing_stats.active() || !guard() {
                return;
            }
            if weak
                .upgrade()
                .is_none_or(|window| !window.get_question().is_empty())
            {
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
        let ask_finish = ask_finish.clone();
        let guard = guard.clone();
        move || {
            if !viewing_stats.active() {
                return;
            }
            if !guard() {
                close();
                return;
            }
            let model = model.borrow();
            match weak.upgrade() {
                Some(window) if model.has_decisions() => {
                    if window.get_question().is_empty() {
                        drop(model);
                        ask_finish();
                    }
                }
                _ => close(),
            }
        }
    });
    window.on_resume({
        let finish_choices = finish_choices.clone();
        let finish_timer = finish_timer.clone();
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
                if window.get_forget_question() {
                    return;
                }
                window.set_question(SharedString::new());
                window.set_forget_question(false);
                finish_timer.stop();
                finish_choices.borrow_mut().take();
            }
            // (finished, back to the last file)
            if model.borrow().is_done() {
                model.borrow_mut().back();
            }
            show();
        }
    });
    window.on_forget({
        let weak = window.as_weak();
        let guard = guard.clone();
        let viewing_stats = viewing_stats.clone();
        move || {
            if viewing_stats.active()
                && guard()
                && let Some(window) = weak.upgrade()
                && !window.get_question().is_empty()
            {
                window.set_forget_question(true);
            }
        }
    });
    window.on_forget_answered({
        let close = close.clone();
        let guard = guard.clone();
        let viewing_stats = viewing_stats.clone();
        let weak = window.as_weak();
        move |yes| {
            if viewing_stats.active()
                && guard()
                && let Some(window) = weak.upgrade()
                && window.get_forget_question()
            {
                window.set_forget_question(false);
                if yes {
                    close();
                }
            }
        }
    });
    let commit: Rc<dyn Fn(i32)> = Rc::new({
        let weak = window.as_weak();
        let viewing_stats = viewing_stats.clone();
        let model = model.clone();
        let close = close.clone();
        let finish_choices = finish_choices.clone();
        move |index| {
            if !viewing_stats.active()
                || !guard()
                || weak
                    .upgrade()
                    .is_none_or(|window| !window.window().is_visible())
            {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_question().is_empty() || window.get_forget_question() {
                return;
            }
            let choices = finish_choices.borrow();
            let Some((options, deadline)) = choices.as_ref() else {
                return;
            };
            if !commit_delay_remaining(*deadline, Instant::now()).is_zero() {
                return;
            }
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            if index >= options.len().max(1) {
                return;
            }
            let model = model.borrow();
            let affected = match model.commit_choice_changed(options.get(index)) {
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
            drop(choices);
            removed(&gone);
            if let Some(file) = returned {
                return_to(file);
            }
            close();
        }
    });
    window.on_commit({
        let commit = commit.clone();
        move || commit(0)
    });
    window.on_commit_choice(move |index| commit(index));
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
