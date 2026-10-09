//! The "review files to import" window, bound to its model ([`Review`]):
//! the paths given parsed a little at a time, as the reference parses them,
//! the list and progress shown as they go, and "import now" opening a local
//! import page with the good files (the reference's `new_hdd_import`).

use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::local_import::{Parse, Review};
use crate::{ImportRow, ReviewImportsWindow};

/// What "import now" hands on: the good files, each with its modified time
/// (seconds), tags for some, the sidecar routers to read each one's
/// metadata with, and whether to delete them once imported.
pub type ImportNow = Rc<
    dyn Fn(
        Vec<(String, Option<i64>)>,
        hydrus_store::queues::PathTags,
        Vec<hydrus_parse::sidecar::Router>,
        bool,
    ),
>;

/// A path as typed or pasted: trimmed, and without the quotes a file
/// manager's "copy as path" puts round it.
fn typed_path(text: &str) -> Option<String> {
    let text = text.trim();
    let text = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text);
    (!text.is_empty()).then(|| text.to_owned())
}

/// A file's modified time, in seconds (the reference's source time for a
/// local file).
fn modified(path: &str) -> Option<i64> {
    let time = std::fs::metadata(path).ok()?.modified().ok()?;
    let seconds = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(seconds).ok()
}

/// The window's state beside its model: which rows are selected, and the
/// row a shift-click selects from.
#[derive(Debug, Default)]
struct Selection {
    rows: BTreeSet<usize>,
    anchor: Option<usize>,
}

