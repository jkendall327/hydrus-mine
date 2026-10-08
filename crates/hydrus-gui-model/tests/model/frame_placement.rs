//! `GetSafeSize` and `SetInitialTLWSizeAndPosition`'s defaults, worked by
//! hand from the reference's source (`ClientGUITopLevelWindows`): there is no
//! Qt in the sandbox to record them with.
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
