//! Qt's direction-sensitive delay when a tag-list wheel reaches an edge.
use hydrus_store::settings::TagWheelPropagation;
#[derive(Debug, Default)]
pub struct WheelGate {
    pub media_transition: f64,
    pub last_list_scroll: f64,
    pub list_direction: i32,
    pub last_parent_wheel: f64,
    pub parent_direction: i32,
}
impl WheelGate {
    pub fn media_changed(&mut self, now: f64) {
        // The reference uses 250 seconds here, despite the millisecond-like name.
        if now > self.last_parent_wheel + 250.0 {
            self.parent_direction = 0;
        }
        self.media_transition = now;
    }
    pub fn list_scrolled(&mut self, now: f64, direction: i32) {
        self.last_list_scroll = now;
        self.list_direction = direction;
    }
    pub fn propagates(
        &mut self,
        policy: TagWheelPropagation,
        scrollbar: bool,
        now: f64,
        direction: i32,
    ) -> bool {
        match policy {
            TagWheelPropagation::Never => false,
            TagWheelPropagation::NoScrollbar => !scrollbar,
            TagWheelPropagation::Immediately => true,
            TagWheelPropagation::AfterDelay => {
                self.last_parent_wheel = self.last_parent_wheel.max(self.last_list_scroll);
                let mut allow = true;
                if scrollbar && now > self.media_transition + 0.57 {
                    if self.parent_direction == 0 {
                        self.parent_direction = self.list_direction;
                    }
                    allow =
                        self.parent_direction == direction && now > self.last_parent_wheel + 0.57;
                }
                self.last_parent_wheel = now;
                self.parent_direction = direction;
                allow
            }
        }
    }
}
