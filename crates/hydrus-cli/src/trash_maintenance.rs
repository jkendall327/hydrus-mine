//! One daemon-owned trash worker; shutdown wakes initial/hourly waits.
use hydrus_store::{Store, maintenance::PurgeControl};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub(crate) struct Worker {
    control: PurgeControl,
    stop: tokio::sync::watch::Sender<bool>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Worker {
    pub fn start(store: Arc<Store>) -> Self {
        let control = PurgeControl::default();
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        let owner = control.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                _=stopped.changed()=>return,
                ()=tokio::time::sleep(Duration::from_secs(30))=>{}
            }
            while !owner.is_cancelled() {
                let store = store.clone();
                let control = owner.clone();
                match tokio::task::spawn_blocking(move || {
                    let gates = store.read(hydrus_store::maintenance_gates::load)?;
                    if gates.allows(hydrus_store::maintenance_gates::Worker::Trash, false) {
                        hydrus_store::trash::maintain_trash_with_control(&store, 256, &control)
                    } else {
                        Ok(hydrus_store::trash::TrashReport::default())
                    }
                })
                .await
                {
                    Ok(Ok(report)) if report.total() > 0 => {
                        tracing::info!(
                            over_size = report.over_size,
                            over_age = report.over_age,
                            "deleted files from the trash"
                        );
                    }
                    Ok(Err(e)) => tracing::error!(error=%e,"emptying the trash failed"),
                    Err(e) => {
                        tracing::error!(error=%e,"trash worker failed");
                        break;
                    }
                    _ => {}
                }
                if owner.is_cancelled() {
                    break;
                }
                tokio::select! {
                    _=stopped.changed()=>break,
                    ()=tokio::time::sleep(Duration::from_secs(3600))=>{}
                }
            }
        });
        Self {
            control,
            stop,
            task: Some(task),
        }
    }
    pub fn control(&self) -> PurgeControl {
        self.control.clone()
    }
    pub async fn shutdown(mut self) {
        self.cancel();
        if let Some(task) = self.task.take()
            && let Err(error) = task.await
        {
            tracing::warn!(%error,"waiting for trash maintenance to stop failed");
        }
    }
    fn cancel(&self) {
        self.control.cancel();
        let _ = self.stop.send(true);
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn dropping_owner_wakes_idle_task_and_releases_store_without_restart() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let weak = Arc::downgrade(&store);
        let mut worker = Worker::start(store.clone());
        let control = worker.control();
        let task = worker.task.take().unwrap();
        drop(store);
        drop(worker);
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap();
        assert!(control.is_cancelled());
        assert!(weak.upgrade().is_none());
    }
}
