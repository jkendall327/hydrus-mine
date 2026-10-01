//! Working on one gallery page (`GallerySeed.WorkOnURL`): read it, queue
//! the posts it lists, and follow its sub-gallery and next pages.

use std::collections::BTreeSet;

use hydrus_core::numbers::human_int;
use hydrus_core::url::{UrlClasses, UrlType};
use hydrus_net::{Job, NetError, Request, StatusKind};
use hydrus_parse::ParsingContext;
use hydrus_parse::content::{ContentKind, PageParser, ParseFailure, ParsedPost};
use hydrus_store::queues::{
    self, GallerySeed, GallerySeedMeta, NewFileSeed, NewGallerySeed, SeedStatus,
};

use crate::seeds::{new_url_seed, seeds_from_posts};
use crate::{Downloader, WorkError, now};

const URL_TYPE_GALLERY: i64 = 3;
const URL_TYPE_NEXT: i64 = 6;
const URL_TYPE_SUB_GALLERY: i64 = 9;

/// What reading a gallery page came to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GalleryOutcome {
    pub num_urls_added: usize,
    pub num_urls_already_in_queue: usize,
    pub num_urls_total: usize,
    /// The page was gone (404, 403, 400).
    pub result_404: bool,
    pub added_new_gallery_pages: bool,
    pub can_search_for_more_files: bool,
    pub stop_reason: String,
    /// The page's title, if its parser finds one.
    pub title: Option<String>,
}

/// What became of a gallery page's file seeds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PageTaken {
    pub added: usize,
    pub already_in: usize,
    pub can_search_for_more_files: bool,
    pub stop_reason: String,
}

/// Where a gallery page's file seeds go, and whether to read on
/// (`file_seeds_callable`).
pub(crate) trait PageSink: Send {
    fn take(
        &mut self,
        downloader: &Downloader,
        seeds: Vec<NewFileSeed>,
    ) -> Result<PageTaken, WorkError>;
}

/// A gallery page's file seeds go to a queue, up to a file limit
/// (`UpdateFileSeedCacheWithFileSeeds`).
pub(crate) struct QueueSink {
    pub queue: i64,
    /// Stop after this many new URLs (a gallery search's file limit).
    pub max_new_urls: Option<usize>,
}

impl PageSink for QueueSink {
    fn take(
        &mut self,
        downloader: &Downloader,
        seeds: Vec<NewFileSeed>,
    ) -> Result<PageTaken, WorkError> {
        let known = KnownSeeds::load(downloader, self.queue)?;
        let classes = &downloader.store.snapshot().url_classes;
        let mut taken = PageTaken {
            can_search_for_more_files: true,
            ..PageTaken::default()
        };
        let mut fresh = Vec::new();
        for seed in seeds {
            if self.max_new_urls.is_some_and(|max| taken.added >= max) {
                taken.can_search_for_more_files = false;
                taken.stop_reason = "hit file limit".into();
                break;
            }
            if known.has(classes, &seed) {
                taken.already_in += 1;
            } else {
                taken.added += 1;
                fresh.push(seed);
            }
        }
        let queue = self.queue;
        downloader
            .store
            .write(move |ctx| queues::add_file_seeds(ctx.conn(), queue, &fresh, false, now()))?;
        Ok(taken)
    }
}

/// A queue's file seeds, to check new ones against (`HasFileSeed`).
pub(crate) struct KnownSeeds(BTreeSet<(i64, String)>);

impl KnownSeeds {
    pub fn load(downloader: &Downloader, queue: i64) -> Result<Self, WorkError> {
        Ok(Self(
            downloader
                .store
                .read(|conn| queues::file_seeds(conn, queue))?
                .into_iter()
                .map(|s| (s.seed_type as i64, s.data_for_comparison))
                .collect(),
        ))
    }

    /// Whether the queue has this seed, under any form its URL may have
    /// been stored under.
    pub fn has(&self, classes: &UrlClasses, seed: &NewFileSeed) -> bool {
        let kind = seed.seed_type as i64;
        if seed.seed_type == queues::SeedType::Path {
            return self.0.contains(&(kind, seed.data_for_comparison.clone()));
        }
        classes
            .search_urls(&seed.data_for_comparison)
            .into_iter()
            .map(|url| classes.normalise(&url, false).unwrap_or(url))
            .any(|key| self.0.contains(&(kind, key)))
    }
}

/// `CanOnlyGenerateGalleryURLs`: the parser's URLs are all gallery URLs.
fn only_generates_gallery_urls(parser: &PageParser) -> bool {
    fn url_types(parser: &PageParser, out: &mut Vec<i64>) {
        for c in &parser.content_parsers {
            if let ContentKind::Url { url_type, .. } = c.kind {
                out.push(url_type);
            }
        }
        for s in &parser.subsidiary {
            url_types(&s.parser, out);
        }
    }
    let mut types = Vec::new();
    url_types(parser, &mut types);
    types.contains(&URL_TYPE_GALLERY) && types.iter().all(|t| *t == URL_TYPE_GALLERY)
}

