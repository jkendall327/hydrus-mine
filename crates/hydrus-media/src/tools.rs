//! The import pipeline: [`MediaTools`] ties detection, metadata, hashing,
//! thumbnails, similarity hashes and flags together, mirroring the
//! reference's `FileImportJob.GenerateInfo`.

use std::ffi::OsStr;
use std::path::Path;

use hydrus_core::{Mime, PerceptualHash, Sha256};

use crate::error::{MediaError, Result};
use crate::ffmpeg::video::{VideoRenderer, py_int};
use crate::ffmpeg::{Ffmpeg, parse};
use crate::formats::archive::{self, Zip};
use crate::formats::{apng, clip, flash, gif, isobmff, ole, pdf, pdn, psd, svg, webp};
use crate::hashes::{self, FileHashes};
use crate::imaging::decode::{self, InfoValue, Opened};
use crate::imaging::{Raster, metadata, resample};
use crate::thumbnail::{self, Thumbnail, ThumbnailSpec};
use crate::{blurhash, detect, mimes, phash};

/// What the database stores about a file's content (`GetFileInfo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileInfo {
    pub mime: Mime,
    /// Bytes.
    pub size: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub num_frames: Option<u64>,
    pub has_audio: bool,
    pub num_words: Option<u64>,
}

/// Flags the import job records alongside the file info.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileFlags {
    /// A human-perceptible alpha channel (in any frame, for animations).
    pub has_transparency: bool,
    /// Non-empty EXIF.
    pub has_exif: bool,
    /// An embedded ICC profile.
    pub has_icc_profile: bool,
    /// Text metadata such as PNG comments or generation parameters.
    pub has_human_readable_embedded_metadata: bool,
    /// An XMP packet.
    pub has_xmp: bool,
    /// IPTC fields worth showing.
    pub has_iptc: bool,
    /// The program that made or edited it (a PNG's Software, Creator or
    /// Source text, or a "Created with ..." comment).
    pub has_software_source: bool,
}

/// Everything importing a file computes.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub info: FileInfo,
    pub hashes: FileHashes,
    /// For types that get thumbnails.
    pub thumbnail: Option<Thumbnail>,
    /// Of the thumbnail.
    pub blurhash: Option<String>,
    /// Zero or one hash (the reference keeps a set).
    pub perceptual_hashes: Vec<PerceptualHash>,
    pub pixel_hash: Option<Sha256>,
    pub flags: FileFlags,
}

/// Media operations, configured with how to run ffmpeg. Cheap to clone and
/// safe to share across threads; holds no state between calls.
#[derive(Clone)]
pub struct MediaTools {
    ffmpeg: Ffmpeg,
    icc_reader: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
}
impl std::fmt::Debug for MediaTools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaTools")
            .field("ffmpeg", &self.ffmpeg)
            .finish_non_exhaustive()
    }
}
impl Default for MediaTools {
    fn default() -> Self {
        Self {
            ffmpeg: Ffmpeg::default(),
            icc_reader: std::sync::Arc::new(|| true),
        }
    }
}

fn unsupported(mime: Mime) -> MediaError {
    let reason = match mime {
        Mime::TextHtml => {
            "Looks like HTML -- maybe the client needs to be taught how to recognise this URL and then parse it?"
        }
        Mime::ApplicationJson => {
            "Looks like JSON -- maybe the client needs to be taught how to recognise this URL and then parse it?"
        }
        Mime::ApplicationUnknown => "Unknown filetype!",
        _ => "Filetype is not permitted!",
    };
    MediaError::Unsupported {
        mime,
        reason: reason.into(),
    }
}

/// Mimes whose pixels we get from ffmpeg because no bit-exact decoder is available.
fn decoded_by_ffmpeg(mime: Mime) -> bool {
    matches!(
        mime,
        Mime::ImageAvif | Mime::ImageHeic | Mime::ImageHeif | Mime::ImageJxl
    )
}

impl MediaTools {
    /// Use `ffmpeg` from `PATH`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use a specific ffmpeg setup.
    pub fn with_ffmpeg(ffmpeg: Ffmpeg) -> Self {
        Self {
            ffmpeg,
            ..Self::default()
        }
    }

    /// Use a fixed embedded ICC policy; defaults remain enabled.
    pub fn with_icc_normalisation(self, enabled: bool) -> Self {
        self.with_icc_reader(std::sync::Arc::new(move || enabled))
    }

    /// Read an explicitly owned policy before each image conversion. The
    /// returned Boolean is held unchanged throughout that conversion.
    pub fn with_icc_reader(
        mut self,
        reader: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        self.icc_reader = reader;
        self
    }

