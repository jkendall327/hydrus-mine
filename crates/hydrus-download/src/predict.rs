//! Whether a seed's file is already known, before fetching anything
//! (`PredictPreImportStatus` and the hash and URL lookups behind it).

use std::collections::BTreeSet;

use hydrus_core::import_options::{FullImportOptions, PrefetchCheck};
use hydrus_core::url::UrlClasses;
use hydrus_core::url::functions::check_full_url;
use hydrus_core::{HashKind, Sha256};
use hydrus_import::{FileImportOptions, ImportResult, ImportStatus};
use hydrus_store::queues::{FileSeed, SeedStatus, SeedType};
use hydrus_store::{master, media, urls};

use crate::seeds::set_status;
use crate::{Downloader, WorkError, seed_status};

fn unknown() -> ImportResult {
    ImportResult {
        status: ImportStatus::Unknown,
        hash: None,
        mime: None,
        note: String::new(),
        raised: None,
    }
}

/// `FileImportStatus.ShouldImport`.
fn should_import(status: ImportStatus, options: &FullImportOptions) -> bool {
    match status {
        ImportStatus::Unknown => true,
        ImportStatus::Deleted => !options.file_filtering.exclude_deleted,
        _ => false,
    }
}

/// URLs normalised for storage, first occurrence kept (`NormaliseURLs`).
fn normalise_all(classes: &UrlClasses, urls: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    urls.into_iter()
        .map(|u| classes.normalise(&u, false).unwrap_or(u))
        .filter(|u| seen.insert(u.clone()))
        .collect()
}

fn domain(url: &str) -> String {
    check_full_url(url).map(|p| p.netloc).unwrap_or_default()
}

/// Single-file post URLs (`FilterOneFileURLs`).
fn one_file_post_urls(classes: &UrlClasses, urls: Vec<String>) -> Vec<String> {
    urls.into_iter()
        .filter(|u| {
            classes.class_for(u).is_some_and(|c| {
                c.url_type == hydrus_core::url::UrlType::Post && !c.can_produce_multiple_files
            })
        })
        .collect()
}

impl Downloader {
    /// Whether to fetch the seed's metadata and its file; sets the seed's
    /// result if the file is not wanted.
    pub(crate) fn predict(
        &self,
        seed: &mut FileSeed,
        options: &FullImportOptions,
        file_url: Option<&str>,
    ) -> Result<(bool, bool), WorkError> {
        let (hash_found, hash_dispositive, by_hash) = self.predict_by_hash(seed, options)?;
        let (url_found, url_dispositive, by_url) = self.predict_by_url(seed, options, file_url)?;
        // a dispositive hash match wins, then a dispositive URL match; else
        // the URL's answer unless the hash already says not to import
        let use_url = !(hash_found && hash_dispositive)
            && ((url_found && url_dispositive) || should_import(by_hash.status, options));
        let mut predicted = if use_url {
            by_url.clone()
        } else {
            by_hash.clone()
        };
        let should_download_file = should_import(predicted.status, options);
        let mut should_download_metadata = should_download_file;
        if !should_download_metadata {
            let prefetch = &options.prefetch;
            let url_override = by_url.status == ImportStatus::SuccessfulButRedundant
                && prefetch.fetch_metadata_even_if_url_recognised_and_file_already_in_db;
            let hash_override = by_hash.status == ImportStatus::SuccessfulButRedundant
                && prefetch.fetch_metadata_even_if_hash_recognised_and_file_already_in_db;
            should_download_metadata = url_override || hash_override;
        }
        let file_options = FileImportOptions::from_full(options, &self.store.snapshot().services);
        if predicted.status == ImportStatus::SuccessfulButRedundant
            && let Some(hash) = predicted.hash
            && !file_options.allows_all_based_on_file_info()
            && let Some(info) = self.stored_file_info(&hash)?
            && let Err(note) =
                file_options.check_values(info.size, info.mime, info.width, info.height)
        {
            predicted.status = ImportStatus::Vetoed;
            predicted.note = note;
        }
        if seed.status == SeedStatus::Unknown && !should_download_file {
            set_status(seed, seed_status(predicted.status), predicted.note);
            if let Some(hash) = predicted.hash {
                seed.meta.set_hash("sha256", hash.to_hex());
            }
        }
        Ok((should_download_metadata, should_download_file))
    }

    fn stored_file_info(&self, hash: &Sha256) -> Result<Option<media::FileInfo>, WorkError> {
        Ok(self.store.read(|conn| {
            let Some(id) = master::hash_id(conn, hash)? else {
                return Ok(None);
            };
            Ok(media::load_basic(conn, &[id])?
                .into_iter()
                .next()
                .and_then(|m| m.info))
        })?)
    }

