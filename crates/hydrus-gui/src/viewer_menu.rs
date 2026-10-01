//! The media viewer's right-click menu, as the reference's
//! (`CanvasMediaListBrowser.ShowMenuFromSignal`), for the file shown:
//! its info, the zoom, fullscreen, the slideshow, the volume, removing it
//! from view,
//! archiving, deleting, managing its tags, its urls, opening and sharing
//! it, and what plays it. The parts it shares with the thumbnails' menu
//! (info, urls, open, share) are built as that one builds them for one
//! file selected and focused, as the reference's are. Plain Rust: the
//! window lays it into its template as the thumbnails' menu is laid.

use hydrus_core::media_viewer::{AudioSettings, InfoLineSettings, SlideshowSettings};
use hydrus_core::{HashId, ServiceType};
use hydrus_store::Store;

use crate::thumbnail_menu::{
    Action, Entry, facts, info_menu, open_menu, share_menu, url_facts, urls_menu,
};

/// What the viewer's own entries do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerAction {
    ZoomIn,
    ZoomOut,
    /// Between 100% and canvas fit (`ZoomSwitch`).
    ZoomSwitch,
    ZoomMax,
    /// Between fullscreen and the window it was.
    Fullscreen,
    /// Flip the global mute, or the viewer's own.
    MuteGlobal,
    MuteViewer,
    /// Mute or unmute this viewer whatever the options say, or follow them
    /// again (`SetPerPlayerMuteState`).
    ForceMute(Option<bool>),
    /// The volume, shown (the reference's slider, which a menu here can't
    /// hold): does nothing.
    Volume,
    /// Take the file off the viewer and its page (`_Remove`).
    RemoveFromView,
    /// Stop the slideshow, or resume it (`SIMPLE_PAUSE_PLAY_SLIDESHOW`).
    PausePlaySlideshow,
    /// Start a slideshow at a period, or one asked for
    /// (`SIMPLE_START_SLIDESHOW`).
    StartSlideshow(Option<Seconds>),
    /// Flip this slideshow's shuffling, or every new one's (the options').
    FlipShuffle,
    FlipGlobalShuffle,
    /// Flip whether this slideshow plays a file through before moving on,
    /// or every new one does (the options').
    FlipOnceThrough,
    FlipGlobalOnceThrough,
}

/// A period in seconds (compared by its bits, so that actions compare).
#[derive(Debug, Clone, Copy)]
pub struct Seconds(pub f64);

impl PartialEq for Seconds {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for Seconds {}

/// What shows the file (`GetCurrentMediaPlayerLabel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    StaticImage,
    Animation,
    Mpv,
    /// Anything else, which the reference shows with a button to open it.
    OpenExternally,
}

impl Player {
    pub fn label(self) -> &'static str {
        match self {
            Player::StaticImage => "Hydrus Native Static Image",
            Player::Animation => "Hydrus Native Animation Player",
            Player::Mpv => "MPV Embed Player",
            Player::OpenExternally => "Open Externally Panel",
        }
    }
}

/// The zoom, as the zoom submenu reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomState {
    pub current: f64,
    /// The zoom that fits the canvas.
    pub canvas: f64,
    /// Whether the file is as big as it may be (`IsAtMaxZoom`).
    pub at_max: bool,
}

/// What the viewer's menu reads from the viewer.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewerState {
    /// The zoom, if the file zooms.
    pub zoom: Option<ZoomState>,
    pub fullscreen: bool,
    pub audio: AudioSettings,
    /// This viewer's own mute, if forced.
    pub forced_mute: Option<bool>,
    pub player: Player,
    /// The viewer's slideshow, and the options' periods and defaults.
    pub slideshow: crate::slideshow::Slideshow,
    pub slideshow_settings: SlideshowSettings,
}

/// A zoom as the reference writes it (`ConvertZoomToPercentage`):
/// `621.88%`, but `100%` and `1250%`.
pub fn zoom_percentage(zoom: f64) -> String {
    let percent = zoom * 100.0;
    let pretty = format!("{percent:.2}%");
    if pretty.ends_with("00%") {
        format!("{percent:.0}%")
    } else {
        pretty
    }
}

