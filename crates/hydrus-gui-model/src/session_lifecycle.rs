//! Reference autosave cadence and idle timer eligibility, without an event loop.
use hydrus_store::settings::{GuiIdleSettings, GuiSessionSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Wait,
    Save,
    Stopped,
}

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
            self.next = Some(now_ms.saturating_add(60000));
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
    i64::from(settings.autosave_minutes.clamp(1, 1440)) * 60000
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
    pub fn eligible(&self, now_ms: i64, settings: &GuiIdleSettings) -> bool {
        fn passed(now: i64, at: i64, seconds: Option<u64>) -> bool {
            seconds.is_none_or(|seconds| {
                now > at
                    .saturating_add(i64::try_from(seconds.saturating_mul(1000)).unwrap_or(i64::MAX))
            })
        }
        settings.enabled
            && now_ms > self.boot.saturating_add(120000)
            && passed(now_ms, self.user, settings.user_seconds)
            && passed(now_ms, self.mouse, settings.mouse_seconds)
            && passed(now_ms, self.api, settings.api_seconds)
    }
}
