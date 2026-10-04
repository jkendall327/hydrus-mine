//! Native notebook settings, independent measured tab fits and hierarchy rows.
use crate::{MainWindow, PageTreeRow, Pages, TabFit, TabNames};
use hydrus_core::pages::{Page, PageContent, PageKey};
use hydrus_store::settings::{self, TabPresentationSettings};
use slint::{ComponentHandle as _, ModelRc, VecModel};

fn begin(text: &str) -> TabFit {
    let count = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
    TabFit {
        low: -1,
        high: count,
        candidate: count,
        display: "".into(),
        done: false,
    }
}
fn step(text: &str, mut fit: TabFit, measured: f32, room: f32) -> TabFit {
    if fit.done {
        return fit;
    }
    if measured.is_finite() && room.is_finite() && measured <= room.max(0.0) {
        fit.low = fit.candidate;
        fit.display = hydrus_gui_model::tab_presentation::middle_name(
            text,
            usize::try_from(fit.candidate).unwrap_or(0),
        )
        .into();
    } else {
        fit.high = fit.candidate - 1;
    }
    if fit.low >= fit.high {
        fit.done = true;
    } else {
        fit.candidate = fit.low + (fit.high - fit.low + 1) / 2;
    }
    fit
}

pub(crate) fn bind_names(window: &MainWindow) {
    let names = window.global::<TabNames>();
    names.on_begin(|text| begin(&text));
    names.on_middle(|text, count| {
        hydrus_gui_model::tab_presentation::middle_name(&text, usize::try_from(count).unwrap_or(0))
            .into()
    });
    names.on_step(|text, fit, measured, room| step(&text, fit, measured, room));
}

fn tree_rows(pages: &[Page], depth: i32, shown: PageKey, rows: &mut Vec<PageTreeRow>) {
    for page in pages {
        rows.push(PageTreeRow {
            key: page.key.to_hex().into(),
            name: page.name.as_str().into(),
            depth,
            selected: page.key == shown,
        });
        if let PageContent::Pages(children) = &page.content {
            tree_rows(children, depth + 1, shown, rows);
        }
    }
}

pub(crate) fn show(window: &MainWindow, pages: &Pages) {
    let settings: TabPresentationSettings = pages.store().read(settings::get).unwrap_or_default();
    window.set_tab_alignment(settings.alignment.code());
    window.set_page_tree_side(settings.tree_side());
    window.set_navigation_tabs_hidden(settings.tabs_hidden());
    window.set_elide_tab_names(settings.elide_names);
    let mut tree = Vec::new();
    tree_rows(&pages.session().pages, 0, pages.shown().key, &mut tree);
    window.set_page_tree(ModelRc::new(VecModel::from(tree)));
}
