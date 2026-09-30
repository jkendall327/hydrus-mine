//! Filetype sets for `system:filetype`.
//!
//! A filetype predicate holds a set of [`Mime`]s that may mix concrete types
//! (`jpeg`) with general classes (`image`). Like the reference implementation,
//! the set is kept in *summary* form: whenever it contains every searchable
//! member of a class, those members are replaced by the class. So
//! `jpeg, png, ..., jxl` and `image` are the same predicate.

use std::collections::BTreeSet;
use std::fmt;

use crate::Mime;

pub use crate::mime::SEARCHABLE_MIMES;

/// The general classes, in the order the reference summarises them.
pub const GENERAL_CLASSES: &[Mime] = &[
    Mime::GeneralImage,
    Mime::GeneralAnimation,
    Mime::GeneralVideo,
    Mime::GeneralAudio,
    Mime::GeneralApplication,
    Mime::GeneralImageProject,
    Mime::GeneralApplicationArchive,
];

/// Whether files of this type can be found by a filetype search.
pub fn is_searchable(mime: Mime) -> bool {
    mime.is_searchable()
}

/// The words `system:filetype` accepts, in the reference's order, and the
/// types each one means. Matching is against lowercased input, so the one
/// entry with capitals can never match (as in the reference).
pub const FILETYPE_WORDS: &[(&str, &[Mime])] = &[
    ("collection", &[Mime::ApplicationHydrusClientCollection]),
    ("image/jpe", &[Mime::ImageJpeg]),
    ("image/jpeg", &[Mime::ImageJpeg]),
    ("image/jpg", &[Mime::ImageJpeg]),
    ("image/x-png", &[Mime::ImagePng]),
    ("image/png", &[Mime::ImagePng]),
    ("image/apng", &[Mime::AnimationApng]),
    ("image/gif", &[Mime::AnimationGif]),
    ("image/bmp", &[Mime::ImageBmp]),
    ("image/webp", &[Mime::ImageWebp]),
    ("image/tiff", &[Mime::ImageTiff]),
    ("image/qoi", &[Mime::ImageQoi]),
    ("image/x-icon", &[Mime::ImageIcon]),
    ("image/svg+xml", &[Mime::ImageSvg]),
    ("image/heif", &[Mime::ImageHeif]),
    ("image/heif-sequence", &[Mime::ImageHeifSequence]),
    ("image/heic", &[Mime::ImageHeic]),
    ("image/heic-sequence", &[Mime::ImageHeicSequence]),
    ("image/avif", &[Mime::ImageAvif]),
    ("image/avif-sequence", &[Mime::ImageAvifSequence]),
    ("image/jxl", &[Mime::ImageJxl]),
    ("image/vnd.microsoft.icon", &[Mime::ImageIcon]),
    ("image", &[Mime::GeneralImage]),
    ("application/x-shockwave-flash", &[Mime::ApplicationFlash]),
    ("application/x-photoshop", &[Mime::ApplicationPsd]),
    ("image/vnd.adobe.photoshop", &[Mime::ApplicationPsd]),
    ("application/vnd.adobe.photoshop", &[Mime::ApplicationPsd]),
    ("application/clip", &[Mime::ApplicationClip]),
    ("application/sai2", &[Mime::ApplicationSai2]),
    ("application/x-krita", &[Mime::ApplicationKrita]),
    ("image/openraster", &[Mime::ImageOpenraster]),
    ("image/vnd.paint.net", &[Mime::ApplicationPaintDotNet]),
    ("application/x-paintnet", &[Mime::ApplicationPaintDotNet]),
    ("application/x-procreate", &[Mime::ApplicationProcreate]),
    ("image/x-xcf", &[Mime::ApplicationXcf]),
    ("application/octet-stream", &[Mime::ApplicationOctetStream]),
    ("application/x-yaml", &[Mime::ApplicationYaml]),
    ("PDF document", &[Mime::ApplicationPdf]),
    ("application/pdf", &[Mime::ApplicationPdf]),
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        &[Mime::ApplicationDocx],
    ),
    (
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        &[Mime::ApplicationXlsx],
    ),
    (
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        &[Mime::ApplicationPptx],
    ),
    ("application/msword", &[Mime::ApplicationDoc]),
    ("application/vnd.ms-word", &[Mime::ApplicationDoc]),
    ("application/vnd.ms-excel", &[Mime::ApplicationXls]),
    ("application/msexcel", &[Mime::ApplicationXls]),
    ("application/vnd.ms-powerpoint", &[Mime::ApplicationPpt]),
    ("application/powerpoint", &[Mime::ApplicationPpt]),
    ("application/mspowerpoint", &[Mime::ApplicationPpt]),
    ("application/epub+zip", &[Mime::ApplicationEpub]),
    ("image/vnd.djvu", &[Mime::ApplicationDjvu]),
    ("image/vnd.djvu+multipage", &[Mime::ApplicationDjvu]),
    ("image/x-djvu", &[Mime::ApplicationDjvu]),
    ("text/rtf", &[Mime::ApplicationRtf]),
    ("application/rtf", &[Mime::ApplicationRtf]),
    ("application/vnd.comicbook+zip", &[Mime::ApplicationCbz]),
    ("application/zip", &[Mime::ApplicationZip]),
    ("application/vnd.rar", &[Mime::ApplicationRar]),
    ("application/x-7z-compressed", &[Mime::Application7z]),
    ("application/gzip", &[Mime::ApplicationGzip]),
    ("application/json", &[Mime::ApplicationJson]),
    ("application/cbor", &[Mime::ApplicationCbor]),
    (
        "application/hydrus-encrypted-zip",
        &[Mime::ApplicationHydrusEncryptedZip],
    ),
    (
        "application/hydrus-update-content",
        &[Mime::ApplicationHydrusUpdateContent],
    ),
    (
        "application/hydrus-update-definitions",
        &[Mime::ApplicationHydrusUpdateDefinitions],
    ),
    ("application", &[Mime::GeneralApplication]),
    ("audio/mp4", &[Mime::AudioM4a]),
    ("audio/mp3", &[Mime::AudioMp3]),
    ("audio/ogg", &[Mime::AudioOgg]),
    ("audio/vnd.rn-realaudio", &[Mime::AudioRealmedia]),
    ("audio/x-tta", &[Mime::AudioTrueaudio]),
    ("audio/flac", &[Mime::AudioFlac]),
    ("audio/x-wav", &[Mime::AudioWave]),
    ("audio/wav", &[Mime::AudioWave]),
    ("audio/wave", &[Mime::AudioWave]),
    ("audio/x-ms-wma", &[Mime::AudioWma]),
    ("audio/wavpack", &[Mime::AudioWavpack]),
    ("text/html", &[Mime::TextHtml]),
    ("text/plain", &[Mime::TextPlain]),
    ("video/x-msvideo", &[Mime::VideoAvi]),
    ("video/x-flv", &[Mime::VideoFlv]),
    ("video/quicktime", &[Mime::VideoMov]),
    ("video/mp4", &[Mime::VideoMp4]),
    ("video/mpeg", &[Mime::VideoMpeg]),
    ("video/x-ms-wmv", &[Mime::VideoWmv]),
    ("video/x-matroska", &[Mime::VideoMkv]),
    ("video/ogg", &[Mime::VideoOgv]),
    ("video/vnd.rn-realvideo", &[Mime::VideoRealmedia]),
    ("application/vnd.rn-realmedia", &[Mime::VideoRealmedia]),
    ("video/webm", &[Mime::VideoWebm]),
    ("video", &[Mime::GeneralVideo]),
    ("application/x-ole-storage", &[Mime::UndeterminedOle]),
    ("unknown filetype", &[Mime::ApplicationUnknown]),
    ("jpeg", &[Mime::ImageJpeg]),
    ("png", &[Mime::ImagePng]),
    ("apng", &[Mime::AnimationApng]),
    ("static gif", &[Mime::ImageGif]),
    ("animated gif", &[Mime::AnimationGif]),
    ("bitmap", &[Mime::ImageBmp]),
    ("webp", &[Mime::ImageWebp]),
    ("animated webp", &[Mime::AnimationWebp]),
    ("tiff", &[Mime::ImageTiff]),
    ("qoi", &[Mime::ImageQoi]),
    ("icon", &[Mime::ImageIcon]),
    ("svg", &[Mime::ImageSvg]),
    ("heif", &[Mime::ImageHeif]),
    ("heif sequence", &[Mime::ImageHeifSequence]),
    ("heic", &[Mime::ImageHeic]),
    ("heic sequence", &[Mime::ImageHeicSequence]),
    ("avif", &[Mime::ImageAvif]),
    ("avif sequence", &[Mime::ImageAvifSequence]),
    ("jxl", &[Mime::ImageJxl]),
    ("animated jxl", &[Mime::AnimationJxl]),
    ("ugoira", &[Mime::AnimationUgoira]),
    ("cbz", &[Mime::ApplicationCbz]),
    ("flash", &[Mime::ApplicationFlash]),
    ("yaml", &[Mime::ApplicationYaml]),
    ("json", &[Mime::ApplicationJson]),
    ("cbor", &[Mime::ApplicationCbor]),
    ("pdf", &[Mime::ApplicationPdf]),
    ("docx", &[Mime::ApplicationDocx]),
    ("xlsx", &[Mime::ApplicationXlsx]),
    ("pptx", &[Mime::ApplicationPptx]),
    ("doc", &[Mime::ApplicationDoc]),
    ("xls", &[Mime::ApplicationXls]),
    ("ppt", &[Mime::ApplicationPpt]),
    ("epub", &[Mime::ApplicationEpub]),
    ("djvu", &[Mime::ApplicationDjvu]),
    ("rtf", &[Mime::ApplicationRtf]),
    ("psd", &[Mime::ApplicationPsd]),
    ("clip", &[Mime::ApplicationClip]),
    ("sai2", &[Mime::ApplicationSai2]),
    ("krita", &[Mime::ApplicationKrita]),
    ("ora", &[Mime::ImageOpenraster]),
    ("paint.net", &[Mime::ApplicationPaintDotNet]),
    ("xcf", &[Mime::ApplicationXcf]),
    ("procreate", &[Mime::ApplicationProcreate]),
    ("zip", &[Mime::ApplicationZip]),
    ("rar", &[Mime::ApplicationRar]),
    ("7z", &[Mime::Application7z]),
    ("gzip", &[Mime::ApplicationGzip]),
    ("windows exe", &[Mime::ApplicationWindowsExe]),
    ("m4a", &[Mime::AudioM4a]),
    ("mp3", &[Mime::AudioMp3]),
    ("ogg", &[Mime::AudioOgg]),
    ("flac", &[Mime::AudioFlac]),
    ("matroska audio", &[Mime::AudioMkv]),
    ("mp4 audio", &[Mime::AudioMp4]),
    ("wave", &[Mime::AudioWave]),
    ("realaudio", &[Mime::AudioRealmedia]),
    ("tta", &[Mime::AudioTrueaudio]),
    ("wma", &[Mime::AudioWma]),
    ("wavpack", &[Mime::AudioWavpack]),
    ("html", &[Mime::TextHtml]),
    ("plaintext", &[Mime::TextPlain]),
    ("avi", &[Mime::VideoAvi]),
    ("flv", &[Mime::VideoFlv]),
    ("quicktime", &[Mime::VideoMov]),
    ("mp4", &[Mime::VideoMp4]),
    ("mpeg", &[Mime::VideoMpeg]),
    ("wmv", &[Mime::VideoWmv]),
    ("matroska", &[Mime::VideoMkv]),
    ("ogv", &[Mime::VideoOgv]),
    ("realvideo", &[Mime::VideoRealmedia]),
    ("webm", &[Mime::VideoWebm]),
    ("wma or wmv", &[Mime::UndeterminedWm]),
    ("mp4 with or without audio", &[Mime::UndeterminedMp4]),
    ("png or apng", &[Mime::UndeterminedPng]),
    ("ole file", &[Mime::UndeterminedOle]),
    ("webp with or without animation", &[Mime::UndeterminedWebp]),
    ("jxl with or without animation", &[Mime::UndeterminedJxl]),
    ("archive", &[Mime::GeneralApplicationArchive]),
    ("image project file", &[Mime::GeneralImageProject]),
    ("audio", &[Mime::GeneralAudio]),
    ("animation", &[Mime::GeneralAnimation]),
    ("all files", &[Mime::GeneralFile]),
    ("gif", &[Mime::ImageGif, Mime::AnimationGif]),
];

