//! Which file types get which treatment on import.
//!
//! These mirror the reference's `HydrusConstants` groupings (`IMAGES`,
//! `MIMES_WITH_THUMBNAILS`, `FILES_THAT_HAVE_PERCEPTUAL_HASH`, ...) and are
//! checked against `oracle/fixtures/constants.json`.

use hydrus_core::Mime;
use hydrus_core::Mime as M;

/// Still images (`HC.IMAGES`).
pub const IMAGES: &[Mime] = &[
    M::ImageJpeg,
    M::ImagePng,
    M::ImageGif,
    M::ImageWebp,
    M::ImageJxl,
    M::ImageAvif,
    M::ImageBmp,
    M::ImageHeic,
    M::ImageHeif,
    M::ImageIcon,
    M::ImageQoi,
    M::ImageTiff,
];

/// Animations (`HC.ANIMATIONS`, which equals `HC.VIEWABLE_ANIMATIONS`).
pub const ANIMATIONS: &[Mime] = &[
    M::AnimationGif,
    M::AnimationApng,
    M::AnimationWebp,
    M::AnimationJxl,
    M::ImageAvifSequence,
    M::ImageHeicSequence,
    M::ImageHeifSequence,
    M::AnimationUgoira,
];

/// `HC.AUDIO`.
pub const AUDIO: &[Mime] = &[
    M::AudioMp3,
    M::AudioOgg,
    M::AudioFlac,
    M::AudioM4a,
    M::AudioMkv,
    M::AudioMp4,
    M::AudioRealmedia,
    M::AudioTrueaudio,
    M::AudioWave,
    M::AudioWavpack,
    M::AudioWma,
];

/// `HC.VIDEO`.
pub const VIDEO: &[Mime] = &[
    M::VideoMp4,
    M::VideoWebm,
    M::VideoMkv,
    M::VideoAvi,
    M::VideoFlv,
    M::VideoMov,
    M::VideoMpeg,
    M::VideoOgv,
    M::VideoRealmedia,
    M::VideoWmv,
];

/// `HC.APPLICATIONS`.
pub const APPLICATIONS: &[Mime] = &[
    M::ApplicationFlash,
    M::ApplicationPdf,
    M::ApplicationEpub,
    M::ApplicationDjvu,
    M::ApplicationDocx,
    M::ApplicationXlsx,
    M::ApplicationPptx,
    M::ApplicationDoc,
    M::ApplicationXls,
    M::ApplicationPpt,
    M::ApplicationRtf,
];

/// `HC.IMAGE_PROJECT_FILES`.
pub const IMAGE_PROJECT_FILES: &[Mime] = &[
    M::ApplicationClip,
    M::ApplicationKrita,
    M::ImageOpenraster,
    M::ApplicationPaintDotNet,
    M::ApplicationProcreate,
    M::ApplicationPsd,
    M::ApplicationSai2,
    M::ImageSvg,
    M::ApplicationXcf,
];

/// `HC.ARCHIVES`.
pub const ARCHIVES: &[Mime] = &[
    M::ApplicationCbz,
    M::Application7z,
    M::ApplicationGzip,
    M::ApplicationRar,
    M::ApplicationZip,
];

/// Project files whose flattened image we can render (`HC.VIEWABLE_IMAGE_PROJECT_FILES`).
pub const VIEWABLE_IMAGE_PROJECT_FILES: &[Mime] =
    &[M::ApplicationPsd, M::ApplicationKrita, M::ImageOpenraster];

const HEIF_STILLS_AND_SEQUENCES: &[Mime] = &[
    M::ImageHeif,
    M::ImageHeifSequence,
    M::ImageHeic,
    M::ImageHeicSequence,
];

fn is_in(mime: Mime, set: &[Mime]) -> bool {
    set.contains(&mime)
}

/// A still image.
pub fn is_image(mime: Mime) -> bool {
    is_in(mime, IMAGES)
}

/// An animation.
pub fn is_animation(mime: Mime) -> bool {
    is_in(mime, ANIMATIONS)
}

/// Audio.
pub fn is_audio(mime: Mime) -> bool {
    is_in(mime, AUDIO)
}

/// Video.
pub fn is_video(mime: Mime) -> bool {
    is_in(mime, VIDEO)
}

