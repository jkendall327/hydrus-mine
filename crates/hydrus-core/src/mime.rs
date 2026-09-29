//! File types.
//!
//! The integer codes are part of the on-disk format and the Client API
//! (`filetype_enum`), so they are fixed forever. The string tables mirror the
//! reference implementation and are checked against
//! `oracle/fixtures/constants.json` in the tests below.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! mimes {
    ($( $variant:ident = $code:literal, $human:literal, $mimetype:literal, $ext:expr, $general:expr; )*) => {
        /// A file type, as detected from file content.
        ///
        /// Some variants (`Undetermined*`, `General*`) are not real file types:
        /// they describe detection ambiguity or filetype *classes* used in search.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Mime {
            $( $variant = $code, )*
        }

        impl Mime {
            /// Every known variant, in code order.
            pub const ALL: &'static [Mime] = &[ $( Mime::$variant, )* ];

            /// Look up a file type by its stable integer code.
            pub const fn from_code(code: u8) -> Option<Mime> {
                match code {
                    $( $code => Some(Mime::$variant), )*
                    _ => None,
                }
            }

            /// The short human-readable name, e.g. `"jpeg"` or `"animated gif"`.
            pub const fn human_name(self) -> &'static str {
                match self { $( Mime::$variant => $human, )* }
            }

            /// The MIME type string, e.g. `"image/jpeg"`.
            pub const fn mimetype(self) -> &'static str {
                match self { $( Mime::$variant => $mimetype, )* }
            }

            /// The canonical file extension including the dot, e.g. `".jpg"`.
            pub const fn extension(self) -> Option<&'static str> {
                match self { $( Mime::$variant => $ext, )* }
            }

            /// The general class this type belongs to (image, video, ...), if any.
            pub const fn general_class(self) -> Option<Mime> {
                match self { $( Mime::$variant => $general, )* }
            }
        }
    };
}