    /// `GetPreImportStatusPredictionHash`: a hash the parser found, sha256
    /// first.
    fn predict_by_hash(
        &self,
        seed: &FileSeed,
        options: &FullImportOptions,
    ) -> Result<(bool, bool, ImportResult), WorkError> {
        let check = options.prefetch.hash_check;
        let dispositive = check == PrefetchCheck::CheckAndMatchesAreDispositive;
        if seed.meta.hashes.is_empty() || check == PrefetchCheck::DoNotCheck {
            return Ok((false, dispositive, unknown()));
        }
        let mut jobs: Vec<&(String, String)> = seed
            .meta
            .hashes
            .iter()
            .filter(|(t, _)| t == "sha256")
            .collect();
        jobs.extend(seed.meta.hashes.iter().filter(|(t, _)| t != "sha256"));
        for (hash_type, hex) in jobs {
            let Ok(bytes) = hex::decode(hex) else {
                continue;
            };
            let kind = match hash_type.as_str() {
                "sha256" => HashKind::Sha256,
                "md5" => HashKind::Md5,
                "sha1" => HashKind::Sha1,
                "sha512" => HashKind::Sha512,
                _ => continue,
            };
            let sha256 = if kind == HashKind::Sha256 {
                Sha256::from_slice(&bytes).ok()
            } else {
                self.store
                    .read(|conn| {
                        master::convert_hashes(
                            conn,
                            kind,
                            HashKind::Sha256,
                            std::slice::from_ref(&bytes),
                        )
                    })?
                    .into_iter()
                    .next()
                    .and_then(|(_, sha)| Sha256::from_slice(&sha).ok())
            };
            let Some(sha256) = sha256 else { continue };
            let (status, id) = self
                .importer
                .known_status(&sha256, &format!("{hash_type} hash recognised: "))?;
            if id.is_none() {
                continue;
            }
            return Ok((true, dispositive, status));
        }
        Ok((false, dispositive, unknown()))
    }

    /// `GetPreImportStatusPredictionURL`: a URL the file is known by, unless
    /// it points at several files or the site seems to reuse URLs.
    fn predict_by_url(
        &self,
        seed: &FileSeed,
        options: &FullImportOptions,
        file_url: Option<&str>,
    ) -> Result<(bool, bool, ImportResult), WorkError> {
        let check = options.prefetch.url_check;
        let dispositive = check == PrefetchCheck::CheckAndMatchesAreDispositive;
        if check == PrefetchCheck::DoNotCheck {
            return Ok((false, dispositive, unknown()));
        }
        let snapshot = self.store.snapshot();
        let classes = &snapshot.url_classes;
        let mut lookup: Vec<String> = Vec::new();
        if seed.seed_type == SeedType::Url {
            lookup.push(seed.data_for_comparison.clone());
        }
        lookup.extend(file_url.map(str::to_owned));
        lookup.extend(seed.meta.primary_urls.iter().cloned());
        let source: Vec<String> = seed
            .meta
            .source_urls
            .iter()
            .filter(|u| classes.refers_to_one_file(u))
            .cloned()
            .collect();
        let mut neighbours = lookup.clone();
        neighbours.extend(source.iter().cloned());
        if options.locations.associate_source_urls {
            lookup.extend(source);
        }
        let lookup = normalise_all(
            classes,
            lookup
                .into_iter()
                .filter(|u| !classes.can_refer_to_multiple_files(u)),
        );
        let neighbours = normalise_all(
            classes,
            neighbours
                .into_iter()
                .filter(|u| !classes.can_refer_to_multiple_files(u)),
        );
        let mut untrustworthy_domains = BTreeSet::new();
        let mut untrustworthy_hashes = BTreeSet::new();
        for url in lookup {
            let url_domain = domain(&url);
            if untrustworthy_domains.contains(&url_domain) {
                continue;
            }
            let search = classes.search_urls(&url);
            let files = self
                .store
                .read(|conn| urls::files_for_urls(conn, &snapshot.services, &search))?;
            let [file] = files.as_slice() else { continue };
            let (status, _) = self.importer.known_status(&file.hash, "url recognised: ")?;
            if untrustworthy_hashes.contains(&file.hash) {
                untrustworthy_domains.insert(url_domain);
                continue;
            }
            if options.prefetch.url_check_looks_for_neighbour_spam
                && self.has_untrustworthy_neighbours(classes, &file.hash, &neighbours)?
            {
                untrustworthy_domains.insert(url_domain);
                untrustworthy_hashes.insert(file.hash);
                continue;
            }
            return Ok((true, dispositive, status));
        }
        Ok((false, dispositive, unknown()))
    }

    /// `FileURLMappingHasUntrustworthyNeighbours`: the file already has
    /// another single-file post URL of the same class on the same site, so
    /// the site may be reusing URLs for different files.
    fn has_untrustworthy_neighbours(
        &self,
        classes: &UrlClasses,
        hash: &Sha256,
        lookup: &[String],
    ) -> Result<bool, WorkError> {
        let lookup: Vec<String> = lookup
            .iter()
            .filter(|u| check_full_url(u).is_ok())
            .cloned()
            .collect();
        let lookup = one_file_post_urls(classes, normalise_all(classes, lookup));
        if lookup.is_empty() {
            return Ok(false);
        }
        let domains: BTreeSet<String> = lookup.iter().map(|u| domain(u)).collect();
        let lookup_classes: BTreeSet<Vec<u8>> = lookup
            .iter()
            .filter_map(|u| classes.class_for(u).map(|c| c.key.clone()))
            .collect();
        let existing: Vec<String> = self.store.read(|conn| {
            let Some(id) = master::hash_id(conn, hash)? else {
                return Ok(Vec::new());
            };
            let snapshot = self.store.snapshot();
            Ok(media::load(conn, &snapshot.services, None, &[id])?
                .results
                .into_iter()
                .next()
                .map(|m| m.urls)
                .unwrap_or_default())
        })?;
        let existing: Vec<String> = existing
            .into_iter()
            .filter(|u| check_full_url(u).is_ok())
            .collect();
        for url in one_file_post_urls(classes, normalise_all(classes, existing)) {
            if lookup.contains(&url) || !domains.contains(&domain(&url)) {
                continue;
            }
            if classes
                .class_for(&url)
                .is_some_and(|c| lookup_classes.contains(&c.key))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
