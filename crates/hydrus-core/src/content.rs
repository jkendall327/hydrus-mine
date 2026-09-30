//! Enumerations describing content and changes to it.

use serde::{Deserialize, Serialize};

macro_rules! int_enum {
    ($(#[$meta:meta])* $name:ident { $( $variant:ident = $code:literal ),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum $name {
            $( $variant = $code, )*
        }

        impl $name {
            pub const ALL: &'static [$name] = &[ $( $name::$variant, )* ];

            pub const fn from_code(code: u8) -> Option<$name> {
                match code {
                    $( $code => Some($name::$variant), )*
                    _ => None,
                }
            }

            pub const fn code(self) -> u8 {
                self as u8
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_u8(self.code())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let code = u8::deserialize(d)?;
                $name::from_code(code).ok_or_else(|| {
                    serde::de::Error::custom(format!(concat!("unknown ", stringify!($name), " {}"), code))
                })
            }
        }
    };
}

int_enum!(
    /// The lifecycle status of a piece of content in a service.
    ContentStatus {
        Current = 0,
        Pending = 1,
        Deleted = 2,
        Petitioned = 3,
    }
);

int_enum!(
    /// What kind of content something is.
    ContentType {
        Mappings = 0,
        TagSiblings = 1,
        TagParents = 2,
        Files = 3,
        Ratings = 4,
        Mapping = 5,
        Directories = 6,
        Urls = 7,
        Veto = 8,
        Accounts = 9,
        Options = 10,
        Services = 11,
        Unknown = 12,
        AccountTypes = 13,
        Variable = 14,
        Hash = 15,
        Timestamp = 16,
        Title = 17,
        Notes = 18,
        FileViewingStats = 19,
        Tag = 20,
        Definitions = 21,
        HttpHeaders = 22,
    }
);

int_enum!(
    /// An action applied to content.
    ContentUpdateAction {
        Add = 0,
        Delete = 1,
        Pend = 2,
        RescindPend = 3,
        Petition = 4,
        RescindPetition = 5,
        EditLog = 6,
        Archive = 7,
        Inbox = 8,
        Rating = 9,
        DenyPend = 11,
        DenyPetition = 12,
        Advanced = 13,
        Undelete = 14,
        Set = 15,
        Flip = 16,
        ClearDeleteRecord = 17,
        Increment = 18,
        Decrement = 19,
        Move = 20,
        DeleteFromSourceAfterMigrate = 21,
        MoveMerge = 22,
    }
);

int_enum!(
    /// Relationship between two files in the duplicates system.
    DuplicateType {
        Potential = 0,
        FalsePositive = 1,
        SameQuality = 2,
        Alternate = 3,
        Better = 4,
        SmallerBetter = 5,
        LargerBetter = 6,
        Worse = 7,
        Member = 8,
        King = 9,
        ConfirmedAlternate = 10,
    }
);

int_enum!(
    /// The kinds of timestamp a file can carry.
    TimestampType {
        ModifiedDomain = 0,
        ModifiedFile = 1,
        ModifiedAggregate = 2,
        Imported = 3,
        Deleted = 4,
        Archived = 5,
        LastViewed = 6,
        PreviouslyImported = 7,
    }
);

int_enum!(
    /// The media viewers whose viewing statistics are tracked separately.
    CanvasType {
        MediaViewer = 0,
        Preview = 1,
        DuplicatesFilter = 2,
        ArchiveDeleteFilter = 3,
        ClientApi = 4,
        Dialog = 5,
    }
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::constants;

    fn check(table: &str, codes: impl Iterator<Item = u8>) {
        let fixture = constants();
        let mut expected: Vec<u8> = fixture[table]
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.parse().unwrap())
            .collect();
        expected.sort_unstable();
        let mut actual: Vec<u8> = codes.collect();
        actual.sort_unstable();
        assert_eq!(actual, expected, "{table}");
    }

    #[test]
    fn codes_match_reference_implementation() {
        check(
            "content_statuses",
            ContentStatus::ALL.iter().map(|c| c.code()),
        );
        check("content_types", ContentType::ALL.iter().map(|c| c.code()));
        // The reference string table omits a couple of actions, so compare
        // against the integer constants themselves.
        let ints = constants()["int_constants"].as_object().unwrap().clone();
        let mut expected: Vec<u8> = ints
            .iter()
            .filter(|(name, _)| name.starts_with("CONTENT_UPDATE_"))
            .map(|(_, v)| u8::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
        expected.sort_unstable();
        let actual: Vec<u8> = ContentUpdateAction::ALL.iter().map(|c| c.code()).collect();
        assert_eq!(actual, expected, "content update actions");
        check(
            "duplicate_types",
            DuplicateType::ALL.iter().map(|c| c.code()),
        );
    }

    #[test]
    fn canvas_codes_match_reference_implementation() {
        let ints = &constants()["client_int_constants"];
        for (name, variant) in [
            ("CANVAS_MEDIA_VIEWER", CanvasType::MediaViewer),
            ("CANVAS_PREVIEW", CanvasType::Preview),
            (
                "CANVAS_MEDIA_VIEWER_DUPLICATES",
                CanvasType::DuplicatesFilter,
            ),
            (
                "CANVAS_MEDIA_VIEWER_ARCHIVE_DELETE",
                CanvasType::ArchiveDeleteFilter,
            ),
            ("CANVAS_CLIENT_API", CanvasType::ClientApi),
            ("CANVAS_DIALOG", CanvasType::Dialog),
        ] {
            assert_eq!(
                ints[name].as_u64(),
                Some(u64::from(variant.code())),
                "{name}"
            );
        }
    }

    #[test]
    fn timestamp_codes_match_reference_implementation() {
        let ints = &constants()["int_constants"];
        for (name, variant) in [
            (
                "TIMESTAMP_TYPE_MODIFIED_DOMAIN",
                TimestampType::ModifiedDomain,
            ),
            (
                "TIMESTAMP_TYPE_MODIFIED_AGGREGATE",
                TimestampType::ModifiedAggregate,
            ),
            ("TIMESTAMP_TYPE_MODIFIED_FILE", TimestampType::ModifiedFile),
            ("TIMESTAMP_TYPE_IMPORTED", TimestampType::Imported),
            ("TIMESTAMP_TYPE_DELETED", TimestampType::Deleted),
            ("TIMESTAMP_TYPE_ARCHIVED", TimestampType::Archived),
            ("TIMESTAMP_TYPE_LAST_VIEWED", TimestampType::LastViewed),
            (
                "TIMESTAMP_TYPE_PREVIOUSLY_IMPORTED",
                TimestampType::PreviouslyImported,
            ),
        ] {
            assert_eq!(
                ints[name].as_u64(),
                Some(u64::from(variant.code())),
                "{name}"
            );
        }
    }
}
