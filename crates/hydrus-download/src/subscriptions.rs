//! Subscriptions: each query's gallery search checked on its schedule for
//! files it hasn't seen, which are then downloaded
//! (`ClientImportSubscriptions.Subscription.Sync`).
//!
//! A sync reads a query's gallery pages from the first, gathering file URLs
//! until it is caught up with what the query found before (a run of known
//! URLs, or most of its history seen again) or hits its file limit. The new
//! URLs join the query's history oldest first, the history is compacted,
//! and the next check is timed from how often files have been appearing.
//! Then each query's new files are downloaded with the subscription's
//! import options, and the query's own tags are added to them.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::time::Duration;

use rand::seq::{IndexedRandom as _, SliceRandom as _};

use hydrus_core::Sha256;
use hydrus_core::bandwidth::GalleryTokenKind;
use hydrus_core::import_options::CallerType;
use hydrus_core::network::NetworkContext;
use hydrus_core::numbers::{human_int, value_range};
use hydrus_core::subscriptions::{
    FileLogEntry, SeedTime, SubscriptionSettings, compact_file_log, compact_gallery_log,
    num_master_file_seeds,
};
use hydrus_core::url::{AnyGug, GugOptions, UrlClasses};
use hydrus_net::{BandwidthScope, Job, NetError};
use hydrus_store::queues::{
    self, FileSeed, GallerySeedMeta, NewFileSeed, NewGallerySeed, SeedStatus,
};
use hydrus_store::settings::Pauses;
use hydrus_store::subscriptions::{self as store_subs, Subscription, SubscriptionQuery};

use crate::gallery::{KnownSeeds, PageSink, PageTaken, set_gallery_status};
use crate::{Downloader, WorkError, now, popups};

/// `WE_HIT_OLD_GROUND_THRESHOLD`.
const CAUGHT_UP_RUN: u64 = 5;

/// What a subscription run did, and what a person should hear about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunReport {
    pub new_urls: u64,
    pub files_worked: u64,
    /// Non-DataMissing errors across this sync, reset for the next run.
    pub file_errors: u64,
    /// Things the reference would pop up: a paused subscription, a query that
    /// died or found nothing, a gap after hitting the periodic file limit.
    pub notices: Vec<String>,
}

/// Why a run stopped early.
enum RunStop {
    /// Stop quietly (the subscription paused or delayed itself, or was
    /// cancelled).
    Stop,
    Network(NetError),
    Failed(String),
}

impl From<WorkError> for RunStop {
    fn from(e: WorkError) -> Self {
        match e {
            WorkError::Network(e) => RunStop::Network(e),
            other => RunStop::Failed(other.to_string()),
        }
    }
}

impl From<hydrus_store::StoreError> for RunStop {
    fn from(e: hydrus_store::StoreError) -> Self {
        RunStop::Failed(e.to_string())
    }
}

/// A sync's rules for what a gallery page's URLs mean
/// (`_SyncQuery`'s `file_seeds_callable`).
struct SyncSink<'a> {
    classes: &'a UrlClasses,
    known: KnownSeeds,
    /// The first URL of the history, to tell a changed URL format from a gap.
    first_url: Option<String>,
    initial: bool,
    file_limit: Option<u64>,
    periodic_file_limit: Option<u64>,
    random_sample: bool,
    master_at_start: usize,
    compaction_number: u64,
    total_new: u64,
    total_already_in: u64,
    /// New this sync, newest first.
    new: Vec<NewFileSeed>,
    new_keys: BTreeSet<(i64, String)>,
    names: (String, String),
    notices: Vec<String>,
}

impl PageSink for SyncSink<'_> {
    fn take(&mut self, _: &Downloader, page: Vec<NewFileSeed>) -> Result<PageTaken, WorkError> {
        // expecting a lot this time: keep more history
        let largest = (page.len() as u64).max(self.periodic_file_limit.unwrap_or(0));
        if largest > self.compaction_number {
            self.compaction_number = largest * 2;
        }
        let mut added = 0u64;
        let mut already_in = 0u64;
        let mut can_search_for_more_files = true;
        let mut stop_reason = "unknown stop reason".to_owned();
        let mut contiguous_known = 0u64;
        for seed in page {
            let key = (seed.seed_type as i64, seed.data_for_comparison.clone());
            if self.new_keys.contains(&key) {
                // uploaded while we were reading: already counted
                continue;
            }
            if self.known.has(self.classes, &seed) {
                already_in += 1;
                contiguous_known += 1;
                if contiguous_known >= 100 {
                    can_search_for_more_files = false;
                    stop_reason =
                        "saw 100 previously seen urls in a row, so assuming this is a large gallery"
                            .into();
                    break;
                }
            } else {
                added += 1;
                contiguous_known = 0;
                self.new_keys.insert(key);
                self.new.push(seed);
            }
            if let Some(limit) = self.file_limit
                && self.total_new + added >= limit
            {
                stop_reason = self.limit_reason(already_in, limit);
                can_search_for_more_files = false;
                break;
            }
            if !self.initial && already_in > 0 {
                // a history smaller than a gallery page: once we've seen
                // most of it again, we're caught up
                let seen_so_far = self.total_already_in + already_in;
                if seen_so_far as f64 >= self.master_at_start as f64 * 0.95 {
                    stop_reason = format!(
                        "saw {} already-seen files, which is so much of what I already knew about that I am assuming I caught up",
                        human_int(seen_so_far)
                    );
                    can_search_for_more_files = false;
                    break;
                }
            }
        }
        if can_search_for_more_files {
            if contiguous_known >= CAUGHT_UP_RUN {
                can_search_for_more_files = false;
                stop_reason = format!(
                    "saw {} contiguous previously seen urls at end of page, so assuming we caught up",
                    human_int(contiguous_known)
                );
            }
            if added == 0 {
                can_search_for_more_files = false;
                stop_reason = "no new urls found".into();
            }
        }
        self.total_new += added;
        self.total_already_in += already_in;
        Ok(PageTaken {
            added: added as usize,
            already_in: already_in as usize,
            can_search_for_more_files,
            stop_reason,
        })
    }
}