    fn raster_from_bytes(&self, bytes: &[u8], strip_alpha: bool) -> Result<Raster> {
        raster_from_bytes_with_icc(bytes, strip_alpha, (self.icc_reader)())
    }

    /// The ffmpeg configuration in use.
    pub fn ffmpeg(&self) -> &Ffmpeg {
        &self.ffmpeg
    }

    /// Detect the file type from content (`GetMime`).
    pub fn detect_mime(&self, path: &Path) -> Result<Mime> {
        detect::detect(&self.ffmpeg, path, false)
    }

    /// Like [`Self::detect_mime`], also recognising repository update files.
    pub fn detect_mime_allowing_updates(&self, path: &Path) -> Result<Mime> {
        detect::detect(&self.ffmpeg, path, true)
    }

    /// Detect the type and read the metadata (`GetFileInfo`).
    pub fn inspect(&self, path: &Path) -> Result<FileInfo> {
        let mime = self.detect_mime(path)?;
        self.inspect_as(path, mime)
    }

    /// Read the metadata of a file whose type is already known.
    pub fn inspect_as(&self, path: &Path, mime: Mime) -> Result<FileInfo> {
        let size = std::fs::metadata(path)?.len();
        if size == 0 {
            return Err(MediaError::ZeroSize);
        }
        if !mimes::is_allowed(mime) {
            return Err(unsupported(mime));
        }
        let mut info = FileInfo {
            mime,
            size,
            width: None,
            height: None,
            duration_ms: None,
            num_frames: None,
            has_audio: mimes::definitely_has_audio(mime),
            num_words: None,
        };
        let set_size = |info: &mut FileInfo, wh: Option<(u32, u32)>| {
            if let Some((w, h)) = wh {
                info.width = Some(w);
                info.height = Some(h);
            }
        };
        match mime {
            Mime::ApplicationCbz | Mime::ApplicationEpub => {
                let size = cover_bytes(path, mime).and_then(|c| decode::image_size(&c).ok());
                set_size(&mut info, size.map(|(w, h)| (w.max(1), h.max(1))));
            }
            Mime::ApplicationClip => {
                let p = clip::properties(&std::fs::read(path)?)?;
                set_size(&mut info, Some((p.width, p.height)));
                info.duration_ms = p.duration_ms.map(py_int);
                info.num_frames = p.num_frames;
            }
            Mime::ApplicationKrita => {
                set_size(
                    &mut info,
                    Zip::open(path).and_then(|mut z| archive::krita_resolution(&mut z)),
                );
            }
            Mime::ImageOpenraster => {
                set_size(
                    &mut info,
                    Zip::open(path).and_then(|mut z| archive::ora_resolution(&mut z)),
                );
            }
            Mime::ApplicationPaintDotNet => {
                set_size(&mut info, Some(pdn::resolution(&std::fs::read(path)?)?));
            }
            Mime::ApplicationProcreate => {
                set_size(
                    &mut info,
                    Zip::open(path).and_then(|mut z| archive::procreate_resolution(&mut z)),
                );
            }
            Mime::ImageSvg => set_size(&mut info, svg::resolution(&std::fs::read(path)?)),
            Mime::ApplicationPdf => {
                if let Some(doc) = pdf::Document::open(std::fs::read(path)?) {
                    info.num_words = Some(doc.word_count());
                    set_size(&mut info, doc.resolution());
                }
            }
            Mime::ApplicationPptx => {
                if let Some(mut z) = Zip::open(path) {
                    set_size(&mut info, archive::pptx_resolution(&mut z));
                    info.num_words = archive::office_word_count(&mut z);
                }
            }
            Mime::ApplicationDocx => {
                info.num_words =
                    Zip::open(path).and_then(|mut z| archive::office_word_count(&mut z));
            }
            Mime::ApplicationDoc | Mime::ApplicationXls | Mime::ApplicationPpt => {
                info.num_words = ole::word_count(path);
            }
            Mime::ApplicationFlash => {
                let p = flash::properties(&std::fs::read(path)?)?;
                set_size(&mut info, Some((p.width, p.height)));
                info.duration_ms = Some(p.duration_ms);
                info.num_frames = Some(p.num_frames);
            }
            Mime::ApplicationPsd => {
                set_size(&mut info, Some(psd::resolution(&std::fs::read(path)?)));
            }
            Mime::AnimationUgoira => Self::ugoira_properties(path, &mut info),
            m if mimes::is_video(m)
                || matches!(
                    m,
                    Mime::ImageHeicSequence
                        | Mime::ImageHeifSequence
                        | Mime::ImageAvifSequence
                        | Mime::AnimationJxl
                ) =>
            {
                let p = self.ffmpeg.video_properties(path)?;
                set_size(&mut info, Some((p.width, p.height)));
                info.duration_ms = Some(p.duration_ms);
                info.num_frames = Some(p.num_frames);
                info.has_audio = p.has_audio;
            }
            Mime::AnimationGif | Mime::AnimationApng | Mime::AnimationWebp => {
                let data = std::fs::read(path)?;
                let (w, h) = decode::image_size(&data)?;
                set_size(&mut info, Some((w.max(1), h.max(1))));
                let (duration, frames) = match mime {
                    Mime::AnimationApng => {
                        let frames =
                            apng::num_frames(&data[..data.len().min(256)]).ok_or_else(|| {
                                MediaError::damaged("This APNG had an unusual file header!")
                            })?;
                        (apng::duration_ms(&data), u64::from(frames))
                    }
                    Mime::AnimationWebp => {
                        let d = webp::frame_durations_ms(&data);
                        (d.iter().sum(), d.len() as u64)
                    }
                    _ => {
                        let d = gif::frame_durations_ms(&gif::parse(&data)?);
                        (d.iter().sum(), d.len() as u64)
                    }
                };
                info.duration_ms = Some(duration);
                info.num_frames = Some(frames);
            }
            m if mimes::is_image(m) => set_size(&mut info, Some(self.image_resolution(path, m)?)),
            m if mimes::is_audio(m) => {
                info.duration_ms = Some(self.ffmpeg.audio_duration_ms(path)?);
            }
            _ => {}
        }
        Ok(info)
    }