/// `ConvertParsedPostsToGallerySeeds`.
fn gallery_seeds_from_posts(
    classes: &UrlClasses,
    posts: &[ParsedPost],
    url_type: i64,
    can_generate_more_pages: bool,
    add_file_metadata: bool,
) -> Vec<NewGallerySeed> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for post in posts {
        for url in post.urls(&[url_type], true) {
            if !seen.insert(url.clone()) {
                continue;
            }
            let mut meta = GallerySeedMeta::default();
            meta.request_headers.extend(post.http_headers());
            if add_file_metadata {
                meta.external_filterable_tags.extend(post.tags());
            }
            out.push(NewGallerySeed {
                url: classes.normalise(&url, true).unwrap_or(url),
                can_generate_more_pages,
                referral_url: None,
                meta,
            });
        }
    }
    out
}

/// `_GiveChildFileSeedMyInfo`.
fn give_file_seed_my_info(
    classes: &UrlClasses,
    parent: &GallerySeed,
    child: &mut NewFileSeed,
    url_for_child_referral: &str,
) {
    if child.referral_url.is_none() {
        child.referral_url = Some(url_for_child_referral.to_owned());
    }
    let mut c = crate::seeds::SeedMut {
        data: &child.data,
        comparison: &child.data_for_comparison,
        referral: child.referral_url.as_deref(),
        source_time: &mut child.source_time,
        meta: &mut child.meta,
    };
    c.add_primary_urls(classes, [parent.url.clone()]);
    child.meta.add_request_headers(&parent.meta.request_headers);
    child
        .meta
        .external_filterable_tags
        .extend(parent.meta.external_filterable_tags.iter().cloned());
    for (service, tags) in &parent.meta.external_additional_tags {
        match child
            .meta
            .external_additional_tags
            .iter_mut()
            .find(|(s, _)| s == service)
        {
            Some((_, existing)) => existing.extend(tags.iter().cloned()),
            None => child
                .meta
                .external_additional_tags
                .push((service.clone(), tags.clone())),
        }
    }
}

/// `_GiveChildGallerySeedMyInfo`.
fn give_gallery_seed_my_info(
    parent: &GallerySeed,
    child: &mut NewGallerySeed,
    url_for_child_referral: &str,
) {
    child.meta.run_token.clone_from(&parent.meta.run_token);
    child.referral_url = Some(url_for_child_referral.to_owned());
    child
        .meta
        .external_filterable_tags
        .extend(parent.meta.external_filterable_tags.iter().cloned());
    for (service, tags) in &parent.meta.external_additional_tags {
        match child
            .meta
            .external_additional_tags
            .iter_mut()
            .find(|(s, _)| s == service)
        {
            Some((_, existing)) => existing.extend(tags.iter().cloned()),
            None => child
                .meta
                .external_additional_tags
                .push((service.clone(), tags.clone())),
        }
    }
}

enum GalleryStop {
    Veto(String, bool),
    Failed(WorkError),
}

impl<E: Into<WorkError>> From<E> for GalleryStop {
    fn from(e: E) -> Self {
        GalleryStop::Failed(e.into())
    }
}

fn gallery_network(e: NetError) -> GalleryStop {
    let gone = |note: &str| GalleryStop::Veto(note.to_owned(), true);
    match &e {
        NetError::Status { kind, .. } => match kind {
            StatusKind::NotFound => gone("404"),
            StatusKind::InsufficientCredentials => gone("403"),
            StatusKind::BadRequest => gone("400"),
            StatusKind::Censorship => GalleryStop::Veto(
                "site reports http status code 451: Unavailable For Legal Reasons".into(),
                false,
            ),
            _ => GalleryStop::Failed(WorkError::Network(e)),
        },
        NetError::Cancelled => GalleryStop::Veto("Cancelled!".into(), false),
        _ => GalleryStop::Failed(WorkError::Network(e)),
    }
}

impl Downloader {
    /// Work on a gallery page. Updates `seed` (the caller saves it). A
    /// network failure the queue should wait out comes back as
    /// [`WorkError::Network`], with the seed marked as an error.
    pub(crate) async fn work_on_gallery_url(
        &self,
        seed: &mut GallerySeed,
        seen: &mut BTreeSet<String>,
        sink: &mut dyn PageSink,
        job: &Job,
    ) -> Result<GalleryOutcome, WorkError> {
        seen.insert(seed.url.clone());
        let mut outcome = GalleryOutcome {
            can_search_for_more_files: true,
            ..GalleryOutcome::default()
        };
        match self.gallery(seed, seen, sink, job, &mut outcome).await {
            Ok((status, note)) => set_gallery_status(seed, status, note),
            Err(GalleryStop::Veto(note, gone)) => {
                outcome.result_404 = gone;
                set_gallery_status(seed, SeedStatus::Vetoed, note);
            }
            Err(GalleryStop::Failed(WorkError::Network(e))) => {
                set_gallery_status(seed, SeedStatus::Error, e.to_string());
                return Err(WorkError::Network(e));
            }
            Err(GalleryStop::Failed(e)) => {
                set_gallery_status(seed, SeedStatus::Error, e.to_string());
            }
        }
        Ok(outcome)
    }

