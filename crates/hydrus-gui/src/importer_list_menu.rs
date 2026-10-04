//! A gallery or watcher downloader page's list menu, bound: a right press
//! on a row selects it (if it isn't already) and opens the menu
//! (hydrus-gui-model's [`importer_menu`](crate::importer_menu)) as the
//! menu bar's popup; its actions act on the selected searches or
//! watchers, mostly as the sidebar's own buttons do.

use std::cell::RefCell;

use slint::ComponentHandle as _;
use std::rc::Rc;

use hydrus_store::queues::{self, SeedStatus, StatusCounts};

use crate::file_log::{self, Entry};
use crate::file_log_window::OpenFiles;
use crate::importer_menu::{Action, Selected, Single, gallery_menu, watcher_menu};
use crate::main_menu::{self, PopupNode};
use crate::page::SearchPage;
use crate::{FileLogWindow, MainWindow, search_log};

fn node(entry: &Entry<Action>) -> PopupNode<'_, Entry<Action>, Action> {
    match entry {
        Entry::Item(
            label,
            Action::FileLog(file_log::Action::NotYet | file_log::Action::SearchUrls),
        ) => PopupNode::Disabled(label),
        Entry::Item(label, action) => PopupNode::Item(label, action),
        Entry::Label(label) => PopupNode::Label(label),
        Entry::Separator => PopupNode::Separator,
        Entry::Menu(label, entries) => PopupNode::Menu(label, entries),
    }
}

/// The selected searches' or watchers' queues, and whether they are a
/// watcher page's.
fn selected_queues(page: &SearchPage) -> (Vec<i64>, bool) {
    if let Some(watchers) = page.watchers() {
        (watchers.selected(), true)
    } else {
        (
            page.gallery()
                .map(crate::gallery::GalleryView::selected)
                .unwrap_or_default(),
            false,
        )
    }
}

fn has(counts: &StatusCounts, status: SeedStatus) -> bool {
    counts.get(&status).is_some_and(|&n| n > 0)
}

/// What the menu needs to know of the selection.
fn facts(page: &SearchPage, queues: &[i64], watcher: bool) -> Selected {
    let store = page.store();
    let counts = |queue: i64| {
        store
            .read(|c| {
                Ok((
                    queues::file_seed_counts(c, queue)?,
                    queues::gallery_seeds(c, queue)?,
                ))
            })
            .unwrap_or_default()
    };
    let mut selected = Selected {
        count: queues.len(),
        ..Selected::default()
    };
    for &queue in queues {
        let (files, _) = counts(queue);
        selected.any_failed |= has(&files, SeedStatus::Error);
        selected.any_ignored |= has(&files, SeedStatus::Vetoed);
    }
    if let [queue] = queues {
        let (files, pages) = counts(*queue);
        let mut page_counts = StatusCounts::new();
        for p in &pages {
            *page_counts.entry(p.status).or_default() += 1;
        }
        let first_is_url = store
            .read(|c| queues::file_seeds(c, *queue))
            .ok()
            .and_then(|seeds| seeds.first().map(|s| s.seed_type))
            .is_none_or(|t| t == queues::SeedType::Url);
        let presentation = store
            .read(|c| queues::queue(c, *queue))
            .ok()
            .flatten()
            .and_then(|q| q.options.presentation);
        selected.single = Some(Single {
            presentation,
            files: file_log::LogFacts {
                len: files.values().sum(),
                counts: files,
                urls: first_is_url,
            },
            searches: search_log::LogFacts {
                counts: page_counts,
                len: pages.len(),
                last_failed: pages.last().is_some_and(|p| p.status == SeedStatus::Error),
                read_only: watcher,
                can_generate_more_pages: !watcher,
            },
        });
    }
    selected
}

/// The menu on a right press on row `row`: its entries, and their
/// actions (by `Command::Popup`'s index).
pub(crate) fn open(page: &mut SearchPage, row: usize) -> (Vec<main_menu::Entry>, Vec<Action>) {
    page.press_query(row);
    let (queues, watcher) = selected_queues(page);
    let selected = facts(page, &queues, watcher);
    let entries = if watcher {
        watcher_menu(&selected)
    } else {
        gallery_menu(&selected)
    };
    main_menu::popup(&entries, &node)
}

/// What the actions work with.
pub(crate) struct Context<'a> {
    pub window: &'a MainWindow,
    pub page: &'a Rc<RefCell<SearchPage>>,
    /// The log window opened, and where its files are shown.
    pub log: &'a Rc<RefCell<Option<FileLogWindow>>>,
    pub open_files: &'a OpenFiles,
    /// Ask a question, then do something.
    pub ask: &'a dyn Fn(String, Rc<dyn Fn()>),
    /// Show the page again (`true`: its files changed).
    pub shown: Rc<dyn Fn(bool)>,
}

