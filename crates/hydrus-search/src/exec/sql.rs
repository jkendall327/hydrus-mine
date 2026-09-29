//! Small helpers for moving id sets between SQLite and roaring bitmaps.
//!
//! Candidate sets go to SQLite as `rarray(?)` parameters, in chunks so a
//! probe over a million files never builds one giant array.

use std::rc::Rc;

use roaring::RoaringBitmap;
use rusqlite::types::Value;
use rusqlite::{Connection, Params, Statement};

use super::Result;

/// How many ids one `rarray` parameter carries when probing.
pub(crate) const CHUNK: usize = 16_384;

/// Probing a candidate costs roughly this many times as much as reading one
/// row from an index scan (the reference measured 2.5, plus 0.1 overhead).
pub(crate) const PROBE_COST: f64 = 2.6;

/// A list of integers as an `rarray` parameter.
pub(crate) fn int_array(ids: impl IntoIterator<Item = u32>) -> Rc<Vec<Value>> {
    Rc::new(
        ids.into_iter()
            .map(|id| Value::Integer(i64::from(id)))
            .collect(),
    )
}

/// Read a hash id column value.
pub(crate) fn hash_id(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u32> {
    row.get::<_, u32>(index)
}

/// Run a query whose first column is a hash id and collect the ids.
pub(crate) fn collect(stmt: &mut Statement<'_>, params: impl Params) -> Result<RoaringBitmap> {
    let mut out = RoaringBitmap::new();
    let mut rows = stmt.query(params)?;
    while let Some(row) = rows.next()? {
        out.insert(hash_id(row, 0)?);
    }
    Ok(out)
}

/// Call `f` with each chunk of `ids` as an `rarray` parameter.
pub(crate) fn for_each_chunk(
    ids: &RoaringBitmap,
    mut f: impl FnMut(Rc<Vec<Value>>) -> Result<()>,
) -> Result<()> {
    let mut chunk = Vec::with_capacity(CHUNK.min(ids.len() as usize));
    for id in ids {
        chunk.push(Value::Integer(i64::from(id)));
        if chunk.len() == CHUNK {
            f(Rc::new(std::mem::take(&mut chunk)))?;
        }
    }
    if !chunk.is_empty() {
        f(Rc::new(chunk))?;
    }
    Ok(())
}

/// Run `sql`, whose first parameter is an `rarray` of hash ids and whose
/// first column is a hash id, once per chunk of `within`, collecting ids.
/// `rest` are the remaining parameters (`?2`, `?3`, ...).
pub(crate) fn probe(
    conn: &Connection,
    sql: &str,
    within: &RoaringBitmap,
    rest: &[&dyn rusqlite::ToSql],
) -> Result<RoaringBitmap> {
    let mut stmt = conn.prepare_cached(sql)?;
    let mut out = RoaringBitmap::new();
    for_each_chunk(within, |chunk| {
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(rest.len() + 1);
        params.push(&chunk);
        params.extend_from_slice(rest);
        out |= collect(&mut stmt, params.as_slice())?;
        Ok(())
    })?;
    Ok(out)
}

/// The number of rows in a table as of the last `ANALYZE`, if known. Cheap;
/// used only to choose between plans.
pub(crate) fn table_rows(conn: &Connection, table: &str) -> Option<u64> {
    let stat: Option<String> = conn
        .prepare_cached("SELECT stat FROM sqlite_stat1 WHERE tbl = ? LIMIT 1")
        .and_then(|mut s| {
            let mut rows = s.query([table])?;
            rows.next()?.map(|r| r.get(0)).transpose()
        })
        .unwrap_or(None);
    stat.and_then(|s| s.split_whitespace().next()?.parse().ok())
}

/// Whether probing `candidates` files is expected to be cheaper than
/// scanning an index that yields `scan_rows` rows.
pub(crate) fn probe_is_cheaper(candidates: u64, scan_rows: Option<u64>) -> bool {
    match scan_rows {
        Some(rows) => (candidates as f64) * PROBE_COST < rows as f64,
        // Unknown scan size: probe modest candidate sets, as the reference does.
        None => candidates < 50_000,
    }
}
