//! `/manage_pages/*`: the desktop client's pages.
//!
//! The client keeps its pages in the store as they change (the session
//! "last session", with the page it shows and each page's selection), so
//! these answer from the store whether or not it is open (DECISIONS.md,
//! 2026-10-01). What they ask of a page goes to the client while it is open,
//! and is otherwise done to the session, which the client opens with.

use crate::auth::PermissionChecks as _;
use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use serde_json::{Map, Value as Json, json};

use hydrus_core::pages::{DownloaderKind, Page, PageContent, PageKey, Session};
use hydrus_core::{HashId, Sha256};
use hydrus_store::sessions::{self, LAST_SESSION, PageCommand};
use hydrus_store::{master, media};

use crate::AppState;
use crate::auth::Permission;
use crate::error::{ApiError, ApiResult};
use crate::media_json::{self, MetadataOptions};
use crate::request::{ApiRequest, ApiResponse};
use crate::routes::files::parse_hashes;

/// The reference's page types (`ClientGUIPagesCore`).
const PAGE_TYPE_PAGE_OF_PAGES: i64 = 10;

/// What a page is, by the reference's page types.
fn page_type(content: &PageContent) -> i64 {
    content.page_type()
}

/// The pages as the client keeps them: the last session, its top notebook's
/// key, and the page it shows.
struct Pages {
    session: Session,
    top: PageKey,
    shown: Option<PageKey>,
}

/// A page found by key: the top notebook, or a page in it.
enum Found<'a> {
    Top,
    Page(&'a Page),
}

impl Pages {
    fn read(app: &AppState) -> ApiResult<Self> {
        let read = |c: &rusqlite::Connection| {
            Ok((
                sessions::load(c, LAST_SESSION)?,
                sessions::top_key(c, LAST_SESSION)?,
                sessions::shown(c, LAST_SESSION)?,
            ))
        };
        if let (Some(session), Some(top), shown) = app.store.read(read)? {
            return Ok(Self {
                session,
                top,
                shown,
            });
        }
        // (a store the client hasn't opened yet: no pages, but a top
        // notebook, whose key is kept)
        app.store.write(|ctx| {
            if sessions::load(ctx.conn(), LAST_SESSION)?.is_none() {
                let empty = Session {
                    name: LAST_SESSION.to_owned(),
                    pages: Vec::new(),
                };
                sessions::save(
                    ctx.conn(),
                    &empty,
                    hydrus_core::time::TimestampMs::now().0 / 1000,
                )?;
            }
            Ok(())
        })?;
        match app.store.read(read)? {
            (Some(session), Some(top), shown) => Ok(Self {
                session,
                top,
                shown,
            }),
            _ => Err(ApiError::server("the session couldn't be kept")),
        }
    }

    fn find(&self, key: &[u8]) -> Option<Found<'_>> {
        if key == self.top.0 {
            return Some(Found::Top);
        }
        self.session
            .all_pages()
            .into_iter()
            .find(|p| p.key.0 == key)
            .map(Found::Page)
    }

    /// The pages on the way to the page shown: the one shown in each
    /// notebook from the top (each notebook's first, past the page shown or
    /// without one).
    fn selected(&self) -> Vec<PageKey> {
        fn path_to(pages: &[Page], key: &PageKey, path: &mut Vec<PageKey>) -> bool {
            for page in pages {
                path.push(page.key);
                if page.key == *key {
                    return true;
                }
                if let PageContent::Pages(children) = &page.content
                    && path_to(children, key, path)
                {
                    return true;
                }
                path.pop();
            }
            false
        }
        // (a notebook shown shows its first page, as the client opens it)
        let mut path = Vec::new();
        let mut pages = self.session.pages.as_slice();
        if let Some(shown) = &self.shown
            && path_to(&self.session.pages, shown, &mut path)
        {
            pages = match self
                .session
                .all_pages()
                .into_iter()
                .find(|p| p.key == *shown)
            {
                Some(Page {
                    content: PageContent::Pages(children),
                    ..
                }) => children,
                _ => return path,
            };
        } else {
            path.clear();
        }
        while let Some(first) = pages.first() {
            path.push(first.key);
            match &first.content {
                PageContent::Pages(children) => pages = children,
                _ => break,
            }
        }
        path
    }
}