/// Whether the client lets users import this type (`HC.ALLOWED_MIMES`).
pub fn is_allowed(mime: Mime) -> bool {
    is_image(mime)
        || is_animation(mime)
        || is_audio(mime)
        || is_video(mime)
        || is_in(mime, APPLICATIONS)
        || is_in(mime, IMAGE_PROJECT_FILES)
        || is_in(mime, ARCHIVES)
        || matches!(
            mime,
            M::ApplicationHydrusUpdateContent | M::ApplicationHydrusUpdateDefinitions
        )
}

/// Whether imports of this type get a thumbnail (`HC.MIMES_WITH_THUMBNAILS`).
pub fn has_thumbnail(mime: Mime) -> bool {
    is_image(mime)
        || is_animation(mime)
        || is_video(mime)
        || is_audio(mime)
        || matches!(
            mime,
            M::ImageSvg
                | M::ApplicationPdf
                | M::ApplicationFlash
                | M::ApplicationClip
                | M::ApplicationProcreate
                | M::ApplicationCbz
                | M::ApplicationPptx
                | M::ApplicationPaintDotNet
                | M::ApplicationEpub
        )
        || is_in(mime, VIEWABLE_IMAGE_PROJECT_FILES)
}

/// `HC.MIMES_THAT_WE_CAN_CHECK_FOR_TRANSPARENCY`.
pub fn can_check_transparency(mime: Mime) -> bool {
    matches!(
        mime,
        M::ImageJxl
            | M::ImagePng
            | M::ImageGif
            | M::ImageWebp
            | M::ImageBmp
            | M::ImageIcon
            | M::ImageTiff
            | M::ImageQoi
            | M::ImageAvif
            | M::ImageHeif
            | M::ImageHeic
            | M::AnimationGif
            | M::AnimationApng
            | M::AnimationWebp
            | M::AnimationJxl
            | M::ImageAvifSequence
            | M::ImageHeifSequence
            | M::ImageHeicSequence
    )
}

/// `HC.FILES_THAT_CAN_HAVE_PIXEL_HASH`.
pub fn can_have_pixel_hash(mime: Mime) -> bool {
    is_image(mime) || is_in(mime, VIEWABLE_IMAGE_PROJECT_FILES)
}

/// `HC.FILES_THAT_HAVE_PERCEPTUAL_HASH`.
pub fn has_perceptual_hash(mime: Mime) -> bool {
    is_image(mime) || is_in(mime, VIEWABLE_IMAGE_PROJECT_FILES)
}

/// `HC.FILES_THAT_CAN_HAVE_ICC_PROFILE`.
pub fn can_have_icc_profile(mime: Mime) -> bool {
    matches!(
        mime,
        M::ImageBmp
            | M::ImageJpeg
            | M::ImageJxl
            | M::ImageTiff
            | M::ImagePng
            | M::ImageGif
            | M::ApplicationPsd
            | M::ImageAvif
            | M::ImageAvifSequence
    ) || is_in(mime, HEIF_STILLS_AND_SEQUENCES)
}

/// `HC.FILES_THAT_CAN_HAVE_EXIF`.
pub fn can_have_exif(mime: Mime) -> bool {
    matches!(
        mime,
        M::ImageJpeg
            | M::ImageJxl
            | M::ImageTiff
            | M::ImagePng
            | M::ImageWebp
            | M::AnimationJxl
            | M::AnimationApng
            | M::AnimationWebp
            | M::ImageAvif
            | M::ImageAvifSequence
    ) || is_in(mime, HEIF_STILLS_AND_SEQUENCES)
}

/// `HC.FILES_THAT_CAN_HAVE_XMP`.
pub fn can_have_xmp(mime: Mime) -> bool {
    matches!(
        mime,
        M::ImageJpeg
            | M::ImageTiff
            | M::ImagePng
            | M::ImageWebp
            | M::AnimationWebp
            | M::AnimationApng
            | M::ImageAvif
            | M::ImageAvifSequence
    ) || is_in(mime, HEIF_STILLS_AND_SEQUENCES)
}

/// `HC.FILES_THAT_CAN_HAVE_IPTC`.
pub fn can_have_iptc(mime: Mime) -> bool {
    matches!(mime, M::ImageJpeg | M::ImageTiff)
}

/// `HC.FILES_THAT_CAN_HAVE_SOFTWARE_SOURCE`: the human-readable metadata
/// types but PDF.
pub fn can_have_software_source(mime: Mime) -> bool {
    mime != M::ApplicationPdf && can_have_human_readable_embedded_metadata(mime)
}