impl SyncSink<'_> {
    fn limit_reason(&mut self, already_in_this_page: u64, limit: u64) -> String {
        if self.initial {
            return "hit initial file limit".into();
        }
        if self.total_already_in + already_in_this_page > 0 {
            // a mix of old files and new ones tagged late: nothing to report
            return "hit periodic file limit after seeing several already-seen files".into();
        }
        if self.random_sample {
            return "hit periodic file limit".into();
        }
        let class_key = |url: &str| self.classes.class_for(url).map(|c| &c.key);
        let format_changed = self.first_url.as_deref().is_some_and(|first| {
            let old = class_key(first);
            !self.new.iter().any(|s| class_key(&s.data) == old)
        });
        let (sub, query) = &self.names;
        if format_changed {
            tracing::info!(
                "The query \"{query}\" for subscription \"{sub}\" found {limit} new URLs without running into any it had seen before, but its URL format appears to have changed"
            );
            return "hit periodic file limit after url class appeared to change. sub may spend some extra time catching up".into();
        }
        self.notices.push(format!(
            "The query \"{query}\" for subscription \"{sub}\" found {limit} new URLs without running into any it had seen before.\n\nIt is likely that a user uploaded a lot of files to that query in a short period, in which case there is now a gap in your subscription that you may wish to fill."
        ));
        "hit periodic file limit without seeing any already-seen files!".into()
    }
}

fn history_entries(seeds: &[FileSeed]) -> Vec<FileLogEntry<'_>> {
    seeds
        .iter()
        .map(|s| FileLogEntry {
            unknown: s.status == SeedStatus::Unknown,
            child_files_note: (s.status == SeedStatus::SuccessfulAndChildFiles)
                .then_some(s.note.as_str()),
            time: SeedTime {
                source_time: s.source_time,
                created: s.created,
            },
        })
        .collect()
}

fn human_name(sub: &Subscription, query: &SubscriptionQuery) -> String {
    let name = query.state.human_name();
    if name == sub.name {
        name.to_owned()
    } else {
        format!("{}: {name}", sub.name)
    }
}

/// What the files a query presents are published as (`_GetPublishingLabel`):
/// the subscription's name, or the label it is given instead, and the
/// query's, unless its queries publish together.
fn publishing_label(name: &str, settings: &SubscriptionSettings, query: &str) -> String {
    let label = settings.publish_label_override.as_deref().unwrap_or(name);
    if settings.merge_query_publish_events {
        label.to_owned()
    } else {
        format!("{label}: {query}")
    }
}

/// What a subscription query's requests count against (the reference's
/// `NetworkJobSubscription`, keyed by subscription and query): they wait at
/// most half a minute for bandwidth, the subscription itself having asked
/// first; gallery pages take subscription turns per site.
fn query_scope(sub: &Subscription, query: &SubscriptionQuery) -> BandwidthScope {
    BandwidthScope {
        contexts: vec![NetworkContext::subscription(
            &sub.name,
            query.state.human_name(),
        )],
        override_after: Some(30),
        gallery_token: Some(GalleryTokenKind::Subscription),
    }
}

/// The contexts a query's file downloads count against, by its next file
/// (`_GetExampleNetworkContexts`).
fn query_contexts(
    sub: &Subscription,
    query: &SubscriptionQuery,
    example_url: Option<&str>,
) -> Vec<NetworkContext> {
    let mut contexts = example_url.map_or_else(
        || vec![NetworkContext::global()],
        hydrus_net::NetEngine::contexts_for,
    );
    contexts.extend(query_scope(sub, query).contexts);
    contexts
}

/// How far ahead a subscription asks for bandwidth before working on a
/// query's files (`SUBSCRIPTION_BANDWIDTH_OK_WINDOW`): rules over this many
/// seconds or less don't stop it.
const BANDWIDTH_OK_WINDOW: u64 = 90;

impl Downloader {
    /// Whether the query's next file has bandwidth (`FileBandwidthOK`).
    fn file_bandwidth_ok(
        &self,
        sub: &Subscription,
        query: &SubscriptionQuery,
        example_url: Option<&str>,
    ) -> bool {
        self.net.can_do_work(
            &query_contexts(sub, query, example_url),
            BANDWIDTH_OK_WINDOW,
        )
    }

    /// Whether subscriptions are paused globally (`pause_subs_sync`, or all
    /// new network traffic).
    fn subscriptions_paused(&self) -> bool {
        self.store
            .read(hydrus_store::settings::get::<Pauses>)
            .is_ok_and(|p| !p.subscriptions_run())
    }

    fn has_file_work(&self, queue: i64) -> Result<bool, WorkError> {
        Ok(self
            .store
            .read(|conn| queues::file_seed_counts(conn, queue))?
            .get(&SeedStatus::Unknown)
            .is_some_and(|n| *n > 0))
    }

    /// When a subscription next wants to work (`GetBestEarliestNextWorkTime`,
    /// ignoring bandwidth): `None` if never.
    pub fn next_work_time(&self, sub: &Subscription) -> Result<Option<i64>, WorkError> {
        if sub.settings.paused {
            return Ok(None);
        }
        let queries = self.store.read(|conn| store_subs::queries(conn, sub.id))?;
        let mut earliest: Option<i64> = None;
        for q in &queries {
            if q.state.paused {
                continue;
            }
            let file_work = self.has_file_work(q.queue_id)?;
            if q.state.dead && !file_work {
                continue;
            }
            let mut time = if q.state.dead {
                i64::MAX
            } else if q.state.check_now {
                0
            } else {
                q.state.next_check_time
            };
            if file_work {
                // when its next file has bandwidth (at least a minute on,
                // when a rule is all but used up but not over)
                let next = self
                    .store
                    .read(|conn| queues::next_file_seed(conn, q.queue_id))?;
                let example = next.as_ref().map(|s| s.data.as_str());
                time = if self.file_bandwidth_ok(sub, q, example) {
                    0
                } else {
                    let contexts = query_contexts(sub, q, example);
                    let wait = self.net.waiting_estimate(&contexts).max(60);
                    now().saturating_add(i64::try_from(wait).unwrap_or(i64::MAX))
                };
            }
            earliest = Some(earliest.map_or(time, |e| e.min(time)));
        }
        Ok(earliest.map(|t| t.max(sub.settings.no_work_until)))
    }

