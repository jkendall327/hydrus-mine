//! Owned thumbnail LRU: byte estimates, last-access monotonic expiry and explicit resets.
//! Reference DataCache admits one item beyond its soft size limit; maintenance trims
//! that overflow. Loading work belongs to the caller and is never counted as cached.
use hydrus_core::HashId;
use hydrus_store::settings::ThumbnailCacheSettings;
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;
struct Entry<T> {
    value: T,
    bytes: u64,
    touched: Duration,
}
/// A GUI incarnation's decoded thumbnails, shared by the pages shown through its grid.
pub struct Cache<T> {
    entries: BTreeMap<HashId, Entry<T>>,
    order: VecDeque<HashId>,
    bytes: u64,
    policy: ThumbnailCacheSettings,
    name: &'static str,
}
impl<T> std::fmt::Debug for Cache<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cache")
            .field("entries", &self.entries.len())
            .field("bytes", &self.bytes)
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}
impl<T> Cache<T> {
    /// Start an empty cache under its saved policy.
    pub fn new(policy: ThumbnailCacheSettings) -> Self {
        Self {
            entries: BTreeMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            policy,
            name: "thumbnail",
        }
    }
    /// Name the cache in cache report mode's lines.
    #[must_use]
    pub fn named(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }
    /// Entries currently owned, including accounted negative entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Whether maintenance or reset has released every entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Sum of admitted byte estimates.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    /// Last applied size and timeout.
    pub fn policy(&self) -> ThumbnailCacheSettings {
        self.policy
    }
    fn oldest(&mut self) {
        if let Some(id) = self.order.pop_front()
            && let Some(entry) = self.entries.remove(&id)
        {
            self.bytes -= entry.bytes;
            hydrus_core::debug_flags::report(hydrus_core::debug_flags::Flag::CacheReport, || {
                let size = hydrus_core::numbers::human_bytes(entry.bytes);
                let current = hydrus_core::numbers::human_bytes(self.bytes);
                let limit = hydrus_core::numbers::human_bytes(self.policy.bytes);
                format!(
                    "Cache \"{}\" removing oldest item \"{id:?}\", size \"{size}\". Current size {current}/{limit}.",
                    self.name
                )
            });
        }
    }
    /// A duplicate admission leaves both its value and its access time untouched.
    pub fn insert(&mut self, id: HashId, value: T, bytes: u64, now: Duration) {
        if self.entries.contains_key(&id) {
            return;
        }
        while self.bytes > self.policy.bytes {
            self.oldest();
        }
        self.entries.insert(
            id,
            Entry {
                value,
                bytes,
                touched: now,
            },
        );
        self.order.push_back(id);
        self.bytes += bytes;
    }
    /// Access refreshes LRU order; expiry is performed by maintenance, as in Qt.
    pub fn get(&mut self, id: HashId, now: Duration) -> Option<T>
    where
        T: Clone,
    {
        self.get_accounted(id, now, |_| None)
    }
    /// Refresh a loaded value's changing footprint at access, as DataCache does.
    pub fn get_accounted(
        &mut self,
        id: HashId,
        now: Duration,
        footprint: impl FnOnce(&T) -> Option<u64>,
    ) -> Option<T>
    where
        T: Clone,
    {
        let entry = self.entries.get_mut(&id)?;
        if let Some(bytes) = footprint(&entry.value) {
            self.bytes = self.bytes - entry.bytes + bytes;
            entry.bytes = bytes;
        }
        entry.touched = now;
        if let Some(at) = self.order.iter().position(|key| *key == id) {
            self.order.remove(at);
        }
        self.order.push_back(id);
        Some(entry.value.clone())
    }
    /// Remove only the captured value, without touching a replacement's LRU time.
    pub fn remove_if(&mut self, id: HashId, matches: impl FnOnce(&T) -> bool) {
        if self
            .entries
            .get(&id)
            .is_some_and(|entry| matches(&entry.value))
            && let Some(entry) = self.entries.remove(&id)
        {
            self.bytes -= entry.bytes;
            self.order.retain(|key| *key != id);
        }
    }
    /// Prefetch frees only finished entries, atomically, and requires strictly
    /// more free space than the incoming estimate. Failed attempts delete none.
    pub fn try_flush_finished_space(&mut self, bytes: u64, finished: impl Fn(&T) -> bool) -> bool {
        let mut free = i128::from(self.policy.bytes) - i128::from(self.bytes);
        if free > i128::from(bytes) {
            return true;
        }
        let mut remove = Vec::new();
        for id in &self.order {
            let Some(entry) = self.entries.get(id) else {
                continue;
            };
            if !finished(&entry.value) {
                continue;
            }
            remove.push(*id);
            free += i128::from(entry.bytes);
            if free > i128::from(bytes) {
                break;
            }
        }
        if free <= i128::from(bytes) {
            return false;
        }
        for id in remove {
            self.remove_if(id, |_| true);
        }
        true
    }
    /// Drop oldest overflow and entries strictly older than the timeout.
    pub fn maintain(&mut self, now: Duration) {
        while self.bytes > self.policy.bytes {
            self.oldest();
        }
        let threshold = now.saturating_sub(Duration::from_secs(self.policy.timeout));
        while self.order.front().is_some_and(|id| {
            self.entries
                .get(id)
                .is_some_and(|entry| entry.touched < threshold)
        }) {
            self.oldest();
        }
    }
    /// Saved policy updates enforce the new limit and timeout immediately.
    pub fn set_policy(&mut self, policy: ThumbnailCacheSettings, now: Duration) {
        self.policy = policy;
        self.maintain(now);
    }
    /// Release entries and their access history without changing policy.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.bytes = 0;
    }
    /// Oldest first, for diagnostics and recorded boundary checks.
    pub fn keys(&self) -> Vec<HashId> {
        self.order.iter().copied().collect()
    }
}
/// BytesControl's amount and selected binary unit (B through TB).
pub fn separated(bytes: u64) -> (i64, usize) {
    let (amount, unit) = raw_separated(bytes);
    (amount.clamp(0, 1_048_576), unit)
}
/// Raw decomposition before BytesControl's spinbox normalization, for changed-value comparison.
pub fn raw_separated(mut bytes: u64) -> (i64, usize) {
    let mut unit = 0;
    while bytes.is_multiple_of(1024) && unit < 4 {
        bytes /= 1024;
        unit += 1;
    }
    (i64::try_from(bytes).unwrap_or(i64::MAX), unit)
}
/// The control changes the multiplier without changing its numeric amount.
pub fn combined(amount: i64, unit: usize) -> u64 {
    (amount.clamp(0, 1_048_576) as u64) * 1024_u64.pow(unit.min(4) as u32)
}
