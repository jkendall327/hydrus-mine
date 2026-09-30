//! Video metadata and frame rendering through ffmpeg
//! (`HydrusVideoHandling.GetFFMPEGVideoProperties`, `VideoHasAudio`,
//! `VideoRendererFFMPEG`).

use std::ffi::OsString;
use std::path::Path;

use hydrus_core::Mime;

use super::{Ffmpeg, parse};
use crate::error::{MediaError, Result};
use crate::imaging::Raster;
use crate::mimes;

/// Resolution, duration, frame count and audio of a video-like file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VideoProperties {
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    pub num_frames: u64,
    pub has_audio: bool,
}

/// Python's `int(x)` for a non-negative float.
pub(crate) fn py_int(x: f64) -> u64 {
    if x.is_finite() && x > 0.0 {
        x.trunc() as u64
    } else {
        0
    }
}

impl Ffmpeg {
    /// `GetFFMPEGVideoProperties`.
    pub(crate) fn video_properties(&self, path: &Path) -> Result<VideoProperties> {
        let lines = self.info_lines(path, None)?;
        let (has_video, _format, mapping) = parse::video_format(&lines)?;
        if !has_video {
            return Err(MediaError::damaged(
                "Wanted to parse video data, but file did not appear to have a video stream!",
            ));
        }
        let (width, height) = parse::video_resolution(&lines, false)?;
        let (file_duration_s, stream_duration_s) = parse::duration(&lines)?;
        let mut duration_s = stream_duration_s;
        let (mut fps, mut confident_fps) = parse::fps(&lines)?;
        if duration_s.is_none() && !confident_fps {
            (fps, confident_fps) = (24.0, true);
        }
        if fps == 0.0 {
            fps = 1.0;
        }
        let mut count_manually = false;
        match duration_s {
            None => count_manually = true,
            Some(d) => {
                let short_enough = d < 30.0;
                let small_enough = std::fs::metadata(path)?.len() < 256 * 1024 * 1024;
                if short_enough && small_enough {
                    let last_frame_unusual = (d * fps) % 1.0 > 0.0;
                    #[allow(clippy::float_cmp)]
                    let unusual_start = file_duration_s != stream_duration_s;
                    if !confident_fps || last_frame_unusual || unusual_start {
                        count_manually = true;
                    }
                }
            }
        }
        let num_frames;
        if count_manually {
            let count_lines = self.info_lines(path, Some(&mapping))?;
            num_frames = parse::num_frames_manually(&count_lines)?;
            if num_frames > 0
                && let Some(d) = duration_s
            {
                let implied_fps = num_frames as f64 / d;
                if 0.0 < fps && fps < 1000.0 && 1000.0 < implied_fps {
                    duration_s = None;
                }
            }
            if duration_s.is_none() {
                duration_s = Some(num_frames as f64 / fps);
            }
        } else {
            let d = *duration_s.get_or_insert(1.0);
            num_frames = py_int(d * fps);
        }
        let duration_ms = py_int(duration_s.unwrap_or(0.0) * 1000.0);
        let has_audio = self.video_has_audio(path, &lines)?;
        Ok(VideoProperties {
            width,
            height,
            duration_ms,
            num_frames,
            has_audio,
        })
    }

    /// `VideoHasAudio`: an audio stream that is not just silence.
    pub(crate) fn video_has_audio(&self, path: &Path, info_lines: &[String]) -> Result<bool> {
        if !parse::audio(info_lines).found {
            return Ok(false);
        }
        let args: Vec<OsString> = vec![
            "-i".into(),
            path.into(),
            "-loglevel".into(),
            "quiet".into(),
            "-f".into(),
            "s16le".into(),
            "-".into(),
        ];
        let mut stream = self.stream(&args, 65536)?;
        loop {
            let chunk = stream.next_chunk()?;
            if chunk.is_empty() {
                return Ok(false);
            }
            // silent PCM is zeros, with the odd 0xff/0x01; real audio has mid values
            if chunk.iter().any(|b| (5..=250).contains(b)) {
                return Ok(true);
            }
        }
    }

    /// `FileIsAnimated` (only used for jxl): more than one frame.
    pub(crate) fn file_is_animated(&self, path: &Path) -> bool {
        self.video_properties(path)
            .map(|p| p.num_frames > 1)
            .unwrap_or(false)
    }

    /// `ParseFFMPEGDuration` for audio files, in ms.
    pub(crate) fn audio_duration_ms(&self, path: &Path) -> Result<u64> {
        let lines = self.info_lines(path, None)?;
        let (file_duration_s, _) = parse::duration(&lines)?;
        let d = file_duration_s
            .ok_or_else(|| MediaError::damaged("Could not determine the duration of this file!"))?;
        Ok(py_int(d * 1000.0))
    }

    /// Render a single PNG of the first attached picture (cover art), if any
    /// (`RenderAnyAttachedStillImageToPath`).
    pub(crate) fn render_attached_image(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        let lines = self.info_lines(path, None)?;
        let Some(first) = parse::image_stream_lines(&lines).first().copied() else {
            return Ok(None);
        };
        let Some(mapping) = parse::stream_mapping(first) else {
            return Ok(None);
        };
        let png = self.render_to_stdout(&[
            "-xerror".as_ref(),
            "-i".as_ref(),
            path.as_os_str(),
            "-map".as_ref(),
            mapping.as_ref(),
            "-frames:v".as_ref(),
            "1".as_ref(),
            "-f".as_ref(),
            "image2pipe".as_ref(),
            "-c:v".as_ref(),
            "png".as_ref(),
            "-".as_ref(),
        ])?;
        Ok(if png.is_empty() { None } else { Some(png) })
    }

