//! Options > media playback > per-filetype handling (`MediaPlaybackPanel`'s
//! list and `EditMediaViewOptionsPanel`): the rows as the reference shows
//! them, which filetypes "add" offers, what "delete" may remove, and each
//! filetype's editor choices (`CC.media_viewer_capabilities`, dumped by
//! `oracle/dump_media_view_options.py`, mpv taken as available).

use std::collections::BTreeMap;

use hydrus_core::Mime;
use hydrus_core::media_viewer::{MediaView, ScaleAction, ShowAction};

use crate::list_selection::ListSelection;

/// What the editor offers a filetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    pub intro: &'static str,
    pub media: &'static [ShowAction],
    pub preview: &'static [ShowAction],
    /// Whether the zoom rows show (some way of showing the file itself).
    pub zoom_rows: bool,
}

/// The editor's choices for a filetype code.
pub fn capability(code: u8) -> Option<Capability> {
    Some(match code {
        1 => Capability {
            intro: "Setting media view options for jpeg.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        2 => Capability {
            intro: "Setting media view options for png.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        3 => Capability {
            intro: "Setting media view options for animated gif.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        4 => Capability {
            intro: "Setting media view options for bitmap.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        5 => Capability {
            intro: "Setting media view options for flash.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        7 => Capability {
            intro: "Setting media view options for icon.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        9 => Capability {
            intro: "Setting media view options for flv.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        10 => Capability {
            intro: "Setting media view options for pdf.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        11 => Capability {
            intro: "Setting media view options for zip.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        13 => Capability {
            intro: "Setting media view options for mp3.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        14 => Capability {
            intro: "Setting media view options for mp4.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        15 => Capability {
            intro: "Setting media view options for ogg.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        16 => Capability {
            intro: "Setting media view options for flac.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        17 => Capability {
            intro: "Setting media view options for wma.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        18 => Capability {
            intro: "Setting media view options for wmv.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        20 => Capability {
            intro: "Setting media view options for matroska.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        21 => Capability {
            intro: "Setting media view options for webm.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        23 => Capability {
            intro: "Setting media view options for apng.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        25 => Capability {
            intro: "Setting media view options for mpeg.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        26 => Capability {
            intro: "Setting media view options for quicktime.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        27 => Capability {
            intro: "Setting media view options for avi.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        28 => Capability {
            intro: "Setting media view options for application/hydrus-update-definitions.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        29 => Capability {
            intro: "Setting media view options for application/hydrus-update-content.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        31 => Capability {
            intro: "Setting media view options for rar.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        32 => Capability {
            intro: "Setting media view options for 7z.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        33 => Capability {
            intro: "Setting media view options for webp.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        34 => Capability {
            intro: "Setting media view options for tiff.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        35 => Capability {
            intro: "Setting media view options for psd.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        36 => Capability {
            intro: "Setting media view options for m4a.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        37 => Capability {
            intro: "Setting media view options for realvideo.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        38 => Capability {
            intro: "Setting media view options for realaudio.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        39 => Capability {
            intro: "Setting media view options for tta.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        40 => Capability {
            intro: "Setting media view options for audio.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        41 => Capability {
            intro: "Setting media view options for image.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        42 => Capability {
            intro: "Setting media view options for video.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        43 => Capability {
            intro: "Setting media view options for application.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        44 => Capability {
            intro: "Setting media view options for animation.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        45 => Capability {
            intro: "Setting media view options for clip.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        46 => Capability {
            intro: "Setting media view options for wave.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        47 => Capability {
            intro: "Setting media view options for ogv.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        48 => Capability {
            intro: "Setting media view options for matroska audio.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        49 => Capability {
            intro: "Setting media view options for mp4 audio.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        53 => Capability {
            intro: "Setting media view options for wavpack.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        54 => Capability {
            intro: "Setting media view options for sai2.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        55 => Capability {
            intro: "Setting media view options for krita.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        56 => Capability {
            intro: "Setting media view options for svg.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        57 => Capability {
            intro: "Setting media view options for xcf.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        58 => Capability {
            intro: "Setting media view options for gzip.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        59 => Capability {
            intro: "Setting media view options for archive.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        60 => Capability {
            intro: "Setting media view options for image project file.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        61 => Capability {
            intro: "Setting media view options for heif.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        62 => Capability {
            intro: "Setting media view options for heif sequence.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        63 => Capability {
            intro: "Setting media view options for heic.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        64 => Capability {
            intro: "Setting media view options for heic sequence.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        65 => Capability {
            intro: "Setting media view options for avif.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        66 => Capability {
            intro: "Setting media view options for avif sequence.\n\nmpv is higher quality but can be buggy. If you have problems, try the QtMediaPlayer as a fallback.",
            media: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Mpv,
                ShowAction::QtMediaPlayer,
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        68 => Capability {
            intro: "Setting media view options for static gif.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        69 => Capability {
            intro: "Setting media view options for procreate.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        70 => Capability {
            intro: "Setting media view options for qoi.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        71 => Capability {
            intro: "Setting media view options for epub.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        72 => Capability {
            intro: "Setting media view options for djvu.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        73 => Capability {
            intro: "Setting media view options for cbz.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        74 => Capability {
            intro: "Setting media view options for ugoira.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        75 => Capability {
            intro: "Setting media view options for rtf.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        76 => Capability {
            intro: "Setting media view options for docx.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        77 => Capability {
            intro: "Setting media view options for xlsx.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        78 => Capability {
            intro: "Setting media view options for pptx.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        80 => Capability {
            intro: "Setting media view options for doc.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        81 => Capability {
            intro: "Setting media view options for xls.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        82 => Capability {
            intro: "Setting media view options for ppt.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        83 => Capability {
            intro: "Setting media view options for animated webp.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        85 => Capability {
            intro: "Setting media view options for jxl.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        86 => Capability {
            intro: "Setting media view options for paint.net.",
            media: &[
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[ShowAction::OpenExternallyButton, ShowAction::DoNotShow],
            zoom_rows: false,
        },
        88 => Capability {
            intro: "Setting media view options for animated jxl.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        89 => Capability {
            intro: "Setting media view options for ora.",
            media: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShowOnActivationOpenExternally,
                ShowAction::DoNotShow,
            ],
            preview: &[
                ShowAction::Native,
                ShowAction::OpenExternallyButton,
                ShowAction::DoNotShow,
            ],
            zoom_rows: true,
        },
        _ => return None,
    })
}

/// A show action's words (`media_viewer_action_string_lookup`).
pub const fn action_text(action: ShowAction) -> &'static str {
    match action {
        ShowAction::Native => "show with native hydrus viewer",
        ShowAction::NativePaused => "show as normal, but start paused -- obselete",
        ShowAction::BehindEmbed => "show, but initially behind an embed button -- obselete",
        ShowAction::BehindEmbedPaused => {
            "show, but initially behind an embed button, and start paused -- obselete"
        }
        ShowAction::OpenExternallyButton => "show an 'open externally' button",
        ShowAction::DoNotShowOnActivationOpenExternally => {
            "do not show in the media viewer. on thumbnail activation, open externally"
        }
        ShowAction::DoNotShow => "do not show at all",
        ShowAction::Mpv => "show using mpv",
        ShowAction::QtMediaPlayerVideoWidget => "show using old QtMediaPlayer test -- obselete",
        ShowAction::QtMediaPlayer => "show using QtMediaPlayer",
    }
}

