//! Animations the reference shows with its own player rather than mpv
//! (ugoiras and animated WebP), decoded a frame at a time and looping, as
//! a viewer plays them.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use hydrus_core::Mime;

use crate::error::{MediaError, Result};
use crate::formats::archive::{self, Zip};
use crate::formats::{apng, gif, webp};
use crate::imaging::Raster;
use crate::tools::{MediaTools, raster_from_bytes_with_icc};

/// An animation's frames, one at a time, from the first again after the
/// last, each shown for its duration: a ugoira's as
/// `GetFrameDurationsMSUgoira` says, an animated WebP's as
/// `GetWebPFrameDurationsMS` does.
pub struct Frames {
    source: Source,
    durations: Vec<u32>,
    next: usize,
    times_to_play: u32,
    icc_reader: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    icc_profile: Option<Vec<u8>>,
}

enum Source {
    /// A ugoira's frame images, in `GetFramePathsUgoira`'s order.
    Ugoira { zip: Zip, names: Vec<String> },
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
        Self::open_with_icc(path, mime, notes, num_frames, true)
    }

    /// Open frames with an immutable embedded-profile policy for this player.
    pub fn open_with_icc(
        path: &Path,
        mime: Mime,
        notes: &[(String, String)],
        num_frames: Option<u64>,
        normalise_icc: bool,
    ) -> Result<Self> {
        let icc_profile = if mime == Mime::AnimationWebp {
            crate::imaging::decode::open(&std::fs::read(path)?)
                .ok()
                .and_then(|opened| opened.image.icc_profile)
        } else {
            None
        };
        let (source, durations) = match mime {
            Mime::AnimationUgoira => {
                let damaged = || MediaError::damaged("Could not read the ugoira's frames!");
                let mut zip = Zip::open(path).ok_or_else(damaged)?;
                let names = archive::ugoira_frame_paths(&mut zip)
                    .filter(|n| !n.is_empty())
                    .ok_or_else(damaged)?;
                let durations = MediaTools::ugoira_frame_durations(path, notes, num_frames);
                let durations = (0..names.len())
                    .map(|i| {
                        durations
                            .get(i)
                            .copied()
                            .unwrap_or(crate::tools::UGOIRA_DEFAULT_FRAME_DURATION_MS)
                    })
                    .collect();
                (Source::Ugoira { zip, names }, durations)
            }
            Mime::AnimationWebp => {
                let decoder = image_webp::WebPDecoder::new(BufReader::new(File::open(path)?))
                    .map_err(|e| MediaError::damaged(e.to_string()))?;
                if !decoder.is_animated() || decoder.num_frames() == 0 {
                    return Err(MediaError::damaged("The WebP is not animated!"));
                }
                let channels = if decoder.has_alpha() { 4 } else { 3 };
                // (a frame without one of its own: the reference's 83ms)
                let mut durations: Vec<u32> = webp::frame_durations_ms(&std::fs::read(path)?)
                    .into_iter()
                    .map(|d| u32::try_from(d).unwrap_or(u32::MAX))
                    .collect();
                durations.resize(decoder.num_frames() as usize, 83);
                (Source::Webp { decoder, channels }, durations)
            }
            other => {
                return Err(MediaError::Unsupported {
                    mime: other,
                    reason: "not an animation the client plays itself".into(),
                });
            }
        };
        let times_to_play = if mime == Mime::AnimationUgoira {
            0
        } else {
            times_to_play(path)
        };
        Ok(Self {
            source,
            durations,
            next: 0,
            times_to_play,
            icc_reader: std::sync::Arc::new(move || normalise_icc),
            icc_profile,
        })
    }

    /// Refresh an explicitly owned policy for each future frame conversion,
    /// without restarting the animation's timing or seek position.
    pub fn with_icc_reader(
        mut self,
        reader: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        self.icc_reader = reader;
        self
    }

    /// Stored play count: zero means infinite, matching the reference.
    pub fn times_to_play(&self) -> u32 {
        self.times_to_play
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

    /// How long each frame shows, in ms.
    pub fn durations(&self) -> &[u32] {
        &self.durations
    }

    /// How long all the frames show, in ms.
    pub fn total_ms(&self) -> u64 {
        self.durations.iter().map(|&d| u64::from(d)).sum()
    }

    /// Make frame `index` (at most the last) the next one, and say how long
    /// the frames before it show, in ms (its place in time). An animated
    /// WebP is played up to it, as its frames build on each other.
    pub fn seek(&mut self, index: usize) -> Result<u64> {
        let index = index.min(self.len().saturating_sub(1));
        if matches!(self.source, Source::Webp { .. }) {
            self.next = 0;
            for _ in 0..index {
                self.next_frame()?;
            }
        }
        self.next = index;
        Ok(self.durations[..index].iter().map(|&d| u64::from(d)).sum())
    }

    /// The next frame and how long it shows, in ms.
    pub fn next_frame(&mut self) -> Result<(Raster, u32)> {
        let normalise_icc = (self.icc_reader)();
        let index = self.next;
        self.next = (index + 1) % self.len().max(1);
        let duration = self.durations.get(index).copied().unwrap_or(83);
        match &mut self.source {
            Source::Ugoira { zip, names } => {
                let bytes = zip
                    .read(&names[index])
                    .ok_or_else(|| MediaError::damaged("Could not read a ugoira frame!"))?;
                Ok((
                    raster_from_bytes_with_icc(&bytes, false, normalise_icc)?,
                    duration,
                ))
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
                decoder
                    .read_frame(&mut buf)
                    .map_err(|e| MediaError::damaged(e.to_string()))?;
                let mode = if *channels == 4 {
                    crate::imaging::pil::Mode::Rgba
                } else {
                    crate::imaging::pil::Mode::Rgb
                };
                let mut image = crate::imaging::pil::PilImage::from_u8(mode, width, height, buf)?;
                image.icc_profile.clone_from(&self.icc_profile);
                Ok((
                    image
                        .normalise_with_icc(normalise_icc)?
                        .into_raster(false)?,
                    duration,
                ))
            }
        }
    }
}

