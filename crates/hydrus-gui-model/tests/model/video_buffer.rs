//! The animation frame buffer against the reference's `RasterContainerVideo`
//! (`video_buffer.json`): its size from "Memory for video buffer", and which
//! frames it decodes as a nine-frame WebP plays twice through.

use hydrus_gui_model::video_buffer::{Buffer, DEFAULT_BYTES, estimate, frames_kept};

fn fixture() -> serde_json::Value {
    hydrus_testkit::fixture_json("video_buffer.json")
}

fn u(value: &serde_json::Value) -> u64 {
    value.as_u64().unwrap()
}

#[test]
fn the_buffer_keeps_two_thirds_behind_and_a_third_ahead_as_the_reference_sizes_it() {
    let fixture = fixture();
    for case in fixture["sizes"].as_array().unwrap() {
        let target = &case["target_resolution"];
        let resolution = (u(&target[0]) as u32, u(&target[1]) as u32);
        // (a clip timed by its frames alone takes their sum)
        let duration = case["duration_ms"]
            .as_u64()
            .or(case["frame_durations_sum"].as_u64());
        let kept = frames_kept(
            u(&case["buffer"]),
            resolution,
            duration,
            case["num_frames"].as_u64(),
        );
        assert_eq!(
            kept,
            (
                u(&case["backwards"]) as usize,
                u(&case["forwards"]) as usize
            ),
            "{case}"
        );
    }
}

#[test]
fn the_options_estimate_reads_as_the_reference_s() {
    let panel = &fixture()["panel"];
    assert_eq!(u(&panel["default"]["value"]), DEFAULT_BYTES);
    assert_eq!(estimate(DEFAULT_BYTES), panel["default"]["text"]);
    for edit in panel["edits"].as_array().unwrap() {
        assert_eq!(estimate(u(&edit["value"])), edit["text"], "{edit}");
    }
    assert_eq!(estimate(u(&panel["saved"])), panel["reopened"]["text"]);
}

/// Decode until the render thread has nothing to do, as the recorder waits
/// for the reference's; the indexes decoded.
fn run(buffer: &mut Buffer<()>) -> Vec<i64> {
    let mut decoded = Vec::new();
    while let Some(job) = buffer.next_job() {
        decoded.push(job.index);
        buffer.rendered(job.index, ());
        assert!(decoded.len() < 1000, "the render thread never settles");
    }
    decoded
}

#[test]
fn a_loop_that_fits_is_decoded_once_and_one_that_does_not_is_decoded_again() {
    let fixture = fixture();
    let animation = &fixture["animation"];
    let durations: Vec<u64> = animation["durations"]
        .as_array()
        .unwrap()
        .iter()
        .map(u)
        .collect();
    let count = durations.len();
    let size = (
        u(&animation["size"][0]) as u32,
        u(&animation["size"][1]) as u32,
    );
    for case in fixture["loops"].as_array().unwrap() {
        let kept = frames_kept(
            u(&case["buffer"]),
            size,
            Some(durations.iter().sum()),
            Some(count as u64),
        );
        assert_eq!(
            kept,
            (
                u(&case["backwards"]) as usize,
                u(&case["forwards"]) as usize
            )
        );
        let mut buffer = Buffer::new(count, kept);
        buffer.get_ready_for(0);
        let first: Vec<i64> = serde_json::from_value(case["decoded_first"].clone()).unwrap();
        assert_eq!(run(&mut buffer), first, "{}", case["buffer"]);
        let mut total = first.len();
        for (step, recorded) in case["steps"].as_array().unwrap().iter().enumerate() {
            let shown = step % count;
            assert_eq!(u(&recorded["shown"]) as usize, shown);
            assert!(buffer.get(shown).is_some(), "{} {step}", case["buffer"]);
            // (GetFrame: the next frame is wanted)
            buffer.get_ready_for((shown + 1) % count);
            let decoded = run(&mut buffer);
            let wanted: Vec<i64> = serde_json::from_value(recorded["decoded"].clone()).unwrap();
            assert_eq!(decoded, wanted, "{} {step}", case["buffer"]);
            let held: Vec<i64> = serde_json::from_value(recorded["held"].clone()).unwrap();
            assert_eq!(buffer.held(), held, "{} {step}", case["buffer"]);
            let bounds: (i64, i64) = serde_json::from_value(recorded["buffer"].clone()).unwrap();
            assert_eq!(buffer.bounds(), bounds, "{} {step}", case["buffer"]);
            total += decoded.len();
        }
        assert_eq!(total as u64, u(&case["total_decoded"]));
        // (the whole loop held: its second time round decodes nothing)
        if kept.0 + 1 + kept.1 >= count {
            assert_eq!(total, count);
        } else {
            assert!(total > count);
        }
    }
}