/// A scale action's words (`media_viewer_scale_string_lookup`), in the editor's order.
pub const SCALES: [(ScaleAction, &str); 3] = [
    (ScaleAction::Full, "show at 100%"),
    (
        ScaleAction::MaxRegular,
        "scale to the largest regular zoom that fits",
    ),
    (ScaleAction::ToCanvas, "scale to the canvas size"),
];

/// The interpolation choices above 100% and below (`zoom_string_lookup`).
pub const SCALE_UP_QUALITIES: [(u8, &str); 4] = [
    (0, "nearest neighbour"),
    (1, "bilinear interpolation"),
    (3, "4x4 bilinear interpolation"),
    (4, "8x8 Lanczos interpolation"),
];
pub const SCALE_DOWN_QUALITIES: [(u8, &str); 3] = [
    (0, "nearest neighbour"),
    (1, "bilinear interpolation"),
    (2, "pixel area resampling"),
];

/// A filetype's name in the list (`_GetPrettyMime`): "image: jpeg" for a
/// specific type, the class's own name for a class.
pub fn pretty(code: u8) -> String {
    let Some(mime) = Mime::from_code(code) else {
        return format!("unknown filetype {code}");
    };
    match mime.general_class() {
        Some(class) if !mime.is_general_class() => {
            format!("{}: {}", class.human_name(), mime.human_name())
        }
        _ => mime.human_name().to_owned(),
    }
}