/// The menu for `file`, shown in `state`.
pub fn viewer_menu(
    store: &Store,
    file: HashId,
    state: &ViewerState,
    info_settings: &InfoLineSettings,
    now_ms: i64,
) -> Vec<Entry> {
    let item =
        |label: &str, action: ViewerAction| Entry::Item(label.into(), Action::Viewer(action));
    let mut entries = Vec::new();
    // the file's info (as the thumbnails' for it alone)
    if let Some(info) = info_menu(
        store,
        Some(file),
        (&[file], crate::status::Items::files(1)),
        info_settings,
        now_ms,
    ) {
        entries.push(info);
    }
    entries.push(Entry::Separator);
    // the zoom, and fullscreen
    if let Some(zoom) = &state.zoom {
        let mut inner = vec![
            item("zoom in", ViewerAction::ZoomIn),
            item("zoom out", ViewerAction::ZoomOut),
        ];
        #[allow(clippy::float_cmp)]
        if zoom.current != 1.0 {
            inner.push(item("zoom to 100%", ViewerAction::ZoomSwitch));
        } else if zoom.current != zoom.canvas {
            inner.push(item("zoom fit", ViewerAction::ZoomSwitch));
        }
        if !zoom.at_max {
            inner.push(item("zoom to max", ViewerAction::ZoomMax));
        }
        entries.push(Entry::Menu(
            format!("zoom: {}", zoom_percentage(zoom.current)),
            inner,
        ));
    }
    entries.push(item(
        if state.fullscreen {
            "exit fullscreen"
        } else {
            "go fullscreen"
        },
        ViewerAction::Fullscreen,
    ));
    entries.push(crate::slideshow::slideshow_menu(
        &state.slideshow,
        &state.slideshow_settings,
    ));
    entries.push(Entry::Separator);
    entries.push(volume_menu(state));
    entries.push(Entry::Separator);
    entries.push(item("remove from view", ViewerAction::RemoveFromView));
    entries.push(Entry::Separator);
    // archiving, deleting
    let file_facts = facts(store, &[file]);
    let snapshot = store.snapshot();
    if let Some(f) = file_facts.first() {
        let local_storage = snapshot
            .services
            .all()
            .find(|s| s.service_type() == ServiceType::HydrusLocalFileStorage)
            .map(|s| s.id);
        let trash = snapshot
            .services
            .all()
            .find(|s| s.service_type() == ServiceType::LocalFileTrashDomain)
            .map(|s| s.id);
        if f.inbox {
            entries.push(Entry::Item("archive".into(), Action::Archive));
        } else if local_storage.is_some_and(|l| f.current.contains(&l)) {
            entries.push(Entry::Item("return to inbox".into(), Action::Inbox));
        }
        entries.push(Entry::Separator);
        let mut domains: Vec<_> = snapshot
            .services
            .all()
            .filter(|s| {
                s.service_type() == ServiceType::LocalFileDomain && f.current.contains(&s.id)
            })
            .collect();
        domains.sort_by(|a, b| a.name.cmp(&b.name));
        match &domains[..] {
            [] => {}
            [one] => entries.push(Entry::Item(
                format!("delete from {}", one.name),
                Action::DeleteFrom(one.id),
            )),
            several => entries.push(Entry::Menu(
                "delete".into(),
                several
                    .iter()
                    .map(|d| Entry::Item(format!("from {}", d.name), Action::DeleteFrom(d.id)))
                    .collect(),
            )),
        }
        if trash.is_some_and(|t| f.current.contains(&t)) {
            entries.push(Entry::Item(
                "delete physically now".into(),
                Action::DeletePhysically,
            ));
            entries.push(Entry::Item("undelete".into(), Action::Undelete));
        }
    }
    entries.push(Entry::Separator);
    entries.push(Entry::Menu(
        "manage".into(),
        vec![Entry::Item("tags".into(), Action::ManageTags)],
    ));
    // its urls, opening and sharing it (as the thumbnails' for it alone)
    // (with no selection, as `AddKnownURLsViewCopyMenu` is called here)
    if let Some(urls) = urls_menu(&url_facts(store, Some(file), &[])) {
        entries.push(urls);
    }
    entries.push(Entry::Menu("open".into(), open_menu(store, Some(file), 1)));
    entries.push(share_menu(store, &file_facts, Some(file), &[file]));
    entries.push(Entry::Separator);
    entries.push(Entry::Menu(
        "player".into(),
        vec![Entry::Label(format!("This is a {}.", state.player.label()))],
    ));
    entries
}

