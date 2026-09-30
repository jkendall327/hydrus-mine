//! Content hashes.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Error parsing a hex hash.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum HashParseError {
    #[error("hash is not valid hex: {0}")]
    InvalidHex(#[from] hex::FromHexError),
    #[error("hash should be {expected} bytes, but was {actual}")]
    WrongLength { expected: usize, actual: usize },
}

macro_rules! fixed_hash {
    ($(#[$meta:meta])* $name:ident, $len:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub [u8; $len]);

        impl $name {
            pub const LEN: usize = $len;

            pub fn from_slice(bytes: &[u8]) -> Result<Self, HashParseError> {
                <[u8; $len]>::try_from(bytes).map(Self).map_err(|_| {
                    HashParseError::WrongLength { expected: $len, actual: bytes.len() }
                })
            }

            #[inline]
            pub fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }

            pub fn to_hex(&self) -> String {
                hex::encode(self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in &self.0 {
                    write!(f, "{byte:02x}")?;
                }
                Ok(())
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self)
            }
        }

        impl FromStr for $name {
            type Err = HashParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let mut out = [0u8; $len];
                if s.len() != $len * 2 {
                    return Err(HashParseError::WrongLength {
                        expected: $len,
                        actual: s.len() / 2,
                    });
                }
                hex::decode_to_slice(s, &mut out)?;
                Ok(Self(out))
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.to_hex())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }

        #[cfg(feature = "sqlite")]
        impl rusqlite::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(rusqlite::types::ToSqlOutput::Borrowed(rusqlite::types::ValueRef::Blob(
                    &self.0,
                )))
            }
        }

        #[cfg(feature = "sqlite")]
        impl rusqlite::types::FromSql for $name {
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                let bytes = value.as_blob()?;
                Self::from_slice(bytes).map_err(|_| rusqlite::types::FromSqlError::InvalidBlobSize {
                    expected_size: $len,
                    blob_size: bytes.len(),
                })
            }
        }
    };
}

fixed_hash!(
    /// A sha256 digest. This is the primary identity of every file.
    Sha256,
    32
);
fixed_hash!(
    /// An md5 digest, kept for looking files up by legacy hashes.
    Md5,
    16
);
fixed_hash!(
    /// A sha1 digest, kept for looking files up by legacy hashes.
    Sha1,
    20
);
fixed_hash!(
    /// A sha512 digest, kept for looking files up by legacy hashes.
    Sha512,
    64
);
fixed_hash!(
    /// A 64-bit DCT perceptual hash of an image, for similar-file search.
    PerceptualHash,
    8
);

impl PerceptualHash {
    /// Hamming distance between two perceptual hashes.
    #[inline]
    pub fn distance(&self, other: &Self) -> u32 {
        (u64::from_be_bytes(self.0) ^ u64::from_be_bytes(other.0)).count_ones()
    }
}

/// Which digest algorithm a hash is, for lookups by non-sha256 hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HashKind {
    Sha256,
    Md5,
    Sha1,
    Sha512,
}

impl HashKind {
    pub const fn byte_len(self) -> usize {
        match self {
            HashKind::Sha256 => 32,
            HashKind::Md5 => 16,
            HashKind::Sha1 => 20,
            HashKind::Sha512 => 64,
        }
    }
}

impl FromStr for HashKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sha256" => Ok(HashKind::Sha256),
            "md5" => Ok(HashKind::Md5),
            "sha1" => Ok(HashKind::Sha1),
            "sha512" => Ok(HashKind::Sha512),
            other => Err(format!("unknown hash type \"{other}\"")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let hex = "6c0ef5bba1e8e8b8b51e6a4bbcde7f2e8a2c1f2e5c0e2c3b2a1f0e9d8c7b6a59";
        let h: Sha256 = hex.parse().unwrap();
        assert_eq!(h.to_string(), hex);
        assert_eq!(serde_json::to_string(&h).unwrap(), format!("\"{hex}\""));
    }

    #[test]
    fn rejects_bad_hashes() {
        assert!("abc".parse::<Sha256>().is_err());
        assert!("zz".repeat(32).parse::<Sha256>().is_err());
        assert!(Sha256::from_slice(&[0; 31]).is_err());
    }

    #[test]
    fn perceptual_distance() {
        let a = PerceptualHash([0; 8]);
        let b = PerceptualHash([0xff, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(a.distance(&b), 9);
        assert_eq!(b.distance(&b), 0);
    }
}
