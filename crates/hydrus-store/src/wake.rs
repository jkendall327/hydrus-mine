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
    /// A wake noticed that nothing has yet taken up (`take_woke`): the client
    /// resets its idle timer and tells the user.
    woke_unseen: bool,
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
    pub fn check_at(&self, now_ms: i64, settings: WakeSettings) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // (as the reference's `SleepCheck`: with detection off it returns at
        // once, touching nothing but the just-woke flag)
        if !settings.detect {
            state.awake_at_ms = None;
            return false;
        }
        let mut woke = false;
        if state.last_check_ms.is_some_and(|t| now_ms - t > 60_000) {
            let delay = i64::try_from(settings.delay_secs).unwrap_or(i64::MAX);
            state.awake_at_ms = Some(now_ms.saturating_add(delay.saturating_mul(1000)));
            woke = true;
            state.woke_unseen = true;
        } else if state.awake_at_ms.is_some_and(|t| now_ms >= t) {
            state.awake_at_ms = None;
        }
        state.last_check_ms = Some(now_ms);
        woke
    }

    /// `SimulateWakeFromSleepEvent`: the last check was an hour ago, and the
    /// check is made now; whether it noticed a wake (not if detection is off).
    pub fn simulate_at(&self, now_ms: i64, settings: WakeSettings) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last_check_ms = Some(now_ms - 3_600_000);
        self.check_at(now_ms, settings)
    }

    /// Whether a wake has been noticed since this was last asked.
    pub fn take_woke(&self) -> bool {
        std::mem::take(
            &mut self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .woke_unseen,
        )
    }

    /// When the computer counts as awake again, if it has just woken (ms).
    pub fn awake_at(&self) -> Option<i64> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .awake_at_ms
    }

    /// The wake delay is over now (the reference's `TouchTime( 'now_awake' )`
    /// when its popup is cancelled).
    pub fn cancel_wake(&self) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .awake_at_ms = None;
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