    /// `GetImageResolution`: header size after EXIF rotation, at least 1x1.
    fn image_resolution(&self, path: &Path, mime: Mime) -> Result<(u32, u32)> {
        let data = std::fs::read(path)?;
        let size = if mime == Mime::ImageJxl {
            let lines = self.ffmpeg.info_lines(path, None)?;
            parse::video_resolution(&lines, true)
        } else {
            decode::image_size(&data)
        };
        let (w, h) = if let Ok(s) = size {
            s
        } else {
            let r = self.load_image(path, mime)?;
            (r.width(), r.height())
        };
        Ok((w.max(1), h.max(1)))
    }

    /// `GetUgoiraProperties`: from animation.json when usable, else by counting frames.
    fn ugoira_properties(path: &Path, info: &mut FileInfo) {
        let Some(mut zip) = Zip::open(path) else {
            info.width = Some(100);
            info.height = Some(100);
            return;
        };
        let first_frame_size = |zip: &mut Zip| -> Option<(u32, u32)> {
            let first = archive::ugoira_frame_paths(zip)?.into_iter().next()?;
            decode::image_size(&zip.read(&first)?).ok()
        };
        if let Some(delays) = archive::ugoira_json_delays(&mut zip)
            && let Some(size) = first_frame_size(&mut zip)
        {
            let total: f64 = delays.iter().filter_map(serde_json::Value::as_f64).sum();
            if delays.iter().all(serde_json::Value::is_number) {
                info.width = Some(size.0);
                info.height = Some(size.1);
                info.duration_ms = Some(py_int(total));
                info.num_frames = Some(delays.len() as u64);
                return;
            }
        }
        let (w, h) = first_frame_size(&mut zip).unwrap_or((100, 100));
        info.width = Some(w);
        info.height = Some(h);
        info.num_frames = archive::ugoira_zip_frame_paths(&zip).map(|p| p.len() as u64);
    }

    /// A ugoira's frames, in `GetFramePathsUgoira`'s order, opened as
    /// `GeneratePILImage` opens them (EXIF-rotated, RGB or RGBA).
    pub fn ugoira_frames(&self, path: &Path) -> Result<Vec<Raster>> {
        let damaged = || MediaError::damaged("Could not read the ugoira's frames!");
        let mut zip = Zip::open(path).ok_or_else(damaged)?;
        let names = archive::ugoira_frame_paths(&mut zip).ok_or_else(damaged)?;
        names
            .iter()
            .map(|name| self.raster_from_bytes(&zip.read(name).ok_or_else(damaged)?, false))
            .collect()
    }

