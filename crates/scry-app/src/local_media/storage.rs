use super::Index;
use anyhow::{Context, Result, ensure};
use std::{fs, io::Read, path::Path};

const INDEX_LIMIT: u64 = 64 * 1024 * 1024;

pub(crate) fn load(data: &Path) -> Result<Index> {
    let file = match fs::File::open(data.join("my-media.json")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Index::default()),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(INDEX_LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= INDEX_LIMIT,
        "Media index is too large"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

pub(crate) fn save(data: &Path, index: &Index) -> Result<()> {
    use std::io::Write;
    let bytes = serde_json::to_vec(index)?;
    ensure!(
        bytes.len() as u64 <= INDEX_LIMIT,
        "Media index is too large"
    );
    fs::create_dir_all(data)?;
    let pending = data.join(format!("my-media-{}.pending", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&pending, data.join("my-media.json"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(pending);
    }
    result.context("Cannot save My Media")
}