    /// Run a subscription once (`Subscription.Sync`): sync every query that
    /// is due, then download what the queries have found. Its settings and
    /// queries are saved as it goes.
    pub async fn run_subscription(
        &self,
        id: i64,
        job: &std::sync::Arc<Job>,
    ) -> Result<RunReport, WorkError> {
        let mut report = RunReport::default();
        let Some(mut sub) = self.store.read(|conn| store_subs::subscription(conn, id))? else {
            return Ok(report);
        };
        if sub.settings.paused || now() < sub.settings.no_work_until || self.subscriptions_paused()
        {
            return Ok(report);
        }
        let started_with = sub.settings.clone();
        // what it shows while it works (`Sync`'s job status), with the
        // download it is doing
        let popup =
            popups::Working::new(self.store(), format!("subscriptions - {}", sub.name), true);
        popup.set_network_job(Some(std::sync::Arc::clone(job)));
        popup.keep_up();
        let result = async {
            if self.due_queries(&sub)?.is_empty() && !self.any_file_work(&sub)? {
                return Ok(());
            }
            if sub.settings.show_a_popup_while_working {
                popup.show();
            }
            loop {
                let due = self.due_queries(&sub)?;
                if due.is_empty()
                    || sub.settings.paused
                    || now() < sub.settings.no_work_until
                    || self.subscriptions_paused()
                {
                    break;
                }
                self.sync_queries(&mut sub, due, job, &mut report, &popup)
                    .await?;
            }
            self.full_options(CallerType::Subscription, &sub.settings.import_options, &[])?
                .locations
                .check_ready_to_import()
                .map_err(|e| RunStop::Failed(e.into()))?;
            self.work_on_queries_files(&mut sub, job, &mut report, &popup)
                .await
        }
        .await;
        match result {
            Ok(()) | Err(RunStop::Stop) => {}
            Err(RunStop::Network(e)) => {
                popup.set_text(Some(
                    "Encountered a network error, will retry again later".into(),
                ));
                delay(
                    &mut sub,
                    self.network.read().subscription_network_error_delay,
                    &format!("network error: {e}"),
                );
            }
            Err(RunStop::Failed(e)) => {
                report.notices.push(format!(
                    "The subscription \"{}\" encountered an error when trying to sync: {e}",
                    sub.name
                ));
                delay(
                    &mut sub,
                    self.network.read().subscription_other_error_delay,
                    &format!("error: {e}"),
                );
            }
        }
        // save what the run changed, keeping changes made meanwhile (say, a
        // pause from the command line)
        let (sub_id, before, after) = (sub.id, started_with, sub.settings.clone());
        self.store.write(move |ctx| {
            let Some(mut current) = store_subs::subscription(ctx.conn(), sub_id)? else {
                return Ok(());
            };
            let s = &mut current.settings;
            if after.paused != before.paused {
                s.paused = after.paused;
            }
            if after.no_work_until != before.no_work_until {
                s.no_work_until = after.no_work_until;
                s.no_work_until_reason = after.no_work_until_reason;
            }
            if (&after.gug_key, &after.gug_name) != (&before.gug_key, &before.gug_name) {
                s.gug_key = after.gug_key;
                s.gug_name = after.gug_name;
            }
            store_subs::set_subscription_settings(ctx.conn(), sub_id, s)
        })?;
        // (a popup with files stays for them)
        popup.set_network_job(None);
        if popup.has_files() {
            popup.finish();
        } else {
            popup.finish_and_dismiss();
        }
        for notice in &report.notices {
            tracing::warn!("{notice}");
            popups::show_text(self.store(), notice.clone());
        }
        Ok(report)
    }

    /// Queries to sync now (`_GetQueryHeadersForProcessing`, `IsSyncDue`).
    fn due_queries(&self, sub: &Subscription) -> Result<Vec<SubscriptionQuery>, RunStop> {
        if sub.settings.gug_name.is_empty() {
            return Ok(Vec::new());
        }
        let t = now();
        let mut queries: Vec<SubscriptionQuery> = self
            .store
            .read(|conn| store_subs::queries(conn, sub.id))?
            .into_iter()
            .filter(|q| !q.state.paused && q.state.is_sync_due(t))
            .collect();
        self.order_queries(&mut queries);
        Ok(queries)
    }

    async fn sync_queries(
        &self,
        sub: &mut Subscription,
        queries: Vec<SubscriptionQuery>,
        job: &Job,
        report: &mut RunReport,
        popup: &popups::Working,
    ) -> Result<(), RunStop> {
        let definitions = self.definitions();
        let Some(gug) = definitions
            .gugs
            .get(&sub.settings.gug_key, &sub.settings.gug_name)
            .cloned()
        else {
            sub.settings.paused = true;
            report.notices.push(format!(
                "The subscription \"{}\" could not find a Gallery URL Generator for \"{}\"! The sub has paused!",
                sub.name, sub.settings.gug_name
            ));
            return Err(RunStop::Stop);
        };
        if let Err(reason) = self.gug_functional(&definitions.gugs, &gug) {
            sub.settings.paused = true;
            report.notices.push(format!(
                "The subscription \"{}\"'s Gallery URL Generator, \"{}\" seems not to be functional! The sub has paused! The given reason was:\n\n{reason}",
                sub.name, sub.settings.gug_name
            ));
            return Err(RunStop::Stop);
        }
        // keep up with a renamed or re-keyed GUG
        gug.key().clone_into(&mut sub.settings.gug_key);
        gug.name().clone_into(&mut sub.settings.gug_name);
        let count = queries.len();
        for (i, mut query) in queries.into_iter().enumerate() {
            let mut prefix = format!("synchronising ({})", value_range(i as u64, count as u64));
            let name = query.state.human_name();
            if name != sub.name {
                prefix.push_str(&format!(" \"{name}\""));
            }
            popup.set_gauge(Some((i as i64, count as i64)));
            let result = self
                .sync_query(
                    sub,
                    &definitions.gugs,
                    &gug,
                    &mut query,
                    job,
                    report,
                    (popup, &prefix),
                )
                .await;
            let (queue, state) = (query.queue_id, query.state.clone());
            self.store
                .write(move |ctx| store_subs::set_query_state(ctx.conn(), queue, &state))?;
            result?;
        }
        Ok(())
    }

