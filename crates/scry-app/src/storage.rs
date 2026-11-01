use anyhow::{Context, Result};
use std::{fs, io::Write, path::Path};

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path.parent().context("Missing storage directory")?;
    fs::create_dir_all(directory)?;
    let pending = directory.join(format!("{}.pending", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&pending, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(pending);
    }
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces_complete_contents() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("nested/settings.json");
        atomic_write(&path, b"first settings")?;
        assert_eq!(fs::read(&path)?, b"first settings");
        atomic_write(&path, b"new")?;
        assert_eq!(fs::read(&path)?, b"new");
        Ok(())
    }

    #[test]
    fn failed_replacement_preserves_destination_and_removes_pending_file() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("settings.json");
        fs::create_dir(&path)?;
        fs::write(path.join("existing"), b"keep")?;
        assert!(atomic_write(&path, b"replacement").is_err());
        assert_eq!(fs::read(path.join("existing"))?, b"keep");
        assert_eq!(fs::read_dir(directory.path())?.count(), 1);
        Ok(())
    }
}
