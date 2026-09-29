//! Interned master data: hashes, tags, urls and free text.
//!
//! Lookups (`*_id`, `*_ids`, reverse lookups) work on any connection.
//! Interning (`intern_*`) creates missing rows and so needs the writer. New
//! subtags also get their derived autocomplete rows (words, integer value)
//! here, which is the single place subtags are created.

use std::collections::HashMap;
use std::rc::Rc;

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params};

use hydrus_core::HashKind;
use hydrus_core::{
    HashId, LabelId, NamespaceId, NoteId, Sha256, SubtagId, Tag, TagId, TextId, UrlDomainId, UrlId,
};

use crate::error::{Result, StoreError};
use crate::text;

/// Pass a list of integers to SQL as `rarray(?)`.
pub(crate) fn id_array<I: Into<u32> + Copy>(ids: &[I]) -> Rc<Vec<Value>> {
    Rc::new(
        ids.iter()
            .map(|&id| Value::Integer(i64::from(id.into())))
            .collect(),
    )
}

fn blob_array(blobs: impl Iterator<Item = Vec<u8>>) -> Rc<Vec<Value>> {
    Rc::new(blobs.map(Value::Blob).collect())
}

fn text_array<'a>(texts: impl Iterator<Item = &'a str>) -> Rc<Vec<Value>> {
    Rc::new(texts.map(|t| Value::Text(t.to_owned())).collect())
}

// hashes ---------------------------------------------------------------------

pub fn hash_id(conn: &Connection, hash: &Sha256) -> Result<Option<HashId>> {
    Ok(conn
        .prepare_cached("SELECT hash_id FROM hashes WHERE sha256 = ?")?
        .query_row([hash], |r| r.get(0))
        .optional()?)
}

