//! Interrupt the normal daemon rest when a folder manager opens or closes.

use std::time::Duration;

use hydrus_store::Store;
use hydrus_store::folder_activity::{self, Kind};

pub(crate) async fn wait(store: &Store, kind: Kind, duration: Duration) {
    let previous = folder_activity::change_time(store.dir(), kind).ok();
    let end = tokio::time::Instant::now() + duration;
    loop {
        let remaining = end.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return;
        }
        tokio::time::sleep(remaining.min(Duration::from_secs(1))).await;
        if folder_activity::change_time(store.dir(), kind).ok() != previous
            || matches!(folder_activity::edit_requested(store.dir(), kind), Ok(true))
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use hydrus_store::folder_activity::{Edit, Kind};

    #[tokio::test]
    async fn manager_acquisition_and_release_interrupt_the_real_scheduler_rest() {
        let dir = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(dir.path()).unwrap();
        for kind in [Kind::Import, Kind::Export] {
            let waiter = tokio::spawn({
                let store = store.clone();
                async move { super::wait(&store, kind, Duration::from_secs(180)).await }
            });
            tokio::time::sleep(Duration::from_millis(50)).await;
            let editor = Edit::request(dir.path(), kind).unwrap().unwrap();
            tokio::time::timeout(Duration::from_secs(3), waiter)
                .await
                .unwrap()
                .unwrap();
            let waiter = tokio::spawn({
                let store = store.clone();
                async move { super::wait(&store, kind, Duration::from_secs(180)).await }
            });
            tokio::time::sleep(Duration::from_millis(50)).await;
            drop(editor);
            tokio::time::timeout(Duration::from_secs(3), waiter)
                .await
                .unwrap()
                .unwrap();
        }
    }
}