/// A page as `get_pages` lists it.
fn listed(page: &Page, selected: &[PageKey]) -> Json {
    let mut row = Map::new();
    row.insert("name".into(), json!(page.name));
    row.insert("page_key".into(), json!(page.key.to_hex()));
    row.insert("page_state".into(), json!(0));
    row.insert("page_type".into(), json!(page_type(&page.content)));
    let is_notebook = matches!(page.content, PageContent::Pages(_));
    row.insert("is_media_page".into(), json!(!is_notebook));
    row.insert("selected".into(), json!(selected.contains(&page.key)));
    if let PageContent::Pages(children) = &page.content {
        let pages: Vec<Json> = children.iter().map(|p| listed(p, selected)).collect();
        row.insert("pages".into(), json!(pages));
    }
    Json::Object(row)
}

/// The top notebook's name, as the reference's is called.
const TOP_NAME: &str = "top page notebook";

pub async fn get_pages(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManagePages)?;
    let body = app
        .clone()
        .blocking(|app| {
            let pages = Pages::read(app)?;
            let selected = pages.selected();
            let children: Vec<Json> = pages
                .session
                .pages
                .iter()
                .map(|p| listed(p, &selected))
                .collect();
            Ok(json!({
                "pages": {
                    "name": TOP_NAME,
                    "page_key": pages.top.to_hex(),
                    "page_state": 0,
                    "page_type": PAGE_TYPE_PAGE_OF_PAGES,
                    "is_media_page": false,
                    "selected": true,
                    "pages": children,
                }
            }))
        })
        .await?;
    Ok(ApiResponse::json(body, &req))
}

pub async fn get_page_info(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManagePages)?;
    let key = req.params.required::<Vec<u8>>("page_key")?;
    let simple = req.params.or("simple", true)?;
    let body = app
        .clone()
        .blocking(move |app| {
            let pages = Pages::read(app)?;
            let not_found = || {
                ApiError::not_found(format!(
                    "Did not find a page for \"{}\"!",
                    hex::encode(&key)
                ))
            };
            let page = match pages.find(&key).ok_or_else(not_found)? {
                Found::Top => {
                    return Ok(json!({ "page_info": {
                        "name": TOP_NAME,
                        "page_key": pages.top.to_hex(),
                        "page_state": 0,
                        "page_type": PAGE_TYPE_PAGE_OF_PAGES,
                        "is_media_page": false,
                    }}));
                }
                Found::Page(page) => page,
            };
            let mut info = Map::new();
            info.insert("name".into(), json!(page.name));
            info.insert("page_key".into(), json!(page.key.to_hex()));
            info.insert("page_state".into(), json!(0));
            info.insert("page_type".into(), json!(page_type(&page.content)));
            if matches!(page.content, PageContent::Pages(_)) {
                info.insert("is_media_page".into(), json!(false));
                return Ok(json!({ "page_info": info }));
            }
            info.insert("is_media_page".into(), json!(true));
            // (a downloader page's importers are described)
            let management = match &page.content {
                PageContent::Downloader {
                    kind: DownloaderKind::Urls,
                    queues,
                    ..
                } if !queues.is_empty() => {
                    json!({ "urls_import": urls_import(app, queues[0], simple)? })
                }
                PageContent::Downloader {
                    kind: DownloaderKind::Simple,
                    queues,
                    ..
                } if !queues.is_empty() => {
                    json!({ "simple_downloader_import": simple_import(app, queues[0], simple)? })
                }
                PageContent::Downloader {
                    kind: DownloaderKind::Local,
                    queues,
                    ..
                } if !queues.is_empty() => {
                    json!({ "hdd_import": hdd_import(app, queues[0], simple)? })
                }
                PageContent::Downloader {
                    kind: DownloaderKind::Gallery,
                    queues,
                    page: state,
                    ..
                } => {
                    let imports = queues
                        .iter()
                        .map(|&q| gallery_import(app, q, simple))
                        .collect::<ApiResult<Vec<_>>>()?;
                    json!({ "multiple_gallery_import": {
                        "gallery_imports": imports,
                        "highlight": highlight(state.as_deref()),
                    }})
                }
                PageContent::Downloader {
                    kind: DownloaderKind::Watchers,
                    queues,
                    page: state,
                    ..
                } => {
                    let imports = queues
                        .iter()
                        .map(|&q| watcher_import(app, q, simple))
                        .collect::<ApiResult<Vec<_>>>()?;
                    json!({ "multiple_watcher_import": {
                        "watcher_imports": imports,
                        "highlight": highlight(state.as_deref()),
                    }})
                }
                _ => json!({}),
            };
            info.insert("management".into(), management);
            let (files, selected) = app.store.read(|c| {
                Ok((
                    sessions::page_files(c, &page.key)?,
                    sessions::page_selected(c, &page.key)?,
                ))
            })?;
            let ids = |files: &[HashId]| files.iter().map(|f| f.get()).collect::<Vec<_>>();
            let mut media = Map::new();
            media.insert("num_files".into(), json!(files.len()));
            media.insert("hash_ids".into(), json!(ids(&files)));
            media.insert("num_files_selected".into(), json!(selected.len()));
            media.insert("hash_ids_selected".into(), json!(ids(&selected)));
            if !simple {
                let all: Vec<HashId> = files.iter().chain(&selected).copied().collect();
                let hashes = app.store.read(|c| master::hashes(c, &all))?;
                let hexes = |files: &[HashId]| {
                    files
                        .iter()
                        .filter_map(|f| hashes.get(f).map(Sha256::to_hex))
                        .collect::<Vec<_>>()
                };
                media.insert("hashes".into(), json!(hexes(&files)));
                media.insert("hashes_selected".into(), json!(hexes(&selected)));
            }
            info.insert("media".into(), Json::Object(media));
            Ok(json!({ "page_info": info }))
        })
        .await?;
    Ok(ApiResponse::json(body, &req))
}

