//! The volume and mute the media viewer plays at, kept as hydrus keeps
//! them in its options (`ClientGUIMediaControls`: `ChangeVolume`,
//! `FlipMute`), so a change lasts.

use hydrus_core::media_viewer::AudioSettings;
use hydrus_store::Store;

/// The volumes and mutes now.
pub fn settings(store: &Store) -> AudioSettings {
    store.read(hydrus_store::settings::get).unwrap_or_default()
}

/// Change them, and keep the change; says what they are now.
pub fn change(store: &Store, change: impl FnOnce(&mut AudioSettings)) -> AudioSettings {
    let mut audio = settings(store);
    change(&mut audio);
    if let Err(e) = store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &audio)) {
        eprintln!("could not keep the volume: {e}");
    }
    audio
}

/// Mute or unmute everything (the global shortcut's
/// `SIMPLE_GLOBAL_AUDIO_MUTE_FLIP`, ctrl+g by default).
pub fn flip_global_mute(store: &Store) -> AudioSettings {
    change(store, |a| a.global_mute = !a.global_mute)
}

/// Whether the preview has a volume of its own
/// (`preview_uses_its_own_audio_volume`, on by default; the Options
/// window's "The preview window has its own volume").
pub fn preview_uses_its_own_volume(store: &Store) -> bool {
    store
        .read(hydrus_store::settings::get::<hydrus_store::reference_options::ReferenceOptions>)
        .unwrap_or_default()
        .boolean("preview_uses_its_own_audio_volume")
}

/// The volume and mute the preview plays at (`GetCorrectCurrentVolume`,
/// `GetCorrectCurrentMute` for `CANVAS_PREVIEW`).
pub fn preview_sound(store: &Store) -> (u8, bool) {
    let audio = settings(store);
    (
        audio.current_preview_volume(preview_uses_its_own_volume(store)),
        audio.preview_muted(),
    )
}

/// The preview's slider moved: the preview's own volume if it has one,
/// else the global one.
pub fn set_preview_volume(store: &Store, volume: u8) -> AudioSettings {
    let own = preview_uses_its_own_volume(store);
    change(store, |a| a.set_preview_volume(own, volume))
}

/// The preview's own mute button (`FlipMute( AUDIO_PREVIEW )`).
pub fn flip_preview_mute(store: &Store) -> AudioSettings {
    change(store, |a| a.preview_mute = !a.preview_mute)
}
