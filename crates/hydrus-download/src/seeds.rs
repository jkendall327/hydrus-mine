//! Working on one URL file seed (`FileSeed.WorkOnURL`).

use std::collections::BTreeSet;

use hydrus_core::import_options::FullImportOptions;
use hydrus_core::url::{UrlClasses, UrlType};
use hydrus_import::FileImportOptions;
use hydrus_net::{Job, NetError, Request, StatusKind};
use hydrus_parse::ParsingContext;
use hydrus_parse::content::{ParseFailure, ParsedPost, TIMESTAMP_MODIFIED_DOMAIN};
use hydrus_store::queues::{self, FileSeed, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};

use crate::{Downloader, WorkError, now, seed_status, sensible};

/// `HC.URL_TYPE_DESIRED` and `HC.URL_TYPE_SOURCE`, as parsers mark URLs.
const URL_TYPE_DESIRED: i64 = 7;
const URL_TYPE_SOURCE: i64 = 8;

/// A seed's identity and what it has gathered, whether it is stored yet or
/// not.
pub(crate) struct SeedMut<'a> {
    pub data: &'a str,
    pub comparison: &'a str,
    pub referral: Option<&'a str>,
    pub source_time: &'a mut Option<i64>,
    pub meta: &'a mut FileSeedMeta,
}

/// URLs normalised for storage, keeping those files should remember
/// (`NormaliseAndFilterAssociableURLs`).
pub(crate) fn associable(
    classes: &UrlClasses,
    urls: impl IntoIterator<Item = String>,
) -> BTreeSet<String> {
    urls.into_iter()
        .filter(|u| hydrus_core::url::functions::check_full_url(u).is_ok())
        .filter_map(|u| classes.normalise(&u, false).ok())
        .filter(|u| classes.should_associate_with_files(u))
        .collect()
}

fn class_key(classes: &UrlClasses, url: &str) -> Option<Vec<u8>> {
    classes.class_for(url).map(|c| c.key.clone())
}

impl SeedMut<'_> {
    /// `_AddPrimaryURLs`: URLs this file is at.
    pub fn add_primary_urls(
        &mut self,
        classes: &UrlClasses,
        urls: impl IntoIterator<Item = String>,
    ) {
        let mut urls = associable(classes, urls);
        urls.remove(self.data);
        urls.remove(self.comparison);
        if let Some(referral) = self.referral {
            urls.remove(referral);
        }
        for url in urls {
            self.meta.source_urls.remove(&url);
            self.meta.primary_urls.insert(url);
        }
    }

    /// `_AddSourceURLs`: URLs the file came from, less the site's own
    /// (a "source" of the same URL class as a primary URL is a loop back).
    pub fn add_source_urls(
        &mut self,
        classes: &UrlClasses,
        urls: impl IntoIterator<Item = String>,
    ) {
        let mut all_primary: BTreeSet<String> = self.meta.primary_urls.clone();
        all_primary.insert(self.data.to_owned());
        all_primary.insert(self.comparison.to_owned());
        if let Some(referral) = self.referral {
            all_primary.insert(referral.to_owned());
        }
        let primary_classes: BTreeSet<Vec<u8>> = all_primary
            .iter()
            .filter_map(|u| class_key(classes, u))
            .collect();
        for url in associable(classes, urls) {
            if all_primary.contains(&url) {
                continue;
            }
            if class_key(classes, &url).is_some_and(|k| primary_classes.contains(&k)) {
                continue;
            }
            self.meta.source_urls.insert(url);
        }
    }

    /// `SetSourceTimeIfSensible`: keep the earliest sensible time, never in
    /// the future.
    pub fn set_source_time_if_sensible(&mut self, time: Option<i64>) {
        if !sensible(time) {
            return;
        }
        let time = time.expect("sensible").min(now() - 30);
        if self.source_time.is_none_or(|existing| time < existing) {
            *self.source_time = Some(time);
        }
    }

    /// `AddParsedPost`: take what a parser found.
    pub fn add_parsed_post(&mut self, classes: &UrlClasses, post: &ParsedPost) {
        let headers: Vec<(String, String)> = post.http_headers().into_iter().collect();
        self.meta.add_request_headers(&headers);
        for (hash_type, hash) in post.hashes() {
            self.meta.add_hash_if_new(&hash_type, hex::encode(hash));
        }
        self.add_source_urls(classes, post.urls(&[URL_TYPE_SOURCE], false));
        self.meta.tags.extend(post.tags());
        for (name, note) in post.notes() {
            self.meta.set_note(&name, note);
        }
        self.set_source_time_if_sensible(post.timestamp(TIMESTAMP_MODIFIED_DOMAIN, now()));
    }
}

