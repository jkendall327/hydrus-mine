//! Real viewer neighbourhood order and one-miss budget planning.
use hydrus_core::HashId;
use hydrus_store::image_cache::Policy;
use std::collections::BTreeSet;

/// Qt visits next before previous at each distance, wrapping and suppressing
/// previously encountered neighbours. Its visited set does not start with current.
pub fn neighbours(files: &[HashId], index: usize, previous: u64, next: u64) -> Vec<HashId> {
    if index >= files.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let (mut back, mut forward) = (index, index);
    let (mut looked_back, mut looked_forward) = (0, 0);
    while looked_forward < next || looked_back < previous {
        if looked_forward < next {
            forward = (forward + 1) % files.len();
            if seen.insert(files[forward]) {
                out.push(files[forward]);
                looked_forward += 1;
            } else {
                looked_forward = next;
            }
        }
        if looked_back < previous {
            back = back.checked_sub(1).unwrap_or(files.len() - 1);
            if seen.insert(files[back]) {
                out.push(files[back]);
                looked_back += 1;
            } else {
                looked_back = previous;
            }
        }
    }
    out
}
/// An ordered cache access: ready footprints count, pending entries stop a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    Ready(u64),
    Pending,
    Missing(Option<u64>),
}
/// At most one cache miss is scheduled per readiness pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Wait,
    Stop,
    Decode(u64),
}
/// Captured policy for one current-prefix neighbourhood pass.
#[derive(Debug)]
pub struct Budget {
    total: f64,
    single: f64,
    used: u64,
}
impl Budget {
    pub fn new(policy: Policy, percentage: u64) -> Self {
        Self {
            total: policy.bytes as f64 * (percentage as f64 / 100.0),
            single: policy.bytes as f64 * (policy.percentage as f64 / 100.0),
            used: 0,
        }
    }
    /// Prefetch refusal uses >, distinct from cache admission's strict <.
    pub fn consider(&mut self, entry: Entry) -> Step {
        match entry {
            Entry::Ready(bytes) => {
                self.used = self.used.saturating_add(bytes);
                Step::Continue
            }
            Entry::Pending => Step::Wait,
            Entry::Missing(None) => Step::Stop,
            Entry::Missing(Some(bytes)) => {
                if self.used as f64 + bytes as f64 > self.total || bytes as f64 > self.single {
                    Step::Stop
                } else {
                    Step::Decode(bytes)
                }
            }
        }
    }
}
/// Reference supporting caption, not a second admission calculation.
pub fn percentage_estimate(bytes: u64, percentage: u64, nice: bool) -> String {
    let pixels = bytes as f64 * (percentage as f64 / 100.0) / 3.0;
    let resolution = |pixels: f64| {
        let unit = (pixels / (16.0 * 9.0)).sqrt();
        let (width, height) = ((16.0 * unit) as u64, (9.0 * unit) as u64);
        if nice {
            hydrus_core::numbers::resolution_text(width, height)
        } else {
            format!(
                "{}x{}",
                hydrus_core::numbers::human_int(width),
                hydrus_core::numbers::human_int(height)
            )
        }
    };
    format!(
        "% - {} pixels: 5x ~{}, max ~{}",
        hydrus_core::numbers::human_int(pixels as u64),
        resolution(pixels / 4.0),
        resolution(pixels)
    )
}

/// Actual supporting warning, including the retained duplicate-filter count.
pub fn warning(
    bytes: u64,
    preferences: hydrus_store::viewer_prefetch::Preferences,
) -> &'static str {
    let available = bytes as f64 * (preferences.percentage as f64 / 100.0);
    let viewer = 1.0 + preferences.previous as f64 + preferences.next as f64;
    let duplicate = 2.0 + preferences.duplicate_pairs.min(25) as f64 * 2.0;
    if viewer * 1080.0 * 1920.0 * 3.0 > available {
        "Yes! You could not prefetch this number of 1080p images in the media viewer!"
    } else if viewer * 2160.0 * 3840.0 * 3.0 > available {
        "Somewhat--you could not prefetch this number of 4k images in the media viewer."
    } else if duplicate * 1080.0 * 1920.0 * 3.0 > available {
        "Yes! You could not prefetch this number of 1080p images in the duplicate filter."
    } else if duplicate * 2160.0 * 3840.0 * 3.0 > available {
        "Somewhat--you could not prefetch this number of 4k images in the duplicate filter."
    } else {
        "No, looks good!"
    }
}