/// Open the window on `paths`; it forgets itself from `slot` when closed,
/// and hands its good files to `import_now`.
pub(crate) fn open(
    slot: &crate::ReviewSlot,
    paths: Vec<String>,
    tag_services: Vec<(String, String)>,
    tagging: &Rc<RefCell<Option<crate::FilenameTaggingWindow>>>,
    sidecars: &crate::filename_tagging_window::Sidecars,
    import_now: &ImportNow,
) -> Result<ReviewImportsWindow, String> {
    let window =
        crate::app_title::new::<crate::ReviewImportsWindow>().map_err(|e| e.to_string())?;
    let tools = hydrus_media::MediaTools::new()
        .with_ffmpeg_timeout_reader(hydrus_store::ffmpeg_policy::reader(&sidecars.store));
    let review = Rc::new(RefCell::new(Review::with_tools(tools)));
    review.borrow_mut().add_paths(paths);
    let selection = Rc::new(RefCell::new(Selection::default()));
    let show = {
        let store = sidecars.store.clone();
        let review = review.clone();
        let selection = selection.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let review = review.borrow();
            let selected = &selection.borrow().rows;
            let formatting = hydrus_gui_model::gui_format::preferences(&store);
            let rows: Vec<ImportRow> = review
                .parsed()
                .iter()
                .enumerate()
                .map(|(i, parsed)| {
                    let cells: Vec<SharedString> = parsed
                        .row_with_format(&formatting)
                        .iter()
                        .map(|c| c.as_str().into())
                        .collect();
                    ImportRow {
                        cells: ModelRc::new(VecModel::from(cells)),
                        selected: selected.contains(&i),
                        problem: !matches!(parsed.result, Parse::Good(_) | Parse::Sidecar),
                    }
                })
                .collect();
            window.set_rows(ModelRc::new(VecModel::from(rows)));
            let (text, done, total) = review.progress();
            window.set_progress(text.into());
            window.set_fraction(if total == 0 {
                0.0
            } else {
                done as f32 / total as f32
            });
            window.set_working(review.working());
            window.set_paused(review.paused());
            window.set_can_import(review.can_import());
            window.set_any_selected(!selected.is_empty());
        }
    };
    show();
    // (parsed a tenth of a second at a time, as the reference's worker
    // does, the list shown after each)
    let timer = Rc::new(slint::Timer::default());
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(30), {
        let review = review.clone();
        let show = show.clone();
        move || {
            if review.borrow_mut().work(Duration::from_millis(100)) {
                show();
            }
        }
    });
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let timer = timer.clone();
        move || {
            timer.stop();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    let at = |i: i32| usize::try_from(i).unwrap_or(usize::MAX);
    window.on_path_entered({
        let review = review.clone();
        let show = show.clone();
        move |text| {
            if let Some(path) = typed_path(&text) {
                review.borrow_mut().add_paths([path]);
                show();
            }
        }
    });
    let add_picked = {
        let review = review.clone();
        let show = show.clone();
        move |kind: crate::Pick, title: &str| {
            let paths = crate::pick(kind, title);
            if !paths.is_empty() {
                review
                    .borrow_mut()
                    .add_paths(paths.iter().map(|p| p.to_string_lossy().into_owned()));
                show();
            }
        }
    };
    window.on_add_files({
        let add_picked = add_picked.clone();
        move || add_picked(crate::Pick::Files, "Select the files to add.")
    });
    window.on_add_folder({
        let add_picked = add_picked.clone();
        move || add_picked(crate::Pick::Folder, "Select a folder to add.")
    });
    window.on_row_clicked({
        let selection = selection.clone();
        let show = show.clone();
        move |row, ctrl, shift| {
            let row = at(row);
            let mut selection = selection.borrow_mut();
            match (ctrl, shift, selection.anchor) {
                (_, true, Some(anchor)) => {
                    let (from, to) = (anchor.min(row), anchor.max(row));
                    if !ctrl {
                        selection.rows.clear();
                    }
                    selection.rows.extend(from..=to);
                }
                (true, _, _) => {
                    if !selection.rows.remove(&row) {
                        selection.rows.insert(row);
                    }
                    selection.anchor = Some(row);
                }
                _ => {
                    selection.rows = BTreeSet::from([row]);
                    selection.anchor = Some(row);
                }
            }
            drop(selection);
            show();
        }
    });
    window.on_remove_files({
        let review = review.clone();
        let selection = selection.clone();
        let show = show.clone();
        move || {
            let rows: HashSet<usize> = std::mem::take(&mut selection.borrow_mut().rows)
                .into_iter()
                .collect();
            selection.borrow_mut().anchor = None;
            review.borrow_mut().remove(&rows);
            show();
        }
    });
    window.on_pause_play({
        let review = review.clone();
        let show = show.clone();
        move || {
            review.borrow_mut().pause_play();
            show();
        }
    });
    window.on_stop({
        let review = review.clone();
        let show = show.clone();
        move || {
            review.borrow_mut().cancel();
            show();
        }
    });
    window.on_subdirectories_toggled({
        let review = review.clone();
        move |on| review.borrow_mut().search_subdirectories = on
    });
    window.on_delete_toggled({
        let review = review.clone();
        move |on| review.borrow_mut().delete_after_success = on
    });
    window.on_import_now({
        let review = review.clone();
        let close = close.clone();
        let import_now = import_now.clone();
        move || {
            let (paths, delete) = {
                let review = review.borrow();
                if !review.can_import() {
                    return;
                }
                let paths: Vec<(String, Option<i64>)> = review
                    .good_paths()
                    .into_iter()
                    .map(|p| {
                        let time = modified(&p);
                        (p, time)
                    })
                    .collect();
                (paths, review.delete_after_success)
            };
            close();
            import_now(
                paths,
                hydrus_store::queues::PathTags::new(),
                Vec::new(),
                delete,
            );
        }
    });
    // "add tags/urls with the import >>": the "filename tagging" dialog,
    // whose "apply" imports with its tags
    window.on_add_tags({
        let review = review.clone();
        let close = close.clone();
        let tagging = tagging.clone();
        let sidecars = sidecars.clone();
        let import_now = import_now.clone();
        move || {
            if tagging.borrow().is_some() {
                return;
            }
            let (paths, delete) = {
                let review = review.borrow();
                if !review.can_import() {
                    return;
                }
                let paths: Vec<(String, Option<i64>)> = review
                    .good_paths()
                    .into_iter()
                    .map(|p| {
                        let time = modified(&p);
                        (p, time)
                    })
                    .collect();
                (paths, review.delete_after_success)
            };
            let names: Vec<String> = paths.iter().map(|p| p.0.clone()).collect();
            let done: crate::filename_tagging_window::Done = {
                let close = close.clone();
                let import_now = import_now.clone();
                Rc::new(move |tags, routers| {
                    close();
                    import_now(paths.clone(), tags, routers, delete);
                })
            };
            match crate::filename_tagging_window::open(
                tag_services.clone(),
                names,
                &tagging,
                sidecars.clone(),
                done,
            ) {
                Ok(dialog) => *tagging.borrow_mut() = Some(dialog),
                Err(e) => eprintln!("could not open the filename tagging: {e}"),
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    // (files dropped on the window join its list, as the reference's do)
    crate::drops::on_files_dropped(window.window(), {
        let review = review.clone();
        let show = show.clone();
        move |paths| {
            review.borrow_mut().add_paths(paths);
            show();
        }
    });
    window.show().map_err(|e| e.to_string())?;
    *slot.borrow_mut() = Some((window.clone_strong(), review));
    Ok(window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_paths_lose_their_quotes_and_spaces() {
        assert_eq!(typed_path("  /a/b.png "), Some("/a/b.png".into()));
        assert_eq!(typed_path("\"/a/b c.png\""), Some("/a/b c.png".into()));
        assert_eq!(typed_path("   "), None);
    }
}
