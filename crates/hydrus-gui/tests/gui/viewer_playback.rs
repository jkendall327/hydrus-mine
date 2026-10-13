//! Each kind of file in the media viewer, as the reference shows it: a
//! still decoded whole to the reference's pixels (`render.json`), with no
//! bar or volume; a GIF, an APNG and webms played through libmpv, with
//! the reference's frames and times on the bar (`media.json`), moving and
//! looping; an audio file's embedded cover art shown as its picture, as
//! hydrus's `default_mpv.conf` asks (`audio-display=embedded-first`); and
//! the volume and mute the reference's viewer plays at
//! (`GetCorrectCurrentVolume`/`GetCorrectCurrentMute`, `preview_audio.json`)
//! reaching the playing player, from the options as it opens and as they
//! change, and from its own volume control. (Ugoiras and animated WebP play
//! in the client's own player: `animation.rs`.) The mpv parts are skipped
//! where libmpv isn't installed; CI installs it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use hydrus_core::media_viewer::AudioSettings;
use hydrus_gui::scanbar::Scanbar;
use hydrus_gui::{MainWindow, MediaViewerWindow, Pages, bind, headless, mpv};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn settle(done: &dyn Fn() -> bool) -> bool {
    let started = Instant::now();
    while !done() && started.elapsed() < Duration::from_secs(20) {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
    done()
}

struct Client {
    store: Arc<Store>,
    ui: MainWindow,
    bound: hydrus_gui::Bound,
    windows: headless::Windows,
    searched: std::cell::Cell<bool>,
    _dirs: [tempfile::TempDir; 2],
}

impl Client {
    fn basic() -> Self {
        let legacy = hydrus_testkit::legacy_fixture("basic");
        let dir = tempfile::tempdir().unwrap();
        import_legacy(
            legacy.path(),
            &dir.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store = Store::open(dir.path()).unwrap();
        let windows = headless::init();
        let ui = MainWindow::new().unwrap();
        // (all local files: some of the fixture's stills are in the trash)
        let bound = bind(
            &ui,
            Pages::single(crate::common::all_local_page(store.clone())),
        );
        Self {
            store,
            ui,
            bound,
            windows,
            searched: std::cell::Cell::new(false),
            _dirs: [legacy, dir],
        }
    }

    fn import(&self, names: &[&str]) {
        let importer = FileImporter::new(self.store.clone(), MediaTools::new());
        for name in names {
            let outcome = importer
                .import_path(
                    &hydrus_testkit::fixture_path(format!("media/{name}")),
                    &FileImportOptions::default(),
                )
                .unwrap();
            assert_eq!(
                outcome.status,
                hydrus_import::ImportStatus::SuccessfulAndNew,
                "{name}"
            );
        }
    }

    /// The stored file with this sha256, and where it is on a page of
    /// everything (once the page's search has found it).
    fn find(&self, sha256: &str) -> (usize, hydrus_store::media::FileInfo) {
        // (searched once, after the imports: entered again, the predicate
        // would come off)
        if !self.searched.replace(true) {
            self.ui.invoke_search_edited("system:everything".into());
            self.ui.invoke_search_accepted();
        }
        let found = || {
            let files = self.bound.current.borrow().borrow().results().to_vec();
            let basic = self
                .store
                .read(|c| hydrus_store::media::load_basic(c, &files))
                .unwrap();
            let at = basic.iter().position(|b| b.hash.to_hex() == sha256)?;
            Some((at, basic[at].info.clone().unwrap()))
        };
        settle(&|| found().is_some());
        found().unwrap_or_else(|| {
            panic!(
                "{sha256} is on the page: {} files, {:?}",
                self.bound.current.borrow().borrow().results().len(),
                self.ui.get_search_text()
            )
        })
    }

    fn info(&self, sha256: &str) -> hydrus_store::media::FileInfo {
        self.find(sha256).1
    }

    /// Open the viewer on the file with this sha256, from a page of
    /// everything, its window drawn at 800 by 600.
    fn view(&self, sha256: &str) -> MediaViewerWindow {
        let (at, _) = self.find(sha256);
        self.ui
            .invoke_thumbnail_activated(i32::try_from(at).unwrap());
        let viewer = self
            .bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the viewer opened");
        let drawn = self.windows.get(self.windows.count() - 1).unwrap();
        headless::render(&drawn, 800, 600);
        viewer
    }
}

fn pixels(image: &slint::Image) -> slint::SharedPixelBuffer<slint::Rgba8Pixel> {
    image
        .to_rgba8()
        .unwrap_or_else(|| slint::SharedPixelBuffer::new(0, 0))
}

/// The player mpv made for the file with this sha256 (the latest, should
/// an earlier one linger).
fn player_of(sha256: &str) -> Option<u64> {
    hydrus_gui::live_mpv_property("path")
        .into_iter()
        .filter(|(_, path)| path.as_deref().is_some_and(|p| p.contains(sha256)))
        .map(|(id, _)| id)
        .max()
}

/// An mpv property of player `id`.
fn property(id: u64, name: &str) -> Option<String> {
    hydrus_gui::live_mpv_property(name)
        .into_iter()
        .find(|(of, _)| *of == id)
        .and_then(|(_, value)| value)
}

/// The viewer's picture, once it has one.
fn picture(viewer: &MediaViewerWindow) -> slint::SharedPixelBuffer<slint::Rgba8Pixel> {
    settle(&|| viewer.get_media().to_rgba8().is_some());
    pixels(&viewer.get_media())
}

/// The reference's metadata for one of the media fixtures.
fn reference_info(name: &str) -> (String, serde_json::Value) {
    let media = hydrus_testkit::fixture_json("media.json");
    let file = media["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["file"] == name)
        .unwrap_or_else(|| panic!("{name} in media.json"));
    (
        file["hashes"]["sha256"].as_str().unwrap().to_owned(),
        file["info"].clone(),
    )
}

// leaf: audit-media-viewer-playback
#[test]
fn stills_show_the_reference_s_pixels_with_no_bar_or_volume() {
    let client = Client::basic();
    let render = hydrus_testkit::fixture_json("render.json");
    let stills: Vec<_> = render["renders"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r["key"] == "full"
                && r["status"] == 200
                && r["params"].as_object().unwrap().len() == 1
                // (the two WebPs are deleted from the fixture's local
                // files after the recording: no viewer shows them)
                && !["webp_static.webp", "webp_alpha.webp"].contains(&r["file"].as_str().unwrap())
        })
        .collect();
    assert_eq!(stills.len(), 7, "the fixture's stills");
    for recorded in stills {
        let name = recorded["file"].as_str().unwrap();
        let viewer = client.view(recorded["params"]["hash"].as_str().unwrap());
        let image = &recorded["image"];
        let size = [
            u32::try_from(image["size"][0].as_u64().unwrap()).unwrap(),
            u32::try_from(image["size"][1].as_u64().unwrap()).unwrap(),
        ];
        // (its thumbnail first, until the whole file is decoded)
        settle(&|| {
            let shown = pixels(&viewer.get_media());
            [shown.width(), shown.height()] == size
        });
        let shown = picture(&viewer);
        assert_eq!(
            [shown.width(), shown.height()],
            size,
            "{name}: decoded whole, at its own resolution"
        );
        let bytes: Vec<u8> = match image["mode"].as_str().unwrap() {
            "RGB" => shown
                .as_bytes()
                .chunks(4)
                .flat_map(|p| p[..3].to_vec())
                .collect(),
            "RGBA" => shown.as_bytes().to_vec(),
            other => panic!("{other}"),
        };
        assert_eq!(
            hydrus_media::hash_bytes(&bytes).sha256.to_hex(),
            image["pixels_sha256"].as_str().unwrap(),
            "{name}: the reference's pixels"
        );
        assert!(!viewer.get_scanbar_shown(), "{name}: no bar");
        assert!(!viewer.get_volume_shown(), "{name}: no volume");
        viewer.invoke_close_requested();
    }
}

// leaf: audit-media-viewer-playback
#[test]
fn gif_apng_and_webm_play_through_mpv_with_the_reference_s_bar() {
    if mpv::skip_without_libmpv() {
        return;
    }
    let client = Client::basic();
    let names = [
        "gif_anim.gif",
        "apng_3frames.png",
        "webm_vp8.webm",
        "webm_vp9_opus.webm",
    ];
    client.import(&names);
    for name in names {
        let (sha256, info) = reference_info(name);
        let duration = info["duration_ms"].as_u64().unwrap();
        let frames = info["num_frames"].as_u64().unwrap();
        // (the store parsed it as the reference did)
        let ours = client.info(&sha256);
        assert_eq!(
            (ours.duration_ms, ours.num_frames, ours.has_audio),
            (
                Some(duration),
                Some(frames),
                info["has_audio"].as_bool().unwrap()
            ),
            "{name}"
        );
        let bar = Scanbar::new(Some(duration), Some(frames)).unwrap();
        let viewer = client.view(&sha256);
        // mpv has it
        let playing = || {
            hydrus_gui::live_mpv_property("path")
                .into_iter()
                .any(|(_, path)| path.is_some_and(|p| p.contains(&sha256)))
        };
        assert!(settle(&playing), "{name}: mpv plays it");
        // the bar, as the reference's, by frame and time from the start
        assert!(viewer.get_scanbar_shown(), "{name}");
        assert_eq!(viewer.get_scanbar_text(), bar.at(0.0).1.as_str(), "{name}");
        // its frames come in place of the thumbnail
        let first = picture(&viewer);
        assert!(
            settle(&|| {
                let now = pixels(&viewer.get_media());
                (now.width(), now.height()) != (first.width(), first.height())
                    || now.as_bytes() != first.as_bytes()
            }),
            "{name}: frames shown"
        );
        // it plays on, and goes round again (a file this short loops
        // within a few seconds)
        let progress = || viewer.get_scanbar_progress();
        assert!(settle(&|| progress() > 0.0), "{name}: the bar moves");
        let looped = std::cell::Cell::new(false);
        let last = std::cell::Cell::new(progress());
        settle(&|| {
            let now = progress();
            if now + 0.05 < last.get() {
                looped.set(true);
            }
            last.set(now);
            looped.get()
        });
        assert!(looped.get(), "{name}: it loops");
        // the text is always the reference's for where the bar is
        let text = viewer.get_scanbar_text().to_string();
        assert!(
            text.starts_with(char::is_numeric) && text.contains(&format!("/{frames} - ")),
            "{name}: {text}"
        );
        // sound has the volume control; silence doesn't
        assert_eq!(
            viewer.get_volume_shown(),
            info["has_audio"].as_bool().unwrap(),
            "{name}"
        );
        viewer.invoke_close_requested();
        assert!(client.bound.viewer.borrow().is_none());
    }
}

// leaf: audit-media-viewer-playback
#[test]
fn embedded_cover_art_is_an_audio_file_s_picture() {
    if mpv::skip_without_libmpv() {
        return;
    }
    let client = Client::basic();
    client.import(&["flac.flac", "flac_cover.flac", "mp3_cover.mp3"]);
    for (name, cover) in [
        ("flac_cover.flac", true),
        ("mp3_cover.mp3", true),
        ("flac.flac", false),
    ] {
        let (sha256, _) = reference_info(name);
        let viewer = client.view(&sha256);
        let thumbnail = picture(&viewer);
        assert!(
            settle(&|| player_of(&sha256).is_some()),
            "{name}: mpv plays it"
        );
        let player = player_of(&sha256).unwrap();
        let album_art =
            || property(player, "current-tracks/video/albumart").as_deref() == Some("yes");
        let shown_other = || {
            let now = pixels(&viewer.get_media());
            (now.width(), now.height()) != (thumbnail.width(), thumbnail.height())
                || now.as_bytes() != thumbnail.as_bytes()
        };
        if cover {
            assert!(settle(&album_art), "{name}: mpv shows its cover");
            assert!(
                settle(&shown_other),
                "{name}: the cover replaces the thumbnail"
            );
            // the picture mpv shows is the file's own, 60 by 60 (as ffprobe
            // reads the fixtures' attached pictures)...
            assert_eq!(
                (
                    property(player, "video-params/w").as_deref(),
                    property(player, "video-params/h").as_deref()
                ),
                (Some("60"), Some("60")),
                "{name}"
            );
            // ...and the frame shown holds it square, centred between bars
            let shown = pixels(&viewer.get_media());
            let (w, h) = (shown.width() as usize, shown.height() as usize);
            let at = |x: usize, y: usize| &shown.as_bytes()[(y * w + x) * 4..(y * w + x) * 4 + 3];
            let column_blank = |x: usize| (0..h).all(|y| at(x, y) == at(0, 0));
            let side = w.min(h);
            let bar = (w - side) / 2;
            assert!(w != h && bar > 0, "{name}: {w}x{h}");
            assert!(
                (0..bar.saturating_sub(1)).all(column_blank) && (w - bar + 1..w).all(column_blank),
                "{name}: bars beside it"
            );
            assert!(
                !column_blank(w / 2) && !column_blank(bar + 2) && !column_blank(w - bar - 3),
                "{name}: the picture fills the square between them"
            );
        } else {
            // (it plays: the bar moves; but it has no picture of its own)
            assert!(settle(&|| viewer.get_scanbar_progress() > 0.0), "{name}");
            assert!(!album_art(), "{name}");
            assert!(!shown_other(), "{name}: its thumbnail stays");
        }
        viewer.invoke_close_requested();
    }
}

/// What a recorded case asks of the options.
fn audio_of(case: &serde_json::Value, volumes: &serde_json::Value) -> AudioSettings {
    let volume = |key: &str| u8::try_from(volumes[key].as_u64().unwrap()).unwrap();
    AudioSettings {
        global_volume: volume("global"),
        preview_volume: volume("preview"),
        viewer_volume: volume("viewer"),
        global_mute: case["global_mute"].as_bool().unwrap(),
        preview_mute: case["preview_mute"].as_bool().unwrap(),
        viewer_mute: case["viewer_mute"].as_bool().unwrap(),
        viewer_uses_its_own_volume: case["viewer_uses_its_own_volume"].as_bool().unwrap(),
    }
}

/// The volume and mute the reference's viewer plays at for these options,
/// from the recorded case that has them.
fn recorded_sound(audio: AudioSettings) -> (u64, bool) {
    let recorded = hydrus_testkit::fixture_json("preview_audio.json");
    let volumes = &recorded["volumes"];
    let case = recorded["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            let theirs = audio_of(case, volumes);
            (
                theirs.global_mute,
                theirs.preview_mute,
                theirs.viewer_mute,
                theirs.viewer_uses_its_own_volume,
            ) == (
                audio.global_mute,
                audio.preview_mute,
                audio.viewer_mute,
                audio.viewer_uses_its_own_volume,
            )
        })
        .expect("a recorded case");
    let volume = case["viewer_volume"].as_u64().unwrap();
    // (the recording's volumes are fixed; map the one it chose to ours)
    let volume = if volume == volumes["viewer"].as_u64().unwrap() {
        u64::from(audio.viewer_volume)
    } else {
        assert_eq!(volume, volumes["global"].as_u64().unwrap());
        u64::from(audio.global_volume)
    };
    (volume, case["viewer_muted"].as_bool().unwrap())
}

fn keep(store: &Store, audio: AudioSettings) {
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &audio))
        .unwrap();
}

