//! Read-only smoke test against a public metadata add-on. Never requests streams.
use anyhow::{Context, Result, ensure};
use scry_addons::Client;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let client = Client::new()?;
    let addon = client
        .install("https://v3-cinemeta.strem.io/manifest.json")
        .await?;
    let catalog = addon
        .manifest
        .catalogs
        .iter()
        .find(|catalog| catalog.kind == "movie" && catalog.validate_search(None).is_ok())
        .context("Metadata provider has no browsable movie catalog")?;
    let entries = client.catalog(&addon, catalog, None).await?;
    let item = entries.first().context("Metadata catalog was empty")?;
    ensure!(
        addon.supports("meta", &item.kind, &item.id),
        "Provider has no matching metadata resource"
    );
    let meta = client.meta(&addon, &item.kind, &item.id).await?;
    ensure!(
        meta.id == item.id,
        "Metadata resource returned a different identity"
    );
    println!(
        "Manifest, catalog ({} entries), and metadata decoded successfully. No streams requested.",
        entries.len()
    );
    Ok(())
}
