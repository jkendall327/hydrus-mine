//! The media viewer's top hover frame (`CanvasHoverFrameTop` and its
//! subclasses): the tooltips of its buttons that name their shortcuts, and
//! the embedded metadata button's.

use hydrus_core::shortcuts::Settings;

/// `SIMPLE_VIEW_FIRST`.
pub const VIEW_FIRST: i32 = 97;
/// `SIMPLE_VIEW_LAST`.
pub const VIEW_LAST: i32 = 98;
/// `SIMPLE_VIEW_NEXT`.
pub const VIEW_NEXT: i32 = 99;
/// `SIMPLE_VIEW_PREVIOUS`.
pub const VIEW_PREVIOUS: i32 = 100;
/// `SIMPLE_ZOOM_IN`.
pub const ZOOM_IN: i32 = 101;
/// `SIMPLE_ZOOM_OUT`.
pub const ZOOM_OUT: i32 = 102;
/// `SIMPLE_OPEN_FILE_IN_EXTERNAL_PROGRAM`.
pub const OPEN_EXTERNALLY: i32 = 61;
/// `SIMPLE_ARCHIVE_DELETE_FILTER_BACK`.
pub const ARCHIVE_DELETE_BACK: i32 = 0;
/// `SIMPLE_ARCHIVE_DELETE_FILTER_SKIP`.
pub const ARCHIVE_DELETE_SKIP: i32 = 3;
/// `SIMPLE_DUPLICATE_FILTER_BACK`.
pub const DUPLICATE_BACK: i32 = 18;
/// `SIMPLE_DUPLICATE_FILTER_SKIP`.
pub const DUPLICATE_SKIP: i32 = 22;

/// The tooltips of the buttons that show their shortcuts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tips {
    pub first: String,
    pub previous: String,
    pub next: String,
    pub last: String,
    pub zoom_in: String,
    pub zoom_out: String,
    pub zoom_switch: String,
    pub open_externally: String,
}

/// The browser's (`CanvasHoverFrameTopNavigableList`) tooltips, with the
/// zoom switch button's command (the `zoom_switch_command` option).
pub fn tips(settings: &Settings, zoom_switch_command: i32) -> Tips {
    let tip = |text: &str, command: i32| with_shortcuts(text, command, settings);
    Tips {
        first: tip("first", VIEW_FIRST),
        previous: tip("previous", VIEW_PREVIOUS),
        next: tip("next", VIEW_NEXT),
        last: tip("last", VIEW_LAST),
        zoom_in: tip("zoom in", ZOOM_IN),
        zoom_out: tip("zoom out", ZOOM_OUT),
        zoom_switch: tip("zoom switch", zoom_switch_command),
        open_externally: tip("open externally", OPEN_EXTERNALLY),
    }
}

/// A tooltip naming the shortcuts that run `command`, set by set
/// (`ShortcutAwareToolTipMixin._RefreshToolTip`), wrapped as the reference
/// wraps tooltips.
pub fn with_shortcuts(text: &str, command: i32, settings: &Settings) -> String {
    let mut tip = format!("{text}\n\n----------");
    let name = crate::shortcut_sets::simple_names()
        .get(&command.to_string())
        .cloned()
        .unwrap_or_else(|| format!("command {command}"));
    let mut any = false;
    // (the sets in name order, as the reference sorts them)
    for (set, bindings) in &settings.sets {
        let mut shortcuts: Vec<String> = bindings
            .iter()
            .filter(|b| b.content.is_none() && b.action == command)
            .map(|b| b.gesture.text(settings.primary_labels))
            .collect();
        if shortcuts.is_empty() {
            continue;
        }
        any = true;
        shortcuts.sort();
        tip.push_str(&format!(
            "\n\n{}\n({}->{})",
            shortcuts.join(", "),
            crate::shortcut_sets::pretty_name(set),
            name
        ));
    }
    if !any {
        tip.push_str(&format!("\n\nno shortcuts set\n({name})"));
    }
    wrap(&tip)
}

/// What a file's embedded metadata holds, as the metadata button's tooltip
/// names it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Embedded {
    /// A JPEG (its "extra info": the reference shows its quality and
    /// subsampling).
    pub jpeg: bool,
    pub exif: bool,
    pub xmp: bool,
    pub iptc: bool,
    pub human_readable: bool,
    pub software: bool,
}

/// The embedded metadata button's tooltip (`_ResetButtons`): what the file
/// has to show.
pub fn embedded_metadata(has: Embedded) -> String {
    let mut tip = "show detailed file metadata".to_owned();
    let parts: Vec<&str> = [
        (has.exif, "exif"),
        (has.xmp, "xmp"),
        (has.iptc, "iptc"),
        (has.human_readable, "human-readable metadata"),
        (has.software, "software/source metadata"),
        (has.jpeg, "extra info"),
    ]
    .into_iter()
    .filter(|(has, _)| *has)
    .map(|(_, part)| part)
    .collect();
    if !parts.is_empty() {
        tip.push_str(&format!(", including {}", parts.join(" and ")));
    }
    wrap(&tip)
}

/// A tooltip's lines broken at words to 80 characters (`WrapToolTip`).
pub fn wrap(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.split('\n') {
        if line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut words: Vec<&str> = Vec::new();
        let mut chars = 0;
        for word in line.split(' ') {
            let length = word.chars().count();
            if chars + words.len() + length > 80 && !words.is_empty() {
                lines.push(words.join(" "));
                words = vec![word];
                chars = length;
            } else {
                words.push(word);
                chars += length;
            }
        }
        if !words.is_empty() {
            lines.push(words.join(" "));
        }
    }
    lines.join("\n")
}

/// A tooltip as Slint's `Tooltip` takes it (Markdown): every punctuation
/// mark escaped, and its lines kept as lines.
pub fn markdown(text: &str) -> String {
    let lines: Vec<String> = text
        .split('\n')
        .map(|line| {
            line.chars()
                .map(|c| {
                    if c.is_ascii_punctuation() {
                        format!("\\{c}")
                    } else {
                        c.to_string()
                    }
                })
                .collect()
        })
        .collect();
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            // (a hard break between two lines; a blank line stays a gap)
            if !line.is_empty() && !lines[i - 1].is_empty() {
                out.push('\\');
            }
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}
