//! The media viewer's keyboard button: its custom sets and their checks.
use hydrus_core::shortcuts::Settings;
use hydrus_gui_model::viewer_shortcut_menu::{checks, custom_names, flip};

#[test]
fn custom_sets_flip_on_and_off_in_order() {
    let mut settings = Settings::default();
    settings.sets.insert("ratings".into(), Vec::new());
    settings.sets.insert("archive keys".into(), Vec::new());
    let names = custom_names(&settings);
    assert_eq!(names, ["archive keys", "ratings"]);
    let mut active = Vec::new();
    flip(&mut active, "ratings");
    flip(&mut active, "archive keys");
    assert_eq!(active, ["archive keys", "ratings"]);
    assert_eq!(checks(&names, &active), [true, true]);
    flip(&mut active, "ratings");
    assert_eq!(checks(&names, &active), [true, false]);
}
