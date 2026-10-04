//! Background parser test requests use the downloader's HTTP jobs and accounting.
use hydrus_gui_model::formula_editors::FetchedDocument;
use hydrus_net::{Job, NetEngine, NetError, NetOptions, Request};
use hydrus_store::{Store, network::NetworkSettings, settings};
use slint::{Timer, TimerMode};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

struct Running {
    job: Arc<Job>,
    _timer: Timer,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.job.cancel();
    }
}
/// A window's single request, retained until completion or owner cancellation.
#[derive(Clone, Default)]
pub(crate) struct Slot(Rc<RefCell<Option<Running>>>);
#[derive(Debug)]
pub(crate) struct Outcome {
    pub document: FetchedDocument,
    pub accounting_error: Option<String>,
}
impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestFetchSlot")
            .field("busy", &self.busy())
            .finish()
    }
}
impl Slot {
    pub fn busy(&self) -> bool {
        self.0.borrow().is_some()
    }
    pub fn cancel(&self) {
        if let Some(running) = self.0.borrow().as_ref() {
            running.job.cancel();
        }
    }
    pub fn stop(&self) {
        self.0.borrow_mut().take();
    }
    pub fn start(
        &self,
        store: Arc<Store>,
        mut request: Request,
        progress: Rc<dyn Fn(String)>,
        completed: Rc<dyn Fn(Outcome)>,
    ) {
        self.stop();
        request.override_bandwidth_after = Some(0);
        let job = Job::new();
        let (send, receive) = crossbeam_channel::bounded(1);
        std::thread::spawn({
            let job = job.clone();
            move || {
                let run = || -> Result<Outcome, String> {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| e.to_string())?;
                    let network: NetworkSettings =
                        store.read(settings::get).map_err(|e| e.to_string())?;
                    let engine = {
                        let _entered = runtime.enter();
                        NetEngine::new(store, NetOptions::from_settings(&network))
                            .map_err(|e| e.to_string())?
                    };
                    let result = runtime.block_on(engine.fetch(&request, &job));
                    let document = match result {
                        Ok(response) => FetchedDocument::Text(response.text()),
                        Err(NetError::Cancelled) => FetchedDocument::Cancelled,
                        Err(error) => FetchedDocument::Failed {
                            error: error.to_string(),
                            text: job.error_text().unwrap_or_default(),
                        },
                    };
                    let accounting_error = engine.save_bandwidth().err().map(|e| e.to_string());
                    Ok(Outcome {
                        document,
                        accounting_error,
                    })
                };
                let outcome = run().unwrap_or_else(|error| Outcome {
                    document: FetchedDocument::Failed {
                        error,
                        text: String::new(),
                    },
                    accounting_error: None,
                });
                let _ = send.send(outcome);
            }
        });
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, Duration::from_millis(50), {
            let slot = Rc::downgrade(&self.0);
            let job = job.clone();
            move || {
                let Some(slot) = slot.upgrade() else {
                    return;
                };
                match receive.try_recv() {
                    Ok(outcome) => {
                        slot.borrow_mut().take();
                        completed(outcome);
                    }
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        slot.borrow_mut().take();
                        completed(Outcome {
                            document: FetchedDocument::Failed {
                                error: "Network worker stopped before returning a document.".into(),
                                text: String::new(),
                            },
                            accounting_error: None,
                        });
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => {
                        let state = job.state();
                        progress(format!("{} — {} bytes", state.status, state.bytes_read));
                    }
                }
            }
        });
        *self.0.borrow_mut() = Some(Running { job, _timer: timer });
    }
}