/// The zoom column: the reference's Python tuple text, for files shown at all.
fn zoom_text(view: &MediaView) -> String {
    let shows = |a: ShowAction| {
        matches!(
            a,
            ShowAction::Native | ShowAction::Mpv | ShowAction::QtMediaPlayer
        )
    };
    if !shows(view.media_show_action) && !shows(view.preview_show_action) {
        return String::new();
    }
    let z = &view.zoom;
    format!(
        "({}, {}, {}, {}, {}, {}, {})",
        z.media_scale_up as u8,
        z.media_scale_down as u8,
        z.preview_scale_up as u8,
        z.preview_scale_down as u8,
        if z.exact_zooms_only { "True" } else { "False" },
        z.scale_up_quality,
        z.scale_down_quality
    )
}

fn action_cell(action: ShowAction, paused: bool, embed: bool) -> String {
    let mut text = action_text(action).to_owned();
    if paused {
        text.push_str(", start paused");
    }
    if embed {
        text.push_str(", start with embed button");
    }
    text
}

/// A list row.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub code: u8,
    pub view: MediaView,
}

impl Row {
    /// Filetype, media show action, preview show action, zoom info.
    pub fn cells(&self) -> [String; 4] {
        let v = &self.view;
        [
            pretty(self.code),
            action_cell(
                v.media_show_action,
                v.media_start_paused,
                v.media_start_with_embed,
            ),
            action_cell(
                v.preview_show_action,
                v.preview_start_paused,
                v.preview_start_with_embed,
            ),
            zoom_text(v),
        ]
    }
}

/// The list as edited, with its selection and sort.
#[derive(Debug, Clone)]
pub struct Table {
    pub rows: Vec<Row>,
    pub selection: ListSelection<u8>,
    pub sort_column: usize,
    pub ascending: bool,
}

