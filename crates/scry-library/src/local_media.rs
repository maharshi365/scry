//! Local media index. Removing index records never touches media files.
use anyhow::{Result, ensure};
use diesel::{
    prelude::*,
    sql_types::{BigInt, Bool, Nullable, Text},
};

use crate::Library;

#[derive(Clone, Debug, QueryableByName)]
pub struct LocalRoot {
    #[diesel(sql_type = BigInt)]
    pub id: i64,
    #[diesel(sql_type = Text)]
    pub path: String,
    #[diesel(sql_type = Bool)]
    pub online: bool,
}

#[derive(Clone, Debug, QueryableByName)]
pub struct LocalFile {
    #[diesel(sql_type = BigInt)]
    pub id: i64,
    #[diesel(sql_type = BigInt)]
    pub root_id: i64,
    #[diesel(sql_type = Text)]
    pub root_path: String,
    #[diesel(sql_type = Text)]
    pub relative_path: String,
    #[diesel(sql_type = BigInt)]
    pub size: i64,
    #[diesel(sql_type = Text)]
    pub hints: String,
    #[diesel(sql_type = Bool)]
    pub available: bool,
    #[diesel(sql_type = Nullable<Text>)]
    pub namespace: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    pub kind: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    pub external_id: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    pub title: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    pub metadata: Option<String>,
    #[diesel(sql_type = Bool)]
    pub manual: bool,
}

pub struct LocalLink {
    pub namespace: String,
    pub kind: String,
    pub external_id: String,
    pub title: String,
    pub metadata: String,
    pub manual: bool,
}

#[derive(Clone, Debug, QueryableByName)]
pub struct LocalGroup {
    #[diesel(sql_type = Text)]
    pub namespace: String,
    #[diesel(sql_type = Text)]
    pub metadata: String,
}

const FILES: &str = "SELECT f.id,f.root_id,f.relative_path,f.size,f.hints,(f.available AND r.online) AS available,f.namespace,f.kind,f.external_id,f.title,f.metadata,f.manual,r.path AS root_path FROM local_files f JOIN local_roots r ON r.id=f.root_id";

pub(crate) fn validate(connection: &mut SqliteConnection) -> Result<()> {
    diesel::sql_query(format!("{FILES} LIMIT 0")).load::<LocalFile>(connection)?;
    diesel::sql_query("SELECT id,path,scan_generation,online FROM local_roots LIMIT 0")
        .load::<LocalRoot>(connection)?;
    diesel::sql_query(
        "SELECT root_id,folder,title_hint,namespace,metadata FROM local_groups LIMIT 0",
    )
    .load::<LocalGroup>(connection)?;
    diesel::sql_query("SELECT namespace,kind,external_id,metadata FROM local_titles LIMIT 0")
        .load::<LocalGroup>(connection)?;
    Ok(())
}

impl Library {
    /// Cache one provider inventory per title, not one full inventory per file.
    /// # Errors
    /// Returns an error on a database write.
    pub fn cache_local_metadata(
        &self,
        namespace: &str,
        kind: &str,
        id: &str,
        metadata: &str,
    ) -> Result<()> {
        diesel::sql_query("INSERT INTO local_titles(namespace,kind,external_id,metadata) VALUES (?,?,?,?) ON CONFLICT(namespace,kind,external_id) DO UPDATE SET metadata=excluded.metadata")
            .bind::<Text, _>(namespace).bind::<Text, _>(kind).bind::<Text, _>(id).bind::<Text, _>(metadata)
            .execute(&mut *self.connection.borrow_mut())?;
        Ok(())
    }

    /// # Errors
    /// Returns an error on a database read.
    pub fn local_metadata(&self, namespace: &str, kind: &str, id: &str) -> Result<Option<String>> {
        #[derive(QueryableByName)]
        struct Metadata {
            #[diesel(sql_type = Text)]
            metadata: String,
        }
        Ok(diesel::sql_query(
            "SELECT metadata FROM local_titles WHERE namespace=? AND kind=? AND external_id=?",
        )
        .bind::<Text, _>(namespace)
        .bind::<Text, _>(kind)
        .bind::<Text, _>(id)
        .get_result::<Metadata>(&mut *self.connection.borrow_mut())
        .optional()?
        .map(|m| m.metadata))
    }

