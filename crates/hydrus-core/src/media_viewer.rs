//! How the media viewer shows files: each file type's way of showing it and
//! its zoom rules (the options' `media_view`), the zoom steps
//! (`media_zooms`), where zooming centres and the default zoom; and the
//! zooms a file has in a canvas (`ClientGUICanvasMedia.CalculateCanvasZooms`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Mime;

/// How a file type is shown (`CC.MEDIA_VIEWER_ACTION_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShowAction {
    Native = 0,
    NativePaused = 1,
    BehindEmbed = 2,
    BehindEmbedPaused = 3,
    OpenExternallyButton = 4,
    DoNotShowOnActivationOpenExternally = 5,
    DoNotShow = 6,
    Mpv = 7,
    QtMediaPlayerVideoWidget = 8,
    QtMediaPlayer = 9,
}

impl ShowAction {
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::Native,
            1 => Self::NativePaused,
            2 => Self::BehindEmbed,
            3 => Self::BehindEmbedPaused,
            4 => Self::OpenExternallyButton,
            5 => Self::DoNotShowOnActivationOpenExternally,
            6 => Self::DoNotShow,
            7 => Self::Mpv,
            8 => Self::QtMediaPlayerVideoWidget,
            9 => Self::QtMediaPlayer,
            _ => return None,
        })
    }

    /// Whether the file is shown, and so zooms (`IsZoomable`).
    pub fn zoomable(self) -> bool {
        !matches!(
            self,
            Self::OpenExternallyButton
                | Self::DoNotShowOnActivationOpenExternally
                | Self::DoNotShow
        )
    }
}

/// What a file too small or too big for the canvas is scaled to
/// (`CC.MEDIA_VIEWER_SCALE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaleAction {
    /// 100%.
    Full = 0,
    /// The largest regular zoom that fits.
    MaxRegular = 1,
    ToCanvas = 2,
}

impl ScaleAction {
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::Full,
            1 => Self::MaxRegular,
            2 => Self::ToCanvas,
            _ => return None,
        })
    }
}

/// A file type's zoom rules (the `media_view` tuple's last item).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoomRules {
    pub media_scale_up: ScaleAction,
    pub media_scale_down: ScaleAction,
    pub preview_scale_up: ScaleAction,
    pub preview_scale_down: ScaleAction,
    /// Zoom only by powers of two.
    pub exact_zooms_only: bool,
    /// `CC.ZOOM_*` interpolations.
    pub scale_up_quality: u8,
    pub scale_down_quality: u8,
}

impl ZoomRules {
    const fn all(action: ScaleAction, up_quality: u8, down_quality: u8) -> Self {
        Self {
            media_scale_up: action,
            media_scale_down: action,
            preview_scale_up: action,
            preview_scale_down: action,
            exact_zooms_only: false,
            scale_up_quality: up_quality,
            scale_down_quality: down_quality,
        }
    }
}

const ZOOM_LINEAR: u8 = 1;
const ZOOM_AREA: u8 = 2;
const ZOOM_LANCZOS4: u8 = 4;

/// How a file type is shown in the media viewer and the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaView {
    pub media_show_action: ShowAction,
    pub media_start_paused: bool,
    pub media_start_with_embed: bool,
    pub preview_show_action: ShowAction,
    pub preview_start_paused: bool,
    pub preview_start_with_embed: bool,
    pub zoom: ZoomRules,
}

impl MediaView {
    const fn new(media: ShowAction, preview: ShowAction, zoom: ZoomRules) -> Self {
        Self {
            media_show_action: media,
            media_start_paused: false,
            media_start_with_embed: false,
            preview_show_action: preview,
            preview_start_paused: false,
            preview_start_with_embed: false,
            zoom,
        }
    }

    /// What a type with no general class gets: not shown.
    const NULL: Self = Self::new(
        ShowAction::DoNotShow,
        ShowAction::DoNotShow,
        ZoomRules::all(ScaleAction::Full, ZOOM_LINEAR, ZOOM_LINEAR),
    );
}

/// Where zooming centres (`ZOOM_CENTERPOINT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoomCentre {
    MediaCentre = 0,
    ViewerCentre = 1,
    /// The pointer, when it is over the viewer (else the viewer's centre).
    Mouse = 2,
    MediaTopLeft = 3,
}

impl ZoomCentre {
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::MediaCentre,
            1 => Self::ViewerCentre,
            2 => Self::Mouse,
            3 => Self::MediaTopLeft,
            _ => return None,
        })
    }
}

/// A zoom a file has in a canvas (`MEDIA_VIEWER_ZOOM_TYPE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ZoomType {
    /// The file type's rules.
    DefaultForFiletype = 0,
    /// As big as it fits.
    Canvas = 1,
    /// 100%.
    Full = 2,
    FillX = 3,
    FillY = 4,
    /// Filling the whole canvas, overflowing it.
    FillAuto = 5,
}

