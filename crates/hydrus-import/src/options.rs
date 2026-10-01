//! What an import may bring in, and where it goes (the reference's file
//! filtering and location import options).

use std::collections::BTreeSet;

use hydrus_core::numbers::{human_bytes, resolution_text};
use hydrus_core::{Mime, ServiceId};
use hydrus_store::services::ServiceRegistry;

/// How to import files. The defaults are the reference's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileImportOptions {
    /// Don't re-import files that were deleted before.
    pub exclude_deleted: bool,
    /// File types allowed; `None` allows every importable type.
    pub allowed_mimes: Option<BTreeSet<Mime>>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    /// A separate upper size limit for GIFs.
    pub max_gif_size: Option<u64>,
    pub min_resolution: Option<(u32, u32)>,
    pub max_resolution: Option<(u32, u32)>,
    /// Local file domains to import into; empty means the first one ("my files").
    pub destinations: Vec<ServiceId>,
    /// Archive new files instead of putting them in the inbox.
    pub automatically_archive: bool,
    /// Add files already in the client to the destinations too.
    pub destinations_for_already_in_db: bool,
    /// Archive files already in the client (with `automatically_archive`).
    pub archive_already_in_db: bool,
    /// Import JPEGs and PNGs of more than Pillow's pixel limit.
    pub allow_decompression_bombs: bool,
}

impl Default for FileImportOptions {
    fn default() -> Self {
        Self {
            exclude_deleted: true,
            allowed_mimes: None,
            min_size: None,
            max_size: None,
            max_gif_size: None,
            min_resolution: None,
            max_resolution: None,
            destinations: Vec::new(),
            automatically_archive: false,
            destinations_for_already_in_db: false,
            archive_already_in_db: true,
            allow_decompression_bombs: true,
        }
    }
}

impl FileImportOptions {
    /// The file filtering and location parts of a full set of import
    /// options. Destinations that no longer exist are left out.
    pub fn from_full(
        full: &hydrus_core::import_options::FullImportOptions,
        services: &ServiceRegistry,
    ) -> Self {
        let filtering = &full.file_filtering;
        let locations = &full.locations;
        let summary: BTreeSet<Mime> = filtering
            .filetypes
            .iter()
            .filter_map(|&code| Mime::from_code(code))
            .collect();
        let everything = summary.contains(&Mime::GeneralFile)
            || Mime::general_classes().iter().all(|c| summary.contains(c));
        let allowed_mimes = (!everything).then(|| {
            summary
                .iter()
                .flat_map(|&m| {
                    if m.is_general_class() {
                        Mime::members_of_class(m).collect::<Vec<_>>()
                    } else {
                        vec![m]
                    }
                })
                .collect()
        });
        let destinations = locations
            .destinations
            .iter()
            .filter_map(|key| hydrus_core::ServiceKey::from_hex(key).ok())
            .filter_map(|key| services.by_key(&key).ok().map(|s| s.id))
            .collect();
        Self {
            exclude_deleted: filtering.exclude_deleted,
            allowed_mimes,
            min_size: filtering.min_size,
            max_size: filtering.max_size,
            max_gif_size: filtering.max_gif_size,
            min_resolution: filtering.min_resolution,
            max_resolution: filtering.max_resolution,
            destinations,
            automatically_archive: locations.automatically_archive,
            destinations_for_already_in_db: locations.destinations_for_already_in_db,
            archive_already_in_db: locations.archive_already_in_db,
            allow_decompression_bombs: filtering.allow_decompression_bombs,
        }
    }

    /// The local file domains new files go to.
    pub fn destinations(&self, services: &ServiceRegistry) -> Vec<ServiceId> {
        if self.destinations.is_empty() {
            crate::local_domains(services).into_iter().take(1).collect()
        } else {
            self.destinations.clone()
        }
    }

    /// Whether a file passes the filtering rules; the error is the note the
    /// reference gives a vetoed file.
    pub fn check(&self, info: &hydrus_media::FileInfo) -> Result<(), String> {
        self.check_values(info.size, info.mime, info.width, info.height)
    }

    /// Whether the filtering rules let every file through whatever its
    /// size, type or resolution (`AllowsAllBasedOnFileInfo`).
    pub fn allows_all_based_on_file_info(&self) -> bool {
        self.allowed_mimes.is_none()
            && self.min_size.is_none()
            && self.max_size.is_none()
            && self.max_gif_size.is_none()
            && self.min_resolution.is_none()
            && self.max_resolution.is_none()
    }

    /// [`Self::check`] on what the client knows about a file.
    pub fn check_values(
        &self,
        size: u64,
        mime: Mime,
        width: Option<u32>,
        height: Option<u32>,
    ) -> Result<(), String> {
        let allowed = match &self.allowed_mimes {
            Some(set) => set.contains(&mime),
            None => hydrus_media::mimes::is_allowed(mime),
        };
        if !allowed {
            return Err(format!(
                "File was a {}, which is not allowed by the File Filtering Import Options.",
                mime.human_name()
            ));
        }
        if let Some(min) = self.min_size
            && size < min
        {
            return Err(format!(
                "File was {} but the lower limit in the File Filtering Import Options is {}.",
                human_bytes(size),
                human_bytes(min)
            ));
        }
        if let Some(max) = self.max_size
            && size > max
        {
            return Err(format!(
                "File was {} but the upper limit in the File Filtering Import Options is {}.",
                human_bytes(size),
                human_bytes(max)
            ));
        }
        if mime == Mime::AnimationGif
            && let Some(max) = self.max_gif_size
            && size > max
        {
            return Err(format!(
                "File was {} but the upper limit for gifs in the File Filtering Import Options is {}.",
                human_bytes(size),
                human_bytes(max)
            ));
        }
        if let Some((min_w, min_h)) = self.min_resolution
            && (width.is_some_and(|w| w < min_w) || height.is_some_and(|h| h < min_h))
        {
            return Err(format!(
                "File had resolution {} but the lower limit in the File Filtering Import Options is {}",
                resolution(width, height),
                resolution(Some(min_w), Some(min_h))
            ));
        }
        if let Some((max_w, max_h)) = self.max_resolution
            && (width.is_some_and(|w| w > max_w) || height.is_some_and(|h| h > max_h))
        {
            return Err(format!(
                "File had resolution {} but the upper limit in the File Filtering Import Options is {}",
                resolution(width, height),
                resolution(Some(max_w), Some(max_h))
            ));
        }
        Ok(())
    }
}

fn resolution(width: Option<u32>, height: Option<u32>) -> String {
    match (width, height) {
        (Some(w), Some(h)) => resolution_text(u64::from(w), u64::from(h)),
        _ => "no resolution".into(),
    }
}
