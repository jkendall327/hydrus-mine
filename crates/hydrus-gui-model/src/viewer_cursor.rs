//! Reference cursor inactivity clock and polling cadence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorWait {
    pub touched_ms: u64,
    pub blank: bool,
    pub next_check_ms: Option<u32>,
}
impl CursorWait {
    pub fn new(now_ms: u64) -> Self {
        Self {
            touched_ms: now_ms,
            blank: false,
            next_check_ms: Some(100),
        }
    }
    pub fn motion(&mut self, now_ms: u64, hidden_drag: bool) {
        self.blank = hidden_drag;
        if hidden_drag {
            self.next_check_ms = None;
        } else {
            self.touched_ms = now_ms;
            self.next_check_ms = Some(100);
        }
    }
    pub fn check(
        &mut self,
        now_ms: u64,
        delay_ms: Option<u32>,
        focused: bool,
        eligible: bool,
        menu_open: bool,
    ) {
        if self.next_check_ms.is_none() {
            return;
        }
        if let Some(delay) = delay_ms {
            if !focused || !eligible || menu_open {
                self.touched_ms = now_ms;
                self.blank = false;
            } else {
                self.blank = now_ms > self.touched_ms.saturating_add(u64::from(delay));
            }
            self.next_check_ms = Some((delay / 5).clamp(100, 250));
        } else {
            self.blank = false;
            self.next_check_ms = Some(1000);
        }
    }
}
