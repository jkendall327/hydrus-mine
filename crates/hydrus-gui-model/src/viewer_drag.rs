//! Reference drag anchor and apparent-touchscreen override state.
#[derive(Debug, Default)]
pub struct Drag {
    last: Option<(i32, i32)>,
    pub touch: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Motion {
    pub delta: (i32, i32),
    pub warp: Option<(i32, i32)>,
}
impl Drag {
    pub fn begin(&mut self, point: (i32, i32)) {
        self.last = Some(point);
        self.touch = false;
    }
    pub fn end(&mut self) {
        self.last = None;
    }
    pub fn last(&self) -> Option<(i32, i32)> {
        self.last
    }
    pub fn step(
        &mut self,
        point: (i32, i32),
        anchor: bool,
        touch_override: bool,
    ) -> Option<Motion> {
        let last = self.last?;
        let delta = (
            point.0.saturating_sub(last.0),
            point.1.saturating_sub(last.1),
        );
        let distance = i64::from(delta.0).abs() + i64::from(delta.1).abs();
        if distance == 0 {
            return None;
        }
        if distance > 50 {
            self.touch = true;
        }
        let warp = if anchor && !(touch_override && self.touch) {
            Some(last)
        } else {
            self.last = Some(point);
            None
        };
        Some(Motion { delta, warp })
    }
}
