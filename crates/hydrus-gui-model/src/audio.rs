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
