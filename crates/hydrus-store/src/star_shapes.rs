//! The rating SVGs a numerical or like/dislike service can draw its stars
//! with (the reference's `static/star_shapes`): the ones it ships, and the
//! user's own in `static/star_shapes` of the database directory.

use std::path::{Path, PathBuf};

/// The rating SVGs the reference ships (`static/star_shapes`).
pub const BUNDLED: &[(&str, &[u8])] = &[
    (
        "architecture",
        include_bytes!("../../../static/star_shapes/architecture.svg"),
    ),
    (
        "art-palette",
        include_bytes!("../../../static/star_shapes/art-palette.svg"),
    ),
    (
        "cinema-reel",
        include_bytes!("../../../static/star_shapes/cinema-reel.svg"),
    ),
    (
        "eye",
        include_bytes!("../../../static/star_shapes/eye.svg"),
    ),
    (
        "heart-cute",
        include_bytes!("../../../static/star_shapes/heart-cute.svg"),
    ),
    (
        "inspiration",
        include_bytes!("../../../static/star_shapes/inspiration.svg"),
    ),
    (
        "scenery",
        include_bytes!("../../../static/star_shapes/scenery.svg"),
    ),
    (
        "smile",
        include_bytes!("../../../static/star_shapes/smile.svg"),
    ),
    (
        "spiral",
        include_bytes!("../../../static/star_shapes/spiral.svg"),
    ),
    (
        "star",
        include_bytes!("../../../static/star_shapes/star.svg"),
    ),
    (
        "wallpaper",
        include_bytes!("../../../static/star_shapes/wallpaper.svg"),
    ),
];


/// The directory of a user's own rating SVGs.
pub fn user_dir(db_dir: &Path) -> PathBuf {
    db_dir.join("static").join("star_shapes")
}

/// The user's own SVGs: name and path.
fn user_files(db_dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(user_dir(db_dir)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
        })
        .filter_map(|path| {
            let name = path.file_stem()?.to_str()?.to_owned();
            Some((name, path))
        })
        .collect()
}

/// The names of the SVGs there are, in the reference's human order
/// (`RefreshUserIcons` over the user's directory and then the installed
/// one; its dictionary is keyed by name, so the installed SVG of a name
/// the user also has is the one it draws).
pub fn names(db_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = BUNDLED.iter().map(|(name, _)| (*name).to_owned()).collect();
    for (name, _) in user_files(db_dir) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    hydrus_core::sort::human_sort(&mut names);
    names
}

/// The SVG called `name`, if there is one.
pub fn data(db_dir: &Path, name: &str) -> Option<Vec<u8>> {
    if let Some((_, svg)) = BUNDLED.iter().find(|(n, _)| *n == name) {
        return Some(svg.to_vec());
    }
    let (_, path) = user_files(db_dir).into_iter().find(|(n, _)| n == name)?;
    std::fs::read(path).ok()
}
