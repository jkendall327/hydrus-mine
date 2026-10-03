//! GUI sessions: the pages a client had open. A session container (104)
//! holds a tree of notebooks (106) and pages (107), each page by the hash of
//! its data (105), which is kept apart in `json_dumps_hashed` so unchanged
//! pages are not saved again. A page's data is its page manager (12): a
//! name, a page type and a dictionary of variables, among them the
//! downloader a downloader page runs: a URL page's importer (28), a gallery
//! page's (20) with its gallery searches (68), or a watcher page's (64) with
//! its watchers (17), or an "import" page's local import (9).
//!
//! Only the versions a current client writes are read: the session a client
//! opens with is saved again every few minutes, so it is always current.

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::subscriptions::CheckerOptions;

use super::auto_resolution::PotentialsSearch;
use super::domain::expect;
use super::favourites::FileSearchContext;
use super::sort::{MediaCollect, MediaSort};
use super::subscriptions::{
    LegacyFileSeed, LegacyGallerySeed, checker_options, file_seed_cache, gallery_seed_log,
    service_keys_to_tags,
};
use super::util::{
    DecodeResult, boolean, dictionary_pairs, int, list_items, malformed, nested, opt_int,
    opt_string, string, strings, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{Meta, SerialisableObject, SerialisableType};

const CONTAINER: SerialisableType = SerialisableType(104);
const PAGE_DATA: SerialisableType = SerialisableType(105);
const NOTEBOOK: SerialisableType = SerialisableType(106);
const SINGLE: SerialisableType = SerialisableType(107);
const PAGE_MANAGER: SerialisableType = SerialisableType(12);
const URLS_IMPORT: SerialisableType = SerialisableType(28);
const GALLERY_IMPORT: SerialisableType = SerialisableType(68);
const MULTIPLE_GALLERY_IMPORT: SerialisableType = SerialisableType(20);
const WATCHER_IMPORT: SerialisableType = SerialisableType(17);
const MULTIPLE_WATCHER_IMPORT: SerialisableType = SerialisableType(64);
const HDD_IMPORT: SerialisableType = SerialisableType(9);
const SIMPLE_DOWNLOADER_IMPORT: SerialisableType = SerialisableType(18);

/// `ClientGUIPagesCore.PAGE_TYPE_*`.
pub mod page_type {
    pub const GALLERY: i64 = 1;
    pub const SIMPLE_DOWNLOADER: i64 = 2;
    pub const IMPORT_FROM_DISK: i64 = 3;
    pub const PETITIONS: i64 = 5;
    pub const QUERY: i64 = 6;
    pub const URLS: i64 = 7;
    pub const DUPLICATE_FILTER: i64 = 8;
    pub const WATCHER: i64 = 9;
}

/// A session: its name and its tree of pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySession {
    pub name: String,
    /// The top notebook.
    pub top: SessionNode,
}

/// A notebook of pages, or a page (by the hash of its data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionNode {
    Notebook {
        name: String,
        pages: Vec<SessionNode>,
    },
    Page {
        name: String,
        page_data_hash: Vec<u8>,
    },
}

impl SessionNode {
    /// Every page's data hash, in the order the pages appear.
    pub fn page_data_hashes(&self) -> Vec<&[u8]> {
        match self {
            SessionNode::Notebook { pages, .. } => pages
                .iter()
                .flat_map(SessionNode::page_data_hashes)
                .collect(),
            SessionNode::Page { page_data_hash, .. } => vec![page_data_hash.as_slice()],
        }
    }
}

/// A page's data: the page and the files it showed.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyPageData {
    pub page: LegacyPage,
    /// The files the page showed, in order.
    pub hashes: Vec<Vec<u8>>,
}

/// A page (`PageManager`).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyPage {
    pub name: String,
    /// `PAGE_TYPE_*` (see [`page_type`]).
    pub page_type: i64,
    /// How the page sorts its files, if it could be read.
    pub sort: Option<MediaSort>,
    /// How the page collects its files, if it could be read.
    pub collect: Option<MediaCollect>,
    pub content: PageContent,
}