    /// `GetFrameDurationsMSUgoira`: how long each of a ugoira's frames
    /// shows, in ms: from its animation.json, else its "ugoira json" or
    /// "ugoira frame delay array" note (`notes` are (name, text)), else
    /// 125ms for each of its `num_frames`.
    pub fn ugoira_frame_durations(
        path: &Path,
        notes: &[(String, String)],
        num_frames: Option<u64>,
    ) -> Vec<u32> {
        let ms = |v: &serde_json::Value| {
            v.as_f64()
                .map(|d| d.round().clamp(0.0, f64::from(u32::MAX)) as u32)
        };
        let from_json = Zip::open(path)
            .and_then(|mut zip| archive::ugoira_json_delays(&mut zip))
            .and_then(|delays| delays.iter().map(ms).collect::<Option<Vec<u32>>>());
        if let Some(durations) = from_json {
            return durations;
        }
        if let Some(durations) = Self::ugoira_note_frame_durations(notes) {
            return durations;
        }
        let n = usize::try_from(num_frames.unwrap_or(0).max(1)).unwrap_or(1);
        vec![UGOIRA_DEFAULT_FRAME_DURATION_MS; n]
    }

    /// A ugoira's frame durations from its notes alone
    /// (`GetFrameDurationsMSFromNote`): its "ugoira json" note, else its
    /// "ugoira frame delay array"; `None` if neither reads.
    pub fn ugoira_note_frame_durations(notes: &[(String, String)]) -> Option<Vec<u32>> {
        let ms = |v: &serde_json::Value| {
            v.as_f64()
                .map(|d| d.round().clamp(0.0, f64::from(u32::MAX)) as u32)
        };
        // (`GetFrameDurationsMSFromNote`: a list whose first delay is an int)
        let note = |name: &str| notes.iter().find(|(n, _)| n == name).map(|(_, t)| t);
        let ints = |delays: &[serde_json::Value]| -> Option<Vec<u32>> {
            delays.first().filter(|d| d.is_i64() || d.is_u64())?;
            delays.iter().map(ms).collect()
        };
        if let Some(text) = note("ugoira json")
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(text)
        {
            let frames = match &json {
                serde_json::Value::Array(frames) => Some(frames),
                serde_json::Value::Object(o) => {
                    o.get("frames").and_then(serde_json::Value::as_array)
                }
                _ => None,
            };
            let delays: Option<Vec<serde_json::Value>> =
                frames.and_then(|frames| frames.iter().map(|f| f.get("delay").cloned()).collect());
            if let Some(durations) = delays.as_deref().and_then(ints) {
                return Some(durations);
            }
        }
        if let Some(text) = note("ugoira frame delay array")
            && let Ok(serde_json::Value::Array(delays)) = serde_json::from_str(text)
            && let Some(durations) = ints(&delays)
        {
            return Some(durations);
        }
        None
    }

    /// Whether a ugoira has a note timing its frames (`HasFrameTimesNote`).
    pub fn has_ugoira_frame_times_note(notes: &[(String, String)]) -> bool {
        notes
            .iter()
            .any(|(name, _)| name == "ugoira json" || name == "ugoira frame delay array")
    }

    /// Decode an image file to the array the reference hashes and thumbnails
    /// (`GenerateNumPyImage`): EXIF-rotated, colour-managed to sRGB, RGB or
    /// RGBA, useless alpha removed.
    pub fn load_image(&self, path: &Path, mime: Mime) -> Result<Raster> {
        match mime {
            Mime::ApplicationPsd => {
                let png = self.ffmpeg.render_image_to_png(path)?;
                if png.is_empty() {
                    return Err(MediaError::damaged(
                        "This PSD has no embedded Preview file that FFMPEG can read!",
                    ));
                }
                self.raster_from_bytes(&png, true)
            }
            Mime::ApplicationKrita | Mime::ImageOpenraster => {
                let merged = Zip::open(path)
                    .and_then(|mut z| z.read("mergedimage.png"))
                    .ok_or_else(|| {
                        MediaError::damaged("Could not read mergedimage.png from this file")
                    })?;
                self.raster_from_bytes(&merged, true)
            }
            m if decoded_by_ffmpeg(m) => Ok(self.ffmpeg_still(path)?.strip_useless_alpha()),
            _ => self.raster_from_bytes(&std::fs::read(path)?, true),
        }
    }