fn seed_mut(seed: &mut FileSeed) -> SeedMut<'_> {
    SeedMut {
        data: &seed.data,
        comparison: &seed.data_for_comparison,
        referral: seed.referral_url.as_deref(),
        source_time: &mut seed.source_time,
        meta: &mut seed.meta,
    }
}

fn new_seed_mut(seed: &mut NewFileSeed) -> SeedMut<'_> {
    SeedMut {
        data: &seed.data,
        comparison: &seed.data_for_comparison,
        referral: seed.referral_url.as_deref(),
        source_time: &mut seed.source_time,
        meta: &mut seed.meta,
    }
}

/// A new URL seed, normalised as the reference's constructor does.
pub(crate) fn new_url_seed(classes: &UrlClasses, url: &str) -> NewFileSeed {
    let (data, comparison) = match (classes.normalise(url, true), classes.normalise(url, false)) {
        (Ok(data), Ok(comparison)) => (data, comparison),
        _ => (url.to_owned(), url.to_owned()),
    };
    NewFileSeed {
        seed_type: SeedType::Url,
        data,
        data_for_comparison: comparison,
        source_time: None,
        referral_url: None,
        meta: FileSeedMeta::default(),
    }
}

/// `ConvertParsedPostsToParsedPostsAndFileSeeds`: a seed per new top
/// file URL of each post, carrying that post's metadata.
pub(crate) fn seeds_from_posts(
    classes: &UrlClasses,
    posts: &[ParsedPost],
    source_url: &str,
) -> Vec<NewFileSeed> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for post in posts {
        for url in post.urls(&[URL_TYPE_DESIRED], true) {
            if !seen.insert(url.clone()) {
                continue;
            }
            let mut seed = new_url_seed(classes, &url);
            seed.referral_url = Some(source_url.to_owned());
            new_seed_mut(&mut seed).add_parsed_post(classes, post);
            out.push(seed);
        }
    }
    out
}

/// How a seed ends.
pub(crate) enum Stop {
    Veto(String),
    Error(String),
    Failed(WorkError),
}

impl From<WorkError> for Stop {
    fn from(e: WorkError) -> Self {
        Stop::Failed(e)
    }
}

impl From<hydrus_store::StoreError> for Stop {
    fn from(e: hydrus_store::StoreError) -> Self {
        Stop::Failed(e.into())
    }
}

impl From<hydrus_import::ImportError> for Stop {
    fn from(e: hydrus_import::ImportError) -> Self {
        Stop::Failed(e.into())
    }
}

impl From<std::io::Error> for Stop {
    fn from(e: std::io::Error) -> Self {
        Stop::Failed(e.into())
    }
}

/// How a file is fetched (`DownloadAndImportRawFile`'s arguments).
#[derive(Default)]
struct FileFetch<'a> {
    /// The referral to send, over the seed's own.
    forced_referral_url: Option<&'a str>,
    /// The page it was found on, whose site's bandwidth it counts against.
    spawning_url: Option<&'a str>,
    override_bandwidth_after: Option<u64>,
}

