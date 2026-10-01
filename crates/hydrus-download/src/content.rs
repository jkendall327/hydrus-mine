//! Writing what a seed learned to its file (`FileSeed.WriteContentUpdates`):
//! URLs, the site's post time, tags per the tag import options and notes
//! per the note import options.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::OptionalExtension;

use hydrus_core::import_options::{
    FullImportOptions, ImportedAs, ServiceTagImportOptions, TagImportOptions,
};
use hydrus_core::url::functions::check_full_url;
use hydrus_core::{ContentStatus, HashId, ServiceId, Sha256, Tag};
use hydrus_import::FileImportOptions;
use hydrus_store::content::{FileTime, MappingAction};
use hydrus_store::queues::{FileSeed, SeedStatus, SeedType};
use hydrus_store::schema::MappingTables;
use hydrus_store::services::ServiceKind;
use hydrus_store::{master, media};

use crate::seeds::associable;
use crate::{Downloader, WorkError, sensible};

/// `HydrusTags.CleanTags`.
fn clean(tags: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    tags.into_iter()
        .filter_map(|t| Tag::new(&t).map(|t| t.as_str().to_owned()))
        .collect()
}

/// What the tag rules need to know about the file.
struct FileFacts {
    hash_id: HashId,
    inbox: bool,
    /// Deleted tags per tag service.
    deleted: BTreeMap<ServiceId, BTreeSet<String>>,
    notes: BTreeMap<String, String>,
}

impl Downloader {
    /// Every tag in the sibling chains of `tags`, on any tag service.
    pub(crate) fn sibling_chain_tags(
        &self,
        tags: &BTreeSet<String>,
    ) -> Result<BTreeSet<String>, WorkError> {
        let snapshot = self.store.snapshot();
        Ok(self.store.read(|conn| {
            let mut out = BTreeSet::new();
            for tag in tags {
                let Some(tag) = Tag::new(tag) else { continue };
                let Some(id) = master::tag_id(conn, &tag)? else {
                    continue;
                };
                for service in snapshot.services.tag_services() {
                    let graph = snapshot.display.get(service.id);
                    if !graph.in_sibling_chain(id) {
                        continue;
                    }
                    let chain = graph.chain(id);
                    for (_, t) in master::tags(conn, &chain)? {
                        out.insert(t.as_str().to_owned());
                    }
                }
            }
            Ok(out)
        })?)
    }

    fn file_facts(&self, hash: &Sha256) -> Result<Option<FileFacts>, WorkError> {
        let snapshot = self.store.snapshot();
        Ok(self.store.read(|conn| {
            let Some(id) = master::hash_id(conn, hash)? else {
                return Ok(None);
            };
            let batch = media::load(conn, &snapshot.services, None, &[id])?;
            let Some(result) = batch.results.into_iter().next() else {
                return Ok(None);
            };
            let deleted = result
                .tags
                .iter()
                .map(|(service, tags)| {
                    let names = tags
                        .by_status
                        .get(&ContentStatus::Deleted)
                        .into_iter()
                        .flatten()
                        .filter_map(|t| batch.tags.get(t).map(|t| t.as_str().to_owned()))
                        .collect();
                    (*service, names)
                })
                .collect();
            Ok(Some(FileFacts {
                hash_id: id,
                inbox: result.inbox,
                deleted,
                notes: result.notes.into_iter().collect(),
            }))
        })?)
    }

    /// Tags with any current mapping on a service
    /// (`filter_existing_tags`).
    fn existing_tags(
        &self,
        service: ServiceId,
        tags: &BTreeSet<String>,
    ) -> Result<BTreeSet<String>, WorkError> {
        let table = MappingTables::new(service).current;
        Ok(self.store.read(|conn| {
            let mut out = BTreeSet::new();
            for tag in tags {
                let Some(t) = Tag::new(tag) else { continue };
                let Some(id) = master::tag_id(conn, &t)? else {
                    continue;
                };
                let exists = conn
                    .prepare_cached(&format!("SELECT 1 FROM {table} WHERE tag_id = ? LIMIT 1"))?
                    .query_row([id], |_| Ok(()))
                    .optional()?
                    .is_some();
                if exists {
                    out.insert(tag.clone());
                }
            }
            Ok(out)
        })?)
    }