/// Animation metadata used by both native and mpv-backed playback. GIFs
/// without a loop extension play once; WebP/APNG zero means infinite.
/// Non-animation formats (including ugoira) impose no finite play limit.
pub fn times_to_play(path: &Path) -> u32 {
    let Ok(mut file) = File::open(path) else {
        return 0;
    };
    let mut header = [0; 12];
    if file.read_exact(&mut header).is_err() {
        return 0;
    }
    if !(header.starts_with(b"GIF87a")
        || header.starts_with(b"GIF89a")
        || header.starts_with(b"\x89PNG\r\n\x1a\n")
        || (header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WEBP")))
    {
        return 0;
    }
    let Ok(data) = std::fs::read(path) else {
        return 0;
    };
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return gif::parse(&data).map_or(1, |gif| gif.times_to_play);
    }
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return apng::times_to_play(&data[..data.len().min(256)]);
    }
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        return webp::chunks(&data)
            .into_iter()
            .find(|(kind, _)| kind == b"ANIM")
            .and_then(|(_, body)| body.get(4..6))
            .and_then(|bytes| bytes.try_into().ok())
            .map_or(0, |bytes| u32::from(u16::from_le_bytes(bytes)));
    }
    0
}

/// The frame showing `timestamp_ms` in, of frames of these `durations`
/// (`GetFrameIndex`): the first not over by then, and past the end, the
/// first.
pub fn frame_index(durations: &[u32], timestamp_ms: u64) -> usize {
    let mut so_far = 0;
    for (index, &duration) in durations.iter().enumerate() {
        so_far += u64::from(duration);
        if so_far > timestamp_ms {
            return index;
        }
    }
    0
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
    fn stored_loop_counts_match_real_qt_animation_metadata() {
        let fixture = hydrus_testkit::fixture_json("viewer_zoom_loop_options.json");
        let directory = tempfile::tempdir().unwrap();
        for case in fixture["metadata"].as_array().unwrap() {
            let path = directory.path().join("animation");
            std::fs::write(&path, hex::decode(case["bytes"].as_str().unwrap()).unwrap()).unwrap();
            assert_eq!(
                u64::from(times_to_play(&path)),
                case["count"].as_u64().unwrap(),
                "{case:?}"
            );
        }
        let original = hex::decode(fixture["animation_bytes"].as_str().unwrap()).unwrap();
        let offset = original
            .windows(4)
            .position(|bytes| bytes == b"ANIM")
            .unwrap()
            + 12;
        for count in 0_u16..=2 {
            let mut data = original.clone();
            data[offset..offset + 2].copy_from_slice(&count.to_le_bytes());
            let path = directory.path().join("animation.webp");
            std::fs::write(&path, data).unwrap();
            let frames = Frames::open(&path, Mime::AnimationWebp, &[], None).unwrap();
            assert_eq!(frames.times_to_play(), u32::from(count));
        }
        assert_eq!(
            Frames::open(&corpus("ugoira_json.zip"), Mime::AnimationUgoira, &[], None)
                .unwrap()
                .times_to_play(),
            0
        );
        let path = directory.path().join("not-animation");
        std::fs::write(&path, vec![0; 1024]).unwrap();
        assert_eq!(
            times_to_play(&path),
            0,
            "other playable formats have no finite animation limit"
        );
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
        assert_eq!(frames.total_ms(), 400);
        assert_eq!(frames.seek(3).unwrap(), 60 + 70 + 80);
        assert_eq!(
            play(&mut frames, 2).iter().map(|f| f.2).collect::<Vec<_>>(),
            [90, 100]
        );
        assert_eq!(frames.seek(99).unwrap(), 300, "the last at most");
        // an animated WebP: played up to it
        let path = corpus("webp_anim.webp");
        let mut frames = Frames::open(&path, Mime::AnimationWebp, &[], None).unwrap();
        let mut played = Vec::new();
        let mut elapsed = vec![0_u64];
        for _ in 0..frames.len() {
            let (image, ms) = frames.next_frame().unwrap();
            played.push(image);
            elapsed.push(elapsed.last().unwrap() + u64::from(ms));
        }
        // (its timings known before playing, as its chunks say)
        assert_eq!(frames.total_ms(), *elapsed.last().unwrap());
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
    fn frames_are_found_by_time() {
        let durations = [60, 70, 80];
        assert_eq!(frame_index(&durations, 0), 0);
        assert_eq!(frame_index(&durations, 59), 0);
        assert_eq!(frame_index(&durations, 60), 1);
        assert_eq!(frame_index(&durations, 209), 2);
        assert_eq!(frame_index(&durations, 210), 0, "past the end, the first");
        assert_eq!(frame_index(&[], 10), 0);
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
