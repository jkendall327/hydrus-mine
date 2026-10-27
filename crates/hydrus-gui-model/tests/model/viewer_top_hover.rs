//! The top hover frame's tooltips: the embedded metadata button's, wrapped
//! as the reference wraps tooltips, and as Slint's Markdown takes them.

use hydrus_gui_model::viewer_top_hover::{Embedded, embedded_metadata, markdown, wrap};

#[test]
fn the_metadata_button_names_what_the_file_has() {
    assert_eq!(
        embedded_metadata(Embedded::default()),
        "show detailed file metadata"
    );
    assert_eq!(
        embedded_metadata(Embedded {
            jpeg: true,
            ..Embedded::default()
        }),
        "show detailed file metadata, including extra info"
    );
    // (several, joined with "and")
    assert_eq!(
        embedded_metadata(Embedded {
            jpeg: true,
            exif: true,
            xmp: true,
            ..Embedded::default()
        }),
        "show detailed file metadata, including exif and xmp and extra info"
    );
    assert_eq!(
        embedded_metadata(Embedded {
            jpeg: true,
            exif: true,
            xmp: true,
            iptc: true,
            human_readable: true,
            software: true,
        }),
        // (over 80 characters, broken at a word as `WrapToolTip` breaks it)
        "show detailed file metadata, including exif and xmp and iptc and human-readable\nmetadata and software/source metadata and extra info"
    );
}

#[test]
fn tooltips_wrap_at_eighty_characters_keeping_their_lines() {
    // (the reference's `WrapToolTip` on thirty words)
    let long = vec!["word"; 30].join(" ");
    assert_eq!(
        wrap(&long),
        format!(
            "{}\n{}",
            vec!["word"; 16].join(" "),
            vec!["word"; 14].join(" ")
        )
    );
    assert_eq!(wrap("a\n\nb"), "a\n\nb");
}

#[test]
fn markdown_shows_the_tooltip_s_text_as_it_is() {
    assert_eq!(
        markdown("zoom in\n\n----------\n\n+, ctrl+scroll up\n(media viewers - all->zoom: in)"),
        "zoom in\n\n\\-\\-\\-\\-\\-\\-\\-\\-\\-\\-\n\n\\+\\, ctrl\\+scroll up\\\n\\(media viewers \\- all\\-\\>zoom\\: in\\)"
    );
}