    /// A still image rendered by ffmpeg as RGBA.
    fn ffmpeg_still(&self, path: &Path) -> Result<Raster> {
        let lines = self.ffmpeg.info_lines(path, None)?;
        let (w, h) = parse::video_resolution(&lines, true)?;
        // newer ffmpeg gives a HEIF or AVIF image's alpha plane as a stream
        // of its own (older ones drop it): merge it back
        if let Some((primary, alpha)) = std::fs::read(path)
            .ok()
            .and_then(|data| isobmff::alpha_item(&data))
            && lines.iter().any(|l| l.contains(&format!("[{alpha:#x}]")))
        {
            let graph = format!("[0:i:{primary}][0:i:{alpha}]alphamerge");
            let merged = self.ffmpeg.render_to_stdout(&[
                OsStr::new("-i"),
                path.as_os_str(),
                OsStr::new("-filter_complex"),
                OsStr::new(&graph),
                OsStr::new("-frames:v"),
                OsStr::new("1"),
                OsStr::new("-loglevel"),
                OsStr::new("quiet"),
                OsStr::new("-f"),
                OsStr::new("rawvideo"),
                OsStr::new("-pix_fmt"),
                OsStr::new("rgba"),
                OsStr::new("-"),
            ]);
            if let Ok(raster) = merged.and_then(|raw| Raster::new(w, h, 4, raw)) {
                return Ok(raster);
            }
        }
        let raw = self.ffmpeg.render_to_stdout(&[
            OsStr::new("-i"),
            path.as_os_str(),
            OsStr::new("-frames:v"),
            OsStr::new("1"),
            OsStr::new("-loglevel"),
            OsStr::new("quiet"),
            OsStr::new("-f"),
            OsStr::new("rawvideo"),
            OsStr::new("-pix_fmt"),
            OsStr::new("rgba"),
            OsStr::new("-"),
        ])?;
        Raster::new(w, h, 4, raw)
    }

    /// The pixel hash (`GetImagePixelHash`): sha256 of the decoded pixels,
    /// for still images only. `None` when not applicable or not decodable.
    pub fn pixel_hash(&self, path: &Path, info: &FileInfo) -> Option<Sha256> {
        if !mimes::can_have_pixel_hash(info.mime) || info.duration_ms.is_some() {
            return None;
        }
        self.load_image(path, info.mime)
            .ok()
            .map(|r| hashes::sha256(r.data()))
    }

    /// What the reference's "system:similar to data" editor pastes for a
    /// file (`_Paste`): its pixel hash and perceptual hashes; why not, for a
    /// file whose type has none or that can't be read.
    pub fn similar_search_hashes(
        &self,
        path: &Path,
    ) -> std::result::Result<(Sha256, Vec<PerceptualHash>), String> {
        let mime = self.detect_mime(path).map_err(|e| e.to_string())?;
        if !mimes::has_perceptual_hash(mime) {
            return Err(format!(
                "Sorry, \"{}\" files are not compatible with the similar file search system!",
                mime.human_name()
            ));
        }
        let image = self
            .load_image(path, mime)
            .map_err(|e| format!("Sorry, seemed to be a problem: {e}"))?;
        Ok((
            hashes::sha256(image.data()),
            self.perceptual_hashes(path, mime),
        ))
    }

    /// Perceptual hashes for similar-file search (empty on failure, as in the reference).
    pub fn perceptual_hashes(&self, path: &Path, mime: Mime) -> Vec<PerceptualHash> {
        if !mimes::has_perceptual_hash(mime) {
            return Vec::new();
        }
        self.load_image(path, mime)
            .map(|r| vec![phash::perceptual_hash(&r)])
            .unwrap_or_default()
    }

    /// Generate a thumbnail (`GenerateThumbnailNumPy` + encoding).
    ///
    /// Types the reference cannot render, and render failures, get the
    /// type's generic icon at the same size.
    pub fn thumbnail(
        &self,
        path: &Path,
        info: &FileInfo,
        spec: &ThumbnailSpec,
    ) -> Result<Thumbnail> {
        let target = thumbnail::thumbnail_resolution(info.width, info.height, spec)
            .ok_or_else(|| MediaError::damaged("degenerate thumbnail bounding box"))?;
        self.thumbnail_at(path, info, spec, target, None)
    }

    fn thumbnail_at(
        &self,
        path: &Path,
        info: &FileInfo,
        spec: &ThumbnailSpec,
        target: (u32, u32),
        decoded: Option<&Raster>,
    ) -> Result<Thumbnail> {
        let rendered = self.render_thumbnail(path, info, spec, target, decoded);
        let (pixels, is_default) = match rendered {
            Some(p) => (p, false),
            None => (thumbnail::default_thumbnail(info.mime, target)?, true),
        };
        let (format, bytes) = thumbnail::encode(&pixels)?;
        Ok(Thumbnail {
            pixels,
            bytes,
            format,
            is_default,
        })
    }

