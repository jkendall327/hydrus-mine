//! Owned global scan/repair confirmations and cancellable background database work.
use crate::ArchiveRepairWindow;
use hydrus_gui_model::archive_repair as model;
use hydrus_store::{
    Store,
    archive_repair::{self, Plan, Repaired},
    content::DomainRoles,
};
use slint::{ComponentHandle as _, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
pub type Slot = Rc<RefCell<Option<ArchiveRepairWindow>>>;
enum Completed {
    Scanned(Result<Plan, String>),
    Repaired(Result<Repaired, String>),
}

pub fn open(
    store: &Arc<Store>,
    slot: &Slot,
    changed: Rc<dyn Fn()>,
    valid: Rc<dyn Fn() -> bool>,
) -> Result<ArchiveRepairWindow, String> {
    let predecessor = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(predecessor) = predecessor {
        predecessor.invoke_close_clicked();
    }
    let window = ArchiveRepairWindow::new().map_err(|e| e.to_string())?;
    window.set_question(model::SCAN_QUESTION.into());
    let active = Rc::new(Cell::new(true));
    let cancel = Arc::new(AtomicBool::new(false));
    let receiver = Rc::new(RefCell::new(None::<mpsc::Receiver<Completed>>));
    let plan = Rc::new(RefCell::new(None::<Plan>));
    let timer = Rc::new(Timer::default());
    let close = Rc::new({
        let active = active.clone();
        let cancel = cancel.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        let timer = timer.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            cancel.store(true, Ordering::Release);
            timer.stop();
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slot.borrow_mut().take();
        }
    });
    window.on_scan_answered({
        let active = active.clone();
        let valid = valid.clone();
        let close = close.clone();
        let weak = window.as_weak();
        let store = store.clone();
        let receiver = receiver.clone();
        let cancel = cancel.clone();
        move |yes| {
            if !active.get() {
                return;
            }
            if !valid() {
                close();
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.window().is_visible() || w.get_phase() != 0 {
                return;
            }
            if !yes {
                close();
                return;
            }
            w.set_phase(1);
            w.set_question(SharedString::new());
            w.set_status("scanning for missing archive timestamps".into());
            let (send, recv) = mpsc::channel();
            *receiver.borrow_mut() = Some(recv);
            let store = store.clone();
            let cancel = cancel.clone();
            std::thread::spawn(move || {
                let result = store
                    .read(|conn| {
                        archive_repair::scan(
                            conn,
                            &DomainRoles::new(&store.snapshot().services)?,
                            &cancel,
                        )
                    })
                    .map_err(|e| e.to_string());
                let _ = send.send(Completed::Scanned(result));
            });
        }
    });
    window.on_chosen({
        let close = close.clone();
        let active = active.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        let store = store.clone();
        let receiver = receiver.clone();
        let cancel = cancel.clone();
        let plan = plan.clone();
        move |index| {
            if !active.get() {
                return;
            }
            if !valid() {
                close();
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.window().is_visible() || w.get_phase() != 2 {
                return;
            }
            let Some(plan) = plan.borrow().clone() else {
                return;
            };
            let Some(choice) = usize::try_from(index)
                .ok()
                .and_then(|i| model::choices(&plan).get(i).cloned())
            else {
                return;
            };
            w.set_phase(1);
            w.set_question(SharedString::new());
            w.set_status("filling in missing archive timestamps".into());
            let (send, recv) = mpsc::channel();
            *receiver.borrow_mut() = Some(recv);
            let store = store.clone();
            let cancel = cancel.clone();
            std::thread::spawn(move || {
                let result = store
                    .write_content(move |writer| {
                        archive_repair::apply(writer, &plan, &choice.populations, &cancel)
                    })
                    .map_err(|e| e.to_string());
                let _ = send.send(Completed::Repaired(result));
            });
        }
    });
    window.on_cancel_work({
        let close = close.clone();
        let active = active.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        let cancel = cancel.clone();
        let receiver = receiver.clone();
        move || {
            if !active.get() {
                return;
            }
            if !valid() {
                close();
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.window().is_visible() || w.get_phase() != 1 {
                return;
            }
            cancel.store(true, Ordering::Release);
            receiver.borrow_mut().take();
            w.set_phase(3);
            w.set_status("Cancelled!".into());
        }
    });
    timer.start(TimerMode::Repeated, Duration::from_millis(30), {
        let weak = window.as_weak();
        let receiver = receiver.clone();
        let close = close.clone();
        let active = active.clone();
        move || {
            if !active.get() { return; }
            if !valid() { close(); return; }
            let Some(w) = weak.upgrade() else { close(); return; };
            if !w.window().is_visible() { close(); return; }
            let result = receiver.borrow().as_ref().and_then(|r| r.try_recv().ok());
            let Some(result) = result else { return; };
            receiver.borrow_mut().take();
            match result {
                Completed::Scanned(Ok(found)) => {
                    let choices = model::choices(&found);
                    if choices.is_empty() {
                        w.set_phase(3);
                        w.set_status("No missing archive times found!".into());
                    } else {
                        w.set_phase(2);
                        w.set_status(SharedString::new());
                        w.set_question(model::question(&found).into());
                        let labels = choices.into_iter().map(|c| SharedString::from(c.label)).collect::<Vec<_>>();
                        w.set_choices(ModelRc::new(VecModel::from(labels)));
                        *plan.borrow_mut() = Some(found);
                    }
                }
                Completed::Repaired(Ok(done)) => {
                    w.set_phase(3);
                    w.set_status(format!("Done!\n{} missing legacy archive times fixed!\n{} missing import archive times fixed!",
                        hydrus_core::numbers::human_int(done.legacy as u64),
                        hydrus_core::numbers::human_int(done.import as u64)).into());
                    changed();
                }
                Completed::Scanned(Err(error)) | Completed::Repaired(Err(error)) => {
                    w.set_phase(3);
                    w.set_status(error.into());
                }
            }
        }
    });
    window.on_close_clicked({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
