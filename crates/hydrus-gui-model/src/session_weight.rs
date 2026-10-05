//! Pages > weight > "total session weight"'s information
//! (`_ShowPageWeightInfo`): a file weighs 1, a URL 20.
use hydrus_core::numbers::human_int;

/// A count of files' weight (`ConvertNumHashesToWeight`).
pub const fn files_weight(files: u64) -> u64 {
    files
}

/// A count of URLs' weight (`ConvertNumSeedsToWeight`).
pub const fn urls_weight(urls: u64) -> u64 {
    urls * 20
}

/// The message, from the open pages' count, files and URLs and the closed
/// ones'.
pub fn report(
    open_pages: usize,
    (open_files, open_urls): (u64, u64),
    closed_pages: usize,
    (closed_files, closed_urls): (u64, u64),
) -> String {
    let (of, ou) = (files_weight(open_files), urls_weight(open_urls));
    let (cf, cu) = (files_weight(closed_files), urls_weight(closed_urls));
    format!(
        "Session weight is a simple representation of your pages combined memory and CPU load. A file counts as 1, and a URL counts as 20.\n\nTry to keep the total below 10 million! It is also generally better to spread it around--have five download pages each of 500k weight rather than one page with 2.5M.\n\nYour {open_pages} open pages' total is: {}\n\nSpecifically, your file weight is {} and URL weight is {}.\n\nFor extra info, your {closed_pages} closed pages (in the undo list) have total weight {}, being file weight {} and URL weight {}.",
        human_int(of + ou),
        human_int(of),
        human_int(ou),
        human_int(cf + cu),
        human_int(cf),
        human_int(cu),
    )
}
