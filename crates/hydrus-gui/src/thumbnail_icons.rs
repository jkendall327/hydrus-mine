//! The icons the reference draws over a thumbnail (`_PaintThumbnailContent`):
//! at the top right, downloading, notes, the trash and the inbox, from the
//! right; at the top left, sound or play, then the file repositories and
//! IPFS the file is in, pending to or petitioned from; and a collection's
//! icon at the bottom left, beside its number of files. Each is 16 pixels
//! square, a pixel in from the border and two apart.

use std::collections::BTreeSet;

use hydrus_core::mime::Mime;
use hydrus_core::service::{ServiceType, builtin_keys};
use hydrus_core::{HashId, ServiceId};
use hydrus_store::media::MediaResult;
use hydrus_store::services::ServiceRegistry;

/// Every icon's width and height.
pub const SIZE: i32 = 16;
/// The space between an icon and the border, and half that between two.
const MARGIN: i32 = 1;

/// An icon, by the reference's name for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Inbox,
    Trash,
    Notes,
    Downloading,
    Sound,
    Play,
    FileRepository,
    FileRepositoryPending,
    FileRepositoryPetitioned,
    Ipfs,
    IpfsPending,
    IpfsPetitioned,
    Collection,
}

impl Icon {
    /// The reference's name (its pixmap's).
    pub fn name(self) -> &'static str {
        match self {
            Icon::Inbox => "inbox",
            Icon::Trash => "trash",
            Icon::Notes => "notes",
            Icon::Downloading => "downloading",
            Icon::Sound => "sound",
            Icon::Play => "play",
            Icon::FileRepository => "file_repository",
            Icon::FileRepositoryPending => "file_repository_pending",
            Icon::FileRepositoryPetitioned => "file_repository_petitioned",
            Icon::Ipfs => "ipfs",
            Icon::IpfsPending => "ipfs_pending",
            Icon::IpfsPetitioned => "ipfs_petitioned",
            Icon::Collection => "collection",
        }
    }

    /// The number the grid draws it by.
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// What a thumbnail's icons say of its file (or a collection's files).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IconFacts {
    pub inbox: bool,
    pub notes: bool,
    /// In the trash, or deleted from hydrus local file storage.
    pub trashed: bool,
    /// Pending to hydrus local file storage.
    pub downloading: bool,
    pub audio: bool,
    /// A duration, or (an ugoira of several frames) a simulated one.
    pub duration: bool,
    /// The remote file services (file repositories, IPFS) it is in, pending
    /// to and petitioned from, each with whether it is IPFS.
    pub remote_current: BTreeSet<(bool, ServiceId)>,
    pub remote_pending: BTreeSet<(bool, ServiceId)>,
    pub remote_petitioned: BTreeSet<(bool, ServiceId)>,
}

impl IconFacts {
    /// A file's (`MediaSingle`'s).
    pub fn of(media: &MediaResult, services: &ServiceRegistry) -> Self {
        let id_of = |key: &[u8]| services.builtin(key).ok().map(|s| s.id);
        let trash = id_of(builtin_keys::TRASH);
        let storage = id_of(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE);
        let remote = |id: &ServiceId| -> Option<(bool, ServiceId)> {
            match services.get(*id).ok()?.service_type() {
                ServiceType::FileRepository => Some((false, *id)),
                ServiceType::Ipfs => Some((true, *id)),
                _ => None,
            }
        };
        let info = media.info.as_ref();
        let has_duration = info.and_then(|i| i.duration_ms).is_some_and(|d| d > 0);
        let simulated = info.is_some_and(|i| {
            i.mime == Mime::AnimationUgoira && i.num_frames.is_some_and(|n| n > 1)
        });
        Self {
            inbox: media.inbox,
            notes: !media.notes.is_empty(),
            trashed: media.current.iter().any(|c| Some(c.service) == trash)
                || media.deleted.iter().any(|d| Some(d.service) == storage),
            downloading: media.pending.iter().any(|p| Some(*p) == storage),
            audio: info.is_some_and(|i| i.has_audio),
            duration: has_duration || simulated,
            remote_current: media
                .current
                .iter()
                .filter_map(|c| remote(&c.service))
                .collect(),
            remote_pending: media.pending.iter().filter_map(remote).collect(),
            remote_petitioned: media.petitioned.iter().filter_map(remote).collect(),
        }
    }

