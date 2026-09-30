//! What an import may bring in, and where it goes (the reference's file
//! filtering and location import options).

use std::collections::BTreeSet;

use hydrus_core::numbers::human_int;
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
        let mime = info.mime;
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
        let size = info.size;
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
        let (width, height) = (info.width, info.height);
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
        (Some(w), Some(h)) => format!("{}x{}", human_int(u64::from(w)), human_int(u64::from(h))),
        _ => "no resolution".into(),
    }
}

/// Sizes as the reference words them: `512B`, `237KB`, `1.5MB`.
pub fn human_bytes(size: u64) -> String {
    if size < 1024 {
        return format!("{}B", human_int(size));
    }
    let mut value = size as f64;
    let mut suffix = 0;
    while value >= 1024.0 && suffix < 5 {
        value /= 1024.0;
        suffix += 1;
    }
    // three significant figures, keeping every integer digit
    let decimals = if value >= 100.0 {
        0
    } else if value >= 10.0 {
        1
    } else {
        2
    };
    let text = format!("{value:.decimals$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    };
    format!("{text}{}B", ["", "K", "M", "G", "T", "P"][suffix])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_read_like_the_reference() {
        assert_eq!(human_bytes(512), "512B");
        assert_eq!(human_bytes(1024), "1KB");
        assert_eq!(human_bytes(1536), "1.5KB");
        assert_eq!(human_bytes(237 * 1024), "237KB");
        assert_eq!(human_bytes(10 * 1024 * 1024 + 300 * 1024), "10.3MB");
    }
}