    /// `CheckFunctional`: each of the GUG's example URLs has a URL class
    /// with a parser.
    fn gug_functional(&self, gugs: &hydrus_core::url::Gugs, gug: &AnyGug) -> Result<(), String> {
        let snapshot = self.store.snapshot();
        let classes = &snapshot.url_classes;
        let options = gug_options(classes, self.network.read().gug_percent_twenty_is_space);
        let examples: Vec<String> = match gug {
            AnyGug::Single(g) => vec![
                g.example_url(options)
                    .map_err(|e| format!("Unusual error: {e}"))?,
            ],
            AnyGug::Nested(n) => n
                .gugs
                .iter()
                .filter_map(|(key, name)| match gugs.get(key, name) {
                    Some(AnyGug::Single(g)) => Some(g.example_url(options)),
                    _ => None,
                })
                .collect::<Result<_, _>>()
                .map_err(|e| format!("Unusual error: {e}"))?,
        };
        for url in examples {
            let capability = classes.parse_capability(&url);
            if capability.url_type == hydrus_core::url::UrlType::Unknown {
                return Err("No URL Class for example URL!".into());
            }
            if let Err(reason) = capability.parser {
                return Err(format!("Cannot parse {}: {reason}", capability.match_name));
            }
        }
        Ok(())
    }

    /// `_SyncQuery`, saying what it does in the popup after the prefix.
    #[allow(clippy::too_many_arguments)]
    async fn sync_query(
        &self,
        sub: &mut Subscription,
        gugs: &hydrus_core::url::Gugs,
        gug: &AnyGug,
        query: &mut SubscriptionQuery,
        job: &Job,
        report: &mut RunReport,
        (popup, prefix): (&popups::Working, &str),
    ) -> Result<(), RunStop> {
        if query.state.paused {
            return Ok(());
        }
        job.set_scope(query_scope(sub, query));
        let queue = query.queue_id;
        let snapshot = self.store.snapshot();
        let classes = &snapshot.url_classes;
        let history = self.store.read(|conn| queues::file_seeds(conn, queue))?;
        let initial = query.state.is_initial_sync();
        let settings = &sub.settings;
        let names = (sub.name.clone(), query.state.human_name().to_owned());
        let mut sink = SyncSink {
            classes,
            known: KnownSeeds::load(self, queue)?,
            first_url: history.first().map(|s| s.data.clone()),
            initial,
            file_limit: if initial {
                settings.initial_file_limit
            } else {
                settings.periodic_file_limit
            },
            periodic_file_limit: settings.periodic_file_limit,
            random_sample: settings.this_is_a_random_sample,
            master_at_start: num_master_file_seeds(&history_entries(&history)),
            compaction_number: query.state.file_seed_compaction_number,
            total_new: 0,
            total_already_in: 0,
            new: Vec::new(),
            new_keys: BTreeSet::new(),
            names: names.clone(),
            notices: Vec::new(),
        };
        drop(history);

        popup.set_text(Some(prefix.to_owned()));
        let urls = gugs
            .gallery_urls(
                gug,
                &query.state.query_text,
                gug_options(classes, self.network.read().gug_percent_twenty_is_space),
            )
            .map_err(|e| RunStop::Failed(e.to_string()))?;
        if urls.is_empty() {
            sub.settings.paused = true;
            report.notices.push(format!(
                "The subscription \"{}\"'s Gallery URL Generator, \"{}\" did not generate any URLs! The sub has paused!",
                sub.name, sub.settings.gug_name
            ));
            return Err(RunStop::Stop);
        }
        let run_token = hex::encode(rand::random::<[u8; 32]>());
        let first_pages: Vec<NewGallerySeed> = urls
            .into_iter()
            .map(|url| NewGallerySeed {
                url: classes.normalise(&url, true).unwrap_or(url),
                can_generate_more_pages: true,
                referral_url: None,
                meta: GallerySeedMeta {
                    run_token: run_token.clone(),
                    ..GallerySeedMeta::default()
                },
            })
            .collect();
        self.store.write(move |ctx| {
            queues::add_gallery_seeds(ctx.conn(), queue, &first_pages, None, now())
        })?;

        let mut seen = BTreeSet::new();
        let mut stopped_classes: HashMap<Vec<u8>, String> = HashMap::new();
        let mut stop_reason = "unknown stop reason".to_owned();
        let outcome: Result<(), RunStop> = async {
            loop {
                if job.is_cancelled() || popup.is_cancelled() {
                    stop_reason = "gallery parsing cancelled, likely by user".into();
                    delay(sub, 600, &stop_reason);
                    return Err(RunStop::Stop);
                }
                let Some(mut seed) = self
                    .store
                    .read(|conn| queues::next_gallery_seed(conn, queue))?
                else {
                    break;
                };
                let class = classes
                    .class_for(&seed.url)
                    .map(|c| (c.key.clone(), c.name.clone()));
                if let Some(reason) = class.as_ref().and_then(|(key, _)| stopped_classes.get(key)) {
                    set_gallery_status(&mut seed, SeedStatus::Skipped, reason.clone());
                    self.store
                        .write(move |ctx| queues::update_gallery_seed(ctx.conn(), &seed))?;
                    continue;
                }
                let checking = format!(
                    "found {} new urls, checking next page",
                    human_int(sink.total_new)
                );
                popup.set_text(Some(format!("{prefix}: {checking}")));
                popup.follow_stage(1, format!("{prefix}: "));
                job.set_status_text(checking);
                let result = self
                    .work_on_gallery_url(&mut seed, &mut seen, &mut sink, job)
                    .await;
                let saved = seed.clone();
                self.store
                    .write(move |ctx| queues::update_gallery_seed(ctx.conn(), &saved))?;
                let outcome = match result {
                    Ok(outcome) => outcome,
                    Err(e) => {
                        stop_reason = e.to_string();
                        return Err(e.into());
                    }
                };
                if !outcome.can_search_for_more_files {
                    stop_reason.clone_from(&outcome.stop_reason);
                    if let Some((key, name)) = class {
                        stopped_classes.insert(
                            key,
                            format!("previous {name} URL said: {}", outcome.stop_reason),
                        );
                    }
                }
                if sink.file_limit.is_some_and(|limit| sink.total_new >= limit) {
                    break;
                }
            }
            Ok(())
        }
        .await;
        popup.stop_following();
        // pages not read this time are vetoed with the reason
        let reason = stop_reason.clone();
        self.store.write(move |ctx| {
            while let Some(mut seed) = queues::next_gallery_seed(ctx.conn(), queue)? {
                set_gallery_status(&mut seed, SeedStatus::Vetoed, reason.clone());
                queues::update_gallery_seed(ctx.conn(), &seed)?;
            }
            Ok(())
        })?;
        outcome?;

        // oldest first, so the history stays in order
        let mut new = std::mem::take(&mut sink.new);
        new.reverse();
        report.new_urls += new.len() as u64;
        report.notices.append(&mut sink.notices);
        query.state.file_seed_compaction_number = sink.compaction_number;
        let found_nothing = new.is_empty();
        self.store
            .write(move |ctx| queues::add_file_seeds(ctx.conn(), queue, &new, false, now()))?;
        self.register_sync_complete(sub, query)?;
        if query.state.dead {
            let (sub_name, query_name) = &names;
            report.notices.push(if initial && found_nothing {
                format!(
                    "The query \"{query_name}\" for subscription \"{sub_name}\" did not find any files on its first sync! Could the query text have a typo, like a missing underscore?"
                )
            } else if initial {
                format!(
                    "The query \"{query_name}\" for subscription \"{sub_name}\" performed its first sync ok, but the query seems to be already dead! Hydrus will get all the outstanding files, but it will not check for new ones in future. If you know this query has not had any uploads in a long time and just wanted to catch up on what was already there, then no worries."
                )
            } else {
                let (files, period) = sub.settings.checker.death_file_velocity;
                format!(
                    "The query \"{query_name}\" for subscription \"{sub_name}\" found fewer than {} files in the last {}, so it appears to be dead!",
                    human_int(files.max(0) as u64),
                    hydrus_core::time::pretty_time_delta(period, true)
                )
            });
        }
        Ok(())
    }