    /// A collection's (`MediaCollection`'s): any of its files' inbox,
    /// notes, audio and duration, and its files' locations together.
    pub fn of_collection(members: &[IconFacts]) -> Self {
        let any = |f: fn(&IconFacts) -> bool| members.iter().any(f);
        let union = |f: fn(&IconFacts) -> &BTreeSet<(bool, ServiceId)>| {
            members.iter().flat_map(|m| f(m).iter().copied()).collect()
        };
        Self {
            inbox: any(|m| m.inbox),
            notes: any(|m| m.notes),
            trashed: any(|m| m.trashed),
            downloading: any(|m| m.downloading),
            audio: any(|m| m.audio),
            duration: any(|m| m.duration),
            remote_current: union(|m| &m.remote_current),
            remote_pending: union(|m| &m.remote_pending),
            remote_petitioned: union(|m| &m.remote_petitioned),
        }
    }
}

/// An icon, and where its top left corner goes in the thumbnail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub icon: Icon,
    pub x: i32,
    pub y: i32,
}

/// The icons over a thumbnail `width` by `height` (its border included,
/// `border` wide), a collection's if `collection`, where the reference
/// draws them (its count no taller than the icon). The top right icons
/// start under any ratings, at `top_right_y` (the border, with none).
pub fn placed(
    facts: &IconFacts,
    collection: bool,
    border: i32,
    width: i32,
    height: i32,
    top_right_y: i32,
) -> Vec<Placed> {
    let mut out = Vec::new();
    // the top right, from the right
    let mut top_right = Vec::new();
    if facts.downloading {
        top_right.push(Icon::Downloading);
    }
    if facts.notes {
        top_right.push(Icon::Notes);
    }
    if facts.trashed {
        top_right.push(Icon::Trash);
    }
    if facts.inbox {
        top_right.push(Icon::Inbox);
    }
    let mut x = -(border + MARGIN);
    for icon in top_right {
        x -= SIZE;
        out.push(Placed {
            icon,
            x: width + x,
            y: top_right_y,
        });
        x -= 2 * MARGIN;
    }
    // a collection's, at the bottom left
    if collection {
        out.push(Placed {
            icon: Icon::Collection,
            x: border + MARGIN,
            y: height - border - MARGIN - SIZE,
        });
    }
    // the top left
    let mut top_left = Vec::new();
    if facts.audio {
        top_left.push(Icon::Sound);
    } else if facts.duration {
        top_left.push(Icon::Play);
    }
    let petitioned = &facts.remote_petitioned;
    let shown: BTreeSet<(bool, ServiceId)> = facts
        .remote_current
        .difference(petitioned)
        .copied()
        .collect();
    for (set, repository, ipfs) in [
        (&shown, Icon::FileRepository, Icon::Ipfs),
        (
            &facts.remote_pending,
            Icon::FileRepositoryPending,
            Icon::IpfsPending,
        ),
        (
            petitioned,
            Icon::FileRepositoryPetitioned,
            Icon::IpfsPetitioned,
        ),
    ] {
        if set.iter().any(|(is_ipfs, _)| !is_ipfs) {
            top_left.push(repository);
        }
        if set.iter().any(|(is_ipfs, _)| *is_ipfs) {
            top_left.push(ipfs);
        }
    }
    let mut x = border + MARGIN;
    for icon in top_left {
        out.push(Placed {
            icon,
            x,
            y: border + MARGIN,
        });
        x += SIZE + 2 * MARGIN;
    }
    out
}

/// Each file's icon facts, read from the store.
pub fn facts(
    store: &hydrus_store::Store,
    files: &[HashId],
) -> std::collections::HashMap<HashId, IconFacts> {
    let snapshot = store.snapshot();
    store
        .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, files))
        .map(|batch| {
            batch
                .results
                .iter()
                .map(|m| (m.hash_id, IconFacts::of(m, &snapshot.services)))
                .collect()
        })
        .unwrap_or_default()
}