    fn render_thumbnail(
        &self,
        path: &Path,
        info: &FileInfo,
        spec: &ThumbnailSpec,
        target: (u32, u32),
        decoded: Option<&Raster>,
    ) -> Option<Raster> {
        let mime = info.mime;
        let static_image = |raster: Raster| thumbnail::resize(&raster, target, None);
        // PIL's `image.resize(target, LANCZOS)` then strip useless alpha
        let pil_resize = |bytes: &[u8]| -> Option<Raster> {
            let raster = self.raster_from_bytes(bytes, false).ok()?;
            Some(resample::resize_lanczos(&raster, target.0, target.1).strip_useless_alpha())
        };
        if mimes::is_image(mime) || mime == Mime::AnimationWebp {
            return match decoded {
                Some(r) => Some(static_image(r.clone())),
                None => self.load_image(path, mime).ok().map(static_image),
            };
        }
        if mime == Mime::AnimationUgoira {
            let n = info.num_frames.unwrap_or(0);
            let index = frame_index(spec, n) as usize;
            let mut zip = Zip::open(path)?;
            let name = archive::ugoira_frame_paths(&mut zip)?
                .into_iter()
                .nth(index)?;
            return pil_resize(&zip.read(&name)?);
        }
        if mimes::is_video(mime) || mimes::is_animation(mime) {
            let n = info.num_frames.unwrap_or(0);
            let index = frame_index(spec, n);
            let render = |start: u64| -> Option<Raster> {
                let mut r = VideoRenderer::new(
                    &self.ffmpeg,
                    path,
                    mime,
                    info.duration_ms,
                    n,
                    target,
                    start,
                )
                .ok()?;
                r.read_frame().ok()
            };
            let frame = render(index).or_else(|| if index != 0 { render(0) } else { None })?;
            return Some(thumbnail::resize(&frame, target, None));
        }
        if mimes::is_audio(mime) {
            let png = self.ffmpeg.render_attached_image(path).ok()??;
            return self.raster_from_bytes(&png, true).ok().map(static_image);
        }
        match mime {
            Mime::ApplicationCbz | Mime::ApplicationEpub => {
                let cover = cover_bytes(path, mime)?;
                decode::sniff(&cover)?;
                self.raster_from_bytes(&cover, true).ok().map(static_image)
            }
            Mime::ApplicationClip => {
                let png = clip::preview_png(&std::fs::read(path).ok()?).ok()?;
                self.raster_from_bytes(&png, true).ok().map(static_image)
            }
            Mime::ApplicationKrita | Mime::ImageOpenraster => {
                let mut zip = Zip::open(path)?;
                let fallback = if mime == Mime::ApplicationKrita {
                    "preview.png"
                } else {
                    "Thumbnails/thumbnail.png"
                };
                let merged = zip
                    .read("mergedimage.png")
                    .filter(|b| self.raster_from_bytes(b, false).is_ok());
                let bytes = merged.or_else(|| zip.read(fallback))?;
                pil_resize(&bytes)
            }
            Mime::ApplicationPaintDotNet => {
                pil_resize(&pdn::thumbnail_png(&std::fs::read(path).ok()?)?)
            }
            Mime::ApplicationProcreate => {
                let png = Zip::open(path)?.read("QuickLook/Thumbnail.png")?;
                self.raster_from_bytes(&png, true).ok().map(static_image)
            }
            Mime::ApplicationPsd => match decoded {
                Some(r) => Some(static_image(r.clone())),
                None => self.load_image(path, mime).ok().map(static_image),
            },
            Mime::ApplicationPptx => pil_resize(&Zip::open(path)?.read("docProps/thumbnail.jpeg")?),
            Mime::ImageSvg => {
                svg::render(&std::fs::read(path).ok()?, target).map(Raster::strip_useless_alpha)
            }
            Mime::ApplicationPdf => pdf::Document::open(std::fs::read(path).ok()?)?
                .render_first_page(target)
                .map(Raster::strip_useless_alpha),
            _ => None,
        }
    }

    /// Transparency, EXIF, ICC and text-metadata flags.
    pub fn flags(&self, path: &Path, info: &FileInfo) -> FileFlags {
        self.flags_with(path, info, None)
    }

