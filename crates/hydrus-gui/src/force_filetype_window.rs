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

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use hydrus_core::{HashId, Mime};
use hydrus_store::Store;

use crate::ForceFiletypeWindow;
use crate::force_filetype::ForceFiletype;

/// Open the dialog on `files`; it forgets itself from `slot` when closed,
/// and calls `applied` once the files are forced.
pub(crate) fn open(
    store: &Arc<Store>,
    files: &[HashId],
    slot: &Rc<RefCell<Option<ForceFiletypeWindow>>>,
    jobs: &crate::metadata_file_jobs::Jobs,
    applied: Rc<dyn Fn()>,
) -> Result<ForceFiletypeWindow, String> {
    let previous = slot.borrow().as_ref().map(ComponentHandle::clone_strong);
    if let Some(previous) = previous {
        previous.invoke_cancel();
    }
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
    let window =
        crate::app_title::new::<crate::ForceFiletypeWindow>().map_err(|e| e.to_string())?;
    window.set_text(dialog.text.as_str().into());
    let labels: Vec<SharedString> = dialog.choices.iter().map(|(l, _)| l.into()).collect();
    window.set_choices(ModelRc::new(VecModel::from(labels)));
    window.set_chosen(0);
    let ended = crate::dialog::Dialog::new(&window);
    ended.on_closed({
        let slot = slot.clone();
        move || {
            slot.borrow_mut().take();
        }
    });
    window.on_apply({
        let store = store.clone();
        let jobs = jobs.clone();
        let ended = ended.clone();
        let weak = window.as_weak();
        move || {
            if !ended.is_open() {
                return;
            }
            let chosen = weak
                .upgrade()
                .and_then(|w| usize::try_from(w.get_chosen()).ok())
                .and_then(|i| dialog.choices.get(i))
                .map(|(_, mime)| *mime);
            let Some(mime) = chosen else {
                return;
            };
            let files = results
                .iter()
                .filter_map(|result| {
                    let info = result.info.as_ref()?;
                    Some(hydrus_store::metadata_jobs::File {
                        id: result.hash_id,
                        hash: result.hash,
                        mime: info.mime,
                        original_mime: info.original_mime.unwrap_or(info.mime),
                    })
                })
                .collect();
            let request = hydrus_store::metadata_jobs::Request::Force { files, mime };
            // (a job that cannot start leaves the dialog open, kept nothing)
            ended.apply(
                || match jobs.start(store.clone(), request, applied.clone()) {
                    Ok(()) => true,
                    Err(error) => {
                        eprintln!("{error}");
                        false
                    }
                },
                || {},
            );
        }
    });
    window.on_cancel(move || ended.cancel());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
