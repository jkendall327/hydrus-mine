//! The desktop client's idle state, published for `hydrus serve`'s workers
//! (the reference's `CurrentlyIdle`, which its maintenance asks). The GUI
//! decides idleness from its own activity; the daemon only reads it. A marker
//! file, as for Client API activity, keeps this off the database writer. A
//! stale marker (no GUI, or one that stopped) reads as not idle: the daemon
//! alone is never idle, as a server.
use std::io::{Read as _, Write as _};
use std::path::Path;

const FILE_NAME: &str = "client_idle_state";

/// How long a published state holds without being republished.
pub const FRESH_MS: i64 = 15_000;

/// Publish whether the client is idle at `now_ms`.
pub fn publish(dir: &Path, idle: bool, now_ms: i64) -> std::io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(&[u8::from(idle)])?;
    file.write_all(&now_ms.to_le_bytes())?;
    file.persist(dir.join(FILE_NAME)).map_err(|e| e.error)?;
    Ok(())
}

/// Whether a fresh published state says idle at `now_ms`.
pub fn is_idle(dir: &Path, now_ms: i64) -> bool {
    let Ok(file) = std::fs::File::open(dir.join(FILE_NAME)) else {
        return false;
    };
    let mut bytes = Vec::with_capacity(10);
    if file.take(10).read_to_end(&mut bytes).is_err() || bytes.len() != 9 {
        return false;
    }
    let at = i64::from_le_bytes(bytes[1..9].try_into().expect("eight bytes"));
    bytes[0] == 1 && now_ms >= at && now_ms - at <= FRESH_MS
}

/// The CPU-busy check (`SystemBusy`): busy when at least `count` cores ran
/// above `percent` between two samples of their times. None: ignore CPU use.
///
/// Each core's use is worked out as the reference's psutil does
/// (`cpu_percent(percpu=True)`): every counter is read in seconds (jiffies
/// over the clock's 100 ticks a second), a counter that went backwards
/// counts as no time, guest time is already in user time so it leaves the
/// total, idle and iowait are not busy, and the percentage is rounded to one
/// decimal place before it is compared.
#[derive(Debug, Default)]
pub struct CpuBusy {
    last: Option<Vec<Vec<f64>>>,
}

/// psutil's Linux CPU time fields: user, nice, system, idle, iowait, irq,
/// softirq, steal, guest and guest_nice.
const FIELDS: usize = 10;
/// `os.sysconf('SC_CLK_TCK')` on Linux.
const CLOCK_TICKS: f64 = 100.0;

impl CpuBusy {
    /// Sample now; `None` when the system's per-core times can't be read
    /// (only Linux's `/proc/stat` is read), or on the first sample.
    pub fn sample(&mut self, percent: u32, count: u32) -> Option<bool> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        self.sample_stat(&stat, percent, count)
    }

    /// [`Self::sample`] with the text of `/proc/stat` given.
    pub fn sample_stat(&mut self, stat: &str, percent: u32, count: u32) -> Option<bool> {
        let now = per_core_times(stat)?;
        let last = self.last.replace(now.clone())?;
        let busy = now
            .iter()
            .zip(&last)
            .filter(|(now, last)| core_percent(last, now) > f64::from(percent))
            .count();
        Some(busy >= count as usize)
    }
}

/// One core's busy percentage between two samples, rounded as psutil
/// rounds it (`round(busy / total * 100, 1)`; 0 when no time passed).
fn core_percent(last: &[f64], now: &[f64]) -> f64 {
    let deltas: Vec<f64> = (0..FIELDS)
        .map(|i| {
            let field = |times: &[f64]| times.get(i).copied().unwrap_or(0.0);
            (field(now) - field(last)).max(0.0)
        })
        .collect();
    let mut total: f64 = deltas.iter().sum();
    total -= deltas[8];
    total -= deltas[9];
    let busy = total - deltas[3] - deltas[4];
    if total == 0.0 {
        return 0.0;
    }
    // Python's round() to a place is the correctly rounded decimal, as
    // Rust's formatting is.
    format!("{:.1}", busy / total * 100.0)
        .parse()
        .unwrap_or(0.0)
}

/// Each core's counters, in seconds, from the text of `/proc/stat` (the
/// lines after the first that start with `cpu`, as psutil reads them).
fn per_core_times(stat: &str) -> Option<Vec<Vec<f64>>> {
    let cores: Vec<Vec<f64>> = stat
        .lines()
        .skip(1)
        .filter(|l| l.starts_with("cpu"))
        .map(|l| {
            l.split_whitespace()
                .skip(1)
                .take(FIELDS)
                .map(|v| v.parse::<f64>().unwrap_or(0.0) / CLOCK_TICKS)
                .collect()
        })
        .collect();
    (!cores.is_empty()).then_some(cores)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_fresh_idle_marker_is_idle() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_idle(dir.path(), 1_000));
        publish(dir.path(), true, 1_000).unwrap();
        assert!(is_idle(dir.path(), 1_000 + FRESH_MS));
        assert!(!is_idle(dir.path(), 1_001 + FRESH_MS));
        assert!(!is_idle(dir.path(), 999));
        publish(dir.path(), false, 2_000).unwrap();
        assert!(!is_idle(dir.path(), 2_000));
    }

    #[test]
    fn pace_follows_the_time_and_caps_the_rest() {
        let ms = std::time::Duration::from_millis;
        let pace = Pace::choose(false, (true, 100, 900), (false, 1000, 100));
        assert_eq!(
            pace,
            Pace {
                allowed: true,
                work: ms(100),
                rest_percentage: 900
            }
        );
        assert_eq!(pace.rest(ms(50)), ms(450));
        assert_eq!(pace.rest(ms(10_000)), ms(4_500));
        assert!(!Pace::choose(true, (true, 100, 900), (false, 1000, 100)).allowed);
    }
}

/// One automatic worker's work packet and rest, in normal or idle time
/// (the reference's "ideal work packet time" and "rest time percentage").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    pub allowed: bool,
    pub work: std::time::Duration,
    pub rest_percentage: u32,
}

impl Pace {
    /// The pace for idle or normal time from each time's switch, packet (ms)
    /// and rest.
    pub fn choose(idle: bool, normal: (bool, u32, u32), idle_time: (bool, u32, u32)) -> Self {
        let (allowed, work_ms, rest_percentage) = if idle { idle_time } else { normal };
        Self {
            allowed,
            work: std::time::Duration::from_millis(u64::from(work_ms)),
            rest_percentage,
        }
    }

    /// The rest after working `worked`, which counts at most five packets.
    pub fn rest(&self, worked: std::time::Duration) -> std::time::Duration {
        worked.min(self.work * 5) * self.rest_percentage / 100
    }
}