mimes! {
    ApplicationHydrusClientCollection = 0, "collection", "collection", Some(".collection"), None;
    ImageJpeg = 1, "jpeg", "image/jpeg", Some(".jpg"), Some(Mime::GeneralImage);
    ImagePng = 2, "png", "image/png", Some(".png"), Some(Mime::GeneralImage);
    AnimationGif = 3, "animated gif", "image/gif", Some(".gif"), Some(Mime::GeneralAnimation);
    ImageBmp = 4, "bitmap", "image/bmp", Some(".bmp"), Some(Mime::GeneralImage);
    ApplicationFlash = 5, "flash", "application/x-shockwave-flash", Some(".swf"), Some(Mime::GeneralApplication);
    ApplicationYaml = 6, "yaml", "application/x-yaml", Some(".yaml"), None;
    ImageIcon = 7, "icon", "image/x-icon", Some(".ico"), Some(Mime::GeneralImage);
    TextHtml = 8, "html", "text/html", Some(".html"), None;
    VideoFlv = 9, "flv", "video/x-flv", Some(".flv"), Some(Mime::GeneralVideo);
    ApplicationPdf = 10, "pdf", "application/pdf", Some(".pdf"), Some(Mime::GeneralApplication);
    ApplicationZip = 11, "zip", "application/zip", Some(".zip"), Some(Mime::GeneralApplicationArchive);
    ApplicationHydrusEncryptedZip = 12, "application/hydrus-encrypted-zip", "application/hydrus-encrypted-zip", Some(".zip.encrypted"), None;
    AudioMp3 = 13, "mp3", "audio/mp3", Some(".mp3"), Some(Mime::GeneralAudio);
    VideoMp4 = 14, "mp4", "video/mp4", Some(".mp4"), Some(Mime::GeneralVideo);
    AudioOgg = 15, "ogg", "audio/ogg", Some(".ogg"), Some(Mime::GeneralAudio);
    AudioFlac = 16, "flac", "audio/flac", Some(".flac"), Some(Mime::GeneralAudio);
    AudioWma = 17, "wma", "audio/x-ms-wma", Some(".wma"), Some(Mime::GeneralAudio);
    VideoWmv = 18, "wmv", "video/x-ms-wmv", Some(".wmv"), Some(Mime::GeneralVideo);
    UndeterminedWm = 19, "wma or wmv", "audio/x-ms-wma or video/x-ms-wmv", None, None;
    VideoMkv = 20, "matroska", "video/x-matroska", Some(".mkv"), Some(Mime::GeneralVideo);
    VideoWebm = 21, "webm", "video/webm", Some(".webm"), Some(Mime::GeneralVideo);
    ApplicationJson = 22, "json", "application/json", Some(".json"), None;
    AnimationApng = 23, "apng", "image/apng", Some(".png"), Some(Mime::GeneralAnimation);
    UndeterminedPng = 24, "png or apng", "image/png or image/apng", None, None;
    VideoMpeg = 25, "mpeg", "video/mpeg", Some(".mpeg"), Some(Mime::GeneralVideo);
    VideoMov = 26, "quicktime", "video/quicktime", Some(".mov"), Some(Mime::GeneralVideo);
    VideoAvi = 27, "avi", "video/x-msvideo", Some(".avi"), Some(Mime::GeneralVideo);
    ApplicationHydrusUpdateDefinitions = 28, "application/hydrus-update-definitions", "application/hydrus-update-definitions", None, None;
    ApplicationHydrusUpdateContent = 29, "application/hydrus-update-content", "application/hydrus-update-content", None, None;
    TextPlain = 30, "plaintext", "text/plain", Some(".txt"), None;
    ApplicationRar = 31, "rar", "application/vnd.rar", Some(".rar"), Some(Mime::GeneralApplicationArchive);
    Application7z = 32, "7z", "application/x-7z-compressed", Some(".7z"), Some(Mime::GeneralApplicationArchive);
    ImageWebp = 33, "webp", "image/webp", Some(".webp"), Some(Mime::GeneralImage);
    ImageTiff = 34, "tiff", "image/tiff", Some(".tiff"), Some(Mime::GeneralImage);
    ApplicationPsd = 35, "psd", "image/vnd.adobe.photoshop", Some(".psd"), Some(Mime::GeneralImageProject);
    AudioM4a = 36, "m4a", "audio/mp4", Some(".m4a"), Some(Mime::GeneralAudio);
    VideoRealmedia = 37, "realvideo", "video/vnd.rn-realvideo", Some(".rm"), Some(Mime::GeneralVideo);
    AudioRealmedia = 38, "realaudio", "audio/vnd.rn-realaudio", Some(".ra"), Some(Mime::GeneralAudio);
    AudioTrueaudio = 39, "tta", "audio/x-tta", Some(".tta"), Some(Mime::GeneralAudio);
    GeneralAudio = 40, "audio", "audio", None, None;
    GeneralImage = 41, "image", "image", None, None;
    GeneralVideo = 42, "video", "video", None, None;
    GeneralApplication = 43, "application", "application", None, None;
    GeneralAnimation = 44, "animation", "animation", None, None;
    ApplicationClip = 45, "clip", "application/clip", Some(".clip"), Some(Mime::GeneralImageProject);
    AudioWave = 46, "wave", "audio/x-wav", Some(".wav"), Some(Mime::GeneralAudio);
    VideoOgv = 47, "ogv", "video/ogg", Some(".ogv"), Some(Mime::GeneralVideo);
    AudioMkv = 48, "matroska audio", "audio/x-matroska", Some(".mkv"), Some(Mime::GeneralAudio);
    AudioMp4 = 49, "mp4 audio", "audio/mp4", Some(".mp4"), Some(Mime::GeneralAudio);
    UndeterminedMp4 = 50, "mp4 with or without audio", "audio/mp4 or video/mp4", None, None;
    ApplicationCbor = 51, "cbor", "application/cbor", None, None;
    ApplicationWindowsExe = 52, "windows exe", "application/octet-stream", Some(".exe"), None;
    AudioWavpack = 53, "wavpack", "audio/wavpack", Some(".wv"), Some(Mime::GeneralAudio);
    ApplicationSai2 = 54, "sai2", "application/sai2", Some(".sai2"), Some(Mime::GeneralImageProject);
    ApplicationKrita = 55, "krita", "application/x-krita", Some(".kra"), Some(Mime::GeneralImageProject);
    ImageSvg = 56, "svg", "image/svg+xml", Some(".svg"), Some(Mime::GeneralImageProject);
    ApplicationXcf = 57, "xcf", "image/x-xcf", Some(".xcf"), Some(Mime::GeneralImageProject);
    ApplicationGzip = 58, "gzip", "application/gzip", Some(".gz"), Some(Mime::GeneralApplicationArchive);
    GeneralApplicationArchive = 59, "archive", "archive", None, None;
    GeneralImageProject = 60, "image project file", "image project file", None, None;
    ImageHeif = 61, "heif", "image/heif", Some(".heif"), Some(Mime::GeneralImage);
    ImageHeifSequence = 62, "heif sequence", "image/heif-sequence", Some(".heifs"), Some(Mime::GeneralAnimation);
    ImageHeic = 63, "heic", "image/heic", Some(".heic"), Some(Mime::GeneralImage);
    ImageHeicSequence = 64, "heic sequence", "image/heic-sequence", Some(".heics"), Some(Mime::GeneralAnimation);
    ImageAvif = 65, "avif", "image/avif", Some(".avif"), Some(Mime::GeneralImage);
    ImageAvifSequence = 66, "avif sequence", "image/avif-sequence", Some(".avifs"), Some(Mime::GeneralAnimation);
    ImageGif = 68, "static gif", "image/gif", Some(".gif"), Some(Mime::GeneralImage);
    ApplicationProcreate = 69, "procreate", "application/x-procreate", Some(".procreate"), Some(Mime::GeneralImageProject);
    ImageQoi = 70, "qoi", "image/qoi", Some(".qoi"), Some(Mime::GeneralImage);
    ApplicationEpub = 71, "epub", "application/epub+zip", Some(".epub"), Some(Mime::GeneralApplication);
    ApplicationDjvu = 72, "djvu", "image/vnd.djvu", Some(".djvu"), Some(Mime::GeneralApplication);
    ApplicationCbz = 73, "cbz", "application/vnd.comicbook+zip", Some(".cbz"), Some(Mime::GeneralApplicationArchive);
    AnimationUgoira = 74, "ugoira", "application/zip", Some(".zip"), Some(Mime::GeneralAnimation);
    ApplicationRtf = 75, "rtf", "application/rtf", Some(".rtf"), Some(Mime::GeneralApplication);
    ApplicationDocx = 76, "docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document", Some(".docx"), Some(Mime::GeneralApplication);
    ApplicationXlsx = 77, "xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", Some(".xlsx"), Some(Mime::GeneralApplication);
    ApplicationPptx = 78, "pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation", Some(".pptx"), Some(Mime::GeneralApplication);
    UndeterminedOle = 79, "ole file", "application/x-ole-storage", None, None;
    ApplicationDoc = 80, "doc", "application/msword", Some(".doc"), Some(Mime::GeneralApplication);
    ApplicationXls = 81, "xls", "application/vnd.ms-excel", Some(".xls"), Some(Mime::GeneralApplication);
    ApplicationPpt = 82, "ppt", "application/vnd.ms-powerpoint", Some(".ppt"), Some(Mime::GeneralApplication);
    AnimationWebp = 83, "animated webp", "image/webp", Some(".webp"), Some(Mime::GeneralAnimation);
    UndeterminedWebp = 84, "webp with or without animation", "image/webp, static or animated", None, None;
    ImageJxl = 85, "jxl", "image/jxl", Some(".jxl"), Some(Mime::GeneralImage);
    ApplicationPaintDotNet = 86, "paint.net", "application/x-paintnet", Some(".pdn"), Some(Mime::GeneralImageProject);
    UndeterminedJxl = 87, "jxl with or without animation", "image/jxl, static or animated", None, None;
    AnimationJxl = 88, "animated jxl", "image/jxl", Some(".jxl"), Some(Mime::GeneralAnimation);
    ImageOpenraster = 89, "ora", "image/openraster", Some(".ora"), Some(Mime::GeneralImageProject);
    GeneralFile = 90, "all files", "file", None, None;
    ApplicationOctetStream = 100, "application/octet-stream", "application/octet-stream", Some(".bin"), None;
    ApplicationUnknown = 101, "unknown filetype", "unknown filetype", None, None;
}

