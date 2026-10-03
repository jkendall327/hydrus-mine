//! Gallery searches made from queries, as a gallery page makes them (the
//! reference's `MultipleGalleryImport.PendQueries`): a queue per query,
//! starting from its downloader's gallery URLs, with the page's file
//! limit, import options and pauses.

use std::collections::BTreeSet;

use hydrus_core::gallery::GallerySearch;
use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::subscriptions::GalleryDefaults;
use hydrus_parse::Downloaders;

use crate::error::StoreError;
use crate::queues::{self, GallerySeedMeta, NewGallerySeed, Queue, QueueKind};
use crate::store::Store;

/// The name of a gallery page made without one.
pub const DEFAULT_GALLERY_PAGE_NAME: &str = "gallery";

#[derive(Debug, thiserror::Error)]
pub enum GallerySearchError {
    #[error("This page does not have a downloader set!")]
    NoDownloaderSet,
    #[error("Could not find a Gallery URL Generator (Downloader) for \"{0}\"!")]
    NoDownloader(String),
    #[error("{0}")]
    Gug(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// How a page makes its searches.
#[derive(Debug, Clone, Default)]
pub struct NewSearches<'a> {
    /// The page (named, and keyed if it is a page of the client's).
    pub page_name: Option<&'a str>,
    pub page_key: Option<&'a [u8]>,
    /// The downloader: its key (hex) and name.
    pub gug_key: &'a str,
    pub gug_name: &'a str,
    /// Stop each after this many new files (`None`: the client's default
    /// file limit).
    pub file_limit: Option<Option<u64>>,
    pub options: ImportOptionsSlice,
    pub start_files_paused: bool,
    pub start_gallery_paused: bool,
    /// Queries made together become one search ("3 queries").
    pub merge: bool,
    /// Skip queries the page has already, by query and downloader.
    pub no_new_dupes: bool,
    pub existing: &'a [(String, String)],
}

/// The searches made, and the downloader as it is now called (found by
/// key, else by name).
#[derive(Debug, Clone)]
pub struct MadeSearches {
    pub queues: Vec<Queue>,
    pub gug_key: String,
    pub gug_name: String,
}

/// Make searches for `queries` as `how` says, all or none: a downloader
/// that gives a query no gallery URLs makes none.
pub fn create_gallery_searches(
    store: &Store,
    definitions: &Downloaders,
    how: &NewSearches<'_>,
    queries: &[String],
    now: i64,
) -> Result<MadeSearches, GallerySearchError> {
    if how.gug_name.is_empty() {
        return Err(GallerySearchError::NoDownloaderSet);
    }
    let gug = definitions
        .gugs
        .get(how.gug_key, how.gug_name)
        .ok_or_else(|| GallerySearchError::NoDownloader(how.gug_name.to_owned()))?;
    let source_name = gug.name().to_owned();
    let snapshot = store.snapshot();
    let classes = &snapshot.url_classes;
    let network: crate::network::NetworkSettings = store.read(crate::settings::get)?;
    let options = hydrus_core::url::GugOptions {
        percent_twenty_is_space: network.gug_percent_twenty_is_space,
        collapse_leading_slashes: classes.settings().collapse_leading_slashes,
    };
    let mut groups = Vec::new();
    for query in queries {
        let urls = definitions
            .gugs
            .gallery_urls(gug, query, options)
            .map_err(|e| GallerySearchError::Gug(e.to_string()))?;
        if urls.is_empty() {
            return Err(GallerySearchError::Gug(format!(
                "The Gallery URL Generator \"{source_name}\" did not produce any URLs!"
            )));
        }
        groups.push((query.clone(), urls));
    }
    if how.merge && groups.len() > 1 {
        let query = format!(
            "{} queries",
            hydrus_core::numbers::human_int(groups.len() as u64)
        );
        let urls = groups.into_iter().flat_map(|(_, urls)| urls).collect();
        groups = vec![(query, urls)];
    }
    let file_limit = match how.file_limit {
        Some(limit) => limit,
        None => {
            store
                .read(crate::settings::get::<GalleryDefaults>)?
                .file_limit
        }
    };
    let mut existing: BTreeSet<(String, String)> = how.existing.iter().cloned().collect();
    let mut searches = Vec::new();
    for (query, urls) in groups {
        if how.no_new_dupes && existing.contains(&(query.clone(), source_name.clone())) {
            continue;
        }
        existing.insert((query.clone(), source_name.clone()));
        let run_token = hex::encode(rand::random::<[u8; 32]>());
        let mut seen = BTreeSet::new();
        let seeds: Vec<NewGallerySeed> = urls
            .into_iter()
            .map(|url| classes.normalise(&url, true).unwrap_or(url))
            .filter(|url| seen.insert(url.clone()))
            .map(|url| NewGallerySeed {
                url,
                can_generate_more_pages: true,
                referral_url: None,
                meta: GallerySeedMeta {
                    run_token: run_token.clone(),
                    ..GallerySeedMeta::default()
                },
            })
            .collect();
        let search = GallerySearch {
            query,
            source_name: source_name.clone(),
            file_limit,
            num_new_urls_found: 0,
            num_urls_found: 0,
        };
        searches.push((search, seeds));
    }
    let name = how
        .page_name
        .unwrap_or(DEFAULT_GALLERY_PAGE_NAME)
        .to_owned();
    let page_key = how.page_key.map(<[u8]>::to_vec);
    let options = how.options.clone();
    let (files_paused, gallery_paused) = (how.start_files_paused, how.start_gallery_paused);
    let made = store.write(move |ctx| {
        let conn = ctx.conn();
        let mut made = Vec::new();
        for (search, seeds) in &searches {
            let id = queues::create_queue(
                conn,
                QueueKind::Gallery,
                &name,
                page_key.as_deref(),
                &options,
                now,
            )?;
            if files_paused || gallery_paused {
                queues::set_paused(conn, id, Some(files_paused), Some(gallery_paused))?;
            }
            let extra = serde_json::to_value(search).expect("plain data serialises");
            queues::set_queue_extra(conn, id, &extra)?;
            queues::add_gallery_seeds(conn, id, seeds, None, now)?;
            queues::nudge(conn, id)?;
            made.push(queues::queue(conn, id)?.expect("just made"));
        }
        Ok(made)
    })?;
    Ok(MadeSearches {
        queues: made,
        gug_key: gug.key().to_owned(),
        gug_name: source_name,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hydrus_core::url::{AnyGug, Gug, Gugs, NestedGug};

    use super::*;

    const KEY: &str = "aa";

    fn setup() -> (tempfile::TempDir, Arc<Store>, Downloaders) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let downloaders = Downloaders {
            gugs: Gugs {
                gugs: vec![
                    AnyGug::Single(Gug {
                        name: "site tag search".into(),
                        key: KEY.into(),
                        url_template: "https://site.example/search?tags=%tags%".into(),
                        replacement_phrase: "%tags%".into(),
                        separator: "+".into(),
                        initial_search_text: String::new(),
                        example_search_text: String::new(),
                    }),
                    AnyGug::Nested(NestedGug {
                        name: "nothing".into(),
                        key: "bb".into(),
                        initial_search_text: String::new(),
                        gugs: vec![("cc".into(), "gone".into())],
                    }),
                ],
                keys_to_display: vec![KEY.into(), "bb".into()],
            },
            ..Downloaders::default()
        };
        (dir, store, downloaders)
    }

    fn queries(items: &[&str]) -> Vec<String> {
        items.iter().map(|&q| q.to_owned()).collect()
    }

    fn search(queue: &Queue) -> GallerySearch {
        serde_json::from_value(queue.extra.clone()).unwrap()
    }

    #[test]
    fn a_page_makes_a_search_per_query_with_its_settings() {
        let (_dir, store, downloaders) = setup();
        let options = ImportOptionsSlice {
            presentation: Some(hydrus_core::import_options::PresentationOptions::default()),
            ..ImportOptionsSlice::default()
        };
        let how = NewSearches {
            page_name: Some("my gallery"),
            page_key: Some(&[7; 32]),
            // (found by its key though renamed since)
            gug_key: KEY,
            gug_name: "old name",
            file_limit: Some(Some(50)),
            options: options.clone(),
            start_files_paused: true,
            ..NewSearches::default()
        };
        let made = create_gallery_searches(
            &store,
            &downloaders,
            &how,
            &queries(&["blue", "red eyes"]),
            9,
        )
        .unwrap();
        assert_eq!(made.gug_name, "site tag search");
        assert_eq!(made.queues.len(), 2);
        for (queue, query) in made.queues.iter().zip(["blue", "red eyes"]) {
            assert_eq!(queue.kind, QueueKind::Gallery);
            assert_eq!(queue.name, "my gallery");
            assert_eq!(queue.page_key.as_deref(), Some(&[7; 32][..]));
            assert_eq!(queue.options, options);
            assert_eq!(queue.created, 9);
            assert!(queue.files_paused && !queue.gallery_paused);
            let search = search(queue);
            assert_eq!(search.query, query);
            assert_eq!(search.source_name, "site tag search");
            assert_eq!(search.file_limit, Some(50));
            let seeds = store.read(|c| queues::gallery_seeds(c, queue.id)).unwrap();
            assert_eq!(seeds.len(), 1);
        }
        // the daemon is told
        let nudged = store.write(|ctx| queues::take_nudges(ctx.conn())).unwrap();
        assert_eq!(nudged.len(), 2);
    }

    #[test]
    fn queries_merge_and_repeats_are_skipped_as_the_page_says() {
        let (_dir, store, downloaders) = setup();
        let merged = NewSearches {
            gug_key: KEY,
            gug_name: "site tag search",
            merge: true,
            ..NewSearches::default()
        };
        let made =
            create_gallery_searches(&store, &downloaders, &merged, &queries(&["a", "b", "c"]), 0)
                .unwrap();
        assert_eq!(made.queues.len(), 1);
        assert_eq!(search(&made.queues[0]).query, "3 queries");
        assert_eq!(
            store
                .read(|c| queues::gallery_seeds(c, made.queues[0].id))
                .unwrap()
                .len(),
            3
        );
        // (one query alone isn't "merged")
        let made =
            create_gallery_searches(&store, &downloaders, &merged, &queries(&["d"]), 0).unwrap();
        assert_eq!(search(&made.queues[0]).query, "d");
        // repeats: of the page's, and within the queries
        let existing = vec![("a".to_owned(), "site tag search".to_owned())];
        let no_dupes = NewSearches {
            gug_key: KEY,
            gug_name: "site tag search",
            no_new_dupes: true,
            existing: &existing,
            ..NewSearches::default()
        };
        let made = create_gallery_searches(
            &store,
            &downloaders,
            &no_dupes,
            &queries(&["a", "e", "e"]),
            0,
        )
        .unwrap();
        let made: Vec<String> = made.queues.iter().map(|q| search(q).query).collect();
        assert_eq!(made, ["e"]);
        // the client's default file limit, and a page's default name
        let made =
            create_gallery_searches(&store, &downloaders, &merged, &queries(&["f"]), 0).unwrap();
        assert_eq!(search(&made.queues[0]).file_limit, Some(2000));
        assert_eq!(made.queues[0].name, DEFAULT_GALLERY_PAGE_NAME);
    }

    #[test]
    fn no_downloader_or_no_urls_makes_nothing() {
        let (_dir, store, downloaders) = setup();
        let count = || store.read(|c| queues::queues(c, None)).unwrap().len();
        let unset = NewSearches::default();
        assert!(matches!(
            create_gallery_searches(&store, &downloaders, &unset, &queries(&["a"]), 0),
            Err(GallerySearchError::NoDownloaderSet)
        ));
        let missing = NewSearches {
            gug_key: "dd",
            gug_name: "missing",
            ..NewSearches::default()
        };
        assert!(matches!(
            create_gallery_searches(&store, &downloaders, &missing, &queries(&["a"]), 0),
            Err(GallerySearchError::NoDownloader(name)) if name == "missing"
        ));
        let nothing = NewSearches {
            gug_key: "bb",
            gug_name: "nothing",
            ..NewSearches::default()
        };
        let err = create_gallery_searches(&store, &downloaders, &nothing, &queries(&["a", "b"]), 0)
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "The Gallery URL Generator \"nothing\" did not produce any URLs!"
        );
        assert_eq!(count(), 0);
    }
}