/// `HC.FILES_THAT_CAN_HAVE_HUMAN_READABLE_EMBEDDED_METADATA`.
pub fn can_have_human_readable_embedded_metadata(mime: Mime) -> bool {
    matches!(
        mime,
        M::ImageJpeg
            | M::ImageJxl
            | M::ImagePng
            | M::ImageBmp
            | M::ImageWebp
            | M::ImageTiff
            | M::ImageIcon
            | M::ImageGif
            | M::ImageAvif
            | M::AnimationJxl
            | M::AnimationGif
            | M::AnimationApng
            | M::AnimationWebp
            | M::ApplicationPdf
    ) || is_in(mime, HEIF_STILLS_AND_SEQUENCES)
}

/// Types whose presence alone means "has audio" (`HC.MIMES_THAT_DEFINITELY_HAVE_AUDIO`).
pub(crate) fn definitely_has_audio(mime: Mime) -> bool {
    mime == M::ApplicationFlash || is_audio(mime)
}

/// Types the reference decodes with pillow-heif.
pub(crate) fn is_pil_heif(mime: Mime) -> bool {
    is_in(mime, HEIF_STILLS_AND_SEQUENCES)
}

/// `HC.IMAGE_FILE_EXTS`: extensions of still images, plus `.jpe`/`.jpeg`.
pub(crate) fn is_image_ext(ext_with_dot: &str) -> bool {
    matches!(ext_with_dot, ".jpe" | ".jpeg")
        || IMAGES.iter().any(|m| m.extension() == Some(ext_with_dot))
}

/// Does a file name end in an image extension (`filename_has_image_ext`)?
pub(crate) fn filename_has_image_ext(filename: &str) -> bool {
    filename
        .rsplit_once('.')
        .is_some_and(|(_, ext)| is_image_ext(&format!(".{ext}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constants() -> serde_json::Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../oracle/fixtures/constants.json"
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn groups_match_reference_implementation() {
        let c = constants();
        for m in c["mimes"].as_array().unwrap() {
            let code = u8::try_from(m["id"].as_u64().unwrap()).unwrap();
            let Some(mime) = Mime::from_code(code) else {
                continue;
            };
            let flag = |k: &str| m[k].as_bool().unwrap();
            assert_eq!(is_allowed(mime), flag("allowed"), "{mime:?} allowed");
            assert_eq!(has_thumbnail(mime), flag("has_thumbnail"), "{mime:?} thumb");
            assert_eq!(
                can_have_pixel_hash(mime),
                flag("can_have_pixel_hash"),
                "{mime:?}"
            );
            assert_eq!(
                has_perceptual_hash(mime),
                flag("has_perceptual_hash"),
                "{mime:?}"
            );
            assert_eq!(can_have_exif(mime), flag("can_have_exif"), "{mime:?} exif");
            assert_eq!(
                can_have_icc_profile(mime),
                flag("can_have_icc_profile"),
                "{mime:?}"
            );
            assert_eq!(
                can_check_transparency(mime),
                flag("can_check_transparency"),
                "{mime:?} transparency"
            );
        }
        let groups = &c["general_mimetypes_to_mime_groups"];
        let check = |class: Mime, ours: &[Mime]| {
            let theirs: Vec<u64> = groups[class.code().to_string()]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect();
            let mut ours: Vec<u64> = ours.iter().map(|m| u64::from(m.code())).collect();
            ours.sort_unstable();
            assert_eq!(ours, theirs, "{class:?}");
        };
        check(M::GeneralImage, IMAGES);
        check(M::GeneralAnimation, ANIMATIONS);
        check(M::GeneralAudio, AUDIO);
        check(M::GeneralVideo, VIDEO);
        check(M::GeneralApplication, APPLICATIONS);
        check(M::GeneralImageProject, IMAGE_PROJECT_FILES);
        check(M::GeneralApplicationArchive, ARCHIVES);
    }

    #[test]
    fn image_extensions() {
        assert!(filename_has_image_ext("a/b/page.jpeg"));
        assert!(filename_has_image_ext("x.png"));
        assert!(!filename_has_image_ext("x.PNG"));
        assert!(!filename_has_image_ext("png"));
        assert!(!filename_has_image_ext("x.mp4"));
    }
}