impl Table {
    pub fn new(views: &BTreeMap<u8, MediaView>) -> Self {
        let mut table = Self {
            rows: views
                .iter()
                .map(|(&code, &view)| Row { code, view })
                .collect(),
            selection: ListSelection::default(),
            sort_column: 0,
            ascending: true,
        };
        table.sort(0, true);
        table
    }
    pub fn values(&self) -> BTreeMap<u8, MediaView> {
        self.rows.iter().map(|r| (r.code, r.view)).collect()
    }
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        let ids: Vec<u8> = self.rows.iter().map(|r| r.code).collect();
        self.selection.click(&ids, index, ctrl, shift);
    }
    pub fn sort(&mut self, column: usize, ascending: bool) {
        if column >= 4 {
            return;
        }
        self.sort_column = column;
        self.ascending = ascending;
        self.rows.sort_by(|a, b| {
            let (a, b) = (a.cells(), b.cells());
            let order = a[column].cmp(&b[column]).then_with(|| a.cmp(&b));
            if ascending { order } else { order.reverse() }
        });
    }
    /// The one selected row (edit).
    pub fn selected(&self) -> Option<&Row> {
        self.selection
            .one()
            .and_then(|code| self.rows.iter().find(|r| r.code == code))
    }
    /// The searchable filetypes with no row yet, as "add" offers them.
    pub fn addable(&self) -> Vec<(String, u8)> {
        let mut out: Vec<(String, u8)> = (0..=u8::MAX)
            .filter_map(Mime::from_code)
            .filter(|m| m.is_searchable())
            .map(Mime::code)
            .filter(|code| !self.rows.iter().any(|r| r.code == *code))
            .map(|code| (pretty(code), code))
            .collect();
        out.sort();
        out
    }
    /// Whether "delete" works: a selection of only specific searchable types
    /// (the general classes can't be removed).
    pub fn can_delete(&self) -> bool {
        !self.selection.is_empty()
            && self.rows.iter().all(|r| {
                !self.selection.is_selected(r.code)
                    || Mime::from_code(r.code).is_some_and(Mime::is_searchable)
            })
    }
    pub fn delete_selected(&mut self) {
        if self.can_delete() {
            let selection = &self.selection;
            self.rows.retain(|r| !selection.is_selected(r.code));
            self.selection = ListSelection::default();
        }
    }
    /// A new row for `code` starts as a copy of its class's
    /// (`_GetCopyOfGeneralMediaViewOptions`).
    pub fn new_row(&self, code: u8) -> Option<Row> {
        let class = Mime::from_code(code)?.general_class()?.code();
        let view = self.rows.iter().find(|r| r.code == class)?.view;
        Some(Row { code, view })
    }
    /// Add or replace a row, keeping the sort and selecting it.
    pub fn put(&mut self, row: Row) {
        let code = row.code;
        match self.rows.iter_mut().find(|r| r.code == code) {
            Some(existing) => *existing = row,
            None => self.rows.push(row),
        }
        self.sort(self.sort_column, self.ascending);
        if let Some(index) = self.rows.iter().position(|r| r.code == code) {
            self.click(index, false, false);
        }
    }
}

/// Which of the editor's controls are enabled (`_UpdateControls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Enabled {
    pub media_paused: bool,
    pub media_embed: bool,
    pub preview_paused: bool,
    pub preview_embed: bool,
    pub media_scales: bool,
    pub preview_scales: bool,
    pub exact_zooms: bool,
    pub qualities: bool,
}

/// What the editor enables for `code` with these show actions.
pub fn enabled(code: u8, media: ShowAction, preview: ShowAction) -> Enabled {
    // (`CC.unsupported_media_actions`)
    let ok = |a: ShowAction| {
        !matches!(
            a,
            ShowAction::OpenExternallyButton
                | ShowAction::DoNotShowOnActivationOpenExternally
                | ShowAction::DoNotShow
        )
    };
    let (media_ok, preview_ok) = (ok(media), ok(preview));
    let mime = Mime::from_code(code);
    let class = mime.and_then(|m| {
        if m.is_general_class() {
            Some(m)
        } else {
            m.general_class()
        }
    });
    let is = |general: Mime| class == Some(general);
    let is_image = is(Mime::GeneralImage);
    let still = is_image
        || is(Mime::GeneralApplication)
        || is(Mime::GeneralApplicationArchive)
        || is(Mime::GeneralImageProject);
    let audio = is(Mime::GeneralAudio);
    Enabled {
        media_paused: media_ok && !still,
        media_embed: media_ok,
        preview_paused: preview_ok && !still,
        preview_embed: preview_ok,
        media_scales: media_ok && !audio,
        preview_scales: preview_ok && !audio,
        exact_zooms: media_ok || preview_ok,
        qualities: (media_ok || preview_ok) && is_image,
    }
}
