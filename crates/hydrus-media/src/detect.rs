//! File type detection from content (`HydrusFileHandling.GetMime`).
//!
//! A table of magic bytes decides most types. Some matches need a closer
//! look: zips (cbz, ugoira, krita, epub, Office, procreate), PNG vs APNG,
//! static vs animated GIF/WebP/JXL, OLE documents, and ISO/ASF containers
//! whose audio/video nature ffmpeg decides. Anything unrecognised is offered
//! to ffmpeg last, since it false-positives on arbitrary data.

use std::io::Read;
use std::path::Path;

use hydrus_core::Mime;

use crate::error::{MediaError, Result};
use crate::ffmpeg::{Ffmpeg, parse};
use crate::formats::{apng, archive, gif, ole, webp};
use crate::text;

/// What a header match means before any closer inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Exact(Mime),
    Png,
    Gif,
    Webp,
    Jxl,
    /// mp4-family or ASF: ffmpeg decides audio vs video.
    FfmpegDecides,
    Zip,
    Ole,
}

type Rule = (
    &'static [(&'static [usize], &'static [&'static [u8]])],
    Kind,
);

/// `headers_and_mime`, in the reference's order (order matters: krita,
/// openraster and epub are zips too).
const HEADERS: &[Rule] = &[
    (&[(&[0], &[b"\xff\xd8"])], Kind::Exact(Mime::ImageJpeg)),
    (&[(&[0], &[b"\x89PNG"])], Kind::Png),
    (&[(&[0], &[b"GIF87a", b"GIF89a"])], Kind::Gif),
    (&[(&[8], &[b"WEBP"])], Kind::Webp),
    (
        &[(&[0], &[b"II*\x00", b"MM\x00*"])],
        Kind::Exact(Mime::ImageTiff),
    ),
    (&[(&[0], &[b"BM"])], Kind::Exact(Mime::ImageBmp)),
    (
        &[(&[0], &[b"\x00\x00\x01\x00", b"\x00\x00\x02\x00"])],
        Kind::Exact(Mime::ImageIcon),
    ),
    (&[(&[0], &[b"qoif"])], Kind::Exact(Mime::ImageQoi)),
    (
        &[(&[0], &[b"\xFF\x0A", b"\0\0\0\x0CJXL \x0D\x0A\x87\x0A"])],
        Kind::Jxl,
    ),
    (
        &[(&[0], &[b"CWS", b"FWS", b"ZWS"])],
        Kind::Exact(Mime::ApplicationFlash),
    ),
    (&[(&[0], &[b"FLV"])], Kind::Exact(Mime::VideoFlv)),
    (&[(&[0], &[b"%PDF"])], Kind::Exact(Mime::ApplicationPdf)),
    (
        &[(&[0], &[b"8BPS\x00\x01", b"8BPS\x00\x02"])],
        Kind::Exact(Mime::ApplicationPsd),
    ),
    (
        &[(&[0], &[b"CSFCHUNK"])],
        Kind::Exact(Mime::ApplicationClip),
    ),
    (
        &[(&[0], &[b"SAI-CANVAS"])],
        Kind::Exact(Mime::ApplicationSai2),
    ),
    (
        &[(&[0], &[b"gimp xcf "])],
        Kind::Exact(Mime::ApplicationXcf),
    ),
    (
        &[(&[38, 42, 58, 63], &[b"application/x-krita"])],
        Kind::Exact(Mime::ApplicationKrita),
    ),
    (
        &[(&[38, 42, 58, 63], &[b"image/openraster"])],
        Kind::Exact(Mime::ImageOpenraster),
    ),
    (
        &[(&[0], &[b"PDN3"])],
        Kind::Exact(Mime::ApplicationPaintDotNet),
    ),
    (
        &[(&[38, 43], &[b"application/epub+zip"])],
        Kind::Exact(Mime::ApplicationEpub),
    ),
    (
        &[
            (&[4], &[b"FORM"]),
            (&[12], &[b"DJVU", b"DJVM", b"PM44", b"BM44", b"SDJV"]),
        ],
        Kind::Exact(Mime::ApplicationDjvu),
    ),
    (&[(&[0], &[b"{\\rtf"])], Kind::Exact(Mime::ApplicationRtf)),
    (
        &[(&[0], &[b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"])],
        Kind::Zip,
    ),
    (
        &[(&[0], &[b"7z\xBC\xAF\x27\x1C"])],
        Kind::Exact(Mime::Application7z),
    ),
    (
        &[(
            &[0],
            &[
                b"\x52\x61\x72\x21\x1A\x07\x00",
                b"\x52\x61\x72\x21\x1A\x07\x01\x00",
            ],
        )],
        Kind::Exact(Mime::ApplicationRar),
    ),
    (
        &[(&[0], &[b"\x1f\x8b"])],
        Kind::Exact(Mime::ApplicationGzip),
    ),
    (
        &[(&[0], &[b"hydrus encrypted zip"])],
        Kind::Exact(Mime::ApplicationHydrusEncryptedZip),
    ),
    (&[(&[4], &[b"ftypavif"])], Kind::Exact(Mime::ImageAvif)),
    (
        &[(&[4], &[b"ftypavis"])],
        Kind::Exact(Mime::ImageAvifSequence),
    ),
    (
        &[(&[4], &[b"ftypmif1"]), (&[16, 20, 24], &[b"avif"])],
        Kind::Exact(Mime::ImageAvif),
    ),
    (
        &[(&[4], &[b"ftypheic", b"ftypheix", b"ftypheim", b"ftypheis"])],
        Kind::Exact(Mime::ImageHeic),
    ),
    (
        &[(&[4], &[b"ftyphevc", b"ftyphevx", b"ftyphevm", b"ftyphevs"])],
        Kind::Exact(Mime::ImageHeicSequence),
    ),
    (&[(&[4], &[b"ftypmif1"])], Kind::Exact(Mime::ImageHeif)),
    (
        &[(&[4], &[b"ftypmsf1"])],
        Kind::Exact(Mime::ImageHeifSequence),
    ),
    (
        &[(
            &[4],
            &[
                b"ftypmp4",
                b"ftypisom",
                b"ftypM4V",
                b"ftypMSNV",
                b"ftypavc1",
                b"ftypFACE",
                b"ftypdash",
            ],
        )],
        Kind::FfmpegDecides,
    ),
    (&[(&[4], &[b"ftypqt"])], Kind::Exact(Mime::VideoMov)),
    (&[(&[0], &[b"fLaC"])], Kind::Exact(Mime::AudioFlac)),
    (
        &[(&[0], &[b"RIFF"]), (&[8], &[b"WAVE"])],
        Kind::Exact(Mime::AudioWave),
    ),
    (&[(&[0], &[b"wvpk"])], Kind::Exact(Mime::AudioWavpack)),
    (&[(&[8], &[b"AVI "])], Kind::Exact(Mime::VideoAvi)),
    (
        &[(
            &[0],
            &[b"\x30\x26\xB2\x75\x8E\x66\xCF\x11\xA6\xD9\x00\xAA\x00\x62\xCE\x6C"],
        )],
        Kind::FfmpegDecides,
    ),
    (
        &[(&[0], &[b"\x4D\x5A\x90\x00\x03"])],
        Kind::Exact(Mime::ApplicationWindowsExe),
    ),
    (
        &[(
            &[0],
            &[
                b"\x31\xbe\x00\x00",
                b"PO^Q",
                b"\xfe\x37\x00\x23",
                b"\xdb\xa5-\x00\x00\x00",
                b"\xDB\xA5\x2D\x00",
            ],
        )],
        Kind::Exact(Mime::ApplicationDoc),
    ),
    (
        &[(&[0], &[b"\xED\xDE\xAD\x0B", b"\x0B\xAD\xDE\xAD"])],
        Kind::Exact(Mime::ApplicationPpt),
    ),
    (&[(&[0], &[b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1"])], Kind::Ole),
];

fn matches(rule: &[(&[usize], &[&[u8]])], head: &[u8]) -> bool {
    rule.iter().all(|(offsets, patterns)| {
        offsets.iter().any(|&o| {
            patterns
                .iter()
                .any(|p| head.get(o..o + p.len()).is_some_and(|s| s == *p))
        })
    })
}

fn read_head(path: &Path, n: usize) -> Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    let mut head = Vec::with_capacity(n);
    (&mut f).take(n as u64).read_to_end(&mut head)?;
    Ok(head)
}

/// Detect a file's type from its content.
pub(crate) fn detect(ffmpeg: &Ffmpeg, path: &Path, look_for_hydrus_updates: bool) -> Result<Mime> {
    let size = std::fs::metadata(path)?.len();
    if size == 0 {
        return Err(MediaError::ZeroSize);
    }
    if look_for_hydrus_updates
        && size < 64 * 1024 * 1024
        && let Some(m) = hydrus_update_mime(&std::fs::read(path)?)
    {
        return Ok(m);
    }
    let head = read_head(path, 256)?;
    if let Some((_, kind)) = HEADERS.iter().find(|(rule, _)| matches(rule, &head)) {
        return Ok(match *kind {
            Kind::Exact(m) => m,
            Kind::Zip => zip_mime(path),
            Kind::FfmpegDecides => mime_from_ffmpeg(ffmpeg, path)?,
            Kind::Png => {
                if apng::is_animated(&head) {
                    Mime::AnimationApng
                } else {
                    Mime::ImagePng
                }
            }
            Kind::Gif => {
                if gif::is_animated(&std::fs::read(path)?) {
                    Mime::AnimationGif
                } else {
                    Mime::ImageGif
                }
            }
            Kind::Jxl => {
                if ffmpeg.file_is_animated(path) {
                    Mime::AnimationJxl
                } else {
                    Mime::ImageJxl
                }
            }
            Kind::Webp => {
                if webp_is_animated(&std::fs::read(path)?) {
                    Mime::AnimationWebp
                } else {
                    Mime::ImageWebp
                }
            }
            Kind::Ole => ole::mime(path),
        });
    }
    if (head.starts_with(b"{") || head.starts_with(b"["))
        && text::looks_like_json(&std::fs::read(path)?)
    {
        return Ok(Mime::ApplicationJson);
    }
    if text::looks_like_html(&head) {
        return Ok(Mime::TextHtml);
    }
    if text::looks_like_svg(&head) {
        return Ok(Mime::ImageSvg);
    }
    // ffmpeg calls text files mpegs, so it goes last and skips obvious text
    let name = path.as_os_str().to_string_lossy();
    if !(name.ends_with(".txt") || name.ends_with(".log") || name.ends_with(".json"))
        && let Ok(mime) = mime_from_ffmpeg(ffmpeg, path)
    {
        return Ok(mime);
    }
    Ok(Mime::ApplicationUnknown)
}

/// Pillow's `is_animated` for WebP: more than one animation frame.
fn webp_is_animated(data: &[u8]) -> bool {
    image_webp::WebPDecoder::new(std::io::Cursor::new(data)).is_ok()
        && webp::frame_durations_ms(data).len() > 1
}

/// The zip branch of `GetMime`.
fn zip_mime(path: &Path) -> Mime {
    let Some(mut zip) = archive::Zip::open(path) else {
        return Mime::ApplicationZip;
    };
    if zip.is_encrypted() {
        return Mime::ApplicationZip;
    }
    if let Some(m) = archive::open_document_mime(&mut zip) {
        return m;
    }
    if let Some(m) = archive::office_mime(&mut zip) {
        return m;
    }
    if archive::looks_like_procreate(&mut zip) {
        return Mime::ApplicationProcreate;
    }
    if archive::looks_like_ugoira(&mut zip) {
        return Mime::AnimationUgoira;
    }
    if archive::looks_like_cbz(&mut zip) {
        return Mime::ApplicationCbz;
    }
    Mime::ApplicationZip
}

/// `GetMimeFromFFMPEG`.
pub(crate) fn mime_from_ffmpeg(ffmpeg: &Ffmpeg, path: &Path) -> Result<Mime> {
    let lines = ffmpeg.info_lines(path, None)?;
    let (has_video, video_format, _) = parse::video_format(&lines)?;
    let audio = parse::audio(&lines);
    let has_audio = audio.found;
    if !(has_video || has_audio) {
        return Ok(Mime::ApplicationUnknown);
    }
    let Ok(mime_text) = parse::mime_text(&lines) else {
        return Ok(Mime::ApplicationUnknown);
    };
    let t = mime_text.as_str();
    let mime = if t.contains("matroska") || t.contains("webm") {
        let webm_video = has_video
            && ["vp8", "vp9", "av1"]
                .iter()
                .any(|f| video_format.contains(f));
        let webm_audio = !has_audio || ["vorbis", "opus"].iter().any(|f| audio.format.contains(f));
        if webm_video && webm_audio {
            Mime::VideoWebm
        } else if has_video {
            Mime::VideoMkv
        } else {
            Mime::AudioMkv
        }
    } else if matches!(t, "mpeg" | "mpegvideo" | "mpegts") {
        Mime::VideoMpeg
    } else if t == "flac" {
        Mime::AudioFlac
    } else if t == "wav" {
        Mime::AudioWave
    } else if t == "mp3" {
        Mime::AudioMp3
    } else if t == "tta" {
        Mime::AudioTrueaudio
    } else if t.contains("mp4") {
        match parse::metadata_container(&lines).as_str() {
            "M4A" => Mime::AudioM4a,
            "qt" => Mime::VideoMov,
            "isom" | "mp42" if has_video => Mime::VideoMp4,
            "isom" | "mp42" => Mime::AudioMp4,
            _ if has_audio && video_format.contains("mjpeg") => Mime::AudioM4a,
            _ if has_video => Mime::VideoMp4,
            _ => Mime::AudioMp4,
        }
    } else if t == "ogg" {
        if has_video {
            Mime::VideoOgv
        } else {
            Mime::AudioOgg
        }
    } else if t.contains("rm") {
        if parse::has_video(&lines)? {
            Mime::VideoRealmedia
        } else {
            Mime::AudioRealmedia
        }
    } else if t == "asf" {
        if parse::has_video(&lines)? {
            Mime::VideoWmv
        } else {
            Mime::AudioWma
        }
    } else if t == "wv" {
        Mime::AudioWavpack
    } else {
        Mime::ApplicationUnknown
    };
    Ok(mime)
}

/// Repository update files: zlib-compressed JSON serialisable tuples of type
/// 34 (content update) or 36 (definitions update). This checks the shape of
/// the tuple, not every field the reference's deserialiser would.
fn hydrus_update_mime(data: &[u8]) -> Option<Mime> {
    let mut json = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut json)
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&json).ok()?;
    let tuple = value.as_array()?;
    if !(tuple.len() == 3 || tuple.len() == 4) {
        return None;
    }
    match tuple.first()?.as_u64()? {
        34 => Some(Mime::ApplicationHydrusUpdateContent),
        36 => Some(Mime::ApplicationHydrusUpdateDefinitions),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_rules() {
        let find = |head: &[u8]| {
            HEADERS
                .iter()
                .find(|(r, _)| matches(r, head))
                .map(|(_, k)| *k)
        };
        assert_eq!(
            find(b"\xff\xd8\xff\xe0"),
            Some(Kind::Exact(Mime::ImageJpeg))
        );
        let mut djvu = b"AT&TFORM\0\0\0\0DJVU".to_vec();
        djvu.resize(40, 0);
        assert_eq!(find(&djvu), Some(Kind::Exact(Mime::ApplicationDjvu)));
        let mut avif = b"\0\0\0\x1cftypmif1\0\0\0\0mif1avif".to_vec();
        avif.resize(40, 0);
        assert_eq!(find(&avif), Some(Kind::Exact(Mime::ImageAvif)));
        let mut heif = b"\0\0\0\x1cftypmif1\0\0\0\0mif1heic".to_vec();
        heif.resize(40, 0);
        assert_eq!(find(&heif), Some(Kind::Exact(Mime::ImageHeif)));
        assert_eq!(find(b"nothing here"), None);
    }
}