    /// Remember a confirmed show for future files with the same folder and title evidence.
    /// # Errors
    /// Returns an error on a database write.
    pub fn set_local_group(
        &self,
        root: i64,
        folder: &str,
        title_hint: &str,
        group: &LocalGroup,
    ) -> Result<()> {
        diesel::sql_query("INSERT INTO local_groups(root_id,folder,title_hint,namespace,metadata) VALUES (?,?,?,?,?) ON CONFLICT(root_id,folder,title_hint) DO UPDATE SET namespace=excluded.namespace,metadata=excluded.metadata")
            .bind::<BigInt, _>(root).bind::<Text, _>(folder).bind::<Text, _>(title_hint)
            .bind::<Text, _>(&group.namespace).bind::<Text, _>(&group.metadata)
            .execute(&mut *self.connection.borrow_mut())?;
        Ok(())
    }

    /// # Errors
    /// Returns an error on a database read.
    pub fn local_group(
        &self,
        root: i64,
        folder: &str,
        title_hint: &str,
    ) -> Result<Option<LocalGroup>> {
        Ok(diesel::sql_query("SELECT namespace,metadata FROM local_groups WHERE root_id=? AND folder=? AND title_hint=?")
            .bind::<BigInt, _>(root).bind::<Text, _>(folder).bind::<Text, _>(title_hint)
            .get_result(&mut *self.connection.borrow_mut()).optional()?)
    }

    /// # Errors
    /// Returns an error when the root cannot be stored or the root limit is reached.
    pub fn add_local_root(&self, path: &str) -> Result<i64> {
        #[derive(QueryableByName)]
        struct Id {
            #[diesel(sql_type = BigInt)]
            id: i64,
        }
        ensure!(!path.trim().is_empty(), "folder path is empty");
        let roots = self.local_roots()?;
        if let Some(root) = roots.iter().find(|root| root.path == path) {
            return Ok(root.id);
        }
        ensure!(roots.len() < 32, "at most 32 local folders are supported");
        let mut connection = self.connection.borrow_mut();
        diesel::sql_query("INSERT INTO local_roots(path) VALUES (?)")
            .bind::<Text, _>(path)
            .execute(&mut *connection)?;
        Ok(diesel::sql_query("SELECT last_insert_rowid() AS id")
            .get_result::<Id>(&mut *connection)?
            .id)
    }

    /// # Errors
    /// Returns an error on a database read.
    pub fn local_roots(&self) -> Result<Vec<LocalRoot>> {
        Ok(
            diesel::sql_query("SELECT id, path, online FROM local_roots ORDER BY id")
                .load(&mut *self.connection.borrow_mut())?,
        )
    }

    /// # Errors
    /// Returns an error on a database write.
    pub fn remove_local_root(&self, id: i64) -> Result<()> {
        diesel::sql_query("DELETE FROM local_roots WHERE id=?")
            .bind::<BigInt, _>(id)
            .execute(&mut *self.connection.borrow_mut())?;
        Ok(())
    }

    /// An offline or incomplete root keeps its links but supplies no playable sources.
    /// # Errors
    /// Returns an error on a database write.
    pub fn set_local_root_online(&self, id: i64, online: bool) -> Result<()> {
        diesel::sql_query("UPDATE local_roots SET online=? WHERE id=?")
            .bind::<Bool, _>(online)
            .bind::<BigInt, _>(id)
            .execute(&mut *self.connection.borrow_mut())?;
        Ok(())
    }

    /// # Errors
    /// Returns an error on a database read or write.
    pub fn begin_local_scan(&self, root: i64) -> Result<i64> {
        #[derive(QueryableByName)]
        struct Generation {
            #[diesel(sql_type = BigInt)]
            scan_generation: i64,
        }
        let mut connection = self.connection.borrow_mut();
        diesel::sql_query("UPDATE local_roots SET scan_generation=scan_generation+1 WHERE id=?")
            .bind::<BigInt, _>(root)
            .execute(&mut *connection)?;
        Ok(
            diesel::sql_query("SELECT scan_generation FROM local_roots WHERE id=?")
                .bind::<BigInt, _>(root)
                .get_result::<Generation>(&mut *connection)?
                .scan_generation,
        )
    }