    /// `RegisterSyncComplete`: compact the history, then time the next check.
    fn register_sync_complete(
        &self,
        sub: &Subscription,
        query: &mut SubscriptionQuery,
    ) -> Result<(), RunStop> {
        let t = now();
        let queue = query.queue_id;
        let checker = sub.settings.checker.clone();
        let before = t - checker.death_file_velocity_period();
        let file_keep = query.state.file_seed_compaction_number as usize;
        let gallery_keep = query.state.gallery_seed_compaction_number as usize;
        let seeds = self.store.write(move |ctx| {
            let conn = ctx.conn();
            let galleries = queues::gallery_seeds(conn, queue)?;
            let entries: Vec<(bool, i64)> = galleries
                .iter()
                .map(|g| (g.status == SeedStatus::Unknown, g.created))
                .collect();
            let drop: Vec<i64> = compact_gallery_log(&entries, gallery_keep, before)
                .into_iter()
                .map(|i| galleries[i].id)
                .collect();
            queues::remove_gallery_seeds_by_id(conn, &drop)?;
            let files = queues::file_seeds(conn, queue)?;
            let drop: Vec<i64> = compact_file_log(&history_entries(&files), file_keep, before)
                .into_iter()
                .map(|i| files[i].id)
                .collect();
            queues::remove_file_seeds_by_id(conn, &drop)?;
            queues::file_seeds(conn, queue)
        })?;
        let times: Vec<SeedTime> = seeds
            .iter()
            .map(|s| SeedTime {
                source_time: s.source_time,
                created: s.created,
            })
            .collect();
        let file_work = seeds.iter().any(|s| s.status == SeedStatus::Unknown);
        query
            .state
            .register_sync_complete(&checker, &times, file_work, t);
        Ok(())
    }

    /// `_WorkOnQueriesFiles`.
    async fn work_on_queries_files(
        &self,
        sub: &mut Subscription,
        job: &Job,
        report: &mut RunReport,
        popup: &popups::Working,
    ) -> Result<(), RunStop> {
        let mut queries = Vec::new();
        for query in self.store.read(|conn| store_subs::queries(conn, sub.id))? {
            if !query.state.paused && self.has_file_work(query.queue_id)? {
                queries.push(query);
            }
        }
        self.order_queries(&mut queries);
        let count = queries.len();
        let mut result = Ok(());
        for (i, query) in queries.iter().enumerate() {
            let name = query.state.human_name();
            let mut text = format!("syncing files ({})", value_range(i as u64, count as u64));
            if name != sub.name {
                text.push_str(&format!(" \"{name}\""));
            }
            popup.set_text(Some(text));
            popup.set_gauge(Some((i as i64, count as i64)));
            result = self
                .work_on_query_files(sub, query, job, report, popup)
                .await;
            if result.is_err() {
                break;
            }
        }
        // (DeleteFiles, DeleteStatusText, DeleteGauge)
        popup.stop_following();
        popup.set_files(Vec::new(), "");
        popup.set_text(None);
        popup.set_text_2(None);
        popup.set_gauge(None);
        popup.set_gauge_2(None);
        result
    }