    fn flags_with(&self, path: &Path, info: &FileInfo, decoded: Option<&Raster>) -> FileFlags {
        let mime = info.mime;
        let mut flags = FileFlags {
            has_transparency: self.has_transparency(path, info, decoded),
            ..FileFlags::default()
        };
        let wants_raw = mimes::can_have_exif(mime)
            || mimes::can_have_icc_profile(mime)
            || mimes::can_have_human_readable_embedded_metadata(mime);
        if !wants_raw {
            return flags;
        }
        let data = std::fs::read(path).unwrap_or_default();
        if isobmff_still(mime) {
            if let Some(item) = isobmff::primary_item(&data) {
                flags.has_icc_profile = mimes::can_have_icc_profile(mime) && item.has_icc;
                flags.has_exif = mime == Mime::ImageAvif && item.rotation != 0;
                flags.has_human_readable_embedded_metadata =
                    mimes::is_pil_heif(mime) && item.has_nclx;
            }
            return flags;
        }
        if mime == Mime::ApplicationPdf {
            flags.has_human_readable_embedded_metadata =
                pdf::Document::open(data).is_some_and(|doc| doc.has_human_readable_metadata());
            return flags;
        }
        if mime == Mime::ApplicationPsd {
            flags.has_icc_profile = psd::has_icc_profile(&data).unwrap_or(false);
            return flags;
        }
        if mime == Mime::ImageJxl {
            // pillow-jxl reports the (synthesised) colour profile of every jxl
            flags.has_icc_profile = decoded.is_some() || self.load_image(path, mime).is_ok();
            return flags;
        }
        if let Ok(opened) = decode::open(&data) {
            let img = &opened.image;
            flags.has_exif = mimes::can_have_exif(mime) && img.exif.has_tags;
            flags.has_icc_profile = mimes::can_have_icc_profile(mime)
                && img.icc_profile.as_ref().is_some_and(|p| !p.is_empty());
            flags.has_human_readable_embedded_metadata =
                mimes::can_have_human_readable_embedded_metadata(mime)
                    && is_human_readable(&opened);
            flags.has_xmp = mimes::can_have_xmp(mime)
                && opened.xmp.as_deref().is_some_and(metadata::xmp_is_readable);
            flags.has_iptc = mimes::can_have_iptc(mime)
                && opened.iptc.as_deref().is_some_and(metadata::has_shown_iptc);
            flags.has_software_source = mimes::can_have_software_source(mime)
                && metadata::has_software_source(&opened.text_info);
        }
        flags
    }

    /// `ClientFiles.HasTransparency`.
    fn has_transparency(&self, path: &Path, info: &FileInfo, decoded: Option<&Raster>) -> bool {
        let mime = info.mime;
        if !mimes::can_check_transparency(mime) {
            return false;
        }
        if mimes::is_image(mime) {
            return match decoded {
                Some(r) => r.has_useful_alpha(),
                None => self
                    .load_image(path, mime)
                    .is_ok_and(|r| r.has_useful_alpha()),
            };
        }
        let (Some(n), Some(w), Some(h)) = (info.num_frames, info.width, info.height) else {
            return false;
        };
        match mime {
            Mime::AnimationGif => std::fs::read(path)
                .ok()
                .and_then(|d| gif::any_frame_has_useful_alpha(&d, n).ok())
                .unwrap_or(false),
            Mime::AnimationWebp => std::fs::read(path)
                .ok()
                .and_then(|d| webp::any_frame_has_useful_alpha(&d, n))
                .unwrap_or(false),
            _ => {
                let Ok(mut renderer) =
                    VideoRenderer::new(&self.ffmpeg, path, mime, info.duration_ms, n, (w, h), 0)
                else {
                    return false;
                };
                for _ in 0..n {
                    match renderer.read_frame() {
                        Ok(frame) if frame.has_useful_alpha() => return true,
                        Ok(frame) if !frame.has_alpha_channel() => return false,
                        Ok(_) => {}
                        Err(_) => return false,
                    }
                }
                false
            }
        }
    }

    /// Everything the import job computes, decoding the file as few times as possible.
    pub fn analyse(&self, path: &Path, spec: &ThumbnailSpec) -> Result<Analysis> {
        let mime = self.detect_mime(path)?;
        let info = self.inspect_as(path, mime)?;
        let hashes = hashes::hash_file(path)?;
        let decoded = if mimes::has_perceptual_hash(mime) || mimes::is_image(mime) {
            self.load_image(path, mime).ok()
        } else {
            None
        };
        let thumbnail = if mimes::has_thumbnail(mime) {
            let target = thumbnail::thumbnail_resolution(info.width, info.height, spec)
                .ok_or_else(|| MediaError::damaged("degenerate thumbnail bounding box"))?;
            Some(self.thumbnail_at(path, &info, spec, target, decoded.as_ref())?)
        } else {
            None
        };
        let blurhash = thumbnail
            .as_ref()
            .and_then(|t| blurhash::blurhash(&t.pixels));
        let perceptual_hashes = match (&decoded, mimes::has_perceptual_hash(mime)) {
            (Some(r), true) => vec![phash::perceptual_hash(r)],
            _ => Vec::new(),
        };
        let pixel_hash = if mimes::can_have_pixel_hash(mime) && info.duration_ms.is_none() {
            decoded.as_ref().map(|r| hashes::sha256(r.data()))
        } else {
            None
        };
        let flags = self.flags_with(path, &info, decoded.as_ref());
        Ok(Analysis {
            info,
            hashes,
            thumbnail,
            blurhash,
            perceptual_hashes,
            pixel_hash,
            flags,
        })
    }
}

