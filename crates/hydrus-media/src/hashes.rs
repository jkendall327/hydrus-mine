//! Content hashes, computed in one streaming pass.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use hydrus_core::{Md5, Sha1, Sha256, Sha512};
use sha2::Digest;

use crate::error::Result;

/// Every digest the database stores for a file.
///
/// `sha256` is the file's identity; the others exist so users can look files
/// up by hashes that other sites publish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileHashes {
    /// The primary identity of the file.
    pub sha256: Sha256,
    /// Legacy lookup hash.
    pub md5: Md5,
    /// Legacy lookup hash.
    pub sha1: Sha1,
    /// Legacy lookup hash.
    pub sha512: Sha512,
}

/// Incremental hasher for all four digests at once.
#[derive(Debug, Default, Clone)]
pub struct Hasher {
    sha256: sha2::Sha256,
    md5: md5::Md5,
    sha1: sha1::Sha1,
    sha512: sha2::Sha512,
}

impl Hasher {
    /// Start a new set of digests.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed more bytes.
    pub fn update(&mut self, data: &[u8]) {
        self.sha256.update(data);
        self.md5.update(data);
        self.sha1.update(data);
        self.sha512.update(data);
    }

    /// Finish and return the digests.
    pub fn finish(self) -> FileHashes {
        FileHashes {
            sha256: Sha256(self.sha256.finalize().into()),
            md5: Md5(self.md5.finalize().into()),
            sha1: Sha1(self.sha1.finalize().into()),
            sha512: Sha512(self.sha512.finalize().into()),
        }
    }
}

/// Hash a file on disk, reading it once in large blocks.
pub fn hash_file(path: &Path) -> Result<FileHashes> {
    let mut file = File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finish())
}

/// Hash an in-memory buffer.
pub fn hash_bytes(data: &[u8]) -> FileHashes {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finish()
}

/// sha256 of a byte buffer, as the typed hash.
pub(crate) fn sha256(data: &[u8]) -> Sha256 {
    Sha256(sha2::Sha256::digest(data).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        let h = hash_bytes(b"abc");
        assert_eq!(
            h.sha256.to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(h.md5.to_hex(), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(h.sha1.to_hex(), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert!(h.sha512.to_hex().starts_with("ddaf35a193617aba"));
    }

    #[test]
    fn streaming_matches_one_shot() {
        let data: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let mut hasher = Hasher::new();
        for chunk in data.chunks(4097) {
            hasher.update(chunk);
        }
        assert_eq!(hasher.finish(), hash_bytes(&data));
    }
}