impl Mime {
    /// The stable integer code.
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Whether this is a filetype class (e.g. "image") rather than a concrete type.
    pub const fn is_general_class(self) -> bool {
        matches!(
            self,
            Mime::GeneralAudio
                | Mime::GeneralImage
                | Mime::GeneralVideo
                | Mime::GeneralApplication
                | Mime::GeneralAnimation
                | Mime::GeneralApplicationArchive
                | Mime::GeneralImageProject
                | Mime::GeneralFile
        )
    }

    /// Extension appended to the hash when storing this file on disk.
    ///
    /// Repository update files have no extension at all.
    pub fn storage_extension(self) -> &'static str {
        self.extension().unwrap_or("")
    }

    /// Whether files of this type get a rendered thumbnail (others show a
    /// type icon).
    pub const fn has_thumbnail(self) -> bool {
        matches!(
            self,
            Mime::ImageJpeg
                | Mime::ImagePng
                | Mime::AnimationGif
                | Mime::ImageBmp
                | Mime::ApplicationFlash
                | Mime::ImageIcon
                | Mime::VideoFlv
                | Mime::ApplicationPdf
                | Mime::AudioMp3
                | Mime::VideoMp4
                | Mime::AudioOgg
                | Mime::AudioFlac
                | Mime::AudioWma
                | Mime::VideoWmv
                | Mime::VideoMkv
                | Mime::VideoWebm
                | Mime::AnimationApng
                | Mime::VideoMpeg
                | Mime::VideoMov
                | Mime::VideoAvi
                | Mime::ImageWebp
                | Mime::ImageTiff
                | Mime::ApplicationPsd
                | Mime::AudioM4a
                | Mime::VideoRealmedia
                | Mime::AudioRealmedia
                | Mime::AudioTrueaudio
                | Mime::ApplicationClip
                | Mime::AudioWave
                | Mime::VideoOgv
                | Mime::AudioMkv
                | Mime::AudioMp4
                | Mime::AudioWavpack
                | Mime::ApplicationKrita
                | Mime::ImageSvg
                | Mime::ImageHeif
                | Mime::ImageHeifSequence
                | Mime::ImageHeic
                | Mime::ImageHeicSequence
                | Mime::ImageAvif
                | Mime::ImageAvifSequence
                | Mime::ImageGif
                | Mime::ApplicationProcreate
                | Mime::ImageQoi
                | Mime::ApplicationEpub
                | Mime::ApplicationCbz
                | Mime::AnimationUgoira
                | Mime::ApplicationPptx
                | Mime::AnimationWebp
                | Mime::ImageJxl
                | Mime::ApplicationPaintDotNet
                | Mime::AnimationJxl
                | Mime::ImageOpenraster
        )
    }

    /// All concrete types belonging to a general class.
    pub fn members_of_class(class: Mime) -> impl Iterator<Item = Mime> {
        Mime::ALL
            .iter()
            .copied()
            .filter(move |m| m.general_class() == Some(class))
    }
}