    /// `ServiceTagImportOptions.GetTags`.
    fn service_tags(
        &self,
        service: ServiceId,
        rules: &ServiceTagImportOptions,
        imported: Option<ImportedAs>,
        facts: &FileFacts,
        filterable: &BTreeSet<String>,
        additional: &BTreeSet<String>,
    ) -> Result<BTreeSet<String>, WorkError> {
        let mut tags = BTreeSet::new();
        if !imported.is_some_and(|i| rules.applies_to(i)) {
            return Ok(tags);
        }
        let empty = BTreeSet::new();
        let deleted = facts.deleted.get(&service).unwrap_or(&empty);
        if rules.get_tags {
            let found: BTreeSet<String> = filterable
                .iter()
                .filter(|t| rules.get_tags_filter.tag_ok(t, false))
                .filter(|t| rules.get_tags_overwrite_deleted || !deleted.contains(*t))
                .cloned()
                .collect();
            tags.extend(found);
        }
        let additional = clean(additional.iter().chain(&rules.additional_tags).cloned());
        tags.extend(
            additional
                .into_iter()
                .filter(|t| rules.additional_tags_overwrite_deleted || !deleted.contains(t)),
        );
        if rules.only_add_existing_tags {
            let applicable: BTreeSet<String> = tags
                .iter()
                .filter(|t| rules.only_add_existing_tags_filter.tag_ok(t, false))
                .cloned()
                .collect();
            let existing = self.existing_tags(service, &applicable)?;
            tags.retain(|t| !applicable.contains(t));
            tags.extend(existing);
        }
        Ok(tags)
    }

    /// Write what the seed learned to its file; whether tags or notes were
    /// written.
    pub(crate) fn write_content_updates(
        &self,
        seed: &FileSeed,
        options: &FullImportOptions,
    ) -> Result<bool, WorkError> {
        let writes_metadata = seed.status.is_successful() || seed.status == SeedStatus::Deleted;
        if !writes_metadata {
            return Ok(false);
        }
        let Some(hash) = seed
            .meta
            .hash("sha256")
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| Sha256::from_slice(&b).ok())
        else {
            return Ok(false);
        };
        let Some(facts) = self.file_facts(&hash)? else {
            return Ok(false);
        };
        let snapshot = self.store.snapshot();
        let classes = &snapshot.url_classes;
        let locations = &options.locations;

        let mut urls: Vec<String> = Vec::new();
        let mut domain_times: Vec<(String, i64)> = Vec::new();
        if locations.associate_primary_urls {
            urls.extend(seed.meta.primary_urls.iter().cloned());
            if seed.seed_type == SeedType::Url {
                urls.push(seed.data_for_comparison.clone());
                let domain = check_full_url(&seed.data)
                    .map(|p| p.netloc)
                    .unwrap_or_default();
                let time = seed.source_time.unwrap_or(seed.created);
                if sensible(Some(time)) {
                    domain_times.push((domain, time * 1000));
                }
                if let Some(time) = seed.meta.cloudflare_last_modified
                    && sensible(Some(time))
                {
                    domain_times.push(("cloudflare.com".into(), time * 1000));
                }
            }
            urls.extend(seed.referral_url.iter().cloned());
        }
        if locations.associate_source_urls {
            urls.extend(seed.meta.source_urls.iter().cloned());
        }
        let urls: Vec<String> = associable(classes, urls).into_iter().collect();

        if seed.status == SeedStatus::SuccessfulButRedundant {
            let file_options = FileImportOptions::from_full(options, &snapshot.services);
            self.importer.update_already_in_db(&hash, &file_options)?;
        }

        // tags, per real tag service
        let imported = match seed.status {
            SeedStatus::SuccessfulAndNew => Some(ImportedAs::New),
            SeedStatus::SuccessfulButRedundant if facts.inbox => Some(ImportedAs::AlreadyInInbox),
            SeedStatus::SuccessfulButRedundant => Some(ImportedAs::AlreadyInArchive),
            _ => None,
        };
        let mut service_tags: Vec<(ServiceId, bool, BTreeSet<String>)> = Vec::new();
        let tags_wanted = !seed.meta.tags.is_empty()
            || !seed.meta.external_filterable_tags.is_empty()
            || !seed.meta.external_additional_tags.is_empty()
            || options.tags.has_additional_tags();
        if tags_wanted {
            let filterable = clean(seed.meta.tags.iter().cloned());
            for service in snapshot.services.tag_services() {
                let key = service.key.to_hex();
                let additional: BTreeSet<String> = seed
                    .meta
                    .external_additional_tags
                    .iter()
                    .filter(|(k, _)| *k == key)
                    .flat_map(|(_, tags)| tags.iter().cloned())
                    .collect();
                let tags = match options.tags.service(&key) {
                    Some(rules) => {
                        let mut filterable = filterable.clone();
                        filterable.extend(seed.meta.external_filterable_tags.iter().cloned());
                        self.service_tags(
                            service.id,
                            rules,
                            imported,
                            &facts,
                            &filterable,
                            &additional,
                        )?
                    }
                    None => additional,
                };
                if !tags.is_empty() {
                    let pend = matches!(service.kind, ServiceKind::TagRepository(_));
                    service_tags.push((service.id, pend, tags));
                }
            }
        }

