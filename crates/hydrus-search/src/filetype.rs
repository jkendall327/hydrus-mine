//! Filetype sets for `system:filetype`.
//!
//! A filetype predicate holds a set of [`Mime`]s that may mix concrete types
//! (`jpeg`) with general classes (`image`). Like the reference implementation,
//! the set is kept in *summary* form: whenever it contains every searchable
//! member of a class, those members are replaced by the class. So
//! `jpeg, png, ..., jxl` and `image` are the same predicate.

use std::collections::BTreeSet;
use std::fmt;

use hydrus_core::Mime;

/// The file types that can be searched for. Types outside this list (e.g.
/// repository update files) are never matched by a filetype predicate.
pub const SEARCHABLE_MIMES: &[Mime] = &[
    Mime::ImageJpeg,
    Mime::ImagePng,
    Mime::AnimationGif,
    Mime::ImageBmp,
    Mime::ApplicationFlash,
    Mime::ImageIcon,
    Mime::VideoFlv,
    Mime::ApplicationPdf,
    Mime::ApplicationZip,
    Mime::AudioMp3,
    Mime::VideoMp4,
    Mime::AudioOgg,
    Mime::AudioFlac,
    Mime::AudioWma,
    Mime::VideoWmv,
    Mime::VideoMkv,
    Mime::VideoWebm,
    Mime::AnimationApng,
    Mime::VideoMpeg,
    Mime::VideoMov,
    Mime::VideoAvi,
    Mime::ApplicationRar,
    Mime::Application7z,
    Mime::ImageWebp,
    Mime::ImageTiff,
    Mime::ApplicationPsd,
    Mime::AudioM4a,
    Mime::VideoRealmedia,
    Mime::AudioRealmedia,
    Mime::AudioTrueaudio,
    Mime::ApplicationClip,
    Mime::AudioWave,
    Mime::VideoOgv,
    Mime::AudioMkv,
    Mime::AudioMp4,
    Mime::AudioWavpack,
    Mime::ApplicationSai2,
    Mime::ApplicationKrita,
    Mime::ImageSvg,
    Mime::ApplicationXcf,
    Mime::ApplicationGzip,
    Mime::ImageHeif,
    Mime::ImageHeifSequence,
    Mime::ImageHeic,
    Mime::ImageHeicSequence,
    Mime::ImageAvif,
    Mime::ImageAvifSequence,
    Mime::ImageGif,
    Mime::ApplicationProcreate,
    Mime::ImageQoi,
    Mime::ApplicationEpub,
    Mime::ApplicationDjvu,
    Mime::ApplicationCbz,
    Mime::AnimationUgoira,
    Mime::ApplicationRtf,
    Mime::ApplicationDocx,
    Mime::ApplicationXlsx,
    Mime::ApplicationPptx,
    Mime::ApplicationDoc,
    Mime::ApplicationXls,
    Mime::ApplicationPpt,
    Mime::AnimationWebp,
    Mime::ImageJxl,
    Mime::ApplicationPaintDotNet,
    Mime::AnimationJxl,
    Mime::ImageOpenraster,
];

/// The general classes, in the order the reference summarises them.
const GENERAL_CLASSES: &[Mime] = &[
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
    SEARCHABLE_MIMES.contains(&mime)
}

/// The words `system:filetype` accepts, in the reference's order, and the
/// types each one means. Matching is against lowercased input, so the one
/// entry with capitals can never match (as in the reference).
pub(crate) const FILETYPE_WORDS: &[(&str, &[Mime])] = &[
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::fixture;

    #[test]
    fn filetype_words_match_reference_implementation() {
        let expected = fixture("system_predicates.json")["filetype_words"].clone();
        let ours: Vec<serde_json::Value> = FILETYPE_WORDS
            .iter()
            .map(|(word, mimes)| {
                let codes: Vec<u8> = mimes.iter().map(|m| m.code()).collect();
                serde_json::json!([word, codes])
            })
            .collect();
        assert_eq!(serde_json::Value::Array(ours), expected);
    }

    #[test]
    fn searchable_mimes_match_reference_implementation() {
        let constants = fixture("constants.json");
        let expected: BTreeSet<u8> = constants["mimes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["searchable"] == true)
            .map(|m| u8::try_from(m["id"].as_u64().unwrap()).unwrap())
            .collect();
        let ours: BTreeSet<u8> = SEARCHABLE_MIMES.iter().map(|m| m.code()).collect();
        assert_eq!(ours, expected);
        assert_eq!(ours.len(), SEARCHABLE_MIMES.len(), "no duplicates");
    }

    #[test]
    fn general_classes_match_reference_implementation() {
        let constants = fixture("constants.json");
        let groups = constants["general_mimetypes_to_mime_groups"]
            .as_object()
            .unwrap();
        let expected: BTreeSet<u8> = groups.keys().map(|k| k.parse().unwrap()).collect();
        let ours: BTreeSet<u8> = GENERAL_CLASSES.iter().map(|m| m.code()).collect();
        assert_eq!(ours, expected);
        for (class, members) in groups {
            let class = Mime::from_code(class.parse().unwrap()).unwrap();
            let expected: BTreeSet<u8> = members
                .as_array()
                .unwrap()
                .iter()
                .map(|m| u8::try_from(m.as_u64().unwrap()).unwrap())
                .collect();
            let ours: BTreeSet<u8> = Mime::members_of_class(class).map(Mime::code).collect();
            assert_eq!(ours, expected, "{class:?}");
        }
    }

    #[test]
    fn complete_classes_are_summarised() {
        let images: Vec<Mime> = Mime::members_of_class(Mime::GeneralImage)
            .filter(|m| is_searchable(*m))
            .collect();
        let set = FiletypeSet::new(images.iter().copied().chain([Mime::VideoMp4]));
        assert_eq!(
            set.summary().iter().copied().collect::<Vec<_>>(),
            vec![Mime::VideoMp4, Mime::GeneralImage]
        );
        assert_eq!(
            FiletypeSet::new([Mime::GeneralImage]).specific_mimes(),
            images.into_iter().collect()
        );
    }

    #[test]
    fn partial_classes_stay_specific() {
        let set = FiletypeSet::new([Mime::ImageJpeg, Mime::ImagePng]);
        assert_eq!(set.summary().len(), 2);
        assert_eq!(set.to_string(), "jpeg, png");
    }
}