impl fmt::Display for Mime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.human_name())
    }
}

impl Serialize for Mime {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for Mime {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let code = u8::deserialize(d)?;
        Mime::from_code(code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown mime code {code}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::constants;

    #[test]
    fn table_matches_reference_implementation() {
        let fixture = constants();
        let mimes = fixture["mimes"].as_array().unwrap();
        assert_eq!(mimes.len(), Mime::ALL.len(), "variant count drifted");
        for m in mimes {
            let code = u8::try_from(m["id"].as_u64().unwrap()).unwrap();
            let mime = Mime::from_code(code).unwrap_or_else(|| panic!("missing mime {code}"));
            assert_eq!(mime.human_name(), m["string"].as_str().unwrap(), "{mime:?}");
            assert_eq!(mime.mimetype(), m["mimetype"].as_str().unwrap(), "{mime:?}");
            let ext = m["ext"].as_str().filter(|e| !e.is_empty());
            assert_eq!(mime.extension(), ext, "{mime:?}");
            let general = m["general"].as_u64().map(|g| u8::try_from(g).unwrap());
            assert_eq!(mime.general_class().map(Mime::code), general, "{mime:?}");
            assert_eq!(
                mime.has_thumbnail(),
                m["has_thumbnail"].as_bool().unwrap(),
                "{mime:?}"
            );
        }
    }

    #[test]
    fn serde_uses_integer_codes() {
        assert_eq!(serde_json::to_string(&Mime::ImagePng).unwrap(), "2");
        assert_eq!(
            serde_json::from_str::<Mime>("88").unwrap(),
            Mime::AnimationJxl
        );
        assert!(serde_json::from_str::<Mime>("250").is_err());
    }
}
