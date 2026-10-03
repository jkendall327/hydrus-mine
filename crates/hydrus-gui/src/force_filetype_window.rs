//! The "force filetypes" dialog, bound (`ui/force_filetype.slint`): the
//! files' original and forced filetypes counted into hydrus-gui-model's
//! [`ForceFiletype`](crate::force_filetype::ForceFiletype), and "apply"
//! forcing them all to the type chosen (or no longer), each file renamed
//! to its new extension, as the reference's `SetFilesForcedFiletypes`
//! does.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::{HashId, Mime};
use hydrus_store::Store;

use crate::ForceFiletypeWindow;
use crate::force_filetype::ForceFiletype;

/// Move a file to its new extension's path (copied if it can't move).
fn rename(from: &std::path::Path, to: &std::path::Path) {
    if from == to || !from.is_file() {
        return;
    }
    if std::fs::rename(from, to).is_err() {
        let _ = std::fs::copy(from, to);
    }
}

/// Open the dialog on `files`; it forgets itself from `slot` when closed,
/// and calls `applied` once the files are forced.
pub(crate) fn open(
    store: &Arc<Store>,
    files: &[HashId],
    slot: &Rc<RefCell<Option<ForceFiletypeWindow>>>,
    applied: Rc<dyn Fn()>,
) -> Result<ForceFiletypeWindow, String> {
    let services = store.snapshot().services.clone();
    let results = store
        .read(|c| hydrus_store::media::load(c, &services, None, files))
        .map_err(|e| e.to_string())?
        .results;
    let mut original: BTreeMap<Mime, usize> = BTreeMap::new();
    let mut forced: BTreeMap<Mime, usize> = BTreeMap::new();
    for info in results.iter().filter_map(|r| r.info.as_ref()) {
        *original
            .entry(info.original_mime.unwrap_or(info.mime))
            .or_default() += 1;
        if info.original_mime.is_some() {
            *forced.entry(info.mime).or_default() += 1;
        }
    }
    let dialog = ForceFiletype::new(&original, &forced);
    let window = ForceFiletypeWindow::new().map_err(|e| e.to_string())?;
    window.set_text(dialog.text.as_str().into());
    let labels: Vec<SharedString> = dialog.choices.iter().map(|(l, _)| l.into()).collect();
    window.set_choices(ModelRc::new(VecModel::from(labels)));
    window.set_chosen(0);
    let close = {
        let slot = slot.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        })
    };
    window.on_apply({
        let store = store.clone();
        let close = close.clone();
        let weak = window.as_weak();
        move || {
            let chosen = weak
                .upgrade()
                .and_then(|w| usize::try_from(w.get_chosen()).ok())
                .and_then(|i| dialog.choices.get(i))
                .map(|(_, mime)| *mime);
            let Some(mime) = chosen else {
                return;
            };
            let ids: Vec<HashId> = results.iter().map(|r| r.hash_id).collect();
            if let Err(e) = store.write_content(move |w| w.force_filetype(&ids, mime)) {
                eprintln!("could not force the filetypes: {e}");
                return;
            }
            // (each file renamed to its new extension)
            let snapshot = store.snapshot();
            for result in &results {
                let Some(info) = &result.info else {
                    continue;
                };
                let original = info.original_mime.unwrap_or(info.mime);
                let to = mime.unwrap_or(original);
                let (Some(from), Some(to)) = (
                    snapshot.storage.file_path(&result.hash, info.mime),
                    snapshot.storage.file_path(&result.hash, to),
                ) else {
                    continue;
                };
                rename(&from, &to);
            }
            applied();
            close();
        }
    });
    window.on_cancel(move || close());
    window.window().on_close_requested({
        let slot = slot.clone();
        move || {
            slot.borrow_mut().take();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
