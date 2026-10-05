//! Runner-owned paged importer permits. Capacity changes keep running counts.
use std::sync::Arc;

use hydrus_store::settings::ImportWorkSlots;
use parking_lot::Mutex;
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Kind {
    GalleryFiles,
    GallerySearch,
    WatcherFiles,
    WatcherCheck,
    Misc,
}
impl Kind {
    fn index(self) -> usize {
        match self {
            Self::GalleryFiles => 0,
            Self::GallerySearch => 1,
            Self::WatcherFiles => 2,
            Self::WatcherCheck => 3,
            Self::Misc => 4,
        }
    }
    fn limit(self, settings: &ImportWorkSlots) -> usize {
        usize::try_from(
            match self {
                Self::GalleryFiles => settings.gallery_files,
                Self::GallerySearch => settings.gallery_search,
                Self::WatcherFiles => settings.watcher_files,
                Self::WatcherCheck => settings.watcher_check,
                Self::Misc => settings.misc,
            }
            .clamp(1, 500),
        )
        .unwrap_or(1)
    }
}

#[derive(Debug, Default)]
pub(crate) struct Slots {
    active: Mutex<[usize; 5]>,
    pub(crate) changed: Notify,
}
impl Slots {
    pub(crate) fn acquire(
        self: &Arc<Self>,
        kind: Kind,
        settings: &ImportWorkSlots,
    ) -> Option<Permit> {
        let mut active = self.active.lock();
        let count = &mut active[kind.index()];
        if *count >= kind.limit(settings) {
            return None;
        }
        *count += 1;
        Some(Permit {
            slots: Arc::clone(self),
            kind,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Permit {
    slots: Arc<Slots>,
    kind: Kind,
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.slots.active.lock()[self.kind.index()] -= 1;
        self.slots.changed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacities_keep_running_counts_across_reference_lower_raise_and_release() {
        let fixture = hydrus_testkit::fixture_json("import_work_slots.json");
        for boundary in fixture["boundaries"].as_array().unwrap() {
            let name = boundary["kind"].as_str().unwrap();
            let kind = match name {
                "gallery_files" => Kind::GalleryFiles,
                "gallery_search" => Kind::GallerySearch,
                "watcher_files" => Kind::WatcherFiles,
                "watcher_check" => Kind::WatcherCheck,
                "misc" => Kind::Misc,
                other => panic!("unknown reference slot {other}"),
            };
            let slots = Arc::new(Slots::default());
            let mut settings = ImportWorkSlots::default();
            settings.apply_legacy(&[(format!("thread_slots_{name}"), 2)].into());
            let mut held = Vec::new();
            for event in boundary["events"].as_array().unwrap() {
                match event["action"].as_str().unwrap() {
                    "acquire" => {
                        let permit = slots.acquire(kind, &settings);
                        assert_eq!(permit.is_some(), event["granted"].as_bool().unwrap());
                        if let Some(permit) = permit {
                            held.push(permit);
                        }
                    }
                    "release" => drop(held.pop().unwrap()),
                    "lower" | "raise" => {
                        settings.apply_legacy(
                            &[(
                                format!("thread_slots_{name}"),
                                event["limit"].as_i64().unwrap(),
                            )]
                            .into(),
                        );
                    }
                    other => panic!("unknown slot event {other}"),
                }
                assert_eq!(
                    serde_json::json!([slots.active.lock()[kind.index()], kind.limit(&settings)]),
                    event["state"]
                );
            }
            assert!(held.is_empty());
        }
    }

    #[tokio::test]
    async fn independent_categories_and_aborted_owners_release_only_their_permit() {
        let slots = Arc::new(Slots::default());
        let settings = ImportWorkSlots {
            gallery_files: 1,
            gallery_search: 1,
            watcher_files: 1,
            watcher_check: 1,
            misc: 1,
        };
        let mut held = Vec::new();
        for kind in [
            Kind::GalleryFiles,
            Kind::GallerySearch,
            Kind::WatcherFiles,
            Kind::WatcherCheck,
        ] {
            held.push(slots.acquire(kind, &settings).unwrap());
            assert!(slots.acquire(kind, &settings).is_none());
        }
        let permit = slots.acquire(Kind::Misc, &settings).unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let owner = tokio::spawn(async move {
            let _permit = permit;
            started.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        ready.await.unwrap();
        assert!(slots.acquire(Kind::Misc, &settings).is_none());
        owner.abort();
        assert!(owner.await.unwrap_err().is_cancelled());
        assert!(slots.acquire(Kind::Misc, &settings).is_some());
        assert!(slots.acquire(Kind::WatcherCheck, &settings).is_none());
        drop(held);
        assert_eq!(*slots.active.lock(), [0; 5]);
    }
}
