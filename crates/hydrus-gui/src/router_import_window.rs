//! Router-list import: compatible subsets and ordered PNG files stay in the
//! owning queue draft until its explicit Apply. No generic codec is relaxed.
use super::{Apply, Preview, Slots, read_bytes};
use crate::DownloaderExchangeWindow;
use hydrus_downloader_exchange::routers::{self, ImportReport};
use hydrus_gui_model::sidecar_editors::{Context, validate_router_import};
use hydrus_parse::sidecar::Router;
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    path::Path,
    rc::Rc,
};

/// Open an importer whose draft belongs to this captured live queue owner.
pub fn open(
    slots: &Slots,
    context: Context,
    preview: Preview<Router>,
    applied: Apply<Router>,
    owner: Rc<dyn Fn() -> bool>,
) -> Result<DownloaderExchangeWindow, String> {
    if let Some(window) = slots.0.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = crate::app_title::new::<crate::DownloaderExchangeWindow>().map_err(|e| e.to_string())?;
    window.set_importing(true);
    window.set_active(true);
    window.set_router_import(true);
    window.set_window_title("import metadata routers".into());
    window.set_instructions("Paste router definitions or choose PNGs. Compatible routers are added in order when you accept; changes are saved when you apply the owning editor.".into());
    let active = Rc::new(Cell::new(true));
    let pending = Rc::new(RefCell::new(Vec::<Router>::new()));
    let close: Rc<dyn Fn()> = Rc::new({
        let active = active.clone();
        let weak = window.as_weak();
        let slot = Rc::downgrade(&slots.0);
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                window.set_active(false);
                let _ = window.hide();
                if let Some(slot) = slot.upgrade() {
                    let owns = slot
                        .borrow()
                        .as_ref()
                        .is_some_and(|current| std::ptr::eq(current.window(), window.window()));
                    if owns {
                        slot.borrow_mut().take();
                    }
                }
                window.invoke_closed();
            }
        }
    });
    window.on_action({
        let weak = window.as_weak();
        let close = close.clone();
        move |action| {
            if !active.get() {
                return;
            }
            if action == "cancel" {
                close();
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !owner() || !window.window().is_visible() {
                return;
            }
            let result = (|| -> Result<(), String> {
                match action.as_str() {
                    "back" => {
                        pending.borrow_mut().clear();
                        window.set_ready(false);
                        window.set_review("".into());
                        window.set_error("".into());
                    }
                    "paste" if !window.get_ready() => {
                        window.set_text(crate::from_clipboard()?.into());
                    }
                    "accept" if window.get_ready() => {
                        applied(pending.borrow().clone())?;
                        close();
                    }
                    "review" | "open" | "import-pngs" if !window.get_ready() => {
                        let mut reports = Vec::new();
                        let mut critical = None;
                        if action == "import-pngs" {
                            let paths = crate::pick_exchange_files(
                                "select the png or pngs with the encoded data",
                                "png",
                            );
                            if !active.get() || !owner() || !window.window().is_visible() {
                                return Ok(());
                            }
                            if paths.is_empty() {
                                return Ok(());
                            }
                            let mut bytes_total = 0usize;
                            for path in paths {
                                let incoming = read_bytes(&path).and_then(|bytes| {
                                    bytes_total = bytes_total.saturating_add(bytes.len());
                                    if bytes_total > hydrus_downloader_exchange::MAX_BYTES {
                                        return Err(
                                            hydrus_downloader_exchange::Error::Limit.to_string()
                                        );
                                    }
                                    routers::inspect_png(&bytes).map_err(|_| {
                                        "I could not understand what was encoded in the png!".into()
                                    })
                                });
                                match incoming {
                                    Ok(report) => reports.push(report),
                                    Err(error) => {
                                        critical = Some(format!("Problem importing!\n\n{error}"));
                                        break;
                                    }
                                }
                            }
                        } else {
                            let report = if action == "open" {
                                read_bytes(Path::new(window.get_path().as_str())).and_then(
                                    |bytes| {
                                        if bytes.starts_with(b"\x89PNG") {
                                            routers::inspect_png(&bytes).map_err(|e| e.to_string())
                                        } else {
                                            routers::inspect_text(
                                                std::str::from_utf8(&bytes)
                                                    .map_err(|e| e.to_string())?,
                                            )
                                            .map_err(|e| e.to_string())
                                        }
                                    },
                                )?
                            } else {
                                routers::inspect_text(window.get_text().as_str())
                                    .map_err(|e| e.to_string())?
                            };
                            reports.push(report);
                        }
                        if !active.get() || !owner() || !window.window().is_visible() {
                            return Ok(());
                        }
                        let (incoming, mut warnings) = permitted(context, reports);
                        if let Some(critical) = critical {
                            warnings.push(critical);
                        }
                        if incoming.len() > hydrus_downloader_exchange::MAX_OBJECTS {
                            return Err(hydrus_downloader_exchange::Error::Limit.to_string());
                        }
                        let description = if incoming.is_empty() {
                            String::new()
                        } else {
                            preview(incoming.clone())?
                        };
                        window.set_review(description.into());
                        window.set_ready(!incoming.is_empty());
                        *pending.borrow_mut() = incoming;
                        window.set_error(warnings.join("\n\n").into());
                    }
                    _ => (),
                }
                Ok(())
            })();
            if let Err(error) = result {
                window.set_error(error.into());
            }
        }
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    *slots.0.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}

fn permitted(context: Context, reports: Vec<ImportReport>) -> (Vec<Router>, Vec<String>) {
    let mut incoming = Vec::new();
    let mut warnings = Vec::new();
    for report in reports {
        let mut other_errors = BTreeSet::new();
        for router in report.routers {
            match validate_router_import(context, std::slice::from_ref(&router)) {
                Ok(()) => incoming.push(router),
                Err(error) => {
                    other_errors.insert(error);
                }
            }
        }
        if !report.other_types.is_empty() {
            warnings.push(format!("The imported objects included these types:\n\n{}\n\nWhereas this control only allows:\n\nSingleFileMetadataRouter", report.other_types.into_iter().collect::<Vec<_>>().join("\n")));
        }
        if !other_errors.is_empty() {
            warnings.push(format!(
                "The imported objects were wrong for this control:\n\n{}",
                other_errors.into_iter().collect::<Vec<_>>().join("\n")
            ));
        }
    }
    (incoming, warnings)
}
