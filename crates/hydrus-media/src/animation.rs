//! Animations the reference shows with its own player rather than mpv
//! (ugoiras and animated WebP), decoded a frame at a time and looping, as
//! a viewer plays them.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use hydrus_core::Mime;

use crate::error::{MediaError, Result};
use crate::formats::archive::{self, Zip};
use crate::imaging::Raster;
use crate::tools::{MediaTools, raster_from_bytes};

/// An animation's frames, one at a time, from the first again after the
/// last.
pub struct Frames {
    source: Source,
    next: usize,
}

enum Source {
    /// A ugoira's frame images, in `GetFramePathsUgoira`'s order, each
    /// shown as `GetFrameDurationsMSUgoira` says.
    Ugoira {
        zip: Zip,
        names: Vec<String>,
        durations: Vec<u32>,
    },
    /// An animated WebP's canvas after each frame.
    Webp {
        decoder: image_webp::WebPDecoder<BufReader<File>>,
        channels: u8,
    },
}

impl std::fmt::Debug for Frames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frames")
            .field("len", &self.len())
            .field("next", &self.next)
            .finish_non_exhaustive()
    }
}

impl Frames {
    /// Whether the reference plays files of this type itself.
    pub fn plays(mime: Mime) -> bool {
        matches!(mime, Mime::AnimationUgoira | Mime::AnimationWebp)
    }

    /// Open the animation at `path`. A ugoira's timings may come from its
    /// `notes` ((name, text)); `num_frames` is its metadata's, for when
    /// nothing gives them.
    pub fn open(
        path: &Path,
        mime: Mime,
        notes: &[(String, String)],
        num_frames: Option<u64>,
    ) -> Result<Self> {
        let source = match mime {
            Mime::AnimationUgoira => {
                let damaged = || MediaError::damaged("Could not read the ugoira's frames!");
                let mut zip = Zip::open(path).ok_or_else(damaged)?;
                let names = archive::ugoira_frame_paths(&mut zip)
                    .filter(|n| !n.is_empty())
                    .ok_or_else(damaged)?;
                let durations = MediaTools::ugoira_frame_durations(path, notes, num_frames);
                Source::Ugoira {
                    zip,
                    names,
                    durations,
                }
            }
            Mime::AnimationWebp => {
                let decoder = image_webp::WebPDecoder::new(BufReader::new(File::open(path)?))
                    .map_err(|e| MediaError::damaged(e.to_string()))?;
                if !decoder.is_animated() || decoder.num_frames() == 0 {
                    return Err(MediaError::damaged("The WebP is not animated!"));
                }
                let channels = if decoder.has_alpha() { 4 } else { 3 };
                Source::Webp { decoder, channels }
            }
            other => {
                return Err(MediaError::Unsupported {
                    mime: other,
                    reason: "not an animation the client plays itself".into(),
                });
            }
        };
        Ok(Self { source, next: 0 })
    }

