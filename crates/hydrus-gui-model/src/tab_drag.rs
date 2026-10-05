//! Qt notebook insertion decisions and an owner-local, timed pointer drag.
use hydrus_core::pages::PageKey;
use std::time::{Duration, Instant};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Body,
    Right,
}
pub fn insertion(
    source: Option<usize>,
    target: Option<usize>,
    edge: Edge,
    count: usize,
) -> Option<usize> {
    if source.is_some() && source == target {
        return None;
    }
    let mut index = target.map_or(count, |index| index + usize::from(edge == Edge::Right));
    if let Some(source) = source {
        if index > source && !(index == source + 1 && edge != Edge::Left) {
            index -= 1;
        }
        if index == source {
            return None;
        }
    }
    Some(index)
}
#[derive(Debug, Clone, Copy)]
pub struct Grab {
    pub key: PageKey,
    pub parent: Option<PageKey>,
    at: Instant,
    pub dragging: bool,
}
#[derive(Debug, Default)]
pub struct Pointer {
    grab: Option<Grab>,
}
impl Pointer {
    pub fn start(&mut self, key: PageKey, parent: Option<PageKey>, now: Instant) {
        self.grab = Some(Grab {
            key,
            parent,
            at: now,
            dragging: false,
        });
    }
    pub fn moved(&mut self, now: Instant, disabled: bool) -> Option<Grab> {
        if disabled {
            self.cancel();
            return None;
        }
        let grab = self.grab.as_mut()?;
        if now.saturating_duration_since(grab.at) >= Duration::from_millis(100) {
            grab.dragging = true;
        }
        grab.dragging.then_some(*grab)
    }
    pub fn release(&mut self) -> Option<Grab> {
        self.grab.take().filter(|grab| grab.dragging)
    }
    pub fn cancel(&mut self) {
        self.grab = None;
    }
}
