//! The filetype tree (the reference's `OptionsPanelMimesTree`): a group's
//! code ticks all of its filetypes, as the client's default file
//! filtering has them; ticking gives specific filetypes back
//! (`GetValue`), a group's box ticking or unticking all of its.

use std::collections::BTreeSet;

use hydrus_core::mime::Mime;
use hydrus_gui_model::filetype_tree::{groups, tick};

#[test]
fn groups_tick_their_filetypes_and_give_back_specific_ones() {
    let defaults: BTreeSet<u8> = Mime::general_classes().iter().map(|m| m.code()).collect();
    let tree = groups(&defaults, &[false; 7]);
    assert_eq!(
        tree.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
        [
            "image",
            "animation",
            "video",
            "audio",
            "application",
            "image project file",
            "archive"
        ]
    );
    assert!(tree.iter().all(|g| g.ticked.iter().all(|t| *t)));
    assert!(tree.iter().all(|g| !g.expanded));
    assert_eq!(tree[0].options[..2], ["jpeg".to_owned(), "png".to_owned()]);

    // jpeg unticked: every other filetype, specifically
    let codes = tick(&defaults, 0, Some(0), false);
    assert!(!codes.contains(&Mime::ImageJpeg.code()));
    assert!(codes.contains(&Mime::ImagePng.code()));
    assert!(!codes.contains(&Mime::GeneralImage.code()));
    let tree = groups(&codes, &[true, false, false, false, false, false, false]);
    assert!(tree[0].expanded && !tree[1].expanded);
    assert_eq!(tree[0].ticked[..2], [false, true]);
    assert!(tree[1].ticked.iter().all(|t| *t));

    // a group's box: all of it, then none
    let codes = tick(&codes, 0, None, true);
    assert!(
        groups(&codes, &[])
            .iter()
            .all(|g| g.ticked.iter().all(|t| *t))
    );
    let codes = tick(&codes, 2, None, false);
    let tree = groups(&codes, &[]);
    assert!(tree[2].ticked.iter().all(|t| !*t));
    assert!(tree[3].ticked.iter().all(|t| *t));
}
