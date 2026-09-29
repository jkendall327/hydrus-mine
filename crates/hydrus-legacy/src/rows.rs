//! Streaming over big tables.
//!
//! Tables like the mappings can hold billions of rows, so readers never load
//! a whole table. [`Rows`] pages through a table in primary-key order with
//! keyset pagination (`WHERE (k1, k2) > (?, ?) ORDER BY k1, k2 LIMIT n`),
//! which SQLite answers with an index seek per batch. Each batch is a short
//! statement, so an iterator can be kept for as long as the caller likes
//! without holding a statement open; wrap a whole import in
//! [`crate::LegacyDb::snapshot`] to see one consistent state across batches.

use rusqlite::Row;
use rusqlite::types::Value;

use crate::db::LegacyDb;
use crate::error::{LegacyError, Result};

/// A row decoder: turns one SQLite row into a typed value.
pub(crate) type RowMapper<T> = Box<dyn Fn(&Row<'_>) -> Result<T>>;

/// A lazily-fetched, typed view of a table, in primary-key order.
///
/// Yields `Err` for a row that cannot be decoded and carries on with the
/// next one; a failing query yields one `Err` and ends the iteration.
pub struct Rows<'db, T> {
    db: &'db LegacyDb,
    table: String,
    first_sql: String,
    next_sql: String,
    key_len: usize,
    last_key: Option<Vec<i64>>,
    buffer: std::vec::IntoIter<Result<T>>,
    finished: bool,
    map: RowMapper<T>,
}

impl<T> std::fmt::Debug for Rows<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rows")
            .field("table", &self.table)
            .field("last_key", &self.last_key)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

/// The shape of a paged query.
pub(crate) struct Paging<'a> {
    /// `schema.table`, used in errors too.
    pub table: &'a str,
    /// Integer key expressions that uniquely order the table (the primary
    /// key, or `rowid`). They are selected first, in this order.
    pub keys: &'a [&'a str],
    /// Further columns, selected after the keys.
    pub columns: &'a [&'a str],
    /// Optional extra SQL for joins, placed after the table name.
    pub join: &'a str,
}

impl<'db, T> Rows<'db, T> {
    pub(crate) fn new(db: &'db LegacyDb, paging: &Paging<'_>, map: RowMapper<T>) -> Self {
        let select: Vec<&str> = paging.keys.iter().chain(paging.columns).copied().collect();
        let select = select.join(", ");
        let order = paging.keys.join(", ");
        let base = format!("SELECT {select} FROM {} {}", paging.table, paging.join);
        let placeholders: Vec<String> = (1..=paging.keys.len()).map(|i| format!("?{i}")).collect();
        let limit = paging.keys.len() + 1;
        let next_where = if paging.keys.len() == 1 {
            format!("{} > ?1", paging.keys[0])
        } else {
            format!("({order}) > ({})", placeholders.join(", "))
        };
        Rows {
            db,
            table: paging.table.to_owned(),
            first_sql: format!("{base} ORDER BY {order} LIMIT ?1"),
            next_sql: format!("{base} WHERE {next_where} ORDER BY {order} LIMIT ?{limit}"),
            key_len: paging.keys.len(),
            last_key: None,
            buffer: Vec::new().into_iter(),
            finished: false,
            map,
        }
    }

    fn fetch(&mut self) -> Result<()> {
        let batch_size = self.db.batch_size();
        let (sql, mut params) = match &self.last_key {
            None => (&self.first_sql, Vec::new()),
            Some(key) => (
                &self.next_sql,
                key.iter().map(|k| Value::Integer(*k)).collect::<Vec<_>>(),
            ),
        };
        params.push(Value::Integer(
            i64::try_from(batch_size).unwrap_or(i64::MAX),
        ));
        let mut statement = self.db.connection().prepare_cached(sql)?;
        let mut rows = statement.query(rusqlite::params_from_iter(params))?;
        let mut out = Vec::with_capacity(batch_size.min(65_536));
        while let Some(row) = rows.next()? {
            let key = (0..self.key_len)
                .map(|i| row.get::<_, i64>(i))
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| {
                    LegacyError::bad_value(&self.table, format!("non-integer primary key: {e}"))
                })?;
            self.last_key = Some(key);
            out.push((self.map)(row));
        }
        if out.len() < batch_size {
            self.finished = true;
        }
        self.buffer = out.into_iter();
        Ok(())
    }
}

impl<T> Iterator for Rows<'_, T> {
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(item) = self.buffer.next() {
                return Some(item);
            }
            if self.finished {
                return None;
            }
            if let Err(e) = self.fetch() {
                self.finished = true;
                return Some(Err(e));
            }
        }
    }
}
