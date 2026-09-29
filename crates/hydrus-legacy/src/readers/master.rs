//! Master (interned definition) tables in `client.master.db`.
//!
//! Every hash, tag, URL and piece of text is interned to an integer id once
//! and referenced by id everywhere else. The importer keeps the ids.

use hydrus_core::{
    HashId, LabelId, Md5, NamespaceId, NoteId, PerceptualHash, PerceptualHashId, Sha1, Sha256,
    Sha512, SubtagId, Tag, TagId, TextId, UrlDomainId, UrlId,
};
use rusqlite::Row;
use rusqlite::types::FromSql;

use super::column;
use crate::db::LegacyDb;
use crate::error::Result;
use crate::rows::{Paging, Rows};

/// The legacy digests of a local file (`local_hashes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHashes {
    pub hash_id: HashId,
    pub md5: Option<Md5>,
    pub sha1: Option<Sha1>,
    pub sha512: Option<Sha512>,
}

/// A tag definition (`tags`): a namespace and subtag pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagDefinition {
    pub tag_id: TagId,
    pub namespace_id: NamespaceId,
    pub subtag_id: SubtagId,
}

/// A URL definition (`urls`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlDefinition {
    pub url_id: UrlId,
    pub domain_id: UrlDomainId,
    pub url: String,
}

fn id_and<I: FromSql + 'static, V: FromSql + 'static>(
    table: &'static str,
) -> impl Fn(&Row<'_>) -> Result<(I, V)> {
    move |row| Ok((column(row, 0, table)?, column(row, 1, table)?))
}

impl LegacyDb {
    fn id_value_rows<I: FromSql + 'static, V: FromSql + 'static>(
        &self,
        table: &'static str,
        id: &'static str,
        value: &'static str,
    ) -> Result<Rows<'_, (I, V)>> {
        let qualified = self.table("external_master", table)?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &qualified,
                keys: &[id],
                columns: &[value],
                join: "",
            },
            Box::new(id_and(table)),
        ))
    }

    /// Every sha256 the client knows (`hashes`): local files, remote files,
    /// repository update files and pixel hashes alike.
    pub fn hashes(&self) -> Result<Rows<'_, (HashId, Sha256)>> {
        self.id_value_rows("hashes", "hash_id", "hash")
    }

    /// md5/sha1/sha512 of local files (`local_hashes`).
    pub fn local_hashes(&self) -> Result<Rows<'_, LocalHashes>> {
        let table = self.table("external_master", "local_hashes")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id"],
                columns: &["md5", "sha1", "sha512"],
                join: "",
            },
            Box::new(|row| {
                Ok(LocalHashes {
                    hash_id: column(row, 0, "local_hashes")?,
                    md5: column(row, 1, "local_hashes")?,
                    sha1: column(row, 2, "local_hashes")?,
                    sha512: column(row, 3, "local_hashes")?,
                })
            }),
        ))
    }

    pub fn namespaces(&self) -> Result<Rows<'_, (NamespaceId, String)>> {
        self.id_value_rows("namespaces", "namespace_id", "namespace")
    }

    pub fn subtags(&self) -> Result<Rows<'_, (SubtagId, String)>> {
        self.id_value_rows("subtags", "subtag_id", "subtag")
    }

    /// Tag definitions (`tags`).
    pub fn tag_definitions(&self) -> Result<Rows<'_, TagDefinition>> {
        let table = self.table("external_master", "tags")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["tag_id"],
                columns: &["namespace_id", "subtag_id"],
                join: "",
            },
            Box::new(|row| {
                Ok(TagDefinition {
                    tag_id: column(row, 0, "tags")?,
                    namespace_id: column(row, 1, "tags")?,
                    subtag_id: column(row, 2, "tags")?,
                })
            }),
        ))
    }

    /// Every tag with its text, joined from `tags`, `namespaces` and `subtags`.
    pub fn tags(&self) -> Result<Rows<'_, (TagId, Tag)>> {
        let table = self.table("external_master", "tags")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &format!("{table} AS t"),
                keys: &["t.tag_id"],
                columns: &["n.namespace", "s.subtag"],
                // LEFT JOIN: a dangling definition is an error, not a missing row
                join: "LEFT JOIN external_master.namespaces AS n USING (namespace_id) \
                       LEFT JOIN external_master.subtags AS s USING (subtag_id)",
            },
            Box::new(|row| {
                let namespace: String = column(row, 1, "tags")?;
                let subtag: String = column(row, 2, "tags")?;
                Ok((
                    column(row, 0, "tags")?,
                    Tag::from_parts(&namespace, &subtag),
                ))
            }),
        ))
    }

    pub fn url_domains(&self) -> Result<Rows<'_, (UrlDomainId, String)>> {
        self.id_value_rows("url_domains", "domain_id", "domain")
    }

    pub fn urls(&self) -> Result<Rows<'_, UrlDefinition>> {
        let table = self.table("external_master", "urls")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["url_id"],
                columns: &["domain_id", "url"],
                join: "",
            },
            Box::new(|row| {
                Ok(UrlDefinition {
                    url_id: column(row, 0, "urls")?,
                    domain_id: column(row, 1, "urls")?,
                    url: column(row, 2, "urls")?,
                })
            }),
        ))
    }

    /// Free texts: deletion and petition reasons (`texts`).
    pub fn texts(&self) -> Result<Rows<'_, (TextId, String)>> {
        self.id_value_rows("texts", "text_id", "text")
    }

    /// Note names (`labels`).
    pub fn labels(&self) -> Result<Rows<'_, (LabelId, String)>> {
        self.id_value_rows("labels", "label_id", "label")
    }

    /// Note bodies (`notes`).
    pub fn notes(&self) -> Result<Rows<'_, (NoteId, String)>> {
        self.id_value_rows("notes", "note_id", "note")
    }

    /// Blurhashes of files (`blurhashes`). Files with no visual content
    /// (audio, archives) have a row with no blurhash.
    pub fn blurhashes(&self) -> Result<Rows<'_, (HashId, Option<String>)>> {
        self.id_value_rows("blurhashes", "hash_id", "blurhash")
    }

    /// Perceptual hash definitions (`shape_perceptual_hashes`).
    pub fn perceptual_hashes(&self) -> Result<Rows<'_, (PerceptualHashId, PerceptualHash)>> {
        self.id_value_rows("shape_perceptual_hashes", "phash_id", "phash")
    }

    /// Which files have which perceptual hashes (`shape_perceptual_hash_map`).
    pub fn perceptual_hash_map(&self) -> Result<Rows<'_, (PerceptualHashId, HashId)>> {
        let table = self.table("external_master", "shape_perceptual_hash_map")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["phash_id", "hash_id"],
                columns: &[],
                join: "",
            },
            Box::new(id_and("shape_perceptual_hash_map")),
        ))
    }
}
