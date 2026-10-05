//! The sidebar's strict wall-clock deadline, independent of downloader work.
#[derive(Debug, Clone, Default)]
pub struct Deadline {
    next: f64,
}
impl Deadline {
    pub fn next(&self) -> f64 {
        self.next
    }
    pub fn due(&self, now: f64) -> bool {
        now > self.next
    }
    pub fn force(&mut self) {
        self.next = 0.0;
    }
    /// Sample the currently displayed item count before refreshing the list.
    pub fn advance(&mut self, now: f64, minimum_ms: i64, denominator: i64, items: usize) -> bool {
        if !self.due(now) {
            return false;
        }
        let period = if denominator == 0 {
            1.0
        } else {
            (minimum_ms as f64 / 1000.0).max(items as f64 / denominator as f64)
        };
        self.next = now + period;
        true
    }
}

/// Qt constructor fields clamp imported values and truncate fractional milliseconds.
pub fn displayed_minimum(milliseconds: i64) -> f64 {
    let seconds = milliseconds.max(250) as f64 / 1000.0;
    let whole = seconds.floor().clamp(0.0, 59.0);
    let fraction_ms = ((seconds % 1.0) * 1000.0) as i64;
    whole + fraction_ms.clamp(0, 999) as f64 / 1000.0
}
/// UpdateOptions accepts displayed values, including unchanged imported bounds.
pub fn normalised(
    raw: &hydrus_store::downloader_update_times::Preferences,
) -> hydrus_store::downloader_update_times::Preferences {
    hydrus_store::downloader_update_times::Preferences {
        gallery_minimum_ms: (displayed_minimum(raw.gallery_minimum_ms) * 1000.0) as i64,
        gallery_denominator: raw.gallery_denominator.clamp(1, 99),
        watcher_minimum_ms: (displayed_minimum(raw.watcher_minimum_ms) * 1000.0) as i64,
        watcher_denominator: raw.watcher_denominator.clamp(1, 99),
    }
}
