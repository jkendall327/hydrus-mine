//! What a player is told of the mpv options as a file loads: looping the
//! playlist rather than the file (`mpv_loop_playlist_instead_of_file`), and
//! the audio device (`mpv_preferred_audio_device`, or `null` for a file
//! without sound under `mpv_null_audio_on_silent_media`), as the reference's
//! `MPVWidget.UpdateConfAndCoreOptions` and `MPVMediator.SetAudioDeviceFromOptions`
//! set them.

use hydrus_store::reference_options::ReferenceOptions;

/// The mpv settings for one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub loop_playlist: bool,
    pub audio_device: String,
}

impl Plan {
    /// The settings for a file that has sound or not.
    pub fn for_file(options: &ReferenceOptions, has_audio: bool) -> Self {
        let audio_device = if options.boolean("mpv_null_audio_on_silent_media") && !has_audio {
            "null".to_owned()
        } else {
            // (`GetNoneableString`, 'auto' for none)
            options
                .string("mpv_preferred_audio_device")
                .unwrap_or_else(|| "auto".to_owned())
        };
        Self {
            loop_playlist: options.boolean("mpv_loop_playlist_instead_of_file"),
            audio_device,
        }
    }

    /// The `set` commands that make mpv so: `loop` and `loop-playlist` are
    /// opposites (`player['loop'] = not loop_playlist`), then the device.
    pub fn commands(&self) -> Vec<Vec<String>> {
        let set =
            |name: &str, value: &str| vec!["set".to_owned(), name.to_owned(), value.to_owned()];
        vec![
            set("loop", if self.loop_playlist { "no" } else { "inf" }),
            set(
                "loop-playlist",
                if self.loop_playlist { "inf" } else { "no" },
            ),
            set("audio-device", &self.audio_device),
        ]
    }
}
