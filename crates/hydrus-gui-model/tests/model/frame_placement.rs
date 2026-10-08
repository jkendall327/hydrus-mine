//! `GetSafeSize` and `SetInitialTLWSizeAndPosition`'s defaults: worked by hand
//! from the reference's source (`ClientGUITopLevelWindows`), and replayed from
//! its recording (`oracle/record_frame_placement.py`).
use hydrus_core::windows::FrameLocation;
use hydrus_gui_model::frame_placement::{
    CHILD_POSITION_PADDING, Parent, Placement, Rect, Surroundings, initial, safe_size,
};

fn frame(gravity: (i32, i32), position: &str) -> FrameLocation {
    FrameLocation {
        remember_size: false,
        remember_position: false,
        last_size: None,
        last_position: None,
        default_gravity: gravity,
        default_position: position.into(),
        maximised: false,
        fullscreen: false,
    }
}

fn around() -> Surroundings {
    Surroundings {
        parent: Some(Parent {
            frame: Rect {
                x: 100,
                y: 50,
                width: 1000,
                height: 800,
            },
            fullscreen: false,
        }),
        display: Some(Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }),
        mouse: Some((700, 500)),
        frame_padding: (0, 0),
    }
}

// leaf: audit-options-nested-frame-location-gravity
#[test]
fn gravity_grows_a_window_toward_its_parent_and_the_display_limits_it() {
    let pad = CHILD_POSITION_PADDING;
    assert_eq!(pad, 24);
    // -1 on an axis: as large as it needs (its size hint)
    assert_eq!(safe_size((300, 200), (-1, -1), &around()), (300, 200));
    // 1: the parent's width, less the padding either side
    assert_eq!(safe_size((300, 200), (1, -1), &around()), (952, 200));
    assert_eq!(safe_size((300, 200), (-1, 1), &around()), (300, 752));
    assert_eq!(safe_size((300, 200), (1, 1), &around()), (952, 752));
    // a fraction is a fraction of that
    assert_eq!(safe_size((300, 200), (0, 0), &around()), (0, 0));
    // never larger than the display less the padding
    let mut small = around();
    small.display = Some(Rect {
        x: 0,
        y: 0,
        width: 800,
        height: 600,
    });
    assert_eq!(safe_size((300, 200), (1, 1), &small), (752, 552));
    assert_eq!(safe_size((900, 700), (-1, -1), &small), (752, 552));
    // without a parent (the main window) there is nothing to grow toward
    let mut orphan = around();
    orphan.parent = None;
    assert_eq!(safe_size((300, 200), (1, 1), &orphan), (300, 200));
}

// leaf: audit-options-nested-frame-location-gravity
#[test]
fn a_remembered_size_and_place_beat_the_defaults_and_each_default_position_is_the_references() {
    let mut f = frame((1, 1), "topleft");
    // topleft: the parent's top-left plus the padding
    assert_eq!(
        initial(&f, (300, 200), &around()),
        Placement {
            size: (952, 752),
            position: Some((124, 74)),
        }
    );
    // (and slid up and left to stay inside the display: this one would end
    // at x 1075 + 952 > 1920 - 24)
    let mut right = around();
    right.parent.as_mut().unwrap().frame.x = 1000;
    let slid = initial(&f, (300, 200), &right);
    assert_eq!(slid.size, (952, 752));
    // right edge pixel = 1024 + 952 - 1 = 1975 > 1896: move by 1896 - 1975
    assert_eq!(slid.position, Some((1024 - 79, 74)));

    // centre: the parent's centre less the window's
    f.default_position = "center".into();
    let centred = initial(&f, (300, 200), &around());
    // parent centre = (100 + 499, 50 + 399) = (599, 449); mine = (475, 375)
    assert_eq!(centred.position, Some((599 - 475, 449 - 375)));

    // mouse: the pointer less the window's centre
    f.default_position = "mouse".into();
    let mouse = initial(&f, (300, 200), &around());
    assert_eq!(mouse.position, Some((700 - 475, 500 - 375)));

    // remembered size and place come first
    f.remember_size = true;
    f.remember_position = true;
    f.last_size = Some((640, 480));
    f.last_position = Some((11, 22));
    assert_eq!(
        initial(&f, (300, 200), &around()),
        Placement {
            size: (640, 480),
            position: Some((11, 22)),
        }
    );
    // remembering without anything remembered falls back to the defaults
    f.last_size = None;
    f.last_position = None;
    f.default_position = "topleft".into();
    assert_eq!(initial(&f, (300, 200), &around()).size, (952, 752));
}