fn audio(store: &Store) -> AudioSettings {
    store.read(hydrus_store::settings::get).unwrap()
}

/// Player `id`'s volume and mute, once they are `want`.
fn player_sound(id: u64, want: (u64, bool)) -> (u64, bool) {
    let now = || {
        let volume = property(id, "volume").and_then(|v| v.parse::<f64>().ok());
        let mute = property(id, "mute");
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        (
            volume.map_or(u64::MAX, |v| v.round() as u64),
            mute.as_deref() == Some("yes"),
        )
    };
    settle(&|| now() == want);
    now()
}

// leaf: audit-media-viewer-playback
#[test]
fn volume_and_mute_reach_the_playing_audio_as_the_reference_s_viewer_sets_them() {
    if mpv::skip_without_libmpv() {
        return;
    }
    let client = Client::basic();
    client.import(&["flac_cover.flac"]);
    let (sha256, _) = reference_info("flac_cover.flac");
    let recorded = hydrus_testkit::fixture_json("preview_audio.json");
    let volumes = &recorded["volumes"];
    let cases = recorded["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32);

    // as it opens: the options as they are then
    let opening = cases
        .iter()
        .find(|c| c["viewer_mute"] == true && c["viewer_uses_its_own_volume"] == true)
        .unwrap();
    keep(&client.store, audio_of(opening, volumes));
    let viewer = client.view(&sha256);
    assert!(settle(&|| player_of(&sha256).is_some()));
    let player = player_of(&sha256).unwrap();
    let want = (
        opening["viewer_volume"].as_u64().unwrap(),
        opening["viewer_muted"].as_bool().unwrap(),
    );
    assert_eq!(player_sound(player, want), want, "as it opened");

    // changed elsewhere while it plays (ctrl+g in the main window, the
    // Options window): every recorded case
    for case in cases {
        keep(&client.store, audio_of(case, volumes));
        viewer.invoke_audio_changed();
        let want = (
            case["viewer_volume"].as_u64().unwrap(),
            case["viewer_muted"].as_bool().unwrap(),
        );
        assert_eq!(player_sound(player, want), want, "{case}");
    }

    // its own control: the slider moves the volume it plays at (its own
    // or the global one), and its mutes flip
    for own in [false, true] {
        keep(
            &client.store,
            AudioSettings {
                viewer_uses_its_own_volume: own,
                ..audio_of(&cases[0], volumes)
            },
        );
        viewer.invoke_audio_changed();
        viewer.invoke_volume_changed(17);
        let now = audio(&client.store);
        assert_eq!(
            (now.global_volume, now.viewer_volume),
            if own { (30, 17) } else { (17, 90) },
            "the slider moves the viewer's volume or the global one"
        );
        let want = recorded_sound(now);
        assert_eq!(want.0, 17);
        assert_eq!(player_sound(player, want), want, "slider, own {own}");

        viewer.invoke_flip_viewer_mute();
        let want = recorded_sound(audio(&client.store));
        assert!(want.1);
        assert_eq!(player_sound(player, want), want, "viewer mute");
        viewer.invoke_flip_viewer_mute();
        viewer.invoke_flip_global_mute();
        let want = recorded_sound(audio(&client.store));
        assert!(want.1);
        assert_eq!(player_sound(player, want), want, "global mute");
        viewer.invoke_flip_global_mute();
        let want = recorded_sound(audio(&client.store));
        assert!(!want.1);
        assert_eq!(player_sound(player, want), want, "unmuted");
    }
    viewer.invoke_close_requested();
}