/// What a page runs, for the pages whose work carries over.
#[derive(Debug, Clone, PartialEq)]
pub enum PageContent {
    Query(LegacyQueryPage),
    Urls(LegacyUrlsImport),
    Gallery(LegacyMultipleGalleryImport),
    Watchers(LegacyMultipleWatcherImport),
    Duplicates(LegacyDuplicatesPage),
    LocalImport(LegacyHddImport),
    SimpleDownloader(LegacySimpleDownloaderImport),
    /// A page whose state isn't read here (a petitions page...).
    Other,
}

/// A simple downloader page's importer (`SimpleDownloaderImport`): the
/// pages waiting with their formulae, its logs, its chosen formula and its
/// pauses.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacySimpleDownloaderImport {
    pub pending: Vec<(String, hydrus_parse::simple::SimpleFormula)>,
    pub gallery_seeds: Vec<LegacyGallerySeed>,
    pub file_seeds: Vec<LegacyFileSeed>,
    pub import_options: ImportOptionsSlice,
    pub formula_name: String,
    pub gallery_paused: bool,
    pub files_paused: bool,
}

/// A duplicates page's filtering settings.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyDuplicatesPage {
    pub search: PotentialsSearch,
    /// Whether the page's searches run as their predicates change.
    pub synchronised: bool,
    /// `DUPE_PAIR_SORT_*`.
    pub sort_type: i64,
    pub sort_ascending: bool,
    pub group_mode: bool,
}

/// A search page's search.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyQueryPage {
    pub search: FileSearchContext,
    /// Whether the page searches as its predicates change (unsynchronised,
    /// it keeps its files until searched again).
    pub synchronised: bool,
    /// Whether the search is locked to its `system:hash`
    /// (`system_hash_locked`).
    pub hash_locked: bool,
    /// Whether that hash takes in files added to the page and lets go of
    /// those removed from it (kept while unlocked).
    pub lock_syncs: LegacyHashLock,
}

/// A search page's `system_hash_locked_syncs_new` and `_syncs_removes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyHashLock {
    pub syncs_new: bool,
    pub syncs_removes: bool,
}

/// A URL page's importer (`URLsImport`).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyUrlsImport {
    pub file_seeds: Vec<LegacyFileSeed>,
    pub gallery_seeds: Vec<LegacyGallerySeed>,
    pub import_options: ImportOptionsSlice,
    pub paused: bool,
}

/// An "import" page's local import (`HDDImport`): its files, by path, each
/// with the tags to add to it.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyHddImport {
    pub file_seeds: Vec<LegacyFileSeed>,
    pub import_options: ImportOptionsSlice,
    /// The sidecar routers it reads its files' metadata with.
    pub metadata_routers: Vec<hydrus_parse::sidecar::Router>,
    pub delete_after_success: bool,
    pub paused: bool,
}

/// A gallery search (`GalleryImport`).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyGalleryImport {
    /// Hex.
    pub key: String,
    pub created: i64,
    pub query: String,
    /// The downloader's name.
    pub source_name: String,
    pub current_page_index: i64,
    pub num_urls_found: i64,
    pub num_new_urls_found: i64,
    pub file_limit: Option<i64>,
    pub gallery_paused: bool,
    pub files_paused: bool,
    pub import_options: ImportOptionsSlice,
    pub gallery_seeds: Vec<LegacyGallerySeed>,
    pub file_seeds: Vec<LegacyFileSeed>,
    pub no_work_until: i64,
    pub no_work_until_reason: String,
}

/// A gallery page's importer (`MultipleGalleryImport`).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyMultipleGalleryImport {
    /// Hex.
    pub gug_key: String,
    pub gug_name: String,
    /// The gallery search shown in the page (hex key).
    pub highlighted: Option<String>,
    pub file_limit: Option<i64>,
    pub start_file_queues_paused: bool,
    pub start_gallery_queues_paused: bool,
    pub do_not_allow_new_dupes: bool,
    pub merge_simultaneous_pends_to_one_importer: bool,
    pub import_options: ImportOptionsSlice,
    pub gallery_imports: Vec<LegacyGalleryImport>,
}