/// A set of file types for `system:filetype`, in summary form.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct FiletypeSet(BTreeSet<Mime>);

impl FiletypeSet {
    /// Build a set, replacing any complete class of searchable types with the
    /// class itself.
    pub fn new(mimes: impl IntoIterator<Item = Mime>) -> Self {
        let mut remaining: BTreeSet<Mime> = mimes.into_iter().collect();
        let mut summary = BTreeSet::new();
        for &class in GENERAL_CLASSES {
            let members: BTreeSet<Mime> = Mime::members_of_class(class)
                .filter(|m| is_searchable(*m))
                .collect();
            if members.is_subset(&remaining) {
                summary.insert(class);
                remaining.retain(|m| !members.contains(m));
            }
        }
        summary.extend(remaining);
        Self(summary)
    }

    /// The types and classes in the set, as summarised.
    pub fn summary(&self) -> &BTreeSet<Mime> {
        &self.0
    }

    /// Every concrete, searchable type the set stands for.
    pub fn specific_mimes(&self) -> BTreeSet<Mime> {
        let mut out = BTreeSet::new();
        for &mime in &self.0 {
            if mime == Mime::GeneralFile {
                for &class in GENERAL_CLASSES {
                    out.extend(Mime::members_of_class(class));
                }
            } else if mime.is_general_class() {
                out.extend(Mime::members_of_class(mime));
            } else {
                out.insert(mime);
            }
        }
        out.retain(|m| is_searchable(*m));
        out
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for FiletypeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.0.iter().map(|m| m.human_name()).collect();
        f.write_str(&names.join(", "))
    }
}