impl ZoomType {
    pub const ALL: [Self; 6] = [
        Self::DefaultForFiletype,
        Self::Canvas,
        Self::Full,
        Self::FillX,
        Self::FillY,
        Self::FillAuto,
    ];

    pub fn from_code(code: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|t| *t as i64 == code)
    }
}

/// The media viewer's options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaViewerSettings {
    /// The zoom steps (`media_zooms`).
    pub media_zooms: Vec<f64>,
    /// `media_viewer_zoom_center`.
    pub zoom_centre: ZoomCentre,
    /// The zoom a file opens at (`media_viewer_default_zoom_type_override`).
    pub default_zoom_type: ZoomType,
    /// By file type or general class (`Mime` code): how it is shown. Types
    /// not listed take their class's.
    pub media_view: BTreeMap<u8, MediaView>,
    /// The size of a like or numerical rating's shape in the viewer
    /// (`media_viewer_rating_icon_size_px`).
    pub rating_icon_size: f64,
    /// The height of an inc/dec rating (`media_viewer_rating_incdec_height_px`).
    pub rating_incdec_height: f64,
}

/// Which of a file's info lines are "interesting" (those the media
/// viewer's top hover frame shows), and how they read: the options'
/// `file_info_line_consider_*`, `hide_uninteresting_modified_time`,
/// `use_nice_resolution_strings` and `has_audio_label`; and whether a page's
/// status bar describes one file selected by them
/// (`show_extended_single_file_info_in_status_bar`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InfoLineSettings {
    pub archived_interesting: bool,
    pub archived_time_interesting: bool,
    pub file_services_interesting: bool,
    pub file_services_import_times_interesting: bool,
    pub trash_time_interesting: bool,
    pub trash_reason_interesting: bool,
    pub hide_uninteresting_modified_time: bool,
    pub nice_resolutions: bool,
    pub has_audio_label: String,
    pub single_file_in_status_bar: bool,
}

impl Default for InfoLineSettings {
    fn default() -> Self {
        Self {
            archived_interesting: true,
            archived_time_interesting: true,
            file_services_interesting: false,
            file_services_import_times_interesting: false,
            trash_time_interesting: false,
            trash_reason_interesting: false,
            hide_uninteresting_modified_time: true,
            nice_resolutions: true,
            has_audio_label: "\u{1F50A}".into(),
            single_file_in_status_bar: true,
        }
    }
}

/// Sound (the reference's audio options, less the preview's, which
/// hydrus-rs has no preview for): the global volume and mute, and the
/// media viewer's own. The viewer plays at its own volume only if it
/// uses it, and is muted if either mute is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// `global_audio_volume`, 0 to 100.
    pub global_volume: u8,
    /// `global_audio_mute`.
    pub global_mute: bool,
    /// `media_viewer_audio_volume`.
    pub viewer_volume: u8,
    /// `media_viewer_audio_mute`.
    pub viewer_mute: bool,
    /// `media_viewer_uses_its_own_audio_volume`.
    pub viewer_uses_its_own_volume: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            global_volume: 70,
            global_mute: false,
            viewer_volume: 70,
            viewer_mute: false,
            viewer_uses_its_own_volume: false,
        }
    }
}

impl AudioSettings {
    /// The media viewer's volume (`GetCorrectCurrentVolume`).
    pub fn current_viewer_volume(&self) -> u8 {
        if self.viewer_uses_its_own_volume {
            self.viewer_volume
        } else {
            self.global_volume
        }
    }

    /// Whether the media viewer is muted (`GetCorrectCurrentMute`).
    pub fn viewer_muted(&self) -> bool {
        self.viewer_mute || self.global_mute
    }

    /// Set the volume the viewer's slider moves: the viewer's own if it
    /// uses it, else the global one (`VolumeSlider`, `ChangeVolume`).
    pub fn set_viewer_volume(&mut self, volume: u8) {
        let volume = volume.min(100);
        if self.viewer_uses_its_own_volume {
            self.viewer_volume = volume;
        } else {
            self.global_volume = volume;
        }
    }
}

