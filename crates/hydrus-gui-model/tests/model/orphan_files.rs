//! Database > file maintenance > clear orphan files: its words.
use hydrus_gui_model::orphan_files::{CHOICES, final_text, found};

#[test]
fn the_popup_says_what_was_cleared() {
    assert_eq!(CHOICES, ["move them somewhere", "delete them"]);
    assert_eq!(final_text(0, 0), "no orphans found!");
    assert_eq!(
        final_text(2, 1_000),
        "2 orphan files and 1,000 orphan thumbnails cleared!"
    );
    assert_eq!(found(3, "files"), "found 3 orphan files, now deleting");
}
