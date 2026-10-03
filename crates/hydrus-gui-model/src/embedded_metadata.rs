//! Detailed file metadata presentation: the basics tree and the EXIF list's
//! selection, sorting and raw clipboard values. Decoding stays in hydrus-media.

use hydrus_core::Mime;
use hydrus_core::casefold::casefold;
use hydrus_media::ExifRow;

use crate::info_lines::InfoLine;
use crate::list_selection::ListSelection;

/// The reference's instruction above the EXIF list, including its PNG note.
pub fn exif_instruction(mime: Mime) -> String {
    let mut text = "Double-click a row to copy its value to clipboard.".to_owned();
    if mime == Mime::ImagePng {
        text.push_str("\nNote that while PNGs can store EXIF data, it is not a well-defined standard. Hydrus does not apply EXIF Orientation (rotation) metadata to PNGs.");
    }
    text
}

/// Convert the info tree to the basics block, indenting submenus two spaces.
pub fn basics(lines: &[InfoLine]) -> String {
    fn append(lines: &[InfoLine], indent: &str, out: &mut Vec<String>) {
        for line in lines {
            if let Some(children) = &line.submenu {
                out.push(format!("{indent}{}:", line.text));
                append(children, &format!("{indent}  "), out);
            } else {
                out.push(format!("{indent}{}", line.text));
            }
        }
    }
    let mut out = Vec::new();
    append(lines, "", &mut out);
    out.join("\n")
}

/// EXIF rows whose stable indices preserve selection when columns are sorted.
#[derive(Debug, Clone)]
pub struct ExifList {
    /// The decoded rows, initially sorted by numeric tag id.
    pub rows: Vec<ExifRow>,
    /// Stable row indices in display order.
    pub order: Vec<usize>,
    /// Extended selection, as in the reference's list.
    pub selection: ListSelection<usize>,
}

impl ExifList {
    /// Start with the decoder's numeric id order and no selection.
    pub fn new(rows: Vec<ExifRow>) -> Self {
        Self {
            order: (0..rows.len()).collect(),
            rows,
            selection: ListSelection::default(),
        }
    }

    /// Sort one of the three columns, casefolding text as the reference does.
    pub fn sort(&mut self, column: usize, ascending: bool) {
        let rows = &self.rows;
        self.order.sort_by(|&a, &b| {
            let (a, b) = (&rows[a], &rows[b]);
            let key = |row: &ExifRow| {
                (
                    row.id,
                    casefold(if row.label == "Unknown" {
                        "zzz"
                    } else {
                        &row.label
                    }),
                    casefold(&row.copy.replace('\0', "[null]")),
                )
            };
            let (a, b) = (key(a), key(b));
            let cmp = match column {
                0 => a.0.cmp(&b.0),
                1 => a.1.cmp(&b.1),
                _ => a.2.cmp(&b.2),
            }
            .then_with(|| a.cmp(&b));
            if ascending { cmp } else { cmp.reverse() }
        });
    }

    /// Copy the first selected row in display order, preserving its raw value.
    pub fn copy(&self) -> Option<&str> {
        self.selection
            .in_order(&self.order)
            .first()
            .map(|&i| self.rows[i].copy.as_str())
    }
}
