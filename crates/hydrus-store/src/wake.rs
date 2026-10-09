//! Waking from system sleep: the file system waits a while too, when asked
//! (`ClientFilesManager._WaitOnWakeup`, for a NAS that takes a few seconds to
//! come back after the machine resumes).
//!
//! A gap of over a minute between two checks means the computer slept
//! (`HydrusController.SleepCheck`); it is then "just woken" for the wake
//! delay, during which file paths are not handed out when the option says so.

use std::sync::Mutex;
use std::time::Duration;

/// The options this reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WakeSettings {
    /// "Allow wake-from-system-sleep detection".
    pub detect: bool,
    /// Seconds to wait after a wake.
    pub delay_secs: u64,
    /// "Include the file system in this wait".
    pub file_system_waits: bool,
}

#[derive(Debug, Default)]
struct State {
    last_check_ms: Option<i64>,
    awake_at_ms: Option<i64>,
}

/// When the computer last seemed to wake, shared by every snapshot of a
/// store.
#[derive(Debug, Default)]
pub struct WakeGate {
    state: Mutex<State>,
}

impl WakeGate {
    /// The sleep check at `now_ms`: a gap of over a minute since the last
    /// one is a wake.
    pub fn check_at(&self, now_ms: i64, settings: WakeSettings) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !settings.detect {
            state.awake_at_ms = None;
            state.last_check_ms = Some(now_ms);
            return;
        }
        if state.last_check_ms.is_some_and(|t| now_ms - t > 60_000) {
            let delay = i64::try_from(settings.delay_secs).unwrap_or(i64::MAX);
            state.awake_at_ms = Some(now_ms.saturating_add(delay.saturating_mul(1000)));
        } else if state.awake_at_ms.is_some_and(|t| now_ms >= t) {
            state.awake_at_ms = None;
        }
        state.last_check_ms = Some(now_ms);
    }

    /// Whether the computer has just woken (`JustWokeFromSleep`), checking
    /// first.
    pub fn just_woke_at(&self, now_ms: i64, settings: WakeSettings) -> bool {
        self.check_at(now_ms, settings);
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .awake_at_ms
            .is_some()
    }

    /// Wait while the computer has just woken, if `settings` say the file
    /// system waits; `poll` is how often to look again.
    pub fn wait_for_file_system(&self, settings: WakeSettings, poll: Duration) {
        if !settings.file_system_waits {
            return;
        }
        while self.just_woke_at(hydrus_core::time::TimestampMs::now().0, settings) {
            std::thread::sleep(poll);
        }
    }
}