    async fn gallery(
        &self,
        seed: &GallerySeed,
        seen: &mut BTreeSet<String>,
        sink: &mut dyn PageSink,
        job: &Job,
        outcome: &mut GalleryOutcome,
    ) -> Result<(SeedStatus, String), GalleryStop> {
        let snapshot = self.store.snapshot();
        let classes = &snapshot.url_classes;
        let mut gallery_url = seed.url.clone();
        let capability = classes.parse_capability(&gallery_url);
        if !matches!(capability.url_type, UrlType::Gallery | UrlType::Watchable) {
            return Err(GalleryStop::Veto(
                "Did not recognise this as a gallery or watchable URL!".into(),
                false,
            ));
        }
        if let Err(reason) = &capability.parser {
            return Err(GalleryStop::Veto(
                format!("Cannot parse {}: {reason}", capability.match_name),
                false,
            ));
        }
        let (mut url_to_fetch, mut parser_key) = classes
            .url_to_fetch_and_parser(&gallery_url)
            .map_err(|e| GalleryStop::Veto(e.to_string(), false))?;
        seen.insert(url_to_fetch.clone());
        job.set_status_text("downloading gallery page");
        let mut request = Request::get(url_to_fetch.clone());
        // gallery pages wait their turn per site, and for bandwidth at most
        // half a minute
        request.gallery_page = true;
        request.override_bandwidth_after = Some(30);
        request.referral_url.clone_from(&seed.referral_url);
        request
            .additional_headers
            .clone_from(&seed.meta.request_headers);
        let response = self
            .net
            .fetch(&request, job)
            .await
            .map_err(gallery_network)?;
        let text = response.text();
        let actual = response.url.clone();
        let url_for_child_referral = actual.clone();
        if actual != url_to_fetch {
            let capability = classes.parse_capability(&actual);
            if capability.url_type == UrlType::Gallery {
                if let Err(reason) = &capability.parser {
                    return Ok((
                        SeedStatus::Error,
                        format!("Could not parse {}: {reason}", capability.match_name),
                    ));
                }
                gallery_url = actual;
                (url_to_fetch, parser_key) = classes
                    .url_to_fetch_and_parser(&gallery_url)
                    .map_err(|e| GalleryStop::Veto(e.to_string(), false))?;
            } else {
                let mut child = new_url_seed(classes, &actual);
                give_file_seed_my_info(classes, seed, &mut child, &url_for_child_referral);
                self.take_page(sink, vec![child], outcome)?;
                return Ok((
                    SeedStatus::SuccessfulAndNew,
                    "was redirected to a non-gallery url, which has been queued as a file import"
                        .into(),
                ));
            }
        }
        let Some(parser) = self.parser(&parser_key) else {
            return Err(GalleryStop::Veto(
                format!("The parser for {gallery_url} is missing!"),
                false,
            ));
        };
        let mut context = ParsingContext::new();
        context.insert("gallery_url".into(), gallery_url);
        context.insert("url".into(), url_to_fetch.clone());
        context.insert("post_index".into(), "0".into());
        let posts = match parser.parse(&mut context, &text) {
            Ok(posts) => posts,
            Err(ParseFailure::Veto(reason)) => {
                return Err(GalleryStop::Veto(format!("veto: {reason}"), false));
            }
            Err(ParseFailure::Error(e)) => {
                return Err(GalleryStop::Failed(WorkError::Parse(e.to_string())));
            }
        };
        if posts.is_empty() {
            return Err(GalleryStop::Veto(
                "The parser found nothing in the document!".into(),
                false,
            ));
        }
        outcome.title = hydrus_parse::content::title(&posts);
        let mut file_seeds = seeds_from_posts(classes, &posts, &url_for_child_referral);
        for child in &mut file_seeds {
            give_file_seed_my_info(classes, seed, child, &url_for_child_referral);
        }
        outcome.num_urls_total = file_seeds.len();
        self.take_page(sink, file_seeds, outcome)?;
        let mut note = format!(
            "{} new urls found",
            human_int(outcome.num_urls_added as u64)
        );
        if outcome.num_urls_already_in_queue > 0 {
            note += &format!(
                " ({} of page already in)",
                human_int(outcome.num_urls_already_in_queue as u64)
            );
        }
        if !outcome.can_search_for_more_files {
            note += &format!(" - {}", outcome.stop_reason);
        }

        let sub_galleries = gallery_seeds_from_posts(
            classes,
            &posts,
            URL_TYPE_SUB_GALLERY,
            seed.can_generate_more_pages,
            true,
        );
        let mut new_subs: Vec<NewGallerySeed> = sub_galleries
            .iter()
            .filter(|s| !seen.contains(&s.url))
            .cloned()
            .collect();
        if !new_subs.is_empty() {
            for sub in &mut new_subs {
                give_gallery_seed_my_info(seed, sub, &url_for_child_referral);
                seen.insert(sub.url.clone());
            }
            let n = new_subs.len();
            let queue = seed.queue_id;
            let parent = seed.clone();
            self.store.write(move |ctx| {
                queues::add_gallery_seeds(ctx.conn(), queue, &new_subs, Some(&parent), now())
            })?;
            outcome.added_new_gallery_pages = true;
            note += &format!(" - {} sub-gallery urls found", human_int(n as u64));
            let dupes = sub_galleries.len() - n;
            if dupes > 0 {
                note += &format!(
                    ", but {} had already been visited this run and were not added",
                    human_int(dupes as u64)
                );
            }
        } else if !sub_galleries.is_empty() {
            note += &format!(
                " - {} sub-gallery urls found, but they had all already been visited this run and were not added",
                human_int(sub_galleries.len() as u64)
            );
        }

        let can_add_more_gallery_urls =
            if only_generates_gallery_urls(&parser) || seed.meta.force_next_page_url_generation {
                true
            } else {
                outcome.num_urls_added > 0 && outcome.can_search_for_more_files
            };
        if seed.can_generate_more_pages && can_add_more_gallery_urls {
            let next_pages = gallery_seeds_from_posts(
                classes,
                &posts,
                URL_TYPE_NEXT,
                seed.can_generate_more_pages,
                false,
            );
            let mut new_next: Vec<NewGallerySeed> = next_pages
                .iter()
                .filter(|s| !seen.contains(&s.url))
                .cloned()
                .collect();
            if new_next.is_empty() {
                if !next_pages.is_empty() {
                    note += &format!(
                        " - {} next gallery pages found, but they had all already been visited this run and were not added",
                        human_int(next_pages.len() as u64)
                    );
                }
                if let Some(next) = classes.next_gallery_page(&url_to_fetch) {
                    match next {
                        Ok(next_url) => {
                            note += " - next gallery page extrapolated from url class";
                            if seen.contains(&next_url) {
                                note +=
                                    ", but it had already been visited this run and was not added";
                            } else {
                                new_next.push(NewGallerySeed {
                                    url: classes.normalise(&next_url, true).unwrap_or(next_url),
                                    can_generate_more_pages: seed.can_generate_more_pages,
                                    referral_url: None,
                                    meta: GallerySeedMeta::default(),
                                });
                            }
                        }
                        Err(e) => {
                            note += &format!(
                                " - Attempted to generate a next gallery page url, but failed!\n{e}"
                            );
                        }
                    }
                }
            } else {
                note += &format!(
                    " - {} next gallery pages found",
                    human_int(new_next.len() as u64)
                );
                let dupes = next_pages.len() - new_next.len();
                if dupes > 0 {
                    note += &format!(
                        ", but {} had already been visited this run and were not added",
                        human_int(dupes as u64)
                    );
                }
            }
            if !new_next.is_empty() {
                for next in &mut new_next {
                    give_gallery_seed_my_info(seed, next, &url_for_child_referral);
                    seen.insert(next.url.clone());
                }
                let queue = seed.queue_id;
                let parent = seed.clone();
                self.store.write(move |ctx| {
                    queues::add_gallery_seeds(ctx.conn(), queue, &new_next, Some(&parent), now())
                })?;
                outcome.added_new_gallery_pages = true;
            }
        }
        Ok((SeedStatus::SuccessfulAndNew, note))
    }

    fn take_page(
        &self,
        sink: &mut dyn PageSink,
        seeds: Vec<NewFileSeed>,
        outcome: &mut GalleryOutcome,
    ) -> Result<(), WorkError> {
        let taken = sink.take(self, seeds)?;
        outcome.num_urls_added = taken.added;
        outcome.num_urls_already_in_queue = taken.already_in;
        outcome.can_search_for_more_files = taken.can_search_for_more_files;
        outcome.stop_reason = taken.stop_reason;
        Ok(())
    }
}

pub(crate) fn set_gallery_status(seed: &mut GallerySeed, status: SeedStatus, note: String) {
    seed.status = status;
    seed.note = note;
    seed.modified = now();
}