        // notes
        let note_updates = if seed.meta.notes.is_empty() {
            BTreeMap::new()
        } else {
            options.notes.updates(&facts.notes, &seed.meta.notes)
        };

        let did_work = !service_tags.is_empty() || !note_updates.is_empty();
        let id = facts.hash_id;
        self.store.write_content(move |w| {
            for (domain, ms) in domain_times {
                let time = FileTime::DomainModified(domain);
                let existing = w.file_time(id, &time)?;
                if existing.is_none_or(|e| ms < e) {
                    w.set_file_time(&[id], &time, ms)?;
                }
            }
            if !urls.is_empty() {
                w.add_urls(&[id], &urls)?;
            }
            for (service, pend, tags) in &service_tags {
                let action = if *pend {
                    MappingAction::Pend
                } else {
                    MappingAction::Add
                };
                for tag in tags {
                    let Some(tag) = Tag::new(tag) else { continue };
                    let tag_id = master::intern_tag(w.conn(), &tag)?;
                    w.update_mappings(*service, &action, tag_id, &[id])?;
                }
            }
            for (name, note) in &note_updates {
                w.set_note(id, name, note)?;
            }
            Ok(())
        })?;
        Ok(did_work)
    }

    /// Add a subscription query's own tags to what a seed imported
    /// (`GetContentUpdatePackage` with no downloaded tags); whether any were
    /// added.
    pub(crate) fn write_query_tags(
        &self,
        seed: &FileSeed,
        tags: &TagImportOptions,
    ) -> Result<bool, WorkError> {
        let writes = seed.status.is_successful() || seed.status == SeedStatus::Deleted;
        if !writes || !tags.has_additional_tags() {
            return Ok(false);
        }
        let Some(hash) = seed
            .meta
            .hash("sha256")
            .and_then(|h| hex::decode(h).ok())
            .and_then(|b| Sha256::from_slice(&b).ok())
        else {
            return Ok(false);
        };
        let Some(facts) = self.file_facts(&hash)? else {
            return Ok(false);
        };
        let imported = match seed.status {
            SeedStatus::SuccessfulAndNew => Some(ImportedAs::New),
            SeedStatus::SuccessfulButRedundant if facts.inbox => Some(ImportedAs::AlreadyInInbox),
            SeedStatus::SuccessfulButRedundant => Some(ImportedAs::AlreadyInArchive),
            _ => None,
        };
        let snapshot = self.store.snapshot();
        let none = BTreeSet::new();
        let mut service_tags: Vec<(ServiceId, bool, BTreeSet<String>)> = Vec::new();
        for service in snapshot.services.tag_services() {
            let Some(rules) = tags.service(&service.key.to_hex()) else {
                continue;
            };
            let found = self.service_tags(service.id, rules, imported, &facts, &none, &none)?;
            if !found.is_empty() {
                let pend = matches!(service.kind, ServiceKind::TagRepository(_));
                service_tags.push((service.id, pend, found));
            }
        }
        if service_tags.is_empty() {
            return Ok(false);
        }
        let id = facts.hash_id;
        self.store.write_content(move |w| {
            for (service, pend, tags) in &service_tags {
                let action = if *pend {
                    MappingAction::Pend
                } else {
                    MappingAction::Add
                };
                for tag in tags {
                    let Some(tag) = Tag::new(tag) else { continue };
                    let tag_id = master::intern_tag(w.conn(), &tag)?;
                    w.update_mappings(*service, &action, tag_id, &[id])?;
                }
            }
            Ok(())
        })?;
        Ok(true)
    }
}
