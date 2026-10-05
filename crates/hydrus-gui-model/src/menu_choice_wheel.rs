//! One choice per wheel event, wrapping by sign rather than accumulated notches.
/// No current/empty choice consumes an enabled event without publishing a value.
/// A single choice still returns itself so its change signal is emitted.
pub fn next(current: usize, count: usize, delta_y: f32) -> Option<usize> {
    if count == 0 || current >= count {
        return None;
    }
    Some(if delta_y > 0.0 {
        if current == 0 { count - 1 } else { current - 1 }
    } else {
        (current + 1) % count
    })
}