/// An importer's key, as the reference gives each a random one: its
/// queue's number, as 32 bytes of hex.
fn importer_key(queue: i64) -> String {
    format!("{queue:064x}")
}

/// The importer a gallery or watcher page shows, or none.
fn highlight(state: Option<&hydrus_core::pages::DownloaderPageState>) -> Json {
    state
        .and_then(|s| s.highlighted)
        .map_or(Json::Null, |q| json!(importer_key(q)))
}

/// A local import page's importer (`HDDImport.GetAPIInfoDict`): its file
/// log, and whether it is paused.
fn hdd_import(app: &AppState, queue: i64, simple: bool) -> ApiResult<Json> {
    let row = app.store.read(|c| hydrus_store::queues::queue(c, queue))?;
    let (imports, _) = logs(app, queue, simple)?;
    Ok(json!({
        "imports": imports,
        "files_paused": row.is_some_and(|q| q.files_paused),
    }))
}

/// A gallery page's search (`GalleryImport.GetAPIInfoDict`).
fn gallery_import(app: &AppState, queue: i64, simple: bool) -> ApiResult<Json> {
    let row = app.store.read(|c| hydrus_store::queues::queue(c, queue))?;
    let search: hydrus_core::gallery::GallerySearch = row
        .as_ref()
        .and_then(|q| serde_json::from_value(q.extra.clone()).ok())
        .unwrap_or_default();
    let (imports, log) = logs(app, queue, simple)?;
    Ok(json!({
        "query_text": search.query,
        "source": search.source_name,
        "gallery_key": importer_key(queue),
        "files_paused": row.as_ref().is_some_and(|q| q.files_paused),
        "gallery_paused": row.as_ref().is_some_and(|q| q.gallery_paused),
        "imports": imports,
        "gallery_log": log,
    }))
}

/// A watcher page's watcher (`WatcherImport.GetAPIInfoDict`).
fn watcher_import(app: &AppState, queue: i64, simple: bool) -> ApiResult<Json> {
    use hydrus_core::watchers::CheckerStatus;

    let row = app.store.read(|c| hydrus_store::queues::queue(c, queue))?;
    let state = row.as_ref().and_then(hydrus_store::watchers::watcher_state);
    let (imports, log) = logs(app, queue, simple)?;
    let status = |s: &hydrus_core::watchers::WatcherState| match s.status {
        CheckerStatus::Ok => 0,
        CheckerStatus::Dead => 1,
        CheckerStatus::NotFound => 2,
    };
    Ok(json!({
        "url": state.as_ref().map(|s| s.url.clone()),
        "watcher_key": importer_key(queue),
        "created": state.as_ref().map(|s| s.created),
        "last_check_time": state.as_ref().map(|s| s.last_check_time),
        "next_check_time": state.as_ref().map(|s| s.next_check_time),
        "files_paused": row.as_ref().is_some_and(|q| q.files_paused),
        "checking_paused": state.as_ref().is_some_and(|s| s.checking_paused),
        "checking_status": state.as_ref().map(status),
        "subject": state.as_ref().map(|s| s.subject.clone()),
        "imports": imports,
        "gallery_log": log,
    }))
}