/// Do a menu's action.
pub(crate) fn act(cx: &Context<'_>, action: &Action) {
    let (queues, watcher) = selected_queues(&cx.page.borrow());
    let store = cx.page.borrow().store().clone();
    let window = cx.window;
    match action {
        Action::CopyQueries => {
            let page = cx.page.borrow();
            let texts: Vec<&str> = page.gallery().map_or_else(Vec::new, |g| {
                g.queries
                    .iter()
                    .filter(|q| queues.contains(&q.queue))
                    .map(|q| q.query.as_str())
                    .collect()
            });
            crate::copy_to_clipboard(&texts.join("\n"));
        }
        Action::CopyUrls | Action::OpenUrls | Action::CopySubjects => {
            let page = cx.page.borrow();
            let rows: Vec<(String, String)> = page.watchers().map_or_else(Vec::new, |w| {
                w.watchers
                    .iter()
                    .filter(|r| queues.contains(&r.queue))
                    .map(|r| (r.state.url.clone(), r.state.subject.clone()))
                    .collect()
            });
            match action {
                Action::OpenUrls => {
                    for (url, _) in &rows {
                        crate::launch(url);
                    }
                }
                Action::CopyUrls => {
                    let urls: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
                    crate::copy_to_clipboard(&urls.join("\n"));
                }
                _ => {
                    let subjects: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
                    crate::copy_to_clipboard(&subjects.join("\n"));
                }
            }
        }
        Action::ShowFiles(options) => {
            let mut files = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for &queue in &queues {
                let presented = store
                    .read(|c| queues::presented_files_as(c, queue, options.as_ref()))
                    .unwrap_or_default();
                files.extend(presented.into_iter().filter(|f| seen.insert(*f)));
            }
            if files.is_empty() {
                window.set_error("No presented files for that selection!".into());
            } else {
                cx.page.borrow_mut().show_importers_files(files);
                (cx.shown)(true);
            }
        }
        Action::ShowFileLog | Action::ShowSearchLog => {
            let Some(&queue) = queues.first() else {
                return;
            };
            let old = cx.log.borrow_mut().take();
            if let Some(old) = old {
                old.invoke_close_window();
            }
            let opened = if *action == Action::ShowFileLog {
                crate::file_log_window::open(&store, queue, cx.log, cx.open_files)
            } else {
                crate::search_log_window::open(&store, queue, cx.log)
            };
            match opened {
                Ok(log) => *cx.log.borrow_mut() = Some(log),
                Err(e) => eprintln!("could not open the log: {e}"),
            }
        }
        Action::FileLog(action) => {
            if let [queue] = queues[..] {
                if *action == file_log::Action::Renormalise {
                    let shown = cx.shown.clone();
                    let weak = window.as_weak();
                    (cx.ask)(
                        file_log::RENORMALISE_QUESTION.into(),
                        Rc::new(move || {
                            if let Err(error) = store.write(move |ctx| {
                                let classes = hydrus_core::url::UrlClasses::new(
                                    hydrus_store::settings::get(ctx.conn())?,
                                );
                                hydrus_store::queues::renormalise_file_seeds(
                                    ctx.conn(),
                                    queue,
                                    &classes,
                                )?;
                                hydrus_store::queues::nudge(ctx.conn(), queue)
                            }) && let Some(w) = weak.upgrade()
                            {
                                w.set_error(error.to_string().into());
                            }
                            shown(false);
                        }),
                    );
                } else {
                    if let Some(error) =
                        crate::file_log_window::act_on_queue(&store, queue, action, cx.open_files)
                    {
                        window.set_error(error.into());
                    }
                    (cx.shown)(false);
                }
            }
        }
        Action::SearchLog(action) => {
            let [queue] = queues[..] else {
                return;
            };
            if let search_log::Action::DeleteStatus(status) = action {
                let kind = if watcher { "check" } else { "search" };
                let exchange = cx.open_files.clone();
                let action = action.clone();
                let store = store.clone();
                (cx.ask)(
                    search_log::delete_question(*status, kind),
                    Rc::new(move || {
                        crate::search_log_window::act_on_queue(&store, queue, &action, &exchange);
                    }),
                );
            } else {
                crate::search_log_window::act_on_queue(&store, queue, action, cx.open_files);
                (cx.shown)(false);
            }
        }
        Action::Remove => {
            if watcher {
                window.invoke_watcher_remove();
            } else {
                window.invoke_gallery_remove();
            }
        }
        Action::PausePlayFiles | Action::PausePlaySearch => {
            let search = *action == Action::PausePlaySearch;
            if watcher {
                window.invoke_watcher_pause_play(search, false);
            } else {
                window.invoke_gallery_pause_play(search, false);
            }
        }
        Action::RetryFailed => window.invoke_watcher_retry(false),
        Action::RetryIgnored => window.invoke_watcher_retry(true),
    }
}
