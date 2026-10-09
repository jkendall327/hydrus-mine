//! The media playback page's "fetch mpv audio devices" button
//! (`MediaPlaybackPanel._FetchMPVAudioDevices`): it asks mpv which audio
//! devices it can see and offers them in a chooser, whose choice fills
//! "Preferred audio output device:".

/// The button's text.
pub const BUTTON: &str = "fetch mpv audio devices";
/// The button's tooltip, wrapped as the reference's `WrapToolTip` does.
pub const TOOLTIP: &str = "This will try to spin up a new MPV instance in the background and query it for\nthe available audio devices. I do not see any way it could fail to work :^)";
/// What the button says when libmpv cannot be loaded.
pub const UNAVAILABLE: &str = "Sorry, MPV is not available!";
/// The chooser's title.
pub const TITLE: &str = "Select mpv audio device";
/// The chooser's message.
pub const MESSAGE: &str = "Populate the option with a value that mpv reports it can see.";

/// One device mpv reports: its name (what the option holds) and
/// description.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Device {
    pub name: String,
    pub description: String,
}

/// mpv's `audio-device-list` property, as its JSON string; nothing when it
/// is not that.
pub fn parse(list: &str) -> Vec<Device> {
    serde_json::from_str(list).unwrap_or_default()
}

/// The chooser's devices: mpv's, then "null", as the reference adds it
/// (`GetAudioDeviceTuples`).
pub fn offered(mut devices: Vec<Device>) -> Vec<Device> {
    devices.push(Device {
        name: "null".to_owned(),
        description: "DEBUG: Do not use any audio output device".to_owned(),
    });
    devices
}

/// A chooser button's text.
pub fn label(device: &Device) -> String {
    format!("{} - {}", device.name, device.description)
}

/// What choosing `device` sets the option to: "auto" is none (mpv's own
/// default).
pub fn value(device: &Device) -> Option<String> {
    (device.name != "auto").then(|| device.name.clone())
}
