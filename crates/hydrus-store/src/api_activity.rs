//! Timestamp-only Client API activity IPC, independent of the paused database.
use std::{
    io::{Read as _, Write as _},
    path::Path,
};

const FILE_NAME: &str = "client_api_activity";

/// Publish a request-start timestamp by replacing the complete marker atomically.
/// No request path, headers, access key or body is retained.
pub fn touch(dir: &Path, now_ms: i64) -> std::io::Result<()> {
    if now_ms < 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "negative activity timestamp",
        ));
    }
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(&now_ms.to_le_bytes())?;
    file.persist(dir.join(FILE_NAME))
        .map_err(|error| error.error)?;
    Ok(())
}

/// Read a complete marker without touching SQLite or its writer lock.
pub fn latest(dir: &Path) -> std::io::Result<Option<i64>> {
    let file = match std::fs::File::open(dir.join(FILE_NAME)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::with_capacity(9);
    file.take(9).read_to_end(&mut bytes)?;
    let bytes: [u8; 8] = bytes.try_into().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid activity timestamp",
        )
    })?;
    let value = i64::from_le_bytes(bytes);
    if value < 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "negative activity timestamp",
        ));
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn marker_replaces_atomically_reopens_and_rejects_malformed_data() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(latest(directory.path()).unwrap(), None);
        touch(directory.path(), 123).unwrap();
        assert_eq!(latest(directory.path()).unwrap(), Some(123));
        touch(directory.path(), 456).unwrap();
        assert_eq!(latest(directory.path()).unwrap(), Some(456));
        assert!(touch(directory.path(), -1).is_err());
        assert_eq!(latest(directory.path()).unwrap(), Some(456));
        for bytes in [vec![1], vec![1; 9], (-1_i64).to_le_bytes().to_vec()] {
            std::fs::write(directory.path().join(FILE_NAME), bytes).unwrap();
            assert!(latest(directory.path()).is_err());
        }
        touch(directory.path(), 789).unwrap();
        assert_eq!(latest(directory.path()).unwrap(), Some(789));
    }
}
