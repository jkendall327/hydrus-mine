//! A simple downloader page's sidebar (the reference's
//! `SidebarImporterSimpleDownloader`): its pending jobs, each a page's URL
//! and the formula it was given, the formula chooser, and the URLs typed
//! or pasted becoming jobs.

use hydrus_parse::simple::SimpleFormula;
use hydrus_store::queues::{SimpleDownloader, SimpleJob};

/// The page's title over its boxes.
pub const TITLE: &str = "simple downloader";

/// The URL box's placeholder.
pub const URL_PLACEHOLDER: &str = "url to be parsed by the selected formula";

/// A job as the pending jobs list shows it (`_ConvertPendingJobToString`):
/// "formula name: url".
pub fn job_label(job: &SimpleJob) -> String {
    format!("{}: {}", job.formula.name, job.url)
}

/// The formula chooser's names, sorted, and which is chosen (the one named
/// `chosen`, if there is one; `_RefreshFormulae`).
pub fn formula_choices(formulae: &[SimpleFormula], chosen: &str) -> (Vec<String>, Option<usize>) {
    let mut names: Vec<String> = formulae.iter().map(|f| f.name.clone()).collect();
    names.sort();
    let index = names.iter().position(|n| n == chosen);
    (names, index)
}

/// The formula the chooser shows: the one chosen, else the first by name
/// (as the reference's chooser falls back to its first entry).
pub fn chosen_formula<'a>(
    formulae: &'a [SimpleFormula],
    chosen: &str,
) -> Option<&'a SimpleFormula> {
    formulae
        .iter()
        .find(|f| f.name == chosen)
        .or_else(|| formulae.iter().min_by(|a, b| a.name.cmp(&b.name)))
}

/// `_PendPageURLs`: each line typed or pasted that looks like a full URL,
/// encoded.
pub fn page_urls(text: &str, collapse_leading_slashes: bool) -> Vec<String> {
    text.lines()
        .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}'))
        .filter(|url| hydrus_core::url::functions::check_full_url(url).is_ok())
        .map(|url| hydrus_core::url::ensure_url_is_encoded(url, true, collapse_leading_slashes))
        .collect()
}

/// `PendJob`: each URL a job with the formula, unless that job is already
/// waiting.
pub fn pend(state: &mut SimpleDownloader, urls: &[String], formula: &SimpleFormula) {
    for url in urls {
        let job = SimpleJob {
            url: url.clone(),
            formula: formula.clone(),
        };
        if !state.pending.contains(&job) {
            state.pending.push(job);
        }
    }
}

/// The jobs list's "X": the jobs at `rows` removed (`SetPendingJobs`).
pub fn remove(state: &mut SimpleDownloader, rows: &[usize]) {
    let mut i = 0;
    state.pending.retain(|_| {
        let keep = !rows.contains(&i);
        i += 1;
        keep
    });
}

/// The jobs list's arrows: the jobs at `rows` moved one up (-1) or down
/// (1), each in turn from the end they move to, as the reference's list
/// moves them; where the rows are afterwards.
pub fn shift(state: &mut SimpleDownloader, rows: &[usize], distance: isize) -> Vec<usize> {
    let mut selected: Vec<bool> = (0..state.pending.len())
        .map(|i| rows.contains(&i))
        .collect();
    let mut order: Vec<usize> = rows.to_vec();
    order.sort_unstable();
    if distance > 0 {
        order.reverse();
    }
    let last = state.pending.len().saturating_sub(1);
    for index in order {
        if index > last {
            continue;
        }
        let new = index.saturating_add_signed(distance).min(last);
        if new != index {
            let job = state.pending.remove(index);
            state.pending.insert(new, job);
            let was = selected.remove(index);
            selected.insert(new, was);
        }
    }
    (0..selected.len()).filter(|&i| selected[i]).collect()
}
