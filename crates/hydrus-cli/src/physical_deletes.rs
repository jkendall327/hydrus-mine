//! One daemon-owned physical-delete worker; shutdown wakes per-pair and idle waits.
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
            while !owner.is_cancelled() {
                let store = store.clone();
                let control = owner.clone();
                match tokio::task::spawn_blocking(move || {
                    let gates = store.read(hydrus_store::maintenance_gates::load)?;
                    if gates.allows(hydrus_store::maintenance_gates::Worker::Deferred, false) {
                        hydrus_store::maintenance::purge_deleted_media_with_control(
                            &store, 1024, &control,
                        )
                    } else {
                        Ok(hydrus_store::maintenance::PurgeReport::default())
                    }
                })
                .await
                {
                    Ok(Ok(report)) if report.files_deleted > 0 => {
                        tracing::info!(files = report.files_deleted, "purged deleted files");
                    }
                    Ok(Err(e)) => tracing::error!(error=%e,"purging deleted files failed"),
                    Err(e) => {
                        tracing::error!(error=%e,"physical-delete worker failed");
                        break;
                    }
                    _ => {}
                }
                if owner.is_cancelled() {
                    break;
                }
                tokio::select! {
                    _=stopped.changed()=>break,
                    ()=tokio::time::sleep(Duration::from_secs(600))=>{}
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
            tracing::warn!(%error,"waiting for physical deletes to stop failed");
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
