//! The media playback page against its consumers: each option is changed in
//! the real options window, applied, and what reads the saved value acts on
//! it.

use slint::ComponentHandle as _;

use hydrus_import::FileImporter;
use hydrus_media::{MediaTools, Raster};

use crate::options_gui_support::{box_of, items, row, show_page};
use crate::options_media_support::Media;

/// A 100x100 RGBA image, every pixel opaque but `faint` of them, which are
/// half transparent.
fn image(faint: usize) -> Raster {
    let mut data = vec![255u8; 100 * 100 * 4];
    for pixel in 0..faint {
        data[pixel * 4 + 3] = 128;
    }
    Raster::new(100, 100, 4, data).unwrap()
}

// leaf: audit-options-media-playback-transparency-consider-a-file-as-having-transparency-when
#[test]
fn the_transparency_strictness_is_what_the_importer_judges_images_by() {
    const LABEL: &str = "Consider a file as \"having transparency\" when:";
    let client = Media::basic();
    let (few, none) = (image(3), image(0));

    let options = client.open_options();
    show_page(&options, "media playback");
    let (_, found) = row(&options, LABEL);
    assert_eq!(box_of(&options, LABEL), "transparency");
    assert_eq!(
        items(&found),
        [
            "it has a transparency channel that a human might recognise",
            "it has a transparency channel that is not completely transparent or opaque",
            "it has a transparency channel",
        ]
    );
    assert_eq!(found.index, 0, "the reference's default");
    options.hide().unwrap();

    let level = |client: &Media| {
        client
            .setting::<hydrus_store::settings::FileHandlingSettings>()
            .transparency_strictness
    };
    // (an importer takes the saved level, as the reference does at boot)
    let judged = |client: &Media| {
        let _importer = FileImporter::new(client.store.clone(), MediaTools::new());
        (few.has_useful_alpha(), none.has_useful_alpha())
    };
    assert_eq!(judged(&client), (false, false), "a human sees neither");
    for (choice, expected) in [(1, (true, false)), (2, (true, true)), (0, (false, false))] {
        let saved_before = level(&client);
        let options = client.open_options();
        show_page(&options, "media playback");
        let (i, _) = row(&options, LABEL);
        options.invoke_choice_chosen(i, choice);
        // (nothing before apply)
        assert_eq!(level(&client), saved_before);
        options.invoke_apply();
        options.hide().unwrap();
        assert_eq!(level(&client), 2 - choice as u8);
        assert_eq!(judged(&client), expected, "choice {choice}");
    }
}