/// A URL downloader page's importer, as the reference describes it
/// (`URLsImport.GetAPIInfoDict`): its file log's and search log's status and
/// progress, with their items unless `simple`, and whether it is paused.
fn urls_import(app: &AppState, queue: i64, simple: bool) -> ApiResult<Json> {
    let row = app.store.read(|c| hydrus_store::queues::queue(c, queue))?;
    let (imports, log) = logs(app, queue, simple)?;
    Ok(json!({
        "imports": imports,
        "gallery_log": log,
        "files_paused": row.is_some_and(|q| q.files_paused),
    }))
}

/// A simple downloader (`SimpleDownloaderImport.GetAPIInfoDict`): its
/// file and gallery logs and whether its files and parsing are paused.
fn simple_import(app: &AppState, queue: i64, simple: bool) -> ApiResult<Json> {
    let row = app.store.read(|c| hydrus_store::queues::queue(c, queue))?;
    let (imports, log) = logs(app, queue, simple)?;
    Ok(json!({
        "imports": imports,
        "gallery_log": log,
        "files_paused": row.as_ref().is_some_and(|q| q.files_paused),
        "gallery_paused": row.is_some_and(|q| q.gallery_paused),
    }))
}

/// An importer's file log and search log (`FileSeedCache.GetAPIInfoDict`,
/// `GallerySeedLog.GetAPIInfoDict`): their status and progress, with their
/// items unless `simple`.
fn logs(app: &AppState, queue: i64, simple: bool) -> ApiResult<(Json, Json)> {
    use hydrus_store::queues;

    let (naming, files, searches, file_seeds, gallery_seeds) = app.store.read(|c| {
        Ok((
            hydrus_store::settings::get::<hydrus_core::pages::PageNameSettings>(c)?,
            queues::file_seed_counts(c, queue)?,
            queues::gallery_seed_counts(c, queue)?,
            (!simple)
                .then(|| queues::file_seeds(c, queue))
                .transpose()?,
            (!simple)
                .then(|| queues::gallery_seeds(c, queue))
                .transpose()?,
        ))
    })?;
    let (done, total) = queues::file_log_value_range(&files);
    let mut imports = json!({
        "status": queues::file_log_status(&files),
        "simple_status": queues::file_log_short_status(
            &files,
            naming.short_summary_new,
            naming.short_summary_deleted,
        ),
        "total_processed": done,
        "total_to_process": total,
    });
    if let Some(seeds) = file_seeds {
        let items: Vec<Json> = seeds
            .iter()
            .map(|seed| {
                let hash = seed
                    .meta
                    .hashes
                    .iter()
                    .find(|(kind, _)| kind == "sha256")
                    .map(|(_, hex)| hex.clone());
                json!({
                    "import_data": seed.data,
                    "created": seed.created,
                    "modified": seed.modified,
                    "source_time": seed.source_time,
                    "status": seed.status.code(),
                    "note": seed.note,
                    "hash": hash,
                })
            })
            .collect();
        imports["import_items"] = json!(items);
    }
    let (status, (done, total)) = queues::search_log_status(&searches);
    let mut log = json!({
        "status": status,
        "total_processed": done,
        "total_to_process": total,
    });
    if let Some(seeds) = gallery_seeds {
        let items: Vec<Json> = seeds
            .iter()
            .map(|seed| {
                json!({
                    "url": seed.url,
                    "created": seed.created,
                    "modified": seed.modified,
                    "status": seed.status.code(),
                    "note": seed.note,
                })
            })
            .collect();
        log["log_items"] = json!(items);
    }
    Ok((imports, log))
}

/// Ask a page to do something: of the client, while it is open; otherwise
/// done to the session it opens with.
fn ask(app: &AppState, page: PageKey, command: PageCommand) -> ApiResult<()> {
    let open = hydrus_store::store::gui_open(app.store.dir());
    app.store.write(move |ctx| {
        if open {
            sessions::push_command(ctx.conn(), &page, &command)
        } else {
            sessions::apply_command(ctx.conn(), &page, &command)
        }
    })?;
    Ok(())
}

