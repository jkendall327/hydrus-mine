//! The "force filetypes" dialog (the reference's
//! `EditFilesForcedFiletypePanel`, opened by manage > force filetype): a
//! warning, what the files are and are forced to, and the filetype to
//! force them all to (or to remove forcing).

use std::collections::BTreeMap;

use hydrus_core::Mime;

/// The dialog's title.
pub const TITLE: &str = "force filetypes";

/// The filetypes offered, in the reference's order (its general classes'
/// groups, then the repository update files), by name.
const ORDER: &[&str] = &[
    "jpeg",
    "png",
    "static gif",
    "webp",
    "jxl",
    "avif",
    "bitmap",
    "heic",
    "heif",
    "icon",
    "qoi",
    "tiff",
    "animated gif",
    "apng",
    "animated webp",
    "animated jxl",
    "avif sequence",
    "heic sequence",
    "heif sequence",
    "ugoira",
    "mp4",
    "webm",
    "matroska",
    "avi",
    "flv",
    "quicktime",
    "mpeg",
    "ogv",
    "realvideo",
    "wmv",
    "mp3",
    "ogg",
    "flac",
    "m4a",
    "matroska audio",
    "mp4 audio",
    "realaudio",
    "tta",
    "wave",
    "wavpack",
    "wma",
    "flash",
    "pdf",
    "epub",
    "djvu",
    "docx",
    "xlsx",
    "pptx",
    "doc",
    "xls",
    "ppt",
    "rtf",
    "clip",
    "krita",
    "ora",
    "paint.net",
    "procreate",
    "psd",
    "sai2",
    "svg",
    "xcf",
    "cbz",
    "7z",
    "gzip",
    "rar",
    "zip",
    "application/hydrus-update-definitions",
    "application/hydrus-update-content",
];

/// The filetypes offered, in order.
pub fn mimes_in_order() -> Vec<Mime> {
    ORDER
        .iter()
        .filter_map(|name| Mime::ALL.iter().copied().find(|m| m.human_name() == *name))
        .collect()
}

/// A choice's label: "image - png", or a type with no class by itself.
pub fn label(mime: Mime) -> String {
    match mime.general_class() {
        Some(class) => format!("{} - {}", class.human_name(), mime.human_name()),
        None => mime.human_name().to_owned(),
    }
}

/// The dialog over files whose original filetypes are `original` and
/// (those forced) are forced to `forced`, each counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForceFiletype {
    pub text: String,
    /// The choices: each label, and the filetype it forces (`None`
    /// removes forcing), first chosen.
    pub choices: Vec<(String, Option<Mime>)>,
}

impl ForceFiletype {
    pub fn new(original: &BTreeMap<Mime, usize>, forced: &BTreeMap<Mime, usize>) -> Self {
        let human = |n: usize| hydrus_core::numbers::human_int(n as u64);
        let total: usize = original.values().sum();
        let total_forced: usize = forced.values().sum();
        let order = mimes_in_order();
        let mut choices = Vec::new();
        if total_forced > 0 {
            choices.push(("remove all forced filetypes".to_owned(), None));
        }
        // (one filetype to start with can't be forced to itself)
        let not_this = (original.len() == 1)
            .then(|| original.keys().next().copied())
            .flatten();
        for &mime in &order {
            if Some(mime) != not_this {
                choices.push((label(mime), Some(mime)));
            }
        }
        let summary = |counts: &BTreeMap<Mime, usize>| {
            order
                .iter()
                .filter_map(|m| {
                    counts
                        .get(m)
                        .map(|c| format!("{} {}", human(*c), m.human_name()))
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let forced_summary = if total_forced == 0 {
            "None are currently forced to be anything else.".to_owned()
        } else if total_forced == total {
            format!("All are currently being forced: {}.", summary(forced))
        } else {
            format!(
                "{} are currently being forced, to: {}.",
                human(total_forced),
                summary(forced)
            )
        };
        let text = format!(
            "WARNING: This is advanced and experimental! Be careful!\n\nThis will override what hydrus thinks the filetype is for all of these files. Files will be renamed to receive their new file extensions. The original filetype is not forgotten, and this can be undone.\n\nOf the {} files, there are {}. {forced_summary}",
            human(total),
            summary(original)
        );
        Self { text, choices }
    }
}