    /// Whether any of its queries has files to work on
    /// (`_WorkOnQueriesFilesCanDoWork`).
    fn any_file_work(&self, sub: &Subscription) -> Result<bool, RunStop> {
        for query in self.store.read(|conn| store_subs::queries(conn, sub.id))? {
            if !query.state.paused && self.has_file_work(query.queue_id)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `_WorkOnQueryFiles`: and however it stops, the files it presents
    /// are published, as the reference does at its end (`finally`).
    async fn work_on_query_files(
        &self,
        sub: &mut Subscription,
        query: &SubscriptionQuery,
        job: &Job,
        report: &mut RunReport,
        popup: &popups::Working,
    ) -> Result<(), RunStop> {
        let mut presented = Vec::new();
        let result = self
            .query_files(sub, query, job, report, (popup, &mut presented))
            .await;
        if sub.settings.publish_files_to_popup_button {
            let label = publishing_label(&sub.name, &sub.settings, query.state.human_name());
            popups::publish_presented(self.store(), &label, presented);
        }
        result
    }

    async fn query_files(
        &self,
        sub: &mut Subscription,
        query: &SubscriptionQuery,
        job: &Job,
        report: &mut RunReport,
        (popup, presented): (&popups::Working, &mut Vec<Sha256>),
    ) -> Result<(), RunStop> {
        let queue = query.queue_id;
        let name = human_name(sub, query);
        // (the files' count from where this sync starts: 1/3 rather than
        // 4001/4003)
        let counts = |conn: &rusqlite::Connection| -> hydrus_store::Result<(u64, u64)> {
            let counts = queues::file_seed_counts(conn, queue)?;
            let total: usize = counts.values().sum();
            let unknown = counts.get(&SeedStatus::Unknown).copied().unwrap_or(0);
            Ok(((total - unknown) as u64, total as u64))
        };
        let (done_before, _) = self.store.read(counts)?;
        job.set_scope(query_scope(sub, query));
        let mut done_work = false;
        loop {
            let Some(mut seed) = self
                .store
                .read(|conn| queues::next_file_seed(conn, queue))?
            else {
                break;
            };
            if job.is_cancelled() || popup.is_cancelled() {
                delay(sub, 300, "recently cancelled");
                return Err(RunStop::Stop);
            }
            if sub.settings.paused
                || now() < sub.settings.no_work_until
                || self.subscriptions_paused()
            {
                return Err(RunStop::Stop);
            }
            if !self.net.domain_ok(&seed.data) {
                if done_work {
                    popup.set_text_2(Some("domain had errors, will try again later".into()));
                    delay(sub, 3600, "domain errors, will try again later");
                }
                return Err(RunStop::Stop);
            }
            // out of bandwidth: stop the subscription's file work until
            // there is some (it is scheduled for then)
            if !self.file_bandwidth_ok(sub, query, Some(&seed.data)) {
                if done_work {
                    let text = "no more bandwidth to download files, will do some more later";
                    popup.set_text_2(Some(text.into()));
                    job.set_status_text(text);
                }
                return Err(RunStop::Stop);
            }
            job.set_status_text(format!("{name}: downloading files"));
            let (done, total) = self.store.read(counts)?;
            let (done, total) = (done - done_before, total - done_before);
            popup.set_gauge_2(Some((done as i64, total as i64)));
            popup.follow_stage(2, format!("files {}: ", value_range(done, total)));
            let lookup: Vec<&str> = std::iter::once(seed.data.as_str())
                .chain(seed.referral_url.as_deref())
                .collect();
            // WorkOnURL handles its own network/import failures. Only failures
            // escaping per-file option generation or subsequent query-tag work
            // spend the subscription's outer error budget.
            let (presentation, failure) = match self.full_options(
                CallerType::Subscription,
                &sub.settings.import_options,
                &lookup,
            ) {
                Ok(options) => {
                    self.work_on_url(&mut seed, &options, job).await;
                    let failure = self
                        .write_query_tags(&seed, &query.state.tag_import_options)
                        .err();
                    (Some(options.presentation), failure)
                }
                Err(error) => (None, Some(error.into())),
            };
            if let Some(error) = &failure {
                crate::seeds::set_status(&mut seed, SeedStatus::Error, error.to_string());
                popup.set_text_2(Some(format!(
                    "files {}: file failed",
                    value_range(done, total)
                )));
                count_file_failure(report, error);
            }
            if let Some(presentation) = &presentation
                && let Some(hash) = popups::presented_file(self.store(), &seed, presentation)
                && !presented.contains(&hash)
            {
                presented.push(hash);
            }
            // (and the files so far, under the query's name; none, the
            // last query's go, having outstayed their welcome)
            popup.set_files(presented.clone(), &human_name(sub, query));
            let saved = seed.clone();
            self.store
                .write(move |ctx| queues::update_file_seed(ctx.conn(), &saved))?;
            report.files_worked += 1;
            done_work = true;
            if let Some(error) = failure {
                if !matches!(error, WorkError::DataMissing(_)) {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                let threshold = self.network.read().subscription_file_error_cancel_threshold;
                if threshold.is_some_and(|limit| report.file_errors >= limit) {
                    return Err(RunStop::Failed(format!(
                        "The subscription {} encountered several errors when downloading files, so it abandoned its sync.",
                        sub.name
                    )));
                }
            }
        }
        Ok(())
    }
}

impl Downloader {
    /// `_GetQueryHeadersForProcessing`'s order: random, or by name.
    fn order_queries(&self, queries: &mut [SubscriptionQuery]) {
        if self.network.read().process_subs_in_random_order {
            queries.shuffle(&mut rand::rng());
        } else {
            queries.sort_by_cached_key(|q| q.state.human_name().to_owned());
        }
    }
}

/// The outer exception budget deliberately excludes only typed DataMissing.
fn count_file_failure(report: &mut RunReport, error: &WorkError) -> bool {
    if matches!(error, WorkError::DataMissing(_)) {
        false
    } else {
        report.file_errors += 1;
        true
    }
}

/// `_DelayWork`.
fn delay(sub: &mut Subscription, seconds: i64, reason: &str) {
    sub.settings.no_work_until = now() + seconds;
    reason
        .lines()
        .next()
        .unwrap_or_default()
        .clone_into(&mut sub.settings.no_work_until_reason);
}

fn gug_options(classes: &UrlClasses, percent_twenty_is_space: bool) -> GugOptions {
    GugOptions {
        percent_twenty_is_space,
        collapse_leading_slashes: classes.settings().collapse_leading_slashes,
    }
}

/// What the subscription runner is doing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunnerStatus {
    /// The first active subscription, retained for existing callers.
    pub running: Option<(String, String)>,
    /// Every active subscription: persistent id, name, and current job text.
    pub active: Vec<(i64, String, String)>,
    /// Notices from recent runs, newest last.
    pub notices: Vec<String>,
}

/// Runs due subscriptions up to the live maximum, with one job per
/// subscription (`SubscriptionsManager`). Lowering the limit leaves existing
/// jobs alone; it prevents new admissions until there is room again.
#[derive(Debug)]
pub struct SubscriptionRunner {
    downloader: std::sync::Arc<Downloader>,
    wake: tokio::sync::Notify,
    started: std::sync::atomic::AtomicBool,
    shutdown: std::sync::atomic::AtomicBool,
    stopped: std::sync::atomic::AtomicBool,
    jobs: parking_lot::Mutex<std::collections::BTreeMap<i64, (String, std::sync::Arc<Job>)>>,
    notices: parking_lot::Mutex<VecDeque<String>>,
}

/// Seconds after a subscription's due time before it starts
/// (`SUB_WORK_DELAY_BUFFER`).
const START_BUFFER: i64 = 3;
/// Minimum repeat interval after a run (`BUFFER_TIME`).
const FINISHED_BUFFER: i64 = 120;

impl SubscriptionRunner {
    pub fn new(downloader: std::sync::Arc<Downloader>) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            downloader,
            wake: tokio::sync::Notify::new(),
            started: std::sync::atomic::AtomicBool::new(false),
            shutdown: std::sync::atomic::AtomicBool::new(false),
            stopped: std::sync::atomic::AtomicBool::new(false),
            jobs: parking_lot::Mutex::default(),
            notices: parking_lot::Mutex::default(),
        })
    }

    /// Start running subscriptions (once).
    pub fn start(self: &std::sync::Arc<Self>) {
        if self.started.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let runner = std::sync::Arc::clone(self);
        tokio::spawn(async move { runner.run().await });
    }

    /// Look again now (after subscriptions or options were changed).
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// Cancel the first active subscription, retained for existing callers.
    /// A cancelled sync waits 5–10 minutes before trying again.
    pub fn cancel_current(&self) {
        if let Some((_, job)) = self.jobs.lock().values().next() {
            job.cancel();
        }
    }

    /// Cancel a particular active subscription, without cancelling its peers.
    pub fn cancel(&self, id: i64) -> bool {
        self.jobs.lock().get(&id).is_some_and(|(_, job)| {
            job.cancel();
            true
        })
    }

    pub fn cancel_all(&self) {
        for (_, job) in self.jobs.lock().values() {
            job.cancel();
        }
    }

    /// Stop the daemon permanently, cancelling and joining its active work.
    /// No new jobs are admitted after this call.
    pub fn shutdown(&self) {
        let jobs = self.jobs.lock();
        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
        for (_, job) in jobs.values() {
            job.cancel();
        }
        drop(jobs);
        self.wake();
    }

    /// Wait for shutdown to join every sync before closing the daemon's store.
    pub async fn wait_stopped(&self) {
        while self.started.load(std::sync::atomic::Ordering::SeqCst)
            && !self.stopped.load(std::sync::atomic::Ordering::SeqCst)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub fn status(&self) -> RunnerStatus {
        let active: Vec<_> = self
            .jobs
            .lock()
            .iter()
            .map(|(id, (name, job))| (*id, name.clone(), job.state().status))
            .collect();
        RunnerStatus {
            running: active
                .first()
                .map(|(_, name, text)| (name.clone(), text.clone())),
            active,
            notices: self.notices.lock().iter().cloned().collect(),
        }
    }

    /// The subscription to admit now, else how long to wait for one.
    fn next(
        &self,
        finished: &HashMap<i64, i64>,
    ) -> Result<Result<Subscription, Duration>, WorkError> {
        self.next_at(finished, now())
    }

    fn next_at(
        &self,
        finished: &HashMap<i64, i64>,
        t: i64,
    ) -> Result<Result<Subscription, Duration>, WorkError> {
        self.downloader.reload_settings()?;
        if self.shutdown.load(std::sync::atomic::Ordering::SeqCst)
            || self.downloader.subscriptions_paused()
        {
            return Ok(Err(Duration::from_secs(30)));
        }
        let active: BTreeSet<_> = self.jobs.lock().keys().copied().collect();
        let limit = self
            .downloader
            .network
            .read()
            .max_simultaneous_subscriptions
            .clamp(1, 100);
        if active.len() >= limit as usize {
            return Ok(Err(Duration::from_millis(500)));
        }
        let subs = self.downloader.store.read(store_subs::subscriptions)?;
        let mut ready = Vec::new();
        let mut soonest: Option<i64> = None;
        for sub in subs {
            if active.contains(&sub.id) {
                continue;
            }
            let Some(mut when) = self.downloader.next_work_time(&sub)? else {
                continue;
            };
            if let Some(buffer) = finished.get(&sub.id) {
                when = when.max(*buffer);
            }
            if t > when.saturating_add(START_BUFFER) {
                ready.push(sub);
            } else {
                soonest = Some(soonest.map_or(when, |s| s.min(when)));
            }
        }
        let next = if self.downloader.network.read().process_subs_in_random_order {
            ready.choose(&mut rand::rng()).cloned()
        } else {
            ready
                .into_iter()
                .min_by_key(|sub| hydrus_core::sort::human_sort_key(&sub.name))
        };
        if let Some(sub) = next {
            return Ok(Ok(sub));
        }
        // Recheck persisted options even when no work is due. Active work is
        // polled at the reference manager's half-second cadence.
        let wait = soonest.map_or(30, |s| (s + START_BUFFER + 1 - t).clamp(1, 30));
        Ok(Err(Duration::from_secs(wait as u64)))
    }

    async fn run(&self) {
        let mut workers = tokio::task::JoinSet::new();
        let mut task_ids = HashMap::new();
        let mut finished = HashMap::new();
        loop {
            if self.shutdown.load(std::sync::atomic::Ordering::SeqCst) && workers.is_empty() {
                break;
            }
            let wait = match self.next(&finished) {
                Ok(Ok(sub)) => {
                    let job = Job::new();
                    // Reserve the id before spawning, so no tick can admit it twice.
                    // Check shutdown while holding the same lock cancel_all uses.
                    let mut jobs = self.jobs.lock();
                    if !self.shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                        jobs.insert(sub.id, (sub.name.clone(), std::sync::Arc::clone(&job)));
                        let downloader = std::sync::Arc::clone(&self.downloader);
                        let id = sub.id;
                        let task = workers.spawn(async move {
                            let result = downloader.run_subscription(id, &job).await;
                            (id, sub.name, result)
                        });
                        task_ids.insert(task.id(), id);
                    }
                    Duration::from_millis(500)
                }
                Ok(Err(wait)) => wait,
                Err(e) => {
                    tracing::error!("finding subscriptions to run: {e}");
                    Duration::from_secs(60)
                }
            };
            let wait = if workers.is_empty() {
                wait
            } else {
                wait.min(Duration::from_millis(500))
            };
            tokio::select! {
                () = self.wake.notified() => {},
                () = tokio::time::sleep(wait) => {},
                joined = workers.join_next_with_id(), if !workers.is_empty() => {
                    match joined {
                        Some(Ok((task_id, (id, name, result)))) => {
                            task_ids.remove(&task_id);
                            self.jobs.lock().remove(&id);
                            if !self.downloader.subscriptions_paused() {
                                finished.insert(id, now().saturating_add(FINISHED_BUFFER));
                            }
                            match result {
                                Ok(report) => {
                                    let mut notices = self.notices.lock();
                                    notices.extend(report.notices);
                                    while notices.len() > 100 {
                                        notices.pop_front();
                                    }
                                }
                                Err(e) => tracing::error!("subscription \"{name}\": {e}"),
                            }
                        }
                        Some(Err(error)) => {
                            if let Some(id) = task_ids.remove(&error.id()) {
                                self.jobs.lock().remove(&id);
                                finished.insert(id, now().saturating_add(FINISHED_BUFFER));
                            }
                            tracing::error!("subscription task: {error}");
                        }
                        None => {},
                    }
                },
            }
        }
        self.stopped
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_admission_replays_reference_due_pause_live_limits_and_single_flight() {
        let recording = hydrus_testkit::fixture_json("subscription_concurrency.json");
        let dir = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(dir.path()).unwrap();
        let net = std::sync::Arc::new(
            hydrus_net::NetEngine::new(
                std::sync::Arc::clone(&store),
                hydrus_net::NetOptions {
                    obey_bandwidth: false,
                    ..hydrus_net::NetOptions::default()
                },
            )
            .unwrap(),
        );
        let importer = hydrus_import::FileImporter::new(
            std::sync::Arc::clone(&store),
            hydrus_media::MediaTools::new(),
        );
        let downloader = std::sync::Arc::new(
            Downloader::new(std::sync::Arc::clone(&store), net, importer).unwrap(),
        );
        let runner = SubscriptionRunner::new(downloader);
        let pairs = store
            .write(|ctx| {
                ["sub 10", "sub 2", "sub 1"]
                    .into_iter()
                    .map(|name| {
                        let id = store_subs::create_subscription(
                            ctx.conn(),
                            name,
                            &SubscriptionSettings::default(),
                        )?
                        .unwrap();
                        let queue = store_subs::add_query(
                            ctx.conn(),
                            id,
                            &hydrus_core::subscriptions::QueryState::new(name),
                            0,
                        )?;
                        Ok((id, queue, name.to_owned()))
                    })
                    .collect::<Result<Vec<_>, hydrus_store::StoreError>>()
            })
            .unwrap();
        let at = recording["now"].as_i64().unwrap();
        for case in recording["cases"].as_array().unwrap() {
            // Native editor transactions do not pause the daemon while their
            // drafts are open; this broader manager difference is not claimed.
            if case["editing"].as_bool().unwrap() {
                continue;
            }
            let limit = u32::try_from(case["limit"].as_u64().unwrap()).unwrap();
            store
                .write(|ctx| {
                    let mut settings: hydrus_store::network::NetworkSettings =
                        hydrus_store::settings::get(ctx.conn())?;
                    settings.max_simultaneous_subscriptions = limit;
                    settings.process_subs_in_random_order = false;
                    hydrus_store::settings::set(ctx.conn(), &settings)?;
                    let pauses = Pauses {
                        subscriptions: case["paused"].as_bool().unwrap(),
                        network_traffic: case["traffic_paused"].as_bool().unwrap(),
                        ..Pauses::default()
                    };
                    hydrus_store::settings::set(ctx.conn(), &pauses)?;
                    for (_, queue, _) in &pairs {
                        let mut state = store_subs::query(ctx.conn(), *queue)?.unwrap().state;
                        state.next_check_time = case["due"].as_i64().unwrap();
                        store_subs::set_query_state(ctx.conn(), *queue, &state)?;
                    }
                    Ok(())
                })
                .unwrap();
            runner.jobs.lock().clear();
            for name in case["running"].as_array().unwrap() {
                let (id, _, name) = pairs
                    .iter()
                    .find(|(_, _, n)| n == name.as_str().unwrap())
                    .unwrap();
                runner.jobs.lock().insert(*id, (name.clone(), Job::new()));
            }
            let expected = case["chosen"].as_str();
            let chosen = runner.next_at(&HashMap::new(), at).unwrap().ok();
            assert_eq!(
                chosen.as_ref().map(|sub| sub.name.as_str()),
                expected,
                "{}",
                case["case"]
            );
            assert_eq!(
                runner.status().active.len(),
                case["running_after"].as_array().unwrap().len()
            );
        }
        runner.jobs.lock().clear();
        let buffer = recording["finished_run_buffer"].as_i64().unwrap();
        assert_eq!(FINISHED_BUFFER, buffer);
        let finished = pairs.iter().map(|(id, _, _)| (*id, at + buffer)).collect();
        assert!(
            runner
                .next_at(&finished, at + buffer + START_BUFFER)
                .unwrap()
                .is_err()
        );
        assert!(
            runner
                .next_at(&finished, at + buffer + START_BUFFER + 1)
                .unwrap()
                .is_ok()
        );
    }

    #[test]
    fn outer_typed_exception_budget_replays_recorded_classes_and_reset() {
        let recording = hydrus_testkit::fixture_json("subscription_failure_limit.json");
        for case in recording["cases"].as_array().unwrap() {
            let mut report = RunReport::default();
            let mut sleeps = Vec::new();
            for call in case["calls"].as_array().unwrap() {
                let kind = call.as_str().unwrap();
                let error = if kind.starts_with("missing") {
                    Some(WorkError::DataMissing("synthetic missing data".into()))
                } else if kind.starts_with("error") || kind.starts_with("other") {
                    Some(WorkError::File("synthetic outer failure".into()))
                } else {
                    None
                };
                if let Some(error) = error
                    && count_file_failure(&mut report, &error)
                {
                    sleeps.push(5);
                }
            }
            assert_eq!(serde_json::json!(report.file_errors), case["file_errors"]);
            let expected = case["sleeps"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| **v == 5)
                .count();
            assert_eq!(sleeps.len(), expected);
        }
        let mut report = RunReport::default();
        assert!(count_file_failure(
            &mut report,
            &WorkError::File("DataMissing is merely text".into())
        ));
        assert_eq!(report.file_errors, 1);
    }

    #[test]
    fn presented_files_are_labelled_as_the_reference_labels_them() {
        let mut settings = SubscriptionSettings::default();
        assert_eq!(publishing_label("sub", &settings, "query"), "sub");
        settings.merge_query_publish_events = false;
        assert_eq!(publishing_label("sub", &settings, "query"), "sub: query");
        settings.publish_label_override = Some("art".into());
        assert_eq!(publishing_label("sub", &settings, "query"), "art: query");
        settings.merge_query_publish_events = true;
        assert_eq!(publishing_label("sub", &settings, "query"), "art");
    }
}