    /// How many frames there are.
    pub fn len(&self) -> usize {
        match &self.source {
            Source::Ugoira { names, .. } => names.len(),
            Source::Webp { decoder, .. } => decoder.num_frames() as usize,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How long a ugoira's frame `index` shows, in ms.
    fn ugoira_duration(durations: &[u32], index: usize) -> u32 {
        durations
            .get(index)
            .copied()
            .unwrap_or(crate::tools::UGOIRA_DEFAULT_FRAME_DURATION_MS)
    }

    /// How long all the frames show, in ms, if known without playing them
    /// (a ugoira's).
    pub fn total_ms(&self) -> Option<u64> {
        match &self.source {
            Source::Ugoira {
                names, durations, ..
            } => Some(
                (0..names.len())
                    .map(|i| u64::from(Self::ugoira_duration(durations, i)))
                    .sum(),
            ),
            Source::Webp { .. } => None,
        }
    }

    /// Make frame `index` (at most the last) the next one, and say how long
    /// the frames before it show, in ms (its place in time). An animated
    /// WebP is played up to it, as its frames build on each other.
    pub fn seek(&mut self, index: usize) -> Result<u64> {
        let index = index.min(self.len().saturating_sub(1));
        if let Source::Ugoira { durations, .. } = &self.source {
            let before = (0..index)
                .map(|i| u64::from(Self::ugoira_duration(durations, i)))
                .sum();
            self.next = index;
            return Ok(before);
        }
        self.next = 0;
        let mut before = 0;
        for _ in 0..index {
            before += u64::from(self.next_frame()?.1);
        }
        Ok(before)
    }

    /// The next frame and how long it shows, in ms.
    pub fn next_frame(&mut self) -> Result<(Raster, u32)> {
        let index = self.next;
        self.next = (index + 1) % self.len().max(1);
        match &mut self.source {
            Source::Ugoira {
                zip,
                names,
                durations,
            } => {
                let bytes = zip
                    .read(&names[index])
                    .ok_or_else(|| MediaError::damaged("Could not read a ugoira frame!"))?;
                let duration = Self::ugoira_duration(durations, index);
                Ok((raster_from_bytes(&bytes, false)?, duration))
            }
            Source::Webp { decoder, channels } => {
                if index == 0 {
                    decoder.reset_animation();
                }
                let (width, height) = decoder.dimensions();
                let size = decoder
                    .output_buffer_size()
                    .ok_or_else(|| MediaError::damaged("The WebP is too big!"))?;
                let mut buf = vec![0; size];
                let duration = decoder
                    .read_frame(&mut buf)
                    .map_err(|e| MediaError::damaged(e.to_string()))?;
                Ok((Raster::new(width, height, *channels, buf)?, duration))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../oracle/fixtures/media")
            .join(name)
    }

    /// (width, height, duration) of the next `n` frames.
    fn play(frames: &mut Frames, n: usize) -> Vec<(u32, u32, u32)> {
        (0..n)
            .map(|_| {
                let (image, ms) = frames.next_frame().unwrap();
                (image.width(), image.height(), ms)
            })
            .collect()
    }

    #[test]
    fn ugoiras_play_with_their_timings_and_loop() {
        let mut frames =
            Frames::open(&corpus("ugoira_json.zip"), Mime::AnimationUgoira, &[], None).unwrap();
        assert_eq!(frames.len(), 5);
        let shown = play(&mut frames, 7);
        let durations: Vec<u32> = shown.iter().map(|f| f.2).collect();
        assert_eq!(durations, [60, 70, 80, 90, 100, 60, 70]);
        assert!(shown.iter().all(|f| (f.0, f.1) == (80, 60)));
        // a note's timings, and rotated frames
        let notes = [(
            "ugoira frame delay array".to_owned(),
            "[10, 20, 30, 40, 50]".to_owned(),
        )];
        let mut frames = Frames::open(
            &corpus("ugoira_plain.zip"),
            Mime::AnimationUgoira,
            &notes,
            Some(5),
        )
        .unwrap();
        assert_eq!(
            play(&mut frames, 2).iter().map(|f| f.2).collect::<Vec<_>>(),
            [10, 20]
        );
        let mut frames = Frames::open(
            &corpus("ugoira_rotated.zip"),
            Mime::AnimationUgoira,
            &[],
            Some(3),
        )
        .unwrap();
        assert_eq!(play(&mut frames, 1)[0], (60, 80, 125));
    }

    #[test]
    fn frames_are_seeked_to() {
        // a ugoira: straight there, its place the frames' timings before it
        let mut frames =
            Frames::open(&corpus("ugoira_json.zip"), Mime::AnimationUgoira, &[], None).unwrap();
        assert_eq!(frames.total_ms(), Some(400));
        assert_eq!(frames.seek(3).unwrap(), 60 + 70 + 80);
        assert_eq!(
            play(&mut frames, 2).iter().map(|f| f.2).collect::<Vec<_>>(),
            [90, 100]
        );
        assert_eq!(frames.seek(99).unwrap(), 300, "the last at most");
        // an animated WebP: played up to it
        let path = corpus("webp_anim.webp");
        let mut frames = Frames::open(&path, Mime::AnimationWebp, &[], None).unwrap();
        assert_eq!(frames.total_ms(), None);
        let mut played = Vec::new();
        let mut elapsed = vec![0_u64];
        for _ in 0..frames.len() {
            let (image, ms) = frames.next_frame().unwrap();
            played.push(image);
            elapsed.push(elapsed.last().unwrap() + u64::from(ms));
        }
        for index in [2, 0, 1] {
            assert_eq!(frames.seek(index).unwrap(), elapsed[index]);
            assert_eq!(
                frames.next_frame().unwrap().0.data(),
                played[index].data(),
                "{index}"
            );
        }
    }

    #[test]
    fn animated_webp_plays_and_loops() {
        for name in ["webp_anim.webp", "webp_anim_alpha.webp"] {
            let mut frames = Frames::open(&corpus(name), Mime::AnimationWebp, &[], None).unwrap();
            let n = frames.len();
            assert!(n > 1, "{name}");
            let first = frames.next_frame().unwrap().0;
            for _ in 1..n {
                frames.next_frame().unwrap();
            }
            // round to the first again
            assert_eq!(
                frames.next_frame().unwrap().0.data(),
                first.data(),
                "{name}"
            );
        }
        assert!(Frames::open(&corpus("webp_lossy.webp"), Mime::AnimationWebp, &[], None).is_err());
    }
}
