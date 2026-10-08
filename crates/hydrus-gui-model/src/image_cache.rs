//! Full-image renderer accounting. Admission uses the pre-decode RGB estimate;
//! loaded footprints change only at access, not at decoder completion.
use crate::thumbnail_cache::Cache as Lru;
use hydrus_core::HashId;
use hydrus_store::{image_cache::Policy, settings::ThumbnailCacheSettings};
use std::time::Duration;

/// The initial BytesControl/QSpinBox values; timeout stays raw until an edit.
pub fn displayed(policy: Policy) -> Policy {
    let (amount, unit) = crate::thumbnail_cache::separated(policy.bytes);
    Policy {
        bytes: crate::thumbnail_cache::combined(amount, unit),
        percentage: policy.percentage.clamp(10, 50),
        ..policy
    }
}

/// A decoded-image owner; values can be shared pending renderers or immutable rasters.
#[derive(Debug)]
pub struct Cache<T> {
    lru: Lru<T>,
    policy: Policy,
}
impl<T> Cache<T> {
    /// Start an empty renderer cache with the saved policy.
    pub fn new(policy: Policy) -> Self {
        Self {
            lru: Lru::new(ThumbnailCacheSettings {
                bytes: policy.bytes,
                timeout: policy.timeout,
            })
            .named("image"),
            policy,
        }
    }
    /// Qt's strict comparison keeps an image exactly at the threshold uncached.
    pub fn admits(&self, estimated_bytes: u64) -> bool {
        (estimated_bytes as f64)
            < self.policy.bytes as f64 * (self.policy.percentage as f64 / 100.0)
    }
    /// Admitting a pending renderer accounts its estimate and permits one-item overflow.
    pub fn insert(&mut self, id: HashId, value: T, estimated_bytes: u64, now: Duration) -> bool {
        if !self.admits(estimated_bytes) {
            return false;
        }
        self.lru.insert(id, value, estimated_bytes, now);
        true
    }
    /// Access alone adjusts a now-loaded renderer's accounted footprint.
    pub fn get(
        &mut self,
        id: HashId,
        now: Duration,
        footprint: impl FnOnce(&T) -> Option<u64>,
    ) -> Option<T>
    where
        T: Clone,
    {
        self.lru.get_accounted(id, now, footprint)
    }
    /// Size/timeout maintenance is immediate; percentage applies only to future admission.
    pub fn set_policy(&mut self, policy: Policy, now: Duration) {
        self.policy = policy;
        self.lru.set_policy(
            ThumbnailCacheSettings {
                bytes: policy.bytes,
                timeout: policy.timeout,
            },
            now,
        );
    }
    /// Remove a failed captured renderer without clearing a replacement.
    pub fn remove_if(&mut self, id: HashId, matches: impl FnOnce(&T) -> bool) {
        self.lru.remove_if(id, matches);
    }
    /// Atomically free only finished renderers for one allowed prefetch miss.
    pub fn try_flush_finished_space(&mut self, bytes: u64, finished: impl Fn(&T) -> bool) -> bool {
        self.lru.try_flush_finished_space(bytes, finished)
    }
    /// Release soft overflow and strictly expired idle renderers.
    pub fn maintain(&mut self, now: Duration) {
        self.lru.maintain(now);
    }
    /// Forget cache-owned renderers while external presentation references survive.
    pub fn clear(&mut self) {
        self.lru.clear();
    }
    /// Estimated bytes owned by the renderer cache.
    pub fn bytes(&self) -> u64 {
        self.lru.bytes()
    }
    /// Renderer identities in least-recently-accessed order.
    pub fn keys(&self) -> Vec<HashId> {
        self.lru.keys()
    }
    /// The policy currently applied by this owner.
    pub fn policy(&self) -> Policy {
        self.policy
    }
}

/// The real panel's decoded RGB screen estimate, sampled from the owning monitor.
pub fn screen_estimate(bytes: u64, screen: (u64, u64)) -> String {
    let per = screen.0.saturating_mul(screen.1).saturating_mul(3).max(1);
    let count = bytes / per;
    format!(
        "(about {}-{} images the size of your screen)",
        hydrus_core::numbers::human_int(count / 2),
        hydrus_core::numbers::human_int(count.saturating_mul(2))
    )
}
/// Qt's supporting 16:9 pixel budget, independent of strict admission equality.
pub fn percentage_estimate(policy: Policy, nice_resolutions: bool) -> String {
    let pixels = policy.bytes as f64 * (policy.percentage as f64 / 100.0) / 3.0;
    let unit = (pixels / (16.0 * 9.0)).sqrt();
    let (width, height) = ((16.0 * unit) as u64, (9.0 * unit) as u64);
    let resolution = if nice_resolutions {
        hydrus_core::numbers::resolution_text(width, height)
    } else {
        format!(
            "{}x{}",
            hydrus_core::numbers::human_int(width),
            hydrus_core::numbers::human_int(height)
        )
    };
    format!(
        "% - {} pixels, or a ~{} image",
        hydrus_core::numbers::human_int(pixels as u64),
        resolution
    )
}
