//! Playing a video through libmpv. Skipped where libmpv isn't installed
//! (CI), as the viewer then falls back to thumbnails.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use hydrus_gui::mpv::{self, Player};

/// The next frame of `size` with more than a few colours (the first may be
/// blank, and frames made before a resize may still be queued).
fn picture(
    player: &Player,
    size: (u32, u32),
) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(20) {
        let Some(frame) = player.wait_frame(Duration::from_secs(1)) else {
            continue;
        };
        let pixels = frame.to_rgba8()?;
        if (pixels.width(), pixels.height()) != size {
            continue;
        }
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
    let frame = picture(&player, (320, 240)).expect("the video's frames at the size asked");
    assert_eq!((frame.width(), frame.height()), (320, 240));
    assert!(frame.as_bytes().chunks(4).all(|p| p[3] == 255), "opaque");

    player.set_size(200, 100);
    let frame = picture(&player, (200, 100)).expect("frames at the new size");
    assert_eq!((frame.width(), frame.height()), (200, 100));

    // where it is and how long it is; it seeks exactly
    let duration = player.duration_ms().expect("its duration, once loaded");
    assert!(duration > 500.0, "{duration}");
    player.toggle_pause().unwrap();
    assert!(player.paused());
    let target = (duration / 2.0).round();
    player.seek_ms(target).unwrap();
    let started = Instant::now();
    let mut position = player.position_ms();
    while position.is_none_or(|p| (p - target).abs() > 100.0)
        && started.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(20));
        position = player.position_ms();
    }
    let position = position.unwrap();
    assert!(
        (position - target).abs() <= 100.0,
        "{position} for {target}"
    );
    player.toggle_pause().unwrap();
    assert!(!player.paused());

    // it plays at the volume, muted or not, asked
    player.set_audio(40, true).unwrap();
    assert_eq!(player.volume(), Some(40.0));
    assert!(player.muted());
    player.set_audio(70, false).unwrap();
    assert_eq!(player.volume(), Some(70.0));
    assert!(!player.muted());
    player.stop().unwrap();
}