pub fn hash_ids(conn: &Connection, hashes: &[Sha256]) -> Result<HashMap<Sha256, HashId>> {
    let array = blob_array(hashes.iter().map(|h| h.0.to_vec()));
    let mut stmt =
        conn.prepare_cached("SELECT sha256, hash_id FROM hashes WHERE sha256 IN rarray(?)")?;
    let rows = stmt.query_map([array], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn intern_hash(conn: &Connection, hash: &Sha256) -> Result<HashId> {
    if let Some(id) = hash_id(conn, hash)? {
        return Ok(id);
    }
    conn.prepare_cached("INSERT INTO hashes (sha256) VALUES (?)")?
        .execute([hash])?;
    last_id(conn)
}

pub fn hash(conn: &Connection, id: HashId) -> Result<Option<Sha256>> {
    Ok(conn
        .prepare_cached("SELECT sha256 FROM hashes WHERE hash_id = ?")?
        .query_row([id], |r| r.get(0))
        .optional()?)
}

pub fn hashes(conn: &Connection, ids: &[HashId]) -> Result<HashMap<HashId, Sha256>> {
    let mut stmt =
        conn.prepare_cached("SELECT hash_id, sha256 FROM hashes WHERE hash_id IN rarray(?)")?;
    let rows = stmt.query_map([id_array(ids)], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Translate hashes of one kind to another (e.g. sha256 to md5) for files we
/// know both of. Unknown hashes are omitted.
pub fn convert_hashes(
    conn: &Connection,
    from: HashKind,
    to: HashKind,
    hashes: &[Vec<u8>],
) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let column = |kind: HashKind| match kind {
        HashKind::Sha256 => "h.sha256",
        HashKind::Md5 => "d.md5",
        HashKind::Sha1 => "d.sha1",
        HashKind::Sha512 => "d.sha512",
    };
    let (from_col, to_col) = (column(from), column(to));
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {from_col}, {to_col} FROM hashes h JOIN hash_digests d USING (hash_id)
         WHERE {from_col} IN rarray(?) AND {to_col} IS NOT NULL"
    ))?;
    let array = blob_array(hashes.iter().cloned());
    let found: HashMap<Vec<u8>, Vec<u8>> = stmt
        .query_map([array], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(hashes
        .iter()
        .filter_map(|h| found.get(h).map(|to| (h.clone(), to.clone())))
        .collect())
}

// tags -----------------------------------------------------------------------

pub fn namespace_id(conn: &Connection, namespace: &str) -> Result<Option<NamespaceId>> {
    Ok(conn
        .prepare_cached("SELECT namespace_id FROM namespaces WHERE namespace = ?")?
        .query_row([namespace], |r| r.get(0))
        .optional()?)
}

pub fn subtag_id(conn: &Connection, subtag: &str) -> Result<Option<SubtagId>> {
    Ok(conn
        .prepare_cached("SELECT subtag_id FROM subtags WHERE subtag = ?")?
        .query_row([subtag], |r| r.get(0))
        .optional()?)
}

pub fn tag_id(conn: &Connection, tag: &Tag) -> Result<Option<TagId>> {
    let (namespace, subtag) = tag.split();
    Ok(conn
        .prepare_cached(
            "SELECT tag_id FROM tags
             JOIN namespaces USING (namespace_id)
             JOIN subtags USING (subtag_id)
             WHERE namespace = ? AND subtag = ?",
        )?
        .query_row([namespace, subtag], |r| r.get(0))
        .optional()?)
}

pub fn tag_ids(conn: &Connection, tags: &[Tag]) -> Result<HashMap<Tag, TagId>> {
    let mut out = HashMap::with_capacity(tags.len());
    for tag in tags {
        if let Some(id) = tag_id(conn, tag)? {
            out.insert(tag.clone(), id);
        }
    }
    Ok(out)
}

pub fn intern_namespace(conn: &Connection, namespace: &str) -> Result<NamespaceId> {
    if let Some(id) = namespace_id(conn, namespace)? {
        return Ok(id);
    }
    conn.prepare_cached("INSERT INTO namespaces (namespace) VALUES (?)")?
        .execute([namespace])?;
    last_id(conn)
}

pub fn intern_subtag(conn: &Connection, subtag: &str) -> Result<SubtagId> {
    if let Some(id) = subtag_id(conn, subtag)? {
        return Ok(id);
    }
    conn.prepare_cached("INSERT INTO subtags (subtag) VALUES (?)")?
        .execute([subtag])?;
    let id: SubtagId = last_id(conn)?;
    index_subtag(conn, id, subtag)?;
    Ok(id)
}

/// Add a subtag's derived autocomplete rows.
pub(crate) fn index_subtag(conn: &Connection, id: SubtagId, subtag: &str) -> Result<()> {
    let mut insert_word = conn.prepare_cached(
        "INSERT OR IGNORE INTO cache_subtag_words (word, subtag_id) VALUES (?, ?)",
    )?;
    for word in text::words(&text::searchable_subtag(subtag)) {
        insert_word.execute(params![word, id])?;
    }
    if let Some(value) = text::integer_subtag(subtag) {
        conn.prepare_cached(
            "INSERT OR REPLACE INTO cache_integer_subtags (subtag_id, value) VALUES (?, ?)",
        )?
        .execute(params![id, value])?;
    }
    Ok(())
}

pub fn intern_tag(conn: &Connection, tag: &Tag) -> Result<TagId> {
    if let Some(id) = tag_id(conn, tag)? {
        return Ok(id);
    }
    let (namespace, subtag) = tag.split();
    let namespace_id = intern_namespace(conn, namespace)?;
    let subtag_id = intern_subtag(conn, subtag)?;
    conn.prepare_cached("INSERT INTO tags (namespace_id, subtag_id) VALUES (?, ?)")?
        .execute(params![namespace_id, subtag_id])?;
    last_id(conn)
}

pub fn tag(conn: &Connection, id: TagId) -> Result<Option<Tag>> {
    Ok(tags(conn, &[id])?.remove(&id))
}

pub fn tags(conn: &Connection, ids: &[TagId]) -> Result<HashMap<TagId, Tag>> {
    let mut stmt = conn.prepare_cached(
        "SELECT tag_id, namespace, subtag FROM tags
         JOIN namespaces USING (namespace_id)
         JOIN subtags USING (subtag_id)
         WHERE tag_id IN rarray(?)",
    )?;
    let rows = stmt.query_map([id_array(ids)], |r| {
        let namespace: String = r.get(1)?;
        let subtag: String = r.get(2)?;
        Ok((r.get(0)?, Tag::from_parts(&namespace, &subtag)))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// urls -----------------------------------------------------------------------

/// The domain the reference files a url under: Python's `urlparse(url).netloc`,
/// or `unknown.com` when the url has no scheme or no netloc.
pub fn url_domain(url: &str) -> &str {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once(':') else {
        return "unknown.com";
    };
    let scheme_ok = scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !scheme_ok {
        return "unknown.com";
    }
    let Some(after_slashes) = rest.strip_prefix("//") else {
        return "unknown.com";
    };
    let end = after_slashes
        .find(['/', '?', '#'])
        .unwrap_or(after_slashes.len());
    let netloc = &after_slashes[..end];
    if netloc.is_empty() {
        "unknown.com"
    } else {
        netloc
    }
}

pub fn url_id(conn: &Connection, url: &str) -> Result<Option<UrlId>> {
    Ok(conn
        .prepare_cached("SELECT url_id FROM urls WHERE url = ?")?
        .query_row([url], |r| r.get(0))
        .optional()?)
}

pub fn url_ids(conn: &Connection, urls: &[&str]) -> Result<HashMap<String, UrlId>> {
    let mut stmt = conn.prepare_cached("SELECT url, url_id FROM urls WHERE url IN rarray(?)")?;
    let rows = stmt.query_map([text_array(urls.iter().copied())], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn url_domain_id(conn: &Connection, domain: &str) -> Result<Option<UrlDomainId>> {
    Ok(conn
        .prepare_cached("SELECT domain_id FROM url_domains WHERE domain = ?")?
        .query_row([domain], |r| r.get(0))
        .optional()?)
}

pub fn intern_url_domain(conn: &Connection, domain: &str) -> Result<UrlDomainId> {
    if let Some(id) = url_domain_id(conn, domain)? {
        return Ok(id);
    }
    conn.prepare_cached("INSERT INTO url_domains (domain) VALUES (?)")?
        .execute([domain])?;
    last_id(conn)
}

pub fn intern_url(conn: &Connection, url: &str) -> Result<UrlId> {
    if let Some(id) = url_id(conn, url)? {
        return Ok(id);
    }
    let domain_id = intern_url_domain(conn, url_domain(url))?;
    conn.prepare_cached("INSERT INTO urls (domain_id, url) VALUES (?, ?)")?
        .execute(params![domain_id, url])?;
    last_id(conn)
}

pub fn urls(conn: &Connection, ids: &[UrlId]) -> Result<HashMap<UrlId, String>> {
    let mut stmt = conn.prepare_cached("SELECT url_id, url FROM urls WHERE url_id IN rarray(?)")?;
    let rows = stmt.query_map([id_array(ids)], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// free text ------------------------------------------------------------------

macro_rules! string_table {
    ($intern:ident, $lookup:ident, $reverse:ident, $id:ty, $table:literal, $id_col:literal, $col:literal) => {
        pub fn $lookup(conn: &Connection, value: &str) -> Result<Option<$id>> {
            Ok(conn
                .prepare_cached(concat!(
                    "SELECT ", $id_col, " FROM ", $table, " WHERE ", $col, " = ?"
                ))?
                .query_row([value], |r| r.get(0))
                .optional()?)
        }

        pub fn $intern(conn: &Connection, value: &str) -> Result<$id> {
            if let Some(id) = $lookup(conn, value)? {
                return Ok(id);
            }
            conn.prepare_cached(concat!("INSERT INTO ", $table, " (", $col, ") VALUES (?)"))?
                .execute([value])?;
            last_id(conn)
        }

        pub fn $reverse(conn: &Connection, ids: &[$id]) -> Result<HashMap<$id, String>> {
            let mut stmt = conn.prepare_cached(concat!(
                "SELECT ",
                $id_col,
                ", ",
                $col,
                " FROM ",
                $table,
                " WHERE ",
                $id_col,
                " IN rarray(?)"
            ))?;
            let rows = stmt.query_map([id_array(ids)], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        }
    };
}

string_table!(
    intern_text,
    text_id,
    texts,
    TextId,
    "texts",
    "text_id",
    "text"
);
string_table!(
    intern_label,
    label_id,
    labels,
    LabelId,
    "labels",
    "label_id",
    "label"
);
string_table!(
    intern_note,
    note_id,
    notes,
    NoteId,
    "notes",
    "note_id",
    "note"
);

fn last_id<I: From<u32>>(conn: &Connection) -> Result<I> {
    let raw = conn.last_insert_rowid();
    u32::try_from(raw)
        .map(I::from)
        .map_err(|_| StoreError::Corrupt(format!("row id {raw} out of range")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        schema::configure(&conn).unwrap();
        schema::migrate(&mut conn).unwrap();
        conn
    }

    #[test]
    fn interning_is_idempotent_and_reversible() {
        let c = conn();
        let tag = Tag::new("character:samus aran").unwrap();
        let id = intern_tag(&c, &tag).unwrap();
        assert_eq!(intern_tag(&c, &tag).unwrap(), id);
        assert_eq!(tag_id(&c, &tag).unwrap(), Some(id));
        assert_eq!(super::tag(&c, id).unwrap(), Some(tag));

        let colon = Tag::new(":)").unwrap();
        let colon_id = intern_tag(&c, &colon).unwrap();
        assert_eq!(super::tag(&c, colon_id).unwrap().unwrap().as_str(), "::)");

        let h = Sha256([7; 32]);
        let hid = intern_hash(&c, &h).unwrap();
        assert_eq!(hashes(&c, &[hid]).unwrap()[&hid], h);
        assert_eq!(hash_ids(&c, &[h, Sha256([8; 32])]).unwrap().len(), 1);
    }

    #[test]
    fn new_subtags_are_indexed_for_autocomplete() {
        let c = conn();
        intern_tag(&c, &Tag::new("blue_eyes").unwrap()).unwrap();
        intern_tag(&c, &Tag::new("page:0012").unwrap()).unwrap();
        let words: Vec<String> = c
            .prepare("SELECT word FROM cache_subtag_words ORDER BY word")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(words, ["0012", "blue", "eyes"]);
        let value: i64 = c
            .query_row("SELECT value FROM cache_integer_subtags", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, 12);
    }

    #[test]
    fn url_domains_match_urlparse_netloc() {
        assert_eq!(
            url_domain("https://danbooru.donmai.us/posts/1"),
            "danbooru.donmai.us"
        );
        assert_eq!(
            url_domain("https://user:pw@example.com:8080/x?y#z"),
            "user:pw@example.com:8080"
        );
        assert_eq!(url_domain("https://example.com?q"), "example.com");
        assert_eq!(url_domain("example.com/x"), "unknown.com");
        assert_eq!(url_domain("not a url"), "unknown.com");
        assert_eq!(url_domain("file:///tmp/x"), "unknown.com");
    }
}