/// The media viewer's slideshows (the reference's slideshow options): the
/// periods its menu offers, whether new slideshows shuffle and play a
/// file with a duration through at least once, and when a file with a
/// duration bends the period (each `None` if turned off): a short one may
/// move on once it has played through in a part of the period
/// (`short_loop_percentage`) or a number of seconds (`short_loop_seconds`),
/// or as soon as it ends if it is most of the period
/// (`short_cutoff_percentage`); a long one may run over by a part of the
/// period to finish (`long_overspill_percentage`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SlideshowSettings {
    /// `slideshow_durations`, in seconds.
    pub durations: Vec<f64>,
    /// `slideshows_progress_randomly`.
    pub shuffle: bool,
    /// `slideshow_always_play_duration_media_once_through`.
    pub once_through: bool,
    /// `slideshow_short_duration_loop_percentage`.
    pub short_loop_percentage: Option<i64>,
    /// `slideshow_short_duration_loop_seconds`.
    pub short_loop_seconds: Option<i64>,
    /// `slideshow_short_duration_cutoff_percentage`.
    pub short_cutoff_percentage: Option<i64>,
    /// `slideshow_long_duration_overspill_percentage`.
    pub long_overspill_percentage: Option<i64>,
}

impl Default for SlideshowSettings {
    fn default() -> Self {
        Self {
            durations: vec![1.0, 5.0, 10.0, 30.0, 60.0],
            shuffle: false,
            once_through: false,
            short_loop_percentage: Some(20),
            short_loop_seconds: Some(10),
            short_cutoff_percentage: Some(75),
            long_overspill_percentage: Some(50),
        }
    }
}

/// A new client's zoom steps.
pub const DEFAULT_MEDIA_ZOOMS: [f64; 19] = [
    0.01, 0.05, 0.1, 0.15, 0.2, 0.3, 0.5, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.5, 2.0, 3.0, 5.0, 10.0,
    20.0,
];

impl Default for MediaViewerSettings {
    fn default() -> Self {
        Self {
            media_zooms: DEFAULT_MEDIA_ZOOMS.to_vec(),
            zoom_centre: ZoomCentre::Mouse,
            default_zoom_type: ZoomType::DefaultForFiletype,
            media_view: default_media_view(),
            rating_icon_size: 12.0,
            rating_incdec_height: 12.0,
        }
    }
}

/// A new client's `media_view` where mpv plays video and audio.
pub fn default_media_view() -> BTreeMap<u8, MediaView> {
    use ShowAction::{Mpv, Native, OpenExternallyButton};
    let image = ZoomRules::all(ScaleAction::ToCanvas, ZOOM_LANCZOS4, ZOOM_AREA);
    let null = ZoomRules::all(ScaleAction::Full, ZOOM_LINEAR, ZOOM_LINEAR);
    [
        (Mime::GeneralImage, MediaView::new(Native, Native, image)),
        (Mime::GeneralAnimation, MediaView::new(Mpv, Mpv, image)),
        (Mime::GeneralVideo, MediaView::new(Mpv, Mpv, image)),
        (Mime::GeneralAudio, MediaView::new(Mpv, Mpv, null)),
        (
            Mime::GeneralApplication,
            MediaView::new(OpenExternallyButton, OpenExternallyButton, null),
        ),
        (
            Mime::GeneralApplicationArchive,
            MediaView::new(OpenExternallyButton, OpenExternallyButton, null),
        ),
        (
            Mime::GeneralImageProject,
            MediaView::new(OpenExternallyButton, OpenExternallyButton, null),
        ),
        (
            Mime::ApplicationPsd,
            MediaView::new(Native, OpenExternallyButton, image),
        ),
        (
            Mime::ApplicationKrita,
            MediaView::new(Native, OpenExternallyButton, image),
        ),
        (
            Mime::ImageOpenraster,
            MediaView::new(Native, OpenExternallyButton, image),
        ),
        (Mime::AnimationWebp, MediaView::new(Native, Native, image)),
        (Mime::AnimationJxl, MediaView::new(Native, Native, image)),
        (Mime::AnimationUgoira, MediaView::new(Native, Native, image)),
    ]
    .into_iter()
    .map(|(mime, view)| (mime.code(), view))
    .collect()
}

impl MediaViewerSettings {
    /// How `mime` is shown (`_GetMediaViewOptions`): its own options, else
    /// its class's (a new client's, if those are missing), else not at all.
    pub fn view(&self, mime: Mime) -> MediaView {
        if let Some(view) = self.media_view.get(&mime.code()) {
            return *view;
        }
        let Some(class) = mime.general_class() else {
            return MediaView::NULL;
        };
        self.media_view
            .get(&class.code())
            .copied()
            .or_else(|| default_media_view().get(&class.code()).copied())
            .unwrap_or(MediaView::NULL)
    }
}

/// The size a file is shown at, at `zoom` (`CalculateMediaSize`): audio
/// and files without a resolution are a 360x240 player, never scaled up.
pub fn media_size(mime: Mime, resolution: Option<(u32, u32)>, zoom: f64) -> (u32, u32) {
    let audio = mime.general_class() == Some(Mime::GeneralAudio);
    let (width, height) = match resolution.filter(|&(w, h)| !audio && w > 0 && h > 0) {
        Some(size) => size,
        None if zoom >= 1.0 => return (360, 240),
        None => (360, 240),
    };
    // (Python's round, halves to even)
    let scale = |side: u32| ((zoom * f64::from(side)).round_ties_even() as u32).max(1);
    (scale(width), scale(height))
}