/// A watcher (`WatcherImport`).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyWatcherImport {
    pub url: String,
    pub gallery_seeds: Vec<LegacyGallerySeed>,
    pub file_seeds: Vec<LegacyFileSeed>,
    pub external_filterable_tags: Vec<String>,
    /// `(service key hex, tags)`.
    pub external_additional_tags: Vec<(String, Vec<String>)>,
    pub checker: CheckerOptions,
    pub import_options: ImportOptionsSlice,
    pub last_check_time: i64,
    pub files_paused: bool,
    pub checking_paused: bool,
    /// `ClientImporting.CHECKER_STATUS_*`: 0 ok, 1 dead, 2 404.
    pub checking_status: i64,
    pub subject: String,
    pub no_work_until: i64,
    pub no_work_until_reason: String,
    pub created: i64,
}

/// A watcher page's importer (`MultipleWatcherImport`).
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyMultipleWatcherImport {
    /// The watcher shown in the page, by its URL.
    pub highlighted: Option<String>,
    pub checker: CheckerOptions,
    pub import_options: ImportOptionsSlice,
    pub watchers: Vec<LegacyWatcherImport>,
}

/// Decode a session container.
pub fn session(object: &SerialisableObject) -> DecodeResult<LegacySession> {
    expect(object, CONTAINER, &[1])?;
    let name = object
        .name
        .clone()
        .ok_or_else(|| malformed(CONTAINER, "session has no name"))?;
    let top = node(&nested(CONTAINER, &object.info(), "top notebook")?)?;
    Ok(LegacySession { name, top })
}

