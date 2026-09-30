//! Integer identifiers for interned master data.
//!
//! Every high-cardinality entity (file hashes, tags, urls, ...) is interned to a
//! dense integer id. Ids are `u32`: they are allocated sequentially by SQLite and
//! even the largest known databases (synced to the public tag repository) are
//! orders of magnitude below `u32::MAX`. The narrow type halves memory for the
//! large id sets that search passes around, and lets us use 32-bit roaring
//! bitmaps directly.
//!
//! Distinct newtypes make it a compile error to pass a tag id where a hash id is
//! expected, which is a real class of bug in the untyped reference
//! implementation.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u32);

        impl $name {
            #[inline]
            pub const fn get(self) -> u32 {
                self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl From<$name> for u32 {
            #[inline]
            fn from(id: $name) -> u32 {
                id.0
            }
        }

        impl From<u32> for $name {
            #[inline]
            fn from(raw: u32) -> Self {
                Self(raw)
            }
        }

        #[cfg(feature = "sqlite")]
        impl rusqlite::ToSql for $name {
            #[inline]
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Integer(
                    i64::from(self.0),
                )))
            }
        }

        #[cfg(feature = "sqlite")]
        impl rusqlite::types::FromSql for $name {
            #[inline]
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                let raw = value.as_i64()?;
                u32::try_from(raw)
                    .map(Self)
                    .map_err(|_| rusqlite::types::FromSqlError::OutOfRange(raw))
            }
        }
    };
}

id_type!(
    /// A file, identified by its sha256 in the master hash table.
    HashId
);
id_type!(
    /// A full tag (`namespace:subtag` pair).
    TagId
);
id_type!(
    /// The namespace part of a tag. The empty namespace has an id too.
    NamespaceId
);
id_type!(
    /// The subtag part of a tag.
    SubtagId
);
id_type!(
    /// A service (tag domain, file domain, rating service, ...).
    ServiceId
);
id_type!(
    /// A URL.
    UrlId
);
id_type!(
    /// The domain part of a URL.
    UrlDomainId
);
id_type!(
    /// A free text string (petition reasons, deletion reasons, ...).
    TextId
);
id_type!(
    /// A note name.
    LabelId
);
id_type!(
    /// The body of a note.
    NoteId
);
id_type!(
    /// A perceptual hash, used for similar-file search.
    PerceptualHashId
);