/// The volume submenu (`AddAudioVolumeMenu`, for the media viewer): the
/// global mute, the viewer's mute and its forcing, and the volume.
fn volume_menu(state: &ViewerState) -> Entry {
    let item = |label: String, action: ViewerAction| Entry::Item(label, Action::Viewer(action));
    let audio = &state.audio;
    let mut inner = vec![
        item(
            if audio.global_mute {
                "unmute global"
            } else {
                "mute global"
            }
            .into(),
            ViewerAction::MuteGlobal,
        ),
        Entry::Separator,
        item(
            if audio.viewer_mute {
                "unmute media viewer"
            } else {
                "mute media viewer"
            }
            .into(),
            ViewerAction::MuteViewer,
        ),
    ];
    match state.forced_mute {
        Some(muted) => {
            inner.push(if muted {
                item(
                    "force unmute just here".into(),
                    ViewerAction::ForceMute(Some(false)),
                )
            } else {
                item(
                    "force mute just here".into(),
                    ViewerAction::ForceMute(Some(true)),
                )
            });
            inner.push(item(
                format!("stop forcing {}", if muted { "mute" } else { "unmute" }),
                ViewerAction::ForceMute(None),
            ));
        }
        None if audio.viewer_mute || audio.global_mute => {
            inner.push(item(
                "force unmute just here".into(),
                ViewerAction::ForceMute(Some(false)),
            ));
        }
        None => inner.push(item(
            "force mute just here".into(),
            ViewerAction::ForceMute(Some(true)),
        )),
    }
    inner.push(Entry::Separator);
    inner.push(item(
        format!("volume: {}", audio.current_viewer_volume()),
        ViewerAction::Volume,
    ));
    Entry::Menu("volume".into(), inner)
}

/// What shows `file` in the viewer: mpv for what the reference plays in
/// it, the client's own player for the animations it plays itself, an
/// image as a still, and anything else by its thumbnail (where the
/// reference has a button to open it).
pub fn player(store: &Store, file: HashId) -> Player {
    if crate::viewer::playable(store, file).is_some() {
        return Player::Mpv;
    }
    let mime = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[file]))
        .ok()
        .and_then(|results| results.into_iter().next()?.info)
        .map(|info| info.mime);
    match mime {
        Some(
            hydrus_core::Mime::AnimationWebp
            | hydrus_core::Mime::AnimationJxl
            | hydrus_core::Mime::AnimationUgoira,
        ) => Player::Animation,
        Some(mime) if mime.general_class() == Some(hydrus_core::Mime::GeneralImage) => {
            Player::StaticImage
        }
        _ => Player::OpenExternally,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zooms_read_as_the_reference_writes_them() {
        assert_eq!(zoom_percentage(1.0), "100%");
        assert_eq!(zoom_percentage(12.5), "1250%");
        assert_eq!(zoom_percentage(6.21875), "621.88%");
        assert_eq!(zoom_percentage(6.666_666_666_666_667), "666.67%");
        assert_eq!(zoom_percentage(0.8), "80%");
        assert_eq!(zoom_percentage(3.125), "312.50%");
    }

    fn state(forced_mute: Option<bool>, audio: AudioSettings) -> ViewerState {
        ViewerState {
            zoom: Some(ZoomState {
                current: 1.0,
                canvas: 1.0,
                at_max: true,
            }),
            fullscreen: false,
            audio,
            forced_mute,
            player: Player::StaticImage,
            slideshow: crate::slideshow::Slideshow::default(),
            slideshow_settings: SlideshowSettings::default(),
        }
    }

    fn labels(entry: &Entry) -> Vec<String> {
        let Entry::Menu(_, inner) = entry else {
            unreachable!()
        };
        inner
            .iter()
            .map(|e| match e {
                Entry::Item(l, _) | Entry::Label(l) | Entry::Check(l, _, _) => l.clone(),
                Entry::Menu(t, _) => t.clone(),
                Entry::Separator => "---".into(),
            })
            .collect()
    }

    /// The volume menu's mutes and forcing, which the recorded menus (with
    /// nothing muted or forced) don't show, as `AddAudioVolumeMenu` lays
    /// them out.
    #[test]
    fn the_volume_menu_follows_the_mutes() {
        let muted = AudioSettings {
            global_mute: true,
            viewer_mute: true,
            viewer_uses_its_own_volume: true,
            viewer_volume: 25,
            ..AudioSettings::default()
        };
        assert_eq!(
            labels(&volume_menu(&state(None, muted))),
            [
                "unmute global",
                "---",
                "unmute media viewer",
                "force unmute just here",
                "---",
                "volume: 25"
            ]
        );
        assert_eq!(
            labels(&volume_menu(&state(Some(true), AudioSettings::default()))),
            [
                "mute global",
                "---",
                "mute media viewer",
                "force unmute just here",
                "stop forcing mute",
                "---",
                "volume: 70"
            ]
        );
        assert_eq!(
            labels(&volume_menu(&state(Some(false), muted)))[3..5],
            ["force mute just here", "stop forcing unmute"]
        );
    }
}