/// The objects in a nested `SerialisableList`.
fn objects(
    k: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<Vec<SerialisableObject>> {
    let list = nested(k, value, what)?;
    list_items(&list)?
        .iter()
        .map(|item| match item {
            Meta::Object(object) => Ok(object.as_ref().clone()),
            _ => Err(malformed(k, format!("{what}: an item is not an object"))),
        })
        .collect()
}

fn node(object: &SerialisableObject) -> DecodeResult<SessionNode> {
    let name = object.name.clone().unwrap_or_default();
    if object.kind == NOTEBOOK {
        expect(object, NOTEBOOK, &[1])?;
        let pages = objects(NOTEBOOK, &object.info(), "pages")?
            .iter()
            .map(node)
            .collect::<DecodeResult<_>>()?;
        Ok(SessionNode::Notebook { name, pages })
    } else {
        expect(object, SINGLE, &[1])?;
        let hash = string(SINGLE, &object.info(), "page data hash")?;
        let page_data_hash =
            hex::decode(&hash).map_err(|_| malformed(SINGLE, "page data hash is not hex"))?;
        Ok(SessionNode::Page {
            name,
            page_data_hash,
        })
    }
}

/// Decode a page's data.
pub fn page_data(object: &SerialisableObject) -> DecodeResult<LegacyPageData> {
    let k = PAGE_DATA;
    expect(object, k, &[1])?;
    let info = object.info();
    let [manager, hashes] = tuple::<2>(k, &info, "page data")?;
    let page = page(&nested(k, manager, "page")?)?;
    let hashes = strings(k, hashes, "hashes")?
        .iter()
        .map(|h| hex::decode(h).map_err(|_| malformed(k, "a file hash is not hex")))
        .collect::<DecodeResult<_>>()?;
    Ok(LegacyPageData { page, hashes })
}

/// Decode a page manager, reading the search of a search page and the
/// downloader of a downloader page.
pub fn page(object: &SerialisableObject) -> DecodeResult<LegacyPage> {
    let k = PAGE_MANAGER;
    expect(object, k, &[17])?;
    let info = object.info();
    let [name, page_type, variables] = tuple::<3>(k, &info, "page")?;
    let name = string(k, name, "page name")?;
    let page_type = int(k, page_type, "page type")?;
    let variables = nested(k, variables, "variables")?;
    let variables = dictionary_pairs(&variables)?;
    let variable = |wanted: &str| {
        variables.iter().find_map(|(key, value)| match key {
            Meta::Json(PyJson::Str(key)) if key == wanted => Some(value),
            _ => None,
        })
    };
    let object_variable = |wanted: &str| match variable(wanted) {
        Some(Meta::Object(object)) => Ok(object.as_ref()),
        _ => Err(malformed(k, format!("the page has no {wanted}"))),
    };
    let flag = |name: &str, default: bool| match variable(name) {
        Some(Meta::Json(value)) => boolean(k, value, name),
        _ => Ok(default),
    };
    let content = match page_type {
        page_type::QUERY => PageContent::Query(LegacyQueryPage {
            search: FileSearchContext::from_object(object_variable("file_search_context")?)?,
            // the reference's defaults for a page that predates the options
            synchronised: flag("synchronised", true)?,
            hash_locked: flag("system_hash_locked", false)?,
            lock_syncs: LegacyHashLock {
                syncs_new: flag("system_hash_locked_syncs_new", true)?,
                syncs_removes: flag("system_hash_locked_syncs_removes", true)?,
            },
        }),
        page_type::URLS => PageContent::Urls(urls_import(object_variable("urls_import")?)?),
        page_type::GALLERY => PageContent::Gallery(multiple_gallery_import(object_variable(
            "multiple_gallery_import",
        )?)?),
        page_type::WATCHER => PageContent::Watchers(multiple_watcher_import(object_variable(
            "multiple_watcher_import",
        )?)?),
        page_type::IMPORT_FROM_DISK => {
            PageContent::LocalImport(hdd_import(object_variable("hdd_import")?)?)
        }
        page_type::SIMPLE_DOWNLOADER => PageContent::SimpleDownloader(simple_downloader_import(
            object_variable("simple_downloader_import")?,
        )?),
        page_type::DUPLICATE_FILTER => {
            PageContent::Duplicates(LegacyDuplicatesPage {
                search: PotentialsSearch::from_object(object_variable(
                    "potential_duplicates_search_context",
                )?)?,
                // the reference's defaults for a page that predates them
                synchronised: flag("synchronised", true)?,
                sort_type: match variable("duplicate_pair_sort_type") {
                    Some(Meta::Json(value)) => int(k, value, "pair sort")?,
                    _ => 0,
                },
                sort_ascending: flag("duplicate_pair_sort_asc", false)?,
                group_mode: flag("filter_group_mode", false)?,
            })
        }
        _ => PageContent::Other,
    };
    // a sort that can't be read is not worth losing the page over
    let sort = object_variable("media_sort")
        .ok()
        .and_then(|object| MediaSort::from_object(object).ok());
    let collect = object_variable("media_collect")
        .ok()
        .and_then(|object| MediaCollect::from_object(object).ok());
    Ok(LegacyPage {
        name,
        page_type,
        sort,
        collect,
        content,
    })
}

fn import_options(k: SerialisableType, value: &PyJson) -> DecodeResult<ImportOptionsSlice> {
    super::import_options::slice(&nested(k, value, "import options")?)
}

fn file_seeds(k: SerialisableType, value: &PyJson) -> DecodeResult<Vec<LegacyFileSeed>> {
    file_seed_cache(&nested(k, value, "file seed cache")?)
}

fn gallery_seeds(k: SerialisableType, value: &PyJson) -> DecodeResult<Vec<LegacyGallerySeed>> {
    gallery_seed_log(&nested(k, value, "gallery log")?)
}

/// Decode a URL page's importer.
pub fn urls_import(object: &SerialisableObject) -> DecodeResult<LegacyUrlsImport> {
    let k = URLS_IMPORT;
    expect(object, k, &[5])?;
    let info = object.info();
    let [gallery_log, file_seed_cache, options, paused] = tuple::<4>(k, &info, "url import")?;
    Ok(LegacyUrlsImport {
        gallery_seeds: gallery_seeds(k, gallery_log)?,
        file_seeds: file_seeds(k, file_seed_cache)?,
        import_options: import_options(k, options)?,
        paused: boolean(k, paused, "paused")?,
    })
}

/// Decode a simple downloader page's importer.
pub fn simple_downloader_import(
    object: &SerialisableObject,
) -> DecodeResult<LegacySimpleDownloaderImport> {
    let k = SIMPLE_DOWNLOADER_IMPORT;
    expect(object, k, &[6])?;
    let info = object.info();
    let [
        pending,
        gallery_log,
        file_seed_cache,
        options,
        formula_name,
        gallery_paused,
        files_paused,
    ] = tuple::<7>(k, &info, "simple downloader")?;
    let pending = pending
        .as_list()
        .ok_or_else(|| malformed(k, "the pending jobs are not a list"))?
        .iter()
        .map(|job| {
            let [url, formula] = tuple::<2>(k, job, "pending job")?;
            let formula = SerialisableObject::from_tuple(formula)
                .map_err(|e| malformed(k, format!("a job's formula: {e}")))?;
            Ok((
                string(k, url, "job url")?,
                super::parsers::simple_formula(&formula)?,
            ))
        })
        .collect::<DecodeResult<_>>()?;
    Ok(LegacySimpleDownloaderImport {
        pending,
        gallery_seeds: gallery_seeds(k, gallery_log)?,
        file_seeds: file_seeds(k, file_seed_cache)?,
        import_options: import_options(k, options)?,
        formula_name: string(k, formula_name, "formula name")?,
        gallery_paused: boolean(k, gallery_paused, "queue paused")?,
        files_paused: boolean(k, files_paused, "files paused")?,
    })
}

/// Decode an "import" page's local import.
pub fn hdd_import(object: &SerialisableObject) -> DecodeResult<LegacyHddImport> {
    let k = HDD_IMPORT;
    expect(object, k, &[4])?;
    let info = object.info();
    let [
        file_seed_cache,
        options,
        routers,
        delete_after_success,
        paused,
    ] = tuple::<5>(k, &info, "local import")?;
    Ok(LegacyHddImport {
        file_seeds: file_seeds(k, file_seed_cache)?,
        import_options: import_options(k, options)?,
        metadata_routers: super::sidecars::routers(k, routers)?,
        delete_after_success: boolean(k, delete_after_success, "delete after success")?,
        paused: boolean(k, paused, "paused")?,
    })
}

/// Decode a gallery search.
pub fn gallery_import(object: &SerialisableObject) -> DecodeResult<LegacyGalleryImport> {
    let k = GALLERY_IMPORT;
    expect(object, k, &[4])?;
    let info = object.info();
    let [
        key,
        created,
        query,
        source_name,
        current_page_index,
        num_urls_found,
        num_new_urls_found,
        file_limit,
        gallery_paused,
        files_paused,
        options,
        gallery_log,
        file_seed_cache,
        no_work_until,
        no_work_until_reason,
    ] = tuple::<15>(k, &info, "gallery import")?;
    Ok(LegacyGalleryImport {
        key: string(k, key, "key")?,
        created: int(k, created, "creation time")?,
        query: string(k, query, "query")?,
        source_name: string(k, source_name, "source name")?,
        current_page_index: int(k, current_page_index, "current page index")?,
        num_urls_found: int(k, num_urls_found, "urls found")?,
        num_new_urls_found: int(k, num_new_urls_found, "new urls found")?,
        file_limit: opt_int(k, file_limit, "file limit")?,
        gallery_paused: boolean(k, gallery_paused, "gallery paused")?,
        files_paused: boolean(k, files_paused, "files paused")?,
        import_options: import_options(k, options)?,
        gallery_seeds: gallery_seeds(k, gallery_log)?,
        file_seeds: file_seeds(k, file_seed_cache)?,
        no_work_until: int(k, no_work_until, "no work until")?,
        no_work_until_reason: string(k, no_work_until_reason, "no work until reason")?,
    })
}

/// Decode a gallery page's importer.
pub fn multiple_gallery_import(
    object: &SerialisableObject,
) -> DecodeResult<LegacyMultipleGalleryImport> {
    let k = MULTIPLE_GALLERY_IMPORT;
    expect(object, k, &[10])?;
    let info = object.info();
    let [
        gug,
        highlighted,
        file_limit,
        pend_options,
        options,
        gallery_imports,
    ] = tuple::<6>(k, &info, "gallery page")?;
    let [gug_key, gug_name] = tuple::<2>(k, gug, "gug key and name")?;
    let [
        start_files_paused,
        start_gallery_paused,
        no_new_dupes,
        merge_pends,
    ] = tuple::<4>(k, pend_options, "pend options")?;
    Ok(LegacyMultipleGalleryImport {
        gug_key: string(k, gug_key, "gug key")?,
        gug_name: string(k, gug_name, "gug name")?,
        highlighted: opt_string(k, highlighted, "highlighted gallery import")?,
        file_limit: opt_int(k, file_limit, "file limit")?,
        start_file_queues_paused: boolean(k, start_files_paused, "start files paused")?,
        start_gallery_queues_paused: boolean(k, start_gallery_paused, "start gallery paused")?,
        do_not_allow_new_dupes: boolean(k, no_new_dupes, "no new dupes")?,
        merge_simultaneous_pends_to_one_importer: boolean(k, merge_pends, "merge pends")?,
        import_options: import_options(k, options)?,
        gallery_imports: objects(k, gallery_imports, "gallery imports")?
            .iter()
            .map(gallery_import)
            .collect::<DecodeResult<_>>()?,
    })
}

/// Decode a watcher.
pub fn watcher_import(object: &SerialisableObject) -> DecodeResult<LegacyWatcherImport> {
    let k = WATCHER_IMPORT;
    expect(object, k, &[10])?;
    let info = object.info();
    let [
        url,
        gallery_log,
        file_seed_cache,
        filterable,
        additional,
        checker,
        options,
        last_check_time,
        files_paused,
        checking_paused,
        checking_status,
        subject,
        no_work_until,
        no_work_until_reason,
        created,
    ] = tuple::<15>(k, &info, "watcher")?;
    Ok(LegacyWatcherImport {
        url: string(k, url, "url")?,
        gallery_seeds: gallery_seeds(k, gallery_log)?,
        file_seeds: file_seeds(k, file_seed_cache)?,
        external_filterable_tags: strings(k, filterable, "filterable tags")?,
        external_additional_tags: service_keys_to_tags(k, additional)?,
        checker: checker_options(&nested(k, checker, "checker options")?)?,
        import_options: import_options(k, options)?,
        last_check_time: int(k, last_check_time, "last check time")?,
        files_paused: boolean(k, files_paused, "files paused")?,
        checking_paused: boolean(k, checking_paused, "checking paused")?,
        checking_status: int(k, checking_status, "checking status")?,
        subject: string(k, subject, "subject")?,
        no_work_until: int(k, no_work_until, "no work until")?,
        no_work_until_reason: string(k, no_work_until_reason, "no work until reason")?,
        created: int(k, created, "creation time")?,
    })
}

/// Decode a watcher page's importer.
pub fn multiple_watcher_import(
    object: &SerialisableObject,
) -> DecodeResult<LegacyMultipleWatcherImport> {
    let k = MULTIPLE_WATCHER_IMPORT;
    expect(object, k, &[4])?;
    let info = object.info();
    let [watchers, highlighted, checker, options] = tuple::<4>(k, &info, "watcher page")?;
    Ok(LegacyMultipleWatcherImport {
        highlighted: opt_string(k, highlighted, "highlighted watcher")?,
        checker: checker_options(&nested(k, checker, "checker options")?)?,
        import_options: import_options(k, options)?,
        watchers: objects(k, watchers, "watchers")?
            .iter()
            .map(watcher_import)
            .collect::<DecodeResult<_>>()?,
    })
}