fn could_not_find() -> ApiError {
    ApiError::not_found("Could not find that page!")
}

pub async fn add_files(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManagePages)?;
    let params = req.params.clone();
    app.clone()
        .blocking(move |app| {
            let Some(key) = params.optional::<Vec<u8>>("page_key")? else {
                return Err(ApiError::bad_request(
                    "You need a page key for this request!",
                ));
            };
            let hashes = parse_hashes(app, &params)?.ok_or_else(|| {
                ApiError::bad_request(
                    "Please include some files in your request--file_id or hash based!",
                )
            })?;
            // (files it hasn't heard of get ids, as reading them does in the
            // reference)
            let files: Vec<HashId> = app.store.write(move |ctx| {
                hashes
                    .iter()
                    .map(|h| master::intern_hash(ctx.conn(), h))
                    .collect::<hydrus_store::Result<_>>()
            })?;
            let pages = Pages::read(app)?;
            match pages.find(&key).ok_or_else(could_not_find)? {
                Found::Page(page) if !matches!(page.content, PageContent::Pages(_)) => {
                    ask(app, page.key, PageCommand::AddFiles(files))
                }
                _ => Err(ApiError::bad_request(
                    "That page key was not for a normal media page!",
                )),
            }
        })
        .await?;
    Ok(ApiResponse::Empty)
}

pub async fn focus_page(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManagePages)?;
    let key = req.params.required::<Vec<u8>>("page_key")?;
    app.clone()
        .blocking(move |app| {
            let pages = Pages::read(app)?;
            match pages.find(&key).ok_or_else(could_not_find)? {
                // (the top notebook is always shown)
                Found::Top => Ok(()),
                Found::Page(page) => ask(app, page.key, PageCommand::Focus),
            }
        })
        .await?;
    Ok(ApiResponse::Empty)
}

pub async fn refresh_page(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManagePages)?;
    let key = req.params.required::<Vec<u8>>("page_key")?;
    app.clone()
        .blocking(move |app| {
            let pages = Pages::read(app)?;
            // (a notebook refreshes the pages in it, where the reference
            // fails)
            let page = match pages.find(&key).ok_or_else(could_not_find)? {
                Found::Top => pages.top,
                Found::Page(page) => page.key,
            };
            ask(app, page, PageCommand::Refresh)
        })
        .await?;
    Ok(ApiResponse::Empty)
}

pub async fn get_media_viewers(
    State(app): State<Arc<AppState>>,
    req: ApiRequest,
) -> ApiResult<ApiResponse> {
    app.authenticate(&req)?.check(Permission::ManagePages)?;
    let body = app
        .clone()
        .blocking(|app| {
            // (none while the client is closed, whatever it left behind)
            if !hydrus_store::store::gui_open(app.store.dir()) {
                return Ok(json!({ "media_viewers": [] }));
            }
            let viewers = app.store.read(sessions::media_viewers)?;
            let files: Vec<HashId> = viewers.iter().filter_map(|v| v.file).collect();
            let snapshot = app.store.snapshot();
            let batch = app
                .store
                .read(|c| media::load(c, &snapshot.services, Some(&snapshot.display), &files))?;
            let by_id: HashMap<HashId, &media::MediaResult> =
                batch.results.iter().map(|m| (m.hash_id, m)).collect();
            let tag_names = media_json::TagNames::new(&batch.tags);
            // (GetMediaResultAPIDict's options)
            let opts = MetadataOptions {
                include_notes: true,
                include_milliseconds: true,
                include_blurhash: true,
                hide_service_keys_tags: true,
                detailed_urls: true,
            };
            let listed: Vec<Json> = viewers
                .iter()
                .map(|v| {
                    let current = v
                        .file
                        .and_then(|f| by_id.get(&f))
                        .map(|m| media_json::full_row(&snapshot, m, &tag_names, opts));
                    json!({
                        "canvas_type": v.canvas_type,
                        "canvas_key": hex::encode(v.canvas_key),
                        "current_media": current,
                    })
                })
                .collect();
            Ok(json!({ "media_viewers": listed }))
        })
        .await?;
    Ok(ApiResponse::json(body, &req))
}
