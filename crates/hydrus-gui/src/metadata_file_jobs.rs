//! Main-GUI-owned finite metadata workers. Closing an accepted editor leaves its
//! work running; dropping the GUI cancels at the next file/block boundary.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

use hydrus_store::{
    Store,
    metadata_jobs::{self, Local, Request},
};

struct Pending {
    timer: Rc<slint::Timer>,
    shutdown: Arc<AtomicBool>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.timer.stop();
    }
}

/// Accepted jobs owned by one bound GUI, independent of its current dialogs.
#[derive(Clone, Default)]
pub struct Jobs(Rc<RefCell<Vec<Rc<Pending>>>>);
impl Jobs {
    /// Number of finite jobs whose completion is still being collected.
    pub fn running(&self) -> usize {
        self.0.borrow().len()
    }

    /// Start a captured operation without blocking a Slint event callback.
    pub(crate) fn start(
        &self,
        store: Arc<Store>,
        request: Request,
        applied: Rc<dyn Fn()>,
    ) -> Result<(), String> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let cancel = shutdown.clone();
        std::thread::Builder::new()
            .name("metadata files".into())
            .spawn(move || {
                let result = metadata_jobs::run(&store, &request, &cancel, &Local);
                let _ = sender.send(result);
            })
            .map_err(|e| format!("Could not start metadata file work: {e}"))?;
        let pending = Rc::new(Pending {
            timer: Rc::new(slint::Timer::default()),
            shutdown,
        });
        let weak = Rc::downgrade(&pending);
        let jobs = Rc::downgrade(&self.0);
        pending.timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(20),
            move || {
                let done = match receiver.try_recv() {
                    Ok(result) => {
                        if let Err(error) = result {
                            eprintln!("Metadata file work: {error}");
                        }
                        true
                    }
                    Err(mpsc::TryRecvError::Disconnected) => true,
                    Err(mpsc::TryRecvError::Empty) => false,
                };
                if done {
                    if let Some(pending) = weak.upgrade() {
                        pending.timer.stop();
                    }
                    if let Some(jobs) = jobs.upgrade() {
                        jobs.borrow_mut()
                            .retain(|job| !std::ptr::eq(Rc::as_ptr(job), weak.as_ptr()));
                    }
                    applied();
                }
            },
        );
        self.0.borrow_mut().push(pending);
        Ok(())
    }
}
