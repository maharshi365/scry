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
    let bytes = serde_json::to_vec(index)?;
    ensure!(
        bytes.len() as u64 <= INDEX_LIMIT,
        "Media index is too large"
    );
    crate::storage::atomic_write(&data.join("my-media.json"), &bytes)
        .context("Cannot save My Media")
}