/// Network failures as `WorkOnURL` treats them: some statuses end the seed
/// as vetoed, the rest as an error.
fn network(e: NetError, job: &Job) -> Stop {
    match &e {
        NetError::Status {
            kind: StatusKind::NotFound,
            ..
        } => Stop::Veto("404".into()),
        NetError::Status {
            kind: StatusKind::InsufficientCredentials,
            ..
        } => Stop::Veto("403".into()),
        NetError::Status {
            kind: StatusKind::Censorship,
            ..
        } => Stop::Veto("site reports http status code 451: Unavailable For Legal Reasons".into()),
        NetError::Cancelled => Stop::Veto(job.cancelled_note()),
        _ => Stop::Failed(WorkError::Network(e)),
    }
}

impl Downloader {
    /// Work on a URL seed: find and import its file(s) and write what was
    /// learned. Updates `seed` (the caller saves it); whether anything
    /// substantial (network or import) was done. As in the reference, a
    /// failure only ends this seed (vetoed or an error) and its queue
    /// carries on; a site that keeps failing is paused by the network
    /// engine instead.
    pub async fn work_on_url(
        &self,
        seed: &mut FileSeed,
        options: &FullImportOptions,
        job: &Job,
    ) -> bool {
        let mut did_work = false;
        let outcome = self.work(seed, options, job, &mut did_work).await;
        match outcome {
            Ok(()) => {}
            Err(Stop::Veto(note)) if note == "403" && self.had_login(&seed.data) => set_status(
                seed,
                SeedStatus::Vetoed,
                "403 (hydrus logged in to this site with a login script, which hydrus-rs doesn't run: its cookies may need refreshing)".into(),
            ),
            Err(Stop::Veto(note)) => set_status(seed, SeedStatus::Vetoed, note),
            Err(Stop::Error(note)) => set_status(seed, SeedStatus::Error, note),
            Err(Stop::Failed(e)) => {
                set_status(seed, SeedStatus::Error, e.to_string());
                // (a moment's pause before the next, as the reference has)
                if matches!(e, WorkError::Network(_)) {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
        }
        did_work
    }

    async fn work(
        &self,
        seed: &mut FileSeed,
        options: &FullImportOptions,
        job: &Job,
        did_work: &mut bool,
    ) -> Result<(), Stop> {
        let snapshot = self.store.snapshot();
        let classes = &snapshot.url_classes;
        if hydrus_core::url::functions::check_full_url(&seed.data).is_err() {
            return Err(Stop::Veto(format!(
                "{} did not look like a full URL!",
                seed.data
            )));
        }
        let capability = classes.parse_capability(&seed.data);
        if !matches!(
            capability.url_type,
            UrlType::Post | UrlType::File | UrlType::Unknown
        ) {
            return Err(Stop::Veto(format!(
                "This URL appeared to be a \"{}\", which is not a File or Post URL!",
                capability.match_name
            )));
        }
        if capability.url_type == UrlType::Post
            && let Err(reason) = &capability.parser
        {
            return Err(Stop::Veto(format!(
                "Cannot parse {}: {reason}",
                capability.match_name
            )));
        }
        job.set_status_text("checking url status");
        let (should_download_metadata, should_download_file) = self.predict(seed, options, None)?;
        if capability.url_type == UrlType::Post {
            if should_download_metadata {
                *did_work = true;
                self.work_on_post(seed, options, job).await?;
            }
        } else if should_download_file {
            self.check_tags_veto(seed, options)?;
            *did_work = true;
            let file_url = seed.data.clone();
            // (it counts against the page it was found on too)
            let spawning = seed.referral_url.clone();
            let how = FileFetch {
                spawning_url: spawning.as_deref(),
                ..FileFetch::default()
            };
            self.download_and_import(seed, &file_url, options, job, how)
                .await?;
        }
        *did_work |= self.write_content_updates(seed, options)?;
        if seed.status == SeedStatus::Unknown {
            return Err(Stop::Veto(
                "Managed to work this job without getting a result! Please report to hydrus_dev!"
                    .into(),
            ));
        }
        Ok(())
    }

    async fn work_on_post(
        &self,
        seed: &mut FileSeed,
        options: &FullImportOptions,
        job: &Job,
    ) -> Result<(), Stop> {
        let snapshot = self.store.snapshot();
        let classes = &snapshot.url_classes;
        let mut post_url = seed.data.clone();
        let (mut url_to_fetch, mut parser_key) = classes
            .url_to_fetch_and_parser(&post_url)
            .map_err(|e| Stop::Veto(e.to_string()))?;
        job.set_status_text("downloading file page");
        let mut request = Request::get(url_to_fetch.clone());
        request.referral_url.clone_from(&seed.referral_url);
        request
            .additional_headers
            .clone_from(&seed.meta.request_headers);
        let response = self
            .net
            .fetch(&request, job)
            .await
            .map_err(|e| network(e, job))?;
        let text = response.text();
        let actual = response.url.clone();
        let url_for_child_referral = actual.clone();
        if actual != url_to_fetch {
            let capability = classes.parse_capability(&actual);
            if capability.url_type == UrlType::Post && capability.parser.is_ok() {
                seed_mut(seed).add_primary_urls(classes, [actual.clone()]);
                post_url = actual;
                (url_to_fetch, parser_key) = classes
                    .url_to_fetch_and_parser(&post_url)
                    .map_err(|e| Stop::Veto(e.to_string()))?;
            }
        }
        let Some(parser) = self.parser(&parser_key) else {
            return Err(Stop::Veto(format!("The parser for {post_url} is missing!")));
        };
        let mut context = ParsingContext::new();
        context.insert("post_url".into(), post_url);
        context.insert("url".into(), url_to_fetch.clone());
        let posts = match parser.parse(&mut context, &text) {
            Ok(posts) => posts,
            Err(ParseFailure::Veto(reason)) => return Err(Stop::Veto(format!("veto: {reason}"))),
            Err(ParseFailure::Error(e)) => return Err(Stop::Error(e.to_string())),
        };
        if posts.is_empty() {
            return self.import_page_as_file(seed, &response.body, options);
        }
        let mut children = seeds_from_posts(classes, &posts, &url_for_child_referral);
        if children.is_empty() {
            return Err(Stop::Veto(
                "The parser found something in the document, but could not find a file or post URL to download!".into(),
            ));
        }
        if children.len() == 1 {
            let file_url = children[0].data.clone();
            let url_type = classes.parse_capability(&file_url).url_type;
            if matches!(url_type, UrlType::File | UrlType::Unknown) {
                children.clear();
                seed_mut(seed).add_parsed_post(classes, &posts_with_url(&posts, &file_url));
                self.check_tags_veto(seed, options)?;
                let (_, should_download_file) = self.predict(seed, options, Some(&file_url))?;
                if should_download_file {
                    // the post page is its referral, and it counts against
                    // the post's site too; by default it waits for bandwidth
                    // only a few seconds after the post
                    let override_after = self
                        .net
                        .bandwidth_settings()
                        .override_on_file_urls_from_posts
                        .then_some(3);
                    let how = FileFetch {
                        forced_referral_url: Some(&url_for_child_referral),
                        spawning_url: Some(&url_to_fetch),
                        override_bandwidth_after: override_after,
                    };
                    self.download_and_import(seed, &file_url, options, job, how)
                        .await?;
                }
            }
        }
        if !children.is_empty() {
            for child in &mut children {
                give_child_my_info(classes, seed, child, &url_for_child_referral);
            }
            let added = self.store.write({
                let seed = seed.clone();
                let children = children.clone();
                move |ctx| queues::insert_file_seeds_after(ctx.conn(), &seed, &children, now())
            })?;
            set_status(
                seed,
                SeedStatus::SuccessfulAndChildFiles,
                format!(
                    "Found {} new URLs.",
                    hydrus_core::numbers::human_int(added as u64)
                ),
            );
        }
        Ok(())
    }

    /// A post URL that gave no posts might have been a file after all.
    fn import_page_as_file(
        &self,
        seed: &mut FileSeed,
        body: &[u8],
        options: &FullImportOptions,
    ) -> Result<(), Stop> {
        let temp = tempfile::NamedTempFile::new_in(self.scratch_dir()?)?;
        std::fs::write(temp.path(), body)?;
        let is_file = self
            .importer
            .tools()
            .detect_mime(temp.path())
            .is_ok_and(hydrus_media::mimes::is_allowed);
        if !is_file {
            return Err(Stop::Veto(
                "The parser found nothing in the document, nor did it seem to be an importable file!".into(),
            ));
        }
        match self.import_file(seed, temp.path(), options) {
            Ok(()) => Ok(()),
            Err(_) => Err(Stop::Veto(
                "The parser found nothing in the document, and while the document initially seemed to actually be an importable file, it looks like it failed to import too! This is probably some malformed JSON or something.".into(),
            )),
        }
    }

    /// Whether the reference logged in to `url`'s site with a login script.
    fn had_login(&self, url: &str) -> bool {
        self.store
            .read(hydrus_store::settings::get::<hydrus_store::network::LoginDomains>)
            .is_ok_and(|logins| logins.covers(url))
    }

    fn scratch_dir(&self) -> std::io::Result<std::path::PathBuf> {
        let dir = self.store.dir().join("tmp");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// `CheckPreFetchMetadata`: veto by the parsed tags (and their
    /// siblings).
    fn check_tags_veto(&self, seed: &FileSeed, options: &FullImportOptions) -> Result<(), Stop> {
        if seed.meta.tags.is_empty() || options.tag_filtering.allows_everything() {
            return Ok(());
        }
        let siblings = self.sibling_chain_tags(&seed.meta.tags)?;
        options
            .tag_filtering
            .check_tags_veto(&seed.meta.tags, &siblings)
            .map_err(Stop::Veto)
    }

    /// `Import`: import a downloaded file and take its result.
    pub(crate) fn import_file(
        &self,
        seed: &mut FileSeed,
        path: &std::path::Path,
        options: &FullImportOptions,
    ) -> Result<(), Stop> {
        let file_options = FileImportOptions::from_full(options, &self.store.snapshot().services);
        let result = self.importer.import_path(path, &file_options)?;
        if let Some(message) = result.raised {
            // the reference's import raised, before the seed took the hash
            return Err(if result.status == hydrus_import::ImportStatus::Vetoed {
                Stop::Veto(message)
            } else {
                Stop::Error(message)
            });
        }
        set_status(seed, seed_status(result.status), result.note);
        if let Some(hash) = result.hash {
            seed.meta.set_hash("sha256", hash.to_hex());
        }
        Ok(())
    }

    /// `DownloadAndImportRawFile`.
    async fn download_and_import(
        &self,
        seed: &mut FileSeed,
        file_url: &str,
        options: &FullImportOptions,
        job: &Job,
        how: FileFetch<'_>,
    ) -> Result<(), Stop> {
        let FileFetch {
            forced_referral_url,
            spawning_url,
            override_bandwidth_after,
        } = how;
        let snapshot = self.store.snapshot();
        let classes = &snapshot.url_classes;
        seed_mut(seed).add_primary_urls(classes, [file_url.to_owned()]);
        let referral = match forced_referral_url {
            Some(r) => Some(r.to_owned()),
            None if seed.data != file_url => Some(seed.data.clone()),
            None => seed.referral_url.clone(),
        };
        job.set_status_text("downloading file");
        let url_to_fetch = classes
            .url_to_fetch(file_url)
            .map_err(|e| Stop::Veto(e.to_string()))?;
        let temp = tempfile::NamedTempFile::new_in(self.scratch_dir()?)?;
        let mut request = Request::get(url_to_fetch.clone());
        request.referral_url = referral;
        request
            .additional_headers
            .clone_from(&seed.meta.request_headers);
        request.destination = Some(temp.path().to_path_buf());
        request
            .bandwidth_urls
            .extend(spawning_url.map(str::to_owned));
        request.override_bandwidth_after = override_bandwidth_after;
        let response = self
            .net
            .fetch(&request, job)
            .await
            .map_err(|e| network(e, job))?;
        if url_to_fetch != file_url {
            seed_mut(seed).add_primary_urls(classes, [url_to_fetch.clone()]);
        }
        let actual = response.url.clone();
        if actual != file_url && actual != url_to_fetch {
            seed_mut(seed).add_primary_urls(classes, [actual.clone()]);
            let capability = classes.parse_capability(&actual);
            if capability.url_type == UrlType::Post && capability.parser.is_ok() {
                let original = classes.parse_capability(&seed.data);
                if original.url_type == capability.url_type
                    && original.match_name == capability.match_name
                {
                    return Err(Stop::Error(format!(
                        "The downloader thought it had a raw file url with \"{file_url}\", but that redirected to the apparent Post URL \"{actual}\". As that URL has the same class as this import job's original URL, we are stopping here in case this is a looping redirect!"
                    )));
                }
                let mut redirected = new_url_seed(classes, &actual);
                redirected.referral_url = Some(file_url.to_owned());
                let queue = seed.queue_id;
                self.store.write(move |ctx| {
                    queues::add_file_seeds(ctx.conn(), queue, &[redirected], false, now())
                })?;
                set_status(
                    seed,
                    SeedStatus::SuccessfulAndChildFiles,
                    "was redirected on file download to a post url, which has been queued in the parent file log".into(),
                );
                return Ok(());
            }
        }
        let mut last_modified = response.last_modified;
        if let (Some(source_time), Some(modified)) = (seed.source_time, last_modified)
            && response.server.as_deref() == Some("cloudflare")
            && (source_time - modified).abs() > 86400 * 2
        {
            seed.meta.cloudflare_last_modified = Some(modified);
            last_modified = None;
        }
        seed_mut(seed).set_source_time_if_sensible(last_modified);
        job.set_status_text("importing file");
        self.import_file(seed, temp.path(), options)
    }
}

/// The post whose top file URL is `file_url` (the one the seed becomes).
fn posts_with_url(posts: &[ParsedPost], file_url: &str) -> ParsedPost {
    posts
        .iter()
        .find(|p| {
            p.urls(&[URL_TYPE_DESIRED], true)
                .iter()
                .any(|u| u == file_url)
        })
        .or_else(|| posts.first())
        .cloned()
        .unwrap_or_default()
}

/// `_GiveChildFileSeedMyInfo`.
fn give_child_my_info(
    classes: &UrlClasses,
    parent: &FileSeed,
    child: &mut NewFileSeed,
    url_for_child_referral: &str,
) {
    child.meta.add_request_headers(&parent.meta.request_headers);
    if child.referral_url.is_none() {
        child.referral_url = Some(url_for_child_referral.to_owned());
    }
    let mut c = new_seed_mut(child);
    if let Some(referral) = &parent.referral_url {
        c.add_source_urls(classes, [referral.clone()]);
    }
    if parent.seed_type == SeedType::Url {
        c.add_primary_urls(classes, [parent.data.clone()]);
    }
    c.add_primary_urls(classes, parent.meta.primary_urls.iter().cloned());
    c.add_source_urls(classes, parent.meta.source_urls.iter().cloned());
    c.meta
        .external_filterable_tags
        .extend(parent.meta.external_filterable_tags.iter().cloned());
    for (service, tags) in &parent.meta.external_additional_tags {
        match c
            .meta
            .external_additional_tags
            .iter_mut()
            .find(|(s, _)| s == service)
        {
            Some((_, existing)) => existing.extend(tags.iter().cloned()),
            None => c
                .meta
                .external_additional_tags
                .push((service.clone(), tags.clone())),
        }
    }
    c.meta.tags.extend(parent.meta.tags.iter().cloned());
    let mut notes = parent.meta.notes.clone();
    notes.sort();
    for (name, note) in notes {
        c.meta.set_note(&name, note);
    }
    c.set_source_time_if_sensible(parent.source_time);
}

/// `SetStatus`: a reset seed forgets the hashes it had been given.
pub(crate) fn set_status(seed: &mut FileSeed, status: SeedStatus, note: String) {
    seed.status = status;
    seed.note = note;
    if status == SeedStatus::Unknown {
        seed.meta.hashes.clear();
    }
    seed.modified = now();
}
