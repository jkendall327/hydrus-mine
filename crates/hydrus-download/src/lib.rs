//! Downloading into the store: what the reference's importers do with the
//! URLs in their queues.
//!
//! A *file seed* is a URL to import. A post URL is fetched and parsed with
//! the parser its URL class links to; what the parser finds (the file URL,
//! tags, notes, hashes, source URLs, times) goes on the seed, and a post
//! with several files becomes a seed per file, queued straight after it. A
//! file URL is downloaded and imported. Before fetching anything the client
//! checks whether it already knows the file by URL or hash, as the import
//! options allow. Afterwards the gathered metadata is written to the file
//! as the import options say.
//!
//! The behaviour is the reference's `ClientImportFileSeeds`; see
//! `docs/rust/DOWNLOADER.md`.

use std::sync::Arc;

use parking_lot::RwLock;

use hydrus_core::import_options::{
    CallerType, FullImportOptions, ImportOptionsManager, ImportOptionsSlice, UrlClassKind,
};
use hydrus_core::url::UrlType;
use hydrus_import::{FileImporter, ImportStatus};
use hydrus_net::{NetEngine, NetError};
use hydrus_parse::{Downloaders, PageParser};
use hydrus_store::queues::SeedStatus;
use hydrus_store::{Store, StoreError};

mod content;
pub mod export;
pub mod folders;
mod gallery;
pub mod popups;
mod predict;
pub mod queue;
mod seeds;
pub mod subscriptions;

pub use gallery::GalleryOutcome;
pub use queue::{QueueRunner, UrlQueueStatus};

/// Why work on a seed stopped short of giving it a result.
#[derive(Debug, thiserror::Error)]
pub enum WorkError {
    /// A network failure the queue should wait out before going on.
    #[error("{0}")]
    Network(NetError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Import(#[from] hydrus_import::ImportError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// A parser could not read a page.
    #[error("{0}")]
    Parse(String),
}

/// Downloads and imports for the store's import queues.
#[derive(Debug)]
pub struct Downloader {
    store: Arc<Store>,
    net: Arc<NetEngine>,
    importer: FileImporter,
    definitions: RwLock<Arc<Downloaders>>,
    network: RwLock<hydrus_store::network::NetworkSettings>,
}

impl Downloader {
    pub fn new(
        store: Arc<Store>,
        net: Arc<NetEngine>,
        importer: FileImporter,
    ) -> Result<Self, StoreError> {
        let (definitions, network) = store.read(|conn| {
            Ok((
                hydrus_store::settings::get::<Downloaders>(conn)?,
                hydrus_store::settings::get(conn)?,
            ))
        })?;
        Ok(Self {
            store,
            net,
            importer,
            definitions: RwLock::new(Arc::new(definitions)),
            network: RwLock::new(network),
        })
    }

    /// The client's network and downloader options.
    pub fn network_settings(&self) -> hydrus_store::network::NetworkSettings {
        self.network.read().clone()
    }

    /// Pick up the store's network and downloader options, and the network
    /// engine's, if they have changed (as the reference reads its options
    /// as it goes); whether they had.
    pub fn reload_settings(&self) -> Result<bool, WorkError> {
        let (network, definitions) = self.store.read(|conn| {
            Ok((
                hydrus_store::settings::get::<hydrus_store::network::NetworkSettings>(conn)?,
                hydrus_store::settings::get::<Downloaders>(conn)?,
            ))
        })?;
        let mut changed = self.net.reload_settings().map_err(WorkError::Network)?;
        if network != *self.network.read() {
            changed = true;
            *self.network.write() = network;
        }
        if definitions != **self.definitions.read() {
            changed = true;
            *self.definitions.write() = Arc::new(definitions);
        }
        Ok(changed)
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn net(&self) -> &Arc<NetEngine> {
        &self.net
    }

    /// Pick up changed parsers and GUGs.
    pub fn reload_definitions(&self) -> Result<(), StoreError> {
        let definitions = self
            .store
            .read(hydrus_store::settings::get::<Downloaders>)?;
        *self.definitions.write() = Arc::new(definitions);
        Ok(())
    }

    pub fn definitions(&self) -> Arc<Downloaders> {
        Arc::clone(&self.definitions.read())
    }

    fn parser(&self, key: &str) -> Option<PageParser> {
        self.definitions.read().parser(key).cloned()
    }

    /// The options an import from this kind of importer uses, given its own
    /// options and the URLs involved (their classes' defaults apply).
    pub fn full_options(
        &self,
        caller: CallerType,
        own: &ImportOptionsSlice,
        urls: &[&str],
    ) -> Result<FullImportOptions, StoreError> {
        let manager: ImportOptionsManager = self.store.read(hydrus_store::settings::get)?;
        let snapshot = self.store.snapshot();
        let classes: Vec<(String, UrlClassKind)> = urls
            .iter()
            .flat_map(|url| snapshot.url_classes.api_class_keys(url))
            .map(|(key, url_type)| {
                let kind = if url_type == UrlType::Watchable {
                    UrlClassKind::Watchable
                } else {
                    UrlClassKind::Other
                };
                (key, kind)
            })
            .collect();
        Ok(manager.full(caller, Some(own), &classes))
    }
}

/// A file import's status as a seed status.
pub(crate) fn seed_status(status: ImportStatus) -> SeedStatus {
    match status {
        ImportStatus::Unknown => SeedStatus::Unknown,
        ImportStatus::SuccessfulAndNew => SeedStatus::SuccessfulAndNew,
        ImportStatus::SuccessfulButRedundant => SeedStatus::SuccessfulButRedundant,
        ImportStatus::Deleted => SeedStatus::Deleted,
        ImportStatus::Error => SeedStatus::Error,
        ImportStatus::Vetoed => SeedStatus::Vetoed,
        ImportStatus::Skipped => SeedStatus::Skipped,
    }
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// `ClientTime.TimestampIsSensible`: not absurdly early.
pub(crate) fn sensible(timestamp: Option<i64>) -> bool {
    timestamp.is_some_and(|t| t > 86400 * 7)
}
