//! Reference objects kept verbatim at migration (`legacy_objects`), for the
//! settings read straight from them.

use rusqlite::{Connection, OptionalExtension};

use crate::error::Result;

/// A singleton object of the reference's `json_dumps` table, by type: its
/// version and `info` JSON.
pub fn singleton(conn: &Connection, type_id: u32) -> Result<Option<(u32, String)>> {
    Ok(conn
        .query_row(
            "SELECT version, dump FROM legacy_objects WHERE source = 'json_dumps' AND type_id = ?",
            [type_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

/// The reference's old-style options (YAML).
pub fn old_options(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT dump FROM legacy_objects WHERE source = 'options'",
            [],
            |r| r.get(0),
        )
        .optional()?)
}