    /// ffmpeg's rendering of an image file as PNG bytes (used for PSD).
    pub(crate) fn render_image_to_png(&self, path: &Path) -> Result<Vec<u8>> {
        self.render_to_stdout(&[
            "-xerror".as_ref(),
            "-i".as_ref(),
            path.as_os_str(),
            "-f".as_ref(),
            "image2pipe".as_ref(),
            "-vcodec".as_ref(),
            "png".as_ref(),
            "-".as_ref(),
        ])
    }
}

/// `VideoRendererFFMPEG`: reads scaled frames of a video as raw pixels.
pub(crate) struct VideoRenderer<'a> {
    ffmpeg: &'a Ffmpeg,
    path: &'a Path,
    mime: Mime,
    num_frames: u64,
    target: (u32, u32),
    fps: f64,
    depth: usize,
    pos: u64,
    stream: Option<super::Stream>,
    last_read: Option<Raster>,
    explicit_mapping: Option<String>,
    tried_explicit_mapping: bool,
}

impl<'a> VideoRenderer<'a> {
    pub(crate) fn new(
        ffmpeg: &'a Ffmpeg,
        path: &'a Path,
        mime: Mime,
        duration_ms: Option<u64>,
        num_frames: u64,
        target: (u32, u32),
        start: u64,
    ) -> Result<Self> {
        let duration_ms = match duration_ms {
            Some(d) if d > 0 => d,
            _ => 100,
        };
        let duration_s = duration_ms as f64 / 1000.0;
        let mut fps = num_frames as f64 / duration_s;
        if fps == 0.0 {
            fps = 24.0;
        }
        let depth = if mimes::can_check_transparency(mime) {
            4
        } else {
            3
        };
        let mut renderer = Self {
            ffmpeg,
            path,
            mime,
            num_frames,
            target,
            fps,
            depth,
            pos: 0,
            stream: None,
            last_read: None,
            explicit_mapping: None,
            tried_explicit_mapping: false,
        };
        renderer.initialize(start)?;
        Ok(renderer)
    }

    fn frame_len(&self) -> usize {
        self.target.0 as usize * self.target.1 as usize * self.depth
    }

    fn initialize(&mut self, start_index: u64) -> Result<()> {
        self.stream = None;
        let (do_ss, ss, skip) = if mimes::is_animation(self.mime) {
            // ffmpeg seeks animations badly, and they are short: skip frames instead
            self.pos = 0;
            (false, 0.0, start_index)
        } else {
            self.pos = start_index;
            (start_index != 0, start_index as f64 / self.fps, 0)
        };
        let (w, h) = self.target;
        let mut args: Vec<OsString> = Vec::new();
        if do_ss {
            args.push("-ss".into());
            args.push(format!("{ss:.3}").into());
        }
        args.push("-i".into());
        args.push(self.path.into());
        if let Some(m) = &self.explicit_mapping {
            args.push("-map".into());
            args.push(m.into());
        }
        let pix_fmt = if self.depth == 4 { "rgba" } else { "rgb24" };
        for a in [
            "-vf".to_owned(),
            format!("scale={w}:{h}"),
            "-loglevel".into(),
            "quiet".into(),
            "-f".into(),
            "rawvideo".into(),
            "-pix_fmt".into(),
            pix_fmt.into(),
            "-fps_mode".into(),
            "passthrough".into(),
            "-vcodec".into(),
            "rawvideo".into(),
            "-".into(),
        ] {
            args.push(a.into());
        }
        self.stream = Some(self.ffmpeg.stream(&args, self.frame_len())?);
        for _ in 0..skip {
            if let Some(stream) = &mut self.stream
                && stream.next_chunk().is_err()
            {
                self.stream = None;
            }
            self.pos += 1;
        }
        Ok(())
    }

    /// The next frame, as `VideoRendererFFMPEG.read_frame` returns it.
    pub(crate) fn read_frame(&mut self) -> Result<Raster> {
        if self.pos == self.num_frames {
            self.initialize(0)?;
        }
        let result = match &mut self.stream {
            None => self
                .last_read
                .clone()
                .ok_or_else(|| MediaError::damaged("Unable to render that video!"))?,
            Some(stream) => {
                let chunk = stream.next_chunk()?;
                if chunk.len() == self.frame_len() {
                    let raster =
                        Raster::new(self.target.0, self.target.1, self.depth as u8, chunk)?;
                    self.last_read = Some(raster.clone());
                    raster
                } else {
                    if self.pos == 1 && !self.tried_explicit_mapping {
                        // we probably picked the still-image track of an avifs/heifs
                        self.tried_explicit_mapping = true;
                        let lines = self.ffmpeg.info_lines(self.path, None)?;
                        let (_, _, mapping) = parse::video_format(&lines)?;
                        self.explicit_mapping = Some(mapping);
                        self.initialize(0)?;
                        return self.read_frame();
                    }
                    if let Some(last) = self.last_read.clone() {
                        self.stream = None;
                        last
                    } else {
                        if self.pos != 0 {
                            // could not start mid-video; try from the start
                            self.initialize(0)?;
                            return self.read_frame();
                        }
                        return Err(MediaError::damaged(
                            "Unable to render that video! It is probably just weird/broken.",
                        ));
                    }
                }
            }
        };
        self.pos += 1;
        Ok(result)
    }
}
