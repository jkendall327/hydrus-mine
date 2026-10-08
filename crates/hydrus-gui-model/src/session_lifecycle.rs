//! Reference autosave cadence and idle timer eligibility, without an event loop.
use hydrus_store::settings::{GuiIdleSettings, GuiSessionSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Wait,
    Save,
    Stopped,
}

#[derive(Debug)]
pub struct Autosave {
    next: Option<i64>,
}
impl Autosave {
    pub fn new(now_ms: i64, settings: &GuiSessionSettings) -> Self {
        Self {
            next: Some(now_ms.saturating_add(period(settings))),
        }
    }
    pub fn next(&self) -> Option<i64> {
        self.next
    }
    pub fn poll(&mut self, now_ms: i64, idle: bool, settings: &GuiSessionSettings) -> Action {
        let Some(next) = self.next else {
            return Action::Stopped;
        };
        if now_ms < next {
            return Action::Wait;
        }
        if settings.only_during_idle && !idle {
            self.next = Some(now_ms.saturating_add(60_000));
            Action::Wait
        } else if settings.startup.as_deref() != Some(hydrus_store::sessions::LAST_SESSION) {
            self.next = None;
            Action::Stopped
        } else {
            self.next = Some(now_ms.saturating_add(period(settings)));
            Action::Save
        }
    }
}
fn period(settings: &GuiSessionSettings) -> i64 {
    i64::from(settings.autosave_minutes.clamp(1, 1440)) * 60_000
}

#[derive(Debug)]
pub struct Idle {
    boot: i64,
    user: i64,
    mouse: i64,
    api: i64,
}
impl Idle {
    pub fn new(now_ms: i64) -> Self {
        Self {
            boot: now_ms,
            user: now_ms,
            mouse: now_ms,
            api: now_ms,
        }
    }
    pub fn user(&mut self, now_ms: i64) {
        self.user = now_ms;
    }
    pub fn mouse(&mut self, now_ms: i64) {
        self.mouse = now_ms;
    }
    pub fn api(&mut self, now_ms: i64) {
        self.api = now_ms;
    }
    /// Why the client is not idle, in the order `ClientController.CurrentlyIdle`
    /// reports them in idle report mode (empty when it is idle).
    pub fn blocked_reasons(&self, now_ms: i64, settings: &GuiIdleSettings) -> Vec<String> {
        let mut reasons = Vec::new();
        if now_ms <= self.boot.saturating_add(120_000) {
            reasons.push("IDLE MODE - Blocked: Program has not been on for 120s yet.".to_owned());
            return reasons;
        }
        if !settings.enabled {
            reasons.push("IDLE MODE - Blocked: Options have disabled normal idle work.".to_owned());
            return reasons;
        }
        let now = now_ms / 1_000;
        for (what, at, seconds) in [
            ("Last user action was", self.user, settings.user_seconds),
            ("Last mouse move was", self.mouse, settings.mouse_seconds),
            ("Last Client API action was", self.api, settings.api_seconds),
        ] {
            if let Some(seconds) = seconds {
                let limit = i64::try_from(seconds.saturating_mul(1_000)).unwrap_or(i64::MAX);
                if now_ms <= at.saturating_add(limit) {
                    reasons.push(format!(
                        "IDLE MODE - Blocked: {what} {}.",
                        hydrus_core::time::timestamp_to_pretty_time_delta(at / 1_000, now, " ago")
                    ));
                }
            }
        }
        reasons
    }
    pub fn eligible(&self, now_ms: i64, settings: &GuiIdleSettings) -> bool {
        fn passed(now: i64, at: i64, seconds: Option<u64>) -> bool {
            seconds.is_none_or(|seconds| {
                now > at.saturating_add(
                    i64::try_from(seconds.saturating_mul(1_000)).unwrap_or(i64::MAX),
                )
            })
        }
        settings.enabled
            && now_ms > self.boot.saturating_add(120_000)
            && passed(now_ms, self.user, settings.user_seconds)
            && passed(now_ms, self.mouse, settings.mouse_seconds)
            && passed(now_ms, self.api, settings.api_seconds)
    }
}

/// One active-session size warning per client boot, independently of autosaves.
#[derive(Debug, Default)]
pub struct SizeWarning {
    shown: bool,
}
impl SizeWarning {
    pub fn shown(&self) -> bool {
        self.shown
    }
    pub fn message(&mut self, weight: u64, enabled: bool) -> Option<String> {
        if !enabled || weight <= 10_000_000 || self.shown {
            return None;
        }
        self.shown = true;
        Some(format!(
            "Your session weight is {}, which is pretty big! To keep your UI lag-free, please try to close some pages or clear some finished downloaders!",
            hydrus_core::numbers::human_int(weight)
        ))
    }
}

/// The exact clean-shutdown recovery question, shown only for an existing name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryQuestion {
    pub title: String,
    pub message: String,
    pub yes: String,
    pub no: String,
    pub auto_yes_seconds: u64,
}
impl RecoveryQuestion {
    pub fn for_session(name: &str) -> Self {
        Self {
            title: "Previous shutdown was bad".into(),
            message: format!(
                "It looks like the last instance of the client did not shut down cleanly.\n\nWould you like to try loading your default session \"{name}\", or just a blank page?\n\nThis will auto-choose to open your default session in 15 seconds."
            ),
            yes: format!("try to load \"{name}\""),
            no: "just load a blank page".into(),
            auto_yes_seconds: 15,
        }
    }
}
