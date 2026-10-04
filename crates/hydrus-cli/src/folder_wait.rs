//! Interrupt the normal daemon rest when a folder manager opens or closes.

use std::time::{Duration, SystemTime};

use hydrus_store::Store;
use hydrus_store::folder_activity::{self, Kind};

/// A producer's pre-work observation, retained until its following rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Marker {
    modified: Option<SystemTime>,
    readable: bool,
}

pub(crate) fn capture(store: &Store, kind: Kind) -> Marker {
    match folder_activity::change_time(store.dir(), kind) {
        Ok(modified) => Marker {
            modified,
            readable: true,
        },
        Err(_) => Marker {
            modified: None,
            readable: false,
        },
    }
}

pub(crate) async fn wait(store: &Store, kind: Kind, previous: Marker, duration: Duration) {
    let end = tokio::time::Instant::now() + duration;
    loop {
        // Check before sleeping, including changes after work finished but
        // before the daemon began this wait. Never establish a new baseline here.
        if capture(store, kind) != previous {
            return;
        }
        let remaining = end.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return;
        }
        tokio::time::sleep(remaining.min(Duration::from_secs(1))).await;
        // A request already seen by the producer gets a bounded retry, rather
        // than repeatedly returning immediately while its manager stays open.
        if matches!(folder_activity::edit_requested(store.dir(), kind), Ok(true)) {
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
            let before = super::capture(&store, kind);
            let waiter = tokio::spawn({
                let store = store.clone();
                async move { super::wait(&store, kind, before, Duration::from_secs(180)).await }
            });
            tokio::time::sleep(Duration::from_millis(50)).await;
            let editor = Edit::request(dir.path(), kind).unwrap().unwrap();
            tokio::time::timeout(Duration::from_secs(3), waiter)
                .await
                .unwrap()
                .unwrap();
            let before = super::capture(&store, kind);
            let waiter = tokio::spawn({
                let store = store.clone();
                async move { super::wait(&store, kind, before, Duration::from_secs(180)).await }
            });
            tokio::time::sleep(Duration::from_millis(50)).await;
            drop(editor);
            tokio::time::timeout(Duration::from_secs(3), waiter)
                .await
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn manager_finishing_between_worker_and_wait_cannot_lose_its_wakeup() {
        let dir = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(dir.path()).unwrap();
        for kind in [Kind::Import, Kind::Export] {
            for apply in [false, true] {
                let before_work = super::capture(&store, kind);
                let mut editor = Edit::request(dir.path(), kind).unwrap().unwrap();
                // The work consumer observes the transient request and skips.
                assert!(hydrus_store::folder_activity::paused(&store, kind).unwrap());
                assert!(editor.try_ready().unwrap());
                if apply {
                    editor.mark_applied().unwrap();
                }
                // Completion occurs before the daemon even constructs wait().
                drop(editor);
                assert!(!hydrus_store::folder_activity::edit_requested(dir.path(), kind).unwrap());
                tokio::time::timeout(
                    Duration::from_millis(500),
                    super::wait(&store, kind, before_work, Duration::from_secs(1800)),
                )
                .await
                .expect("completed manager wake must be noticed immediately");
            }
        }
    }

    #[tokio::test]
    async fn an_already_seen_active_request_does_not_spin_the_scheduler() {
        let dir = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(dir.path()).unwrap();
        for kind in [Kind::Import, Kind::Export] {
            let editor = Edit::request(dir.path(), kind).unwrap().unwrap();
            let before_work = super::capture(&store, kind);
            assert!(
                tokio::time::timeout(
                    Duration::from_millis(100),
                    super::wait(&store, kind, before_work, Duration::from_secs(1800)),
                )
                .await
                .is_err(),
                "an unchanged active request must retain the bounded retry rest"
            );
            drop(editor);
        }
    }
}