/// The zoom of each zoom type a file has in a canvas of `canvas` logical
/// pixels at `device_pixel_ratio`, in the media viewer
/// (`CalculateCanvasZooms`); all 1.0 for a file that isn't shown.
pub fn canvas_zooms(
    settings: &MediaViewerSettings,
    mime: Mime,
    resolution: Option<(u32, u32)>,
    canvas: (u32, u32),
    device_pixel_ratio: f64,
) -> BTreeMap<ZoomType, f64> {
    let mut zooms: BTreeMap<ZoomType, f64> = ZoomType::ALL.iter().map(|t| (*t, 1.0)).collect();
    let view = settings.view(mime);
    if !view.media_show_action.zoomable() {
        return zooms;
    }
    let (media_width, media_height) = media_size(mime, resolution, 1.0);
    let (media_width, media_height) = (f64::from(media_width), f64::from(media_height));
    let canvas_width = f64::from(canvas.0.max(80)) * device_pixel_ratio;
    let canvas_height = f64::from(canvas.1.max(60)) * device_pixel_ratio;
    let width_zoom = canvas_width / media_width;
    let height_zoom = canvas_height / media_height;
    let canvas_zoom = width_zoom.min(height_zoom);
    // (overfilling the canvas)
    let fill_auto = if media_width / media_height > canvas_width / canvas_height {
        height_zoom
    } else {
        width_zoom
    };
    zooms.insert(ZoomType::Canvas, canvas_zoom);
    zooms.insert(ZoomType::FillX, width_zoom);
    zooms.insert(ZoomType::FillY, height_zoom);
    zooms.insert(ZoomType::FillAuto, fill_auto);

    let rules = view.zoom;
    let max_regular = if rules.exact_zooms_only {
        let mut zoom = 1.0;
        if canvas_zoom > 1.0 {
            while zoom * 2.0 < canvas_zoom {
                zoom *= 2.0;
            }
        } else if canvas_zoom < 1.0 {
            while zoom > canvas_zoom {
                zoom /= 2.0;
            }
        }
        zoom
    } else {
        settings
            .media_zooms
            .iter()
            .copied()
            .filter(|&z| z < canvas_zoom)
            .reduce(f64::max)
            .unwrap_or(canvas_zoom)
    };
    let (scale_up, scale_down) = if mime.general_class() == Some(Mime::GeneralAudio) {
        (ScaleAction::Full, ScaleAction::ToCanvas)
    } else {
        (rules.media_scale_up, rules.media_scale_down)
    };
    let action = if media_width < canvas_width && media_height < canvas_height {
        scale_up
    } else if media_width > canvas_width || media_height > canvas_height {
        scale_down
    } else {
        ScaleAction::Full
    };
    let default = match action {
        ScaleAction::Full => 1.0,
        ScaleAction::MaxRegular => max_regular,
        ScaleAction::ToCanvas => canvas_zoom,
    };
    zooms.insert(ZoomType::DefaultForFiletype, default);
    zooms
}

/// The zoom steps zooming in or out goes through from `current`
/// (`ZoomIn`, `ZoomOut`): the regular zooms, or with exact zooms only the
/// next power of two (none past the largest regular zoom, zooming in),
/// and canvas fit. `None` if there is no further step.
pub fn next_zoom(
    settings: &MediaViewerSettings,
    rules: &ZoomRules,
    current: f64,
    canvas_zoom: f64,
    zoom_in: bool,
) -> Option<f64> {
    let mut steps: Vec<f64> = if rules.exact_zooms_only {
        let mut exact = 1.0;
        if zoom_in {
            if exact <= current {
                while exact <= current {
                    exact *= 2.0;
                }
            } else {
                while exact / 2.0 > current {
                    exact /= 2.0;
                }
            }
            let largest = settings
                .media_zooms
                .iter()
                .copied()
                .fold(f64::MIN, f64::max);
            if exact > largest {
                return None;
            }
        } else if exact < current {
            while exact * 2.0 < current {
                exact *= 2.0;
            }
        } else {
            while exact >= current {
                exact /= 2.0;
            }
        }
        vec![exact]
    } else {
        settings.media_zooms.clone()
    };
    steps.push(canvas_zoom);
    if zoom_in {
        steps.into_iter().filter(|&z| z > current).reduce(f64::min)
    } else {
        steps.into_iter().filter(|&z| z < current).reduce(f64::max)
    }
}