fn isobmff_still(mime: Mime) -> bool {
    matches!(mime, Mime::ImageAvif | Mime::ImageHeic | Mime::ImageHeif)
}

/// `int((percentage_in / 100.0) * (num_frames - 1))`, floored at 0.
fn frame_index(spec: &ThumbnailSpec, num_frames: u64) -> u64 {
    let v = (f64::from(spec.video_percentage_in) / 100.0) * (num_frames as f64 - 1.0);
    if v > 0.0 { v.trunc() as u64 } else { 0 }
}

/// Decode image bytes through the full Pillow-equivalent pipeline.
pub(crate) fn raster_from_bytes(data: &[u8], strip_useless_alpha: bool) -> Result<Raster> {
    decode::open(data)?
        .image
        .normalise()?
        .into_raster(strip_useless_alpha)
}

pub(crate) fn raster_from_bytes_with_icc(
    data: &[u8],
    strip_useless_alpha: bool,
    normalise_icc: bool,
) -> Result<Raster> {
    decode::open(data)?
        .image
        .normalise_with_icc(normalise_icc)?
        .into_raster(strip_useless_alpha)
}

/// The cover image of a cbz or epub (`ExtractCoverPage`).
fn cover_bytes(path: &Path, mime: Mime) -> Option<Vec<u8>> {
    let mut zip = Zip::open(path)?;
    let name = if mime == Mime::ApplicationEpub {
        archive::epub_cover_path(&mut zip)?
    } else {
        archive::cover_page_path(&zip)?
    };
    zip.read(&name)
}

/// Info keys the reference never shows as human-readable metadata.
/// A ugoira frame's duration when nothing says (`UGOIRA_DEFAULT_FRAME_DURATION_MS`).
pub const UGOIRA_DEFAULT_FRAME_DURATION_MS: u32 = 125;

pub(crate) const NOT_HUMAN_READABLE: &[&str] = &[
    "exif",
    "Raw profile type exif",
    "icc_profile",
    "progression",
    "progressive",
    "srgb",
    "gamma",
    "chromaticity",
    "dpi",
    "jfif",
    "jfif_unit",
    "jfif_density",
    "jfif_version",
    "compression",
    "resolution",
    "Software",
    "software",
    "adobe",
    "adobe_transform",
    "transparency",
    "background",
    "duration",
    "bit_depth",
    "primary",
    "chroma",
    "loop",
    "photoshop",
    "extension",
    "bbox",
    "blend",
    "disposal",
    "sizes",
    "interlace",
    "aspect",
    "xmp",
    "XML:com.adobe.xmp",
    "iptc",
    "Raw profile type iptc",
    "default_image",
    "distortion",
    "Creator",
    "creator",
    "Source",
    "source",
    "mpoffset",
    "Creation Time",
    "create-date",
    "modify-date",
    "date:create",
    "date:modify",
    "date:timestamp",
    "Thumb::MTime",
];

/// `GetSoftwareSourceFromCommentInfoField` matched: such comments are not counted.
fn is_software_comment(text: &str) -> bool {
    metadata::software_from_comment(text).is_some()
}

/// `HasHumanReadableEmbeddedMetadata` over Pillow's `info` dict.
fn is_human_readable(opened: &Opened) -> bool {
    let mut last: std::collections::BTreeMap<&str, &InfoValue> = std::collections::BTreeMap::new();
    for (k, v) in &opened.text_info {
        last.insert(k.as_str(), v);
    }
    last.into_iter().any(|(k, v)| {
        if NOT_HUMAN_READABLE.contains(&k) || k.starts_with("Thumb::") {
            return false;
        }
        match v {
            InfoValue::Bytes => false,
            InfoValue::Text(t) => !((k == "comment" || k == "Comment") && is_software_comment(t)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn software_comments() {
        assert!(is_software_comment("Created with GIMP"));
        assert!(is_software_comment("edited with something"));
        assert!(!is_software_comment("Created with "));
        assert!(!is_software_comment("a nice picture"));
    }

    #[test]
    fn video_frame_index() {
        let spec = ThumbnailSpec::default();
        assert_eq!(frame_index(&spec, 10), 3);
        assert_eq!(frame_index(&spec, 1), 0);
        assert_eq!(frame_index(&spec, 0), 0);
    }
}
