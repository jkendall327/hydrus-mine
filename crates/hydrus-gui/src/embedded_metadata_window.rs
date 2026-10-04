//! The Detailed File Metadata window, with ordinary EXIF selection and raw
//! clipboard copy. The store supplies basics and local file locations; the
//! existing media decoder supplies the conditional metadata sections.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use hydrus_core::{HashId, Mime};
use hydrus_store::Store;
use hydrus_store::media::FileFlags;
use hydrus_store::services::ServiceKind;
use slint::{ComponentHandle as _, ModelRc, VecModel};

use crate::embedded_metadata::{ExifList, basics, exif_instruction};
use crate::{EmbeddedMetadataWindow, TableRow};

fn refresh(window: &EmbeddedMetadataWindow, list: &ExifList) {
    window.set_rows(ModelRc::new(VecModel::from(
        list.order
            .iter()
            .map(|&index| {
                let row = &list.rows[index];
                TableRow {
                    cells: ModelRc::new(VecModel::from(vec![
                        row.id.to_string().into(),
                        row.label.as_str().into(),
                        row.value.as_str().into(),
                    ])),
                    selected: list.selection.is_selected(index),
                }
            })
            .collect::<Vec<_>>(),
    )));
}

fn publish(
    window: &EmbeddedMetadataWindow,
    list: &mut ExifList,
    metadata: hydrus_media::EmbeddedMetadata,
) {
    window.set_has_exif(metadata.exif.is_some());
    window.set_has_xmp(metadata.xmp.is_some());
    window.set_xmp(metadata.xmp.unwrap_or_default().into());
    window.set_has_iptc(metadata.iptc.is_some());
    window.set_iptc(metadata.iptc.unwrap_or_default().into());
    window.set_has_text(metadata.text.is_some());
    window.set_text(metadata.text.unwrap_or_default().into());
    window.set_extra(ModelRc::new(VecModel::from(
        metadata
            .extra
            .into_iter()
            .map(|(k, v)| format!("{k}: {v}").into())
            .collect::<Vec<_>>(),
    )));
    *list = ExifList::new(metadata.exif.unwrap_or_default());
    list.sort(
        usize::try_from(window.get_sort_column()).unwrap_or(0),
        window.get_ascending(),
    );
    refresh(window, list);
}

struct MetadataRequest {
    path: Option<std::path::PathBuf>,
    mime: Mime,
    has_icc: bool,
    local: bool,
}

fn load(
    window: &EmbeddedMetadataWindow,
    list: &Rc<RefCell<ExifList>>,
    request: MetadataRequest,
) -> Result<Rc<slint::Timer>, String> {
    let (sender, receiver) = mpsc::channel();
    let MetadataRequest {
        path,
        mime,
        has_icc,
        local,
    } = request;
    std::thread::Builder::new()
        .name("embedded metadata".into())
        .spawn(move || {
            let result = if local && hydrus_media::embedded_metadata_looks_at(mime) {
                path.ok_or_else(|| "File location not found".to_owned())
                    .and_then(|path| {
                        std::fs::read(path)
                            .map_err(|e| e.to_string())
                            .map(|bytes| hydrus_media::embedded_metadata(&bytes, mime, has_icc))
                    })
            } else {
                Ok(hydrus_media::EmbeddedMetadata {
                    text: (!local).then(|| "This file is not local to this computer!".to_owned()),
                    ..Default::default()
                })
            };
            let _ = sender.send(result);
        })
        .map_err(|e| e.to_string())?;
    let timer = Rc::new(slint::Timer::default());
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(20), {
        let weak = window.as_weak();
        let list = list.clone();
        let timer = Rc::downgrade(&timer);
        move || {
            let result = match receiver.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err("Metadata worker stopped unexpectedly".to_owned())
                }
            };
            if let Some(timer) = timer.upgrade() {
                timer.stop();
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.set_loading(false);
            match result {
                Ok(metadata) => publish(&window, &mut list.borrow_mut(), metadata),
                Err(error) => {
                    window.set_error(format!("Could not read embedded metadata: {error}").into());
                }
            }
        }
    });
    Ok(timer)
}

fn bind_list(window: &EmbeddedMetadataWindow, list: &Rc<RefCell<ExifList>>) {
    window.on_row_clicked({
        let list = list.clone();
        let weak = window.as_weak();
        move |row, ctrl, shift| {
            if let (Some(window), Ok(row)) = (weak.upgrade(), usize::try_from(row)) {
                let mut list = list.borrow_mut();
                let order = list.order.clone();
                list.selection.click(&order, row, ctrl, shift);
                refresh(&window, &list);
            }
        }
    });
    window.on_row_activated({
        let list = list.clone();
        move |_| {
            if let Some(text) = list.borrow().copy() {
                crate::copy_to_clipboard(text);
            }
        }
    });
    window.on_sort({
        let list = list.clone();
        let weak = window.as_weak();
        move |column, ascending| {
            if let (Some(window), Ok(column)) = (weak.upgrade(), usize::try_from(column)) {
                list.borrow_mut().sort(column, ascending);
                window.set_sort_column(i32::try_from(column).unwrap_or(0));
                window.set_ascending(ascending);
                refresh(&window, &list.borrow());
            }
        }
    });
}

/// Open one file's metadata and clear its owning slot when the window closes.
pub(crate) fn open(
    store: &Arc<Store>,
    file: HashId,
    slot: &Rc<RefCell<Option<EmbeddedMetadataWindow>>>,
) -> Result<EmbeddedMetadataWindow, String> {
    let snapshot = store.snapshot();
    let media = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
        .map_err(|e| e.to_string())?
        .results
        .into_iter()
        .next()
        .ok_or("File not found")?;
    let info = media.info.as_ref().ok_or("File information not found")?;
    let settings = store.read(hydrus_store::settings::get).unwrap_or_default();
    let lines = crate::info_lines::info_lines(
        &media,
        &snapshot.services,
        &settings,
        hydrus_core::TimestampMs::now().0,
        false,
    );
    let local = snapshot
        .services
        .all()
        .any(|s| matches!(s.kind, ServiceKind::LocalFileStorage) && media.is_current_in(s.id));
    // Hide any previous window. Its worker only owns its own result channel,
    // and the old window's close callback cannot remove a newer slot.
    if let Some(previous) = slot.borrow_mut().take() {
        let _ = previous.hide();
    }
    let window = EmbeddedMetadataWindow::new().map_err(|e| e.to_string())?;
    window.set_basics(basics(&lines).into());
    window.set_instruction(exif_instruction(info.mime).into());
    window.set_loading(true);
    let list = Rc::new(RefCell::new(ExifList::new(Vec::new())));
    let timer = load(
        &window,
        &list,
        MetadataRequest {
            path: snapshot.storage.file_path(&media.hash, info.mime),
            mime: info.mime,
            has_icc: info.flags.has(FileFlags::ICC_PROFILE),
            local,
        },
    )?;
    bind_list(&window, &list);
    let close = {
        let slot = slot.clone();
        let weak = window.as_weak();
        move || {
            timer.stop();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
                let is_current = slot
                    .borrow()
                    .as_ref()
                    .is_some_and(|current| std::ptr::eq(current.window(), window.window()));
                if is_current {
                    slot.borrow_mut().take();
                }
            }
        }
    };
    window.on_dismissed(close.clone());
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
