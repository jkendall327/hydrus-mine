//! Playing a video through libmpv. Skipped where libmpv isn't installed
//! (CI), as the viewer then falls back to thumbnails.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use hydrus_gui::mpv::{self, Player};

/// The next frame with more than a few colours (the first may be blank).
fn picture(player: &Player) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(20) {
        let Some(frame) = player.wait_frame(Duration::from_secs(1)) else {
            continue;
        };
        let pixels = frame.to_rgba8()?;
        let colours: HashSet<&[u8]> = pixels.as_bytes().chunks(4).collect();
        if colours.len() > 4 {
            return Some(pixels);
        }
    }
    None
}

#[test]
fn a_video_plays_into_frames_at_the_size_asked() {
    if !mpv::available() {
        eprintln!("libmpv is not installed here; skipped");
        return;
    }
    let player = Player::new(None).unwrap();
    player.set_size(320, 240);
    player
        .load(&hydrus_testkit::fixture_path("media/mp4_h264.mp4"))
        .unwrap();
    let frame = picture(&player).expect("the video's frames");
    assert_eq!((frame.width(), frame.height()), (320, 240));
    assert!(frame.as_bytes().chunks(4).all(|p| p[3] == 255), "opaque");

    player.set_size(200, 100);
    let frame = picture(&player).expect("frames at the new size");
    assert_eq!((frame.width(), frame.height()), (200, 100));

    player.toggle_pause().unwrap();
    player.stop().unwrap();
}