// leaf: audit-options-nested-frame-location-gravity
#[test]
fn the_frame_padding_comes_off_the_parent_and_display_sizes_but_not_a_fullscreen_parent() {
    let mut padded = around();
    // a 2-pixel border and 28-pixel title bar
    padded.frame_padding = (4, 32);
    // `parent_frame_size - frame_padding`, less 24 either side
    assert_eq!(safe_size((300, 200), (1, 1), &padded), (948, 720));
    padded.parent.as_mut().unwrap().fullscreen = true;
    assert_eq!(safe_size((300, 200), (1, 1), &padded), (952, 752));
    // the display's size loses the padding too
    padded.parent = None;
    padded.display = Some(Rect {
        x: 0,
        y: 0,
        width: 800,
        height: 600,
    });
    assert_eq!(safe_size((900, 700), (-1, -1), &padded), (748, 520));
}

// leaf: audit-options-nested-frame-location-gravity
// (not tagged audit-options-geometry: this covers default placement only, not
// screen fitting, stale-geometry restore or maximised/fullscreen persistence)
// leaf: audit-options-gui-frame-locations-flip-remember-position
#[test]
fn every_recorded_frame_setting_opens_a_window_where_the_reference_did() {
    let recorded = hydrus_testkit::fixture_json("frame_placement.json");
    let rect = |v: &serde_json::Value| Rect {
        x: i32::try_from(v[0].as_i64().unwrap()).unwrap(),
        y: i32::try_from(v[1].as_i64().unwrap()).unwrap(),
        width: i32::try_from(v[2].as_i64().unwrap()).unwrap(),
        height: i32::try_from(v[3].as_i64().unwrap()).unwrap(),
    };
    let number = |v: &serde_json::Value| i32::try_from(v.as_i64().unwrap()).unwrap();
    let main = rect(&recorded["main"]);
    let client = &recorded["main_client"];
    let around = Surroundings {
        parent: Some(Parent {
            frame: main,
            fullscreen: false,
        }),
        display: Some(rect(&recorded["screen"])),
        mouse: Some((number(&recorded["mouse"][0]), number(&recorded["mouse"][1]))),
        // (the frame padding is the main window's own: the dialog had none yet)
        frame_padding: (
            main.width - number(&client[0]),
            main.height - number(&client[1]),
        ),
    };
    let hint = (number(&recorded["hint"][0]), number(&recorded["hint"][1]));
    assert_eq!(
        number(&recorded["child_position_padding"]),
        CHILD_POSITION_PADDING
    );
    let pair = |v: &serde_json::Value| -> Option<(i32, i32)> {
        (!v.is_null()).then(|| (number(&v[0]), number(&v[1])))
    };
    for case in recorded["cases"].as_array().unwrap() {
        let f = &case["frame"];
        let frame = FrameLocation {
            remember_size: f["remember_size"].as_bool().unwrap(),
            remember_position: f["remember_position"].as_bool().unwrap(),
            last_size: pair(&f["last_size"]),
            last_position: pair(&f["last_position"]),
            default_gravity: pair(&f["gravity"]).unwrap(),
            default_position: f["position"].as_str().unwrap().into(),
            maximised: false,
            fullscreen: false,
        };
        let placed = initial(&frame, hint, &around);
        assert_eq!(
            (placed.size, placed.position),
            (pair(&case["size"]).unwrap(), pair(&case["position"])),
            "{f}"
        );
    }
}
