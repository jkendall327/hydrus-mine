//! Typed decoders for the serialised objects the importer and the Client
//! API need.
//!
//! Each decoder accepts every version of its object that can occur in a
//! v688 database and normalises it to v688's meaning, applying the
//! reference's `_UpdateSerialisableInfo` upgrades where an object might not
//! have been re-saved since an older version (these are all small field
//! additions with defaults). Objects newer than v688 are rejected.
//!
//! | object | type | versions accepted |
//! |---|---|---|
//! | [`ServiceConfig`] (service dictionaries) | 21 | 1, 2 |
//! | [`ClientApiManager`] / [`ApiPermissions`] | 75 / 76 | 1 / 1, 2 |
//! | [`TagFilter`] | 44 | 1 |
//! | [`TagDisplayManager`] / [`TagAutocompleteOptions`] | 79 / 85 | 1-4 / 1-5 |
//! | [`FavouriteSearchManager`] / [`FileSearchContext`] | 81 / 15 | 1 / 1-5 |
//! | [`MediaSort`], [`MediaCollect`], [`TagSort`] | 49, 78, 101 | 1-3, 1-2, 1 |
//! | [`LocationContext`], [`TagContext`] | 103, 80 | 1, 1-2 |
//! | [`ClientOptions`] | 22 | 8 (every v688 database; see its docs) |
//! | [`DuplicateMergeOptions`] / [`NoteImportOptions`] (in the client options) | 43 / 153 | 8 / 1 |
//! | [`domain::url_class_settings`] (domain manager: URL classes, parser links) | 53 | 7 (URL classes 15; see its docs) |
//! | [`LegacyOptions`] (the YAML `options` table) | — | — |

mod client_api;
mod client_options;
pub mod domain;
mod duplicates;
mod favourites;
mod legacy_options;
mod location;
mod services;
mod sort;
mod tag_display;
mod tag_filter;
pub(crate) mod util;

pub use client_api::{ApiPermission, ApiPermissions, ClientApiManager};
pub use client_options::ClientOptions;
pub use duplicates::{ArchiveSync, DuplicateMergeOptions, MergeAction, NoteImportOptions};
pub use favourites::{FavouriteSearch, FavouriteSearchManager, FileSearchContext, SearchType};
pub use legacy_options::{LegacyOptions, YamlValue};
pub use location::{LocationContext, TagContext};
pub use services::{
    ClientApiServiceConfig, Credentials, FractionPosition, IpfsConfig, LikeRatingConfig,
    NumericalRatingConfig, RatingColours, RatingDisplay, RatingState, RemoteConfig,
    RepositoryConfig, RepositoryMetadata, RepositoryUpdatePeriod, RestrictedConfig, Rgb,
    ServiceConfig, StarAppearance, StarShape, repository_metadata,
};
pub use sort::{MediaCollect, MediaSort, MediaSortType, SortOrder, TagSort};
pub use tag_display::{TagAutocompleteOptions, TagDisplayManager};
pub use tag_filter::{CompiledTagFilter, TagFilter, TagRule};