    /// # Errors
    /// Returns an error for an invalid relative path or a failed database write.
    pub fn record_local_file(
        &self,
        root: i64,
        path: &str,
        size: i64,
        modified: i64,
        hints: &str,
        generation: i64,
    ) -> Result<()> {
        let relative = std::path::Path::new(path);
        ensure!(
            !path.is_empty()
                && relative
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_))),
            "invalid relative media path"
        );
        diesel::sql_query("INSERT INTO local_files(root_id,relative_path,size,modified,hints,seen) VALUES (?,?,?,?,?,?) ON CONFLICT(root_id,relative_path) DO UPDATE SET size=excluded.size,modified=excluded.modified,hints=excluded.hints,seen=excluded.seen,available=1")
            .bind::<BigInt, _>(root).bind::<Text, _>(path).bind::<BigInt, _>(size)
            .bind::<BigInt, _>(modified).bind::<Text, _>(hints).bind::<BigInt, _>(generation)
            .execute(&mut *self.connection.borrow_mut())?;
        Ok(())
    }

    /// Call only after a full, successful traversal.
    /// # Errors
    /// Returns an error on a database write.
    pub fn finish_local_scan(&self, root: i64, generation: i64) -> Result<()> {
        self.connection.borrow_mut().immediate_transaction::<_, anyhow::Error, _>(|connection| {
            diesel::sql_query("UPDATE local_files SET available=0 WHERE root_id=? AND seen<>? AND EXISTS(SELECT 1 FROM local_roots WHERE id=? AND scan_generation=?)")
            .bind::<BigInt, _>(root).bind::<BigInt, _>(generation).bind::<BigInt, _>(root).bind::<BigInt, _>(generation)
            .execute(connection)?;
            diesel::sql_query("UPDATE local_roots SET online=1 WHERE id=? AND scan_generation=?")
                .bind::<BigInt, _>(root).bind::<BigInt, _>(generation).execute(connection)?;
            Ok(())
        })?;
        Ok(())
    }

    /// Pages contain at most 100 files.
    /// # Errors
    /// Returns an error on a database read.
    pub fn local_files(&self, offset: i64, unmatched: bool) -> Result<Vec<LocalFile>> {
        Ok(diesel::sql_query(format!(
            "{FILES} WHERE (?=0 OR f.external_id IS NULL) ORDER BY f.id LIMIT 100 OFFSET ?"
        ))
        .bind::<Bool, _>(unmatched)
        .bind::<BigInt, _>(offset.max(0))
        .load(&mut *self.connection.borrow_mut())?)
    }

    /// # Errors
    /// Returns an error when the file is absent or the read fails.
    pub fn local_file(&self, id: i64) -> Result<LocalFile> {
        Ok(diesel::sql_query(format!("{FILES} WHERE f.id=?"))
            .bind::<BigInt, _>(id)
            .get_result(&mut *self.connection.borrow_mut())?)
    }

    /// # Errors
    /// Returns an error on a database read.
    pub fn local_sources(&self, namespace: &str, kind: &str, id: &str) -> Result<Vec<LocalFile>> {
        if namespace == "local"
            && kind == "movie"
            && let Some(file_id) = id
                .strip_prefix("local-file:")
                .and_then(|id| id.parse::<i64>().ok())
        {
            return Ok(diesel::sql_query(format!(
                "{FILES} WHERE f.id=? AND f.available=1 AND r.online=1 AND f.external_id IS NULL"
            ))
            .bind::<BigInt, _>(file_id)
            .load(&mut *self.connection.borrow_mut())?);
        }
        Ok(diesel::sql_query(format!("{FILES} WHERE f.namespace=? AND f.kind=? AND f.external_id=? AND f.available=1 AND r.online=1 ORDER BY f.id LIMIT 100"))
            .bind::<Text, _>(namespace).bind::<Text, _>(kind).bind::<Text, _>(id)
            .load(&mut *self.connection.borrow_mut())?)
    }

    /// Automatic decisions never replace a manual link.
    /// # Errors
    /// Returns an error for an invalid identity or a failed database write.
    pub fn link_local_file(&self, id: i64, link: &LocalLink) -> Result<()> {
        ensure!(
            !link.namespace.is_empty()
                && !link.external_id.is_empty()
                && matches!(link.kind.as_str(), "movie" | "episode"),
            "invalid local media identity"
        );
        diesel::sql_query("UPDATE local_files SET namespace=?,kind=?,external_id=?,title=?,metadata=?,manual=? WHERE id=? AND (manual=0 OR ?=1)")
            .bind::<Text, _>(&link.namespace).bind::<Text, _>(&link.kind).bind::<Text, _>(&link.external_id)
            .bind::<Text, _>(&link.title).bind::<Text, _>(&link.metadata).bind::<Bool, _>(link.manual)
            .bind::<BigInt, _>(id).bind::<Bool, _>(link.manual).execute(&mut *self.connection.borrow_mut())?;
        Ok(())
    }

    /// # Errors
    /// Returns an error on a database write.
    pub fn clear_local_link(&self, id: i64) -> Result<()> {
        diesel::sql_query("UPDATE local_files SET namespace=NULL,kind=NULL,external_id=NULL,title=NULL,metadata=NULL,manual=0 WHERE id=?")
            .bind::<BigInt, _>(id).execute(&mut *self.connection.borrow_mut())?;
        Ok(())
    }
}
