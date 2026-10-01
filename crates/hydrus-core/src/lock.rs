//! The client's lock password (the old options' `password`): the sha256 of
//! a password the desktop client asks for before it opens.

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LockPassword {
    /// The sha256 of the password's UTF-8, in hex; none is no lock.
    pub sha256: Option<String>,
}

impl LockPassword {
    /// A lock with this password.
    pub fn new(password: &str) -> Self {
        Self {
            sha256: Some(hex::encode(sha2::Sha256::digest(password.as_bytes()))),
        }
    }

    pub fn is_set(&self) -> bool {
        self.sha256.is_some()
    }

    /// Whether `password` opens the client: always, with no lock.
    pub fn accepts(&self, password: &str) -> bool {
        self.sha256.as_ref().is_none_or(|want| {
            hex::encode(sha2::Sha256::digest(password.as_bytes())).eq_ignore_ascii_case(want)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_password_opens_a_lock() {
        assert!(LockPassword::default().accepts("anything"));
        let lock = LockPassword::new("hunter2");
        // (hashlib.sha256(b'hunter2').hexdigest())
        assert_eq!(
            lock.sha256.as_deref(),
            Some("f52fbd32b2b3b86ff88ef6c490628285f482af15ddcb29541f94bcf526a3f6c7")
        );
        assert!(lock.accepts("hunter2"));
        assert!(!lock.accepts("hunter3"));
        assert!(!lock.accepts(""));
    }
}
