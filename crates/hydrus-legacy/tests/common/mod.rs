//! Shared helpers for the fixture tests.

#![allow(dead_code)]

use hydrus_legacy::{LegacyDb, OpenOptions, Rows};
use serde_json::Value;
use tempfile::TempDir;

/// An extracted fixture database, opened, with its reference expectations.
pub struct Fixture {
    pub dir: TempDir,
    pub db: LegacyDb,
    pub expected: Value,
}

/// Open the `basic` fixture. A tiny batch size makes every streamed table
/// span several batches, exercising the keyset pagination.
pub fn basic() -> Fixture {
    let dir = hydrus_testkit::legacy_fixture("basic");
    let db = LegacyDb::open_with(
        dir.path(),
        OpenOptions {
            batch_size: 7,
            ..OpenOptions::default()
        },
    )
    .expect("fixture opens");
    Fixture {
        dir,
        db,
        expected: expected(),
    }
}

pub fn expected() -> Value {
    hydrus_testkit::fixture_json("legacy_db/basic.expected.json")
}

/// Collect a streamed table, panicking on any row error.
pub fn collect<T>(rows: hydrus_legacy::Result<Rows<'_, T>>) -> Vec<T> {
    rows.expect("reader opens")
        .collect::<Result<Vec<_>, _>>()
        .expect("every row decodes")
}
