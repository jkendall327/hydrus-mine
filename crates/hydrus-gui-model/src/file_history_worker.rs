//! One owned history worker, with a bounded wakeup and one replaceable request.
use crate::file_history;
use hydrus_core::search::context::FileSearchContext;
use hydrus_store::{Store, file_history::History};
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
};

pub type Task = Box<dyn FnOnce() + Send + 'static>;
#[derive(Debug)]
struct Request {
    context: FileSearchContext,
    cancel: Arc<AtomicBool>,
}
#[derive(Debug)]
struct Completed {
    cancel: Arc<AtomicBool>,
    result: Result<History, String>,
}
#[derive(Debug, Default)]
struct Queue {
    pending: Option<Request>,
    completed: Option<Completed>,
    closed: bool,
}
#[derive(Debug)]
pub struct Worker {
    wake: Option<SyncSender<()>>,
    queue: Arc<Mutex<Queue>>,
    current: Option<Arc<AtomicBool>>,
}
impl Worker {
    pub fn start(store: Arc<Store>) -> io::Result<Self> {
        Self::start_with(
            move |context, cancel| {
                file_history::load(&store, context, 7680, cancel).map_err(|e| e.to_string())
            },
            |task| {
                std::thread::Builder::new()
                    .name("file-history".into())
                    .spawn(task)
                    .map(drop)
            },
        )
    }
    /// Explicit executor/startup seam: the starter must schedule the task and
    /// return without waiting for it. Used to verify queue and startup failure
    /// boundaries without global hooks or real database timing dependencies.
    pub fn start_with(
        execute: impl Fn(&FileSearchContext, &AtomicBool) -> Result<History, String> + Send + 'static,
        start: impl FnOnce(Task) -> io::Result<()>,
    ) -> io::Result<Self> {
        let queue = Arc::new(Mutex::new(Queue::default()));
        let (wake, receive) = mpsc::sync_channel(1);
        let worker_queue = queue.clone();
        start(Box::new(move || {
            while receive.recv().is_ok() {
                let request = {
                    let Ok(mut queue) = worker_queue.lock() else {
                        break;
                    };
                    if queue.closed {
                        break;
                    }
                    queue.pending.take()
                };
                let Some(request) = request else {
                    continue;
                };
                if request.cancel.load(Ordering::Acquire) {
                    continue;
                }
                let result = execute(&request.context, &request.cancel);
                let Ok(mut queue) = worker_queue.lock() else {
                    break;
                };
                if queue.closed {
                    break;
                }
                if !request.cancel.load(Ordering::Acquire) {
                    queue.completed = Some(Completed {
                        cancel: request.cancel,
                        result,
                    });
                }
            }
            if let Ok(mut queue) = worker_queue.lock() {
                queue.closed = true;
                queue.pending.take();
            }
        }))?;
        Ok(Self {
            wake: Some(wake),
            queue,
            current: None,
        })
    }
    pub fn submit(&mut self, context: FileSearchContext) -> Result<(), String> {
        self.cancel();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut queue = self
                .queue
                .lock()
                .map_err(|_| "file-history worker queue failed".to_string())?;
            if queue.closed {
                return Err("file-history worker is closed".into());
            }
            queue.pending = Some(Request {
                context,
                cancel: cancel.clone(),
            });
        }
        self.current = Some(cancel);
        let Some(wake) = &self.wake else {
            return Err("file-history worker is closed".into());
        };
        match wake.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => Ok(()),
            Err(TrySendError::Disconnected(())) => {
                self.close();
                Err("file-history worker is closed".into())
            }
        }
    }
    pub fn poll(&mut self) -> Option<Result<History, String>> {
        let Ok(mut queue) = self.queue.lock() else {
            return Some(Err("file-history worker queue failed".into()));
        };
        if let Some(completed) = queue.completed.take()
            && self
                .current
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &completed.cancel))
            && !completed.cancel.load(Ordering::Acquire)
        {
            self.current.take();
            return Some(completed.result);
        }
        if queue.closed && self.current.take().is_some() {
            return Some(Err("file-history worker stopped".into()));
        }
        None
    }
    pub fn cancel(&mut self) {
        if let Some(cancel) = self.current.take() {
            cancel.store(true, Ordering::Release);
        }
        if let Ok(mut queue) = self.queue.lock() {
            if let Some(pending) = queue.pending.take() {
                pending.cancel.store(true, Ordering::Release);
            }
            queue.completed.take();
        }
    }
    pub fn close(&mut self) {
        self.cancel();
        if let Ok(mut queue) = self.queue.lock() {
            queue.closed = true;
        }
        self.wake.take();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.close();
    }
}
