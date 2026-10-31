#![forbid(unsafe_code)]

pub mod http_cache;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Addon {
    pub manifest_url: String,
    pub manifest: Manifest,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub resources: Vec<serde_json::Value>,
    #[serde(default)]
    pub types: Option<Vec<String>>,
    #[serde(default, rename = "idPrefixes")]
    pub id_prefixes: Option<Vec<String>>,
    #[serde(default)]
    pub catalogs: Vec<Catalog>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub extra: Vec<Extra>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Extra {
    pub name: String,
    #[serde(default, rename = "isRequired")]
    pub is_required: bool,
}

#[derive(Clone, Debug)]
pub struct Meta(std::sync::Arc<MetaData>);

/// Shared metadata payload. Read access does not copy descriptions or episodes;
/// editing through `Meta` detaches the payload from history and worker snapshots.
#[derive(Clone, Debug)]
pub struct MetaData {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub poster: Option<String>,
    pub background: Option<String>,
    pub logo: Option<String>,
    pub year: String,
    pub released: Option<String>,
    pub genres: Vec<String>,
    pub runtime: String,
    pub imdb_rating: String,
    pub cast: Vec<String>,
    pub director: Vec<String>,
    pub trailer_streams: Vec<Trailer>,
    pub videos: Vec<Video>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Trailer {
    #[serde(rename = "ytId")]
    pub yt_id: String,
}

impl std::ops::Deref for Meta {
    type Target = MetaData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Meta {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::sync::Arc::make_mut(&mut self.0)
    }
}

impl Meta {
    #[must_use]
    pub fn into_data(self) -> MetaData {
        std::sync::Arc::unwrap_or_clone(self.0)
    }
}

impl<'de> Deserialize<'de> for Meta {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            id: String,
            #[serde(rename = "type")]
            kind: String,
            name: String,
            #[serde(default, deserialize_with = "optional_text")]
            description: String,
            poster: Option<String>,
            background: Option<String>,
            logo: Option<String>,
            #[serde(default, rename = "releaseInfo", deserialize_with = "release_info")]
            release: String,
            #[serde(default, deserialize_with = "release_info")]
            year: String,
            released: Option<String>,
            #[serde(default, deserialize_with = "optional_genres")]
            genres: Vec<String>,
            #[serde(default, deserialize_with = "optional_text")]
            runtime: String,
            #[serde(default, rename = "imdbRating", deserialize_with = "rating_text")]
            imdb_rating: String,
            #[serde(default, deserialize_with = "optional_genres")]
            cast: Vec<String>,
            #[serde(default, deserialize_with = "optional_genres")]
            director: Vec<String>,
            #[serde(default, rename = "trailerStreams")]
            trailer_streams: Vec<Trailer>,
            #[serde(default)]
            videos: Vec<Video>,
        }
        let mut fields = Fields::deserialize(deserializer)?;
        // Episode inventories are retained by details and navigation snapshots.
        fields.videos.shrink_to_fit();
        Ok(Self(std::sync::Arc::new(MetaData {
            id: fields.id,
            kind: fields.kind,
            name: fields.name,
            description: fields.description,
            poster: fields.poster,
            background: fields.background,
            logo: fields.logo,
            year: if fields.release.is_empty() {
                fields.year
            } else {
                fields.release
            },
            genres: fields.genres,
            runtime: fields.runtime,
            imdb_rating: fields.imdb_rating,
            cast: fields.cast,
            director: fields.director,
            trailer_streams: fields.trailer_streams,
            released: fields.released,
            videos: fields.videos,
        })))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(from = "VideoFields")]
pub struct Video {
    pub id: String,
    pub title: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub released: Option<String>,
    pub thumbnail: Option<String>,
    pub runtime: String,
}

#[derive(Deserialize)]
struct VideoFields {
    id: String,
    #[serde(default, deserialize_with = "optional_text")]
    title: String,
    #[serde(default, deserialize_with = "optional_text")]
    name: String,
    season: Option<u32>,
    episode: Option<u32>,
    released: Option<String>,
    thumbnail: Option<String>,
    #[serde(default, deserialize_with = "optional_text")]
    runtime: String,
}

impl From<VideoFields> for Video {
    fn from(fields: VideoFields) -> Self {
        Self {
            id: fields.id,
            title: if fields.title.trim().is_empty() {
                fields.name
            } else {
                fields.title
            },
            season: fields.season,
            episode: fields.episode,
            released: fields.released,
            thumbnail: fields.thumbnail,
            runtime: fields.runtime,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    #[serde(default)]
    pub subtitles: Vec<Subtitle>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub url: Option<String>,
    pub info_hash: Option<String>,
    pub file_idx: Option<usize>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub behavior_hints: serde_json::Value,
}

#[derive(Clone)]
pub struct Client(http_cache::HttpCache);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Subtitle {
    pub id: String,
    pub lang: String,
    pub url: String,
    #[serde(default)]
    pub label: String,
}

impl Subtitle {
    /// # Errors
    /// Rejects URLs that are not plain HTTP(S) resources.
    pub fn validate_url(&self) -> Result<Url> {
        let url = Url::parse(&self.url).map_err(|_| anyhow::anyhow!("Invalid subtitle URL"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            bail!("Unsupported subtitle URL");
        }
        Ok(url)
    }
}

fn optional_text<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn rating_text<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Rating {
        Text(String),
        Number(f64),
    }
    Ok(match Option::<Rating>::deserialize(deserializer)? {
        Some(Rating::Text(value)) => value,
        Some(Rating::Number(value)) => value.to_string(),
        None => String::new(),
    })
}

fn release_info<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Release {
        Text(String),
        Year(u16),
    }
    Ok(match Option::<Release>::deserialize(deserializer)? {
        Some(Release::Text(value)) => value,
        Some(Release::Year(value)) => value.to_string(),
        None => String::new(),
    })
}

fn optional_genres<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.unwrap_or_default())
}

impl Stream {
    /// Check before handing a stream to the plain URL / torrent playback adapter.
    /// Proxy headers need a proxy-aware adapter; silently dropping them is unsafe.
    ///
    /// # Errors
    /// Returns an error if the source is invalid or requires unsupported playback features.
    pub fn validate_playback(&self) -> Result<()> {
        if let Some(value) = &self.url {
            let url = Url::parse(value).map_err(|_| anyhow::anyhow!("Invalid stream URL"))?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                bail!("Stream requires an unsupported URL or login credentials");
            }
            if let Some(headers) = self.behavior_hints.get("proxyHeaders")
                && !headers.is_null()
                && headers.as_object().is_none_or(|object| !object.is_empty())
            {
                bail!("Stream requires proxy headers, which this player does not support");
            }
            return Ok(());
        }
        if let Some(hash) = &self.info_hash {
            if hash.len() != 40 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!("Invalid torrent info hash");
            }
            return Ok(());
        }
        bail!("Stream has no supported direct URL or torrent source")
    }
}

impl Catalog {
    #[must_use]
    pub fn supports_paging(&self) -> bool {
        self.extra.iter().any(|extra| extra.name == "skip")
    }
    /// This client supplies only the search extra, not genre, skip, date, etc.
    /// # Errors
    /// Returns an error if required catalog filters cannot be satisfied.
    pub fn validate_search(&self, search: Option<&str>) -> Result<()> {
        if search.is_some() && !self.extra.iter().any(|extra| extra.name == "search") {
            bail!("Catalog does not support search");
        }
        if self.extra.iter().any(|extra| {
            extra.is_required && (extra.name != "search" || search.is_none_or(str::is_empty))
        }) {
            bail!("Catalog requires extra filters that were not supplied");
        }
        Ok(())
    }
}

/// # Errors
/// Returns an error for an invalid catalog URL or unsupported pagination.
pub fn catalog_page_url(addon: &Addon, catalog: &Catalog, skip: u32) -> Result<Url> {
    if skip > 0 && !catalog.supports_paging() {
        bail!("Catalog does not support pagination");
    }
    let mut url = resource_url(
        &addon.manifest_url,
        "catalog",
        &catalog.kind,
        &catalog.id,
        None,
    )?;
    if skip > 0
        || catalog
            .extra
            .iter()
            .any(|extra| extra.name == "skip" && extra.is_required)
    {
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("Invalid catalog URL"))?
            .pop()
            .push(&catalog.id)
            .push(&format!("skip={skip}.json"));
    }
    Ok(url)
}

/// # Errors
/// Returns an error if the input is not a valid HTTP or HTTPS add-on manifest URL.
pub fn manifest_url(input: &str) -> Result<Url> {
    let input = input.trim();
    let input = input
        .strip_prefix("stremio://")
        .map_or_else(|| input.to_owned(), |rest| format!("https://{rest}"));
    let mut url = Url::parse(&input).context("Enter an HTTP or HTTPS add-on manifest URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("Add-ons must use HTTP or HTTPS and cannot contain login credentials");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("Use the add-on's manifest URL without a query or fragment");
    }
    if !url.path().ends_with("/manifest.json") {
        url.set_path(&format!(
            "{}/manifest.json",
            url.path().trim_end_matches('/')
        ));
    }
    Ok(url)
}

/// # Errors
/// Returns an error for invalid or unsafe URLs and resource path segments.
pub fn resource_url(
    base: &str,
    resource: &str,
    kind: &str,
    id: &str,
    search: Option<&str>,
) -> Result<Url> {
    if [resource, kind, id]
        .iter()
        .any(|part| part.is_empty() || matches!(*part, "." | ".."))
    {
        bail!("Resource, type and ID must be nonempty path segments");
    }
    let mut url = manifest_url(base)?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|()| anyhow::anyhow!("Invalid add-on URL"))?;
        path.pop().push(resource).push(kind);
        if search.is_some() {
            path.push(id);
        } else {
            path.push(&format!("{id}.json"));
        }
    }
    if let Some(query) = search {
        // Extras use query-string escaping, but live in a path, not the URL query.
        // set_path preserves the escapes; push would encode the percent signs again.
        let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        url.set_path(&format!(
            "{}/search={}.json",
            url.path(),
            encoded.replace('+', "%20")
        ));
    }
    Ok(url)
}

impl Addon {
    #[must_use]
    pub fn supports(&self, resource: &str, kind: &str, id: &str) -> bool {
        if self.disabled {
            return false;
        }
        if resource == "catalog" {
            return self
                .manifest
                .catalogs
                .iter()
                .any(|catalog| catalog.kind == kind && catalog.id == id);
        }
        self.manifest.resources.iter().any(|r| {
            if r.as_str() == Some(resource) {
                return self
                    .manifest
                    .types
                    .as_ref()
                    .is_none_or(|types| types.iter().any(|t| t == kind))
                    && self.manifest.id_prefixes.as_ref().is_none_or(|prefixes| {
                        prefixes.iter().any(|prefix| id.starts_with(prefix))
                    });
            }
            r["name"].as_str() == Some(resource)
                && r.get("types").is_none_or(|value| {
                    value
                        .as_array()
                        .is_some_and(|types| types.iter().any(|t| t.as_str() == Some(kind)))
                })
                && r.get("idPrefixes").is_none_or(|value| {
                    value.as_array().is_some_and(|prefixes| {
                        prefixes
                            .iter()
                            .any(|p| p.as_str().is_some_and(|p| id.starts_with(p)))
                    })
                })
        })
    }
}

impl Client {
    /// Retrieve one browse page. Providers that do not advertise skip are not paged.
    /// # Errors
    /// Returns an error for unsupported filters, network failures, or invalid data.
    pub async fn catalog_page(
        &self,
        addon: &Addon,
        catalog: &Catalog,
        skip: u32,
    ) -> Result<Vec<Meta>> {
        #[derive(Clone, Deserialize)]
        struct Response {
            metas: Vec<Meta>,
        }
        if catalog
            .extra
            .iter()
            .any(|extra| extra.is_required && extra.name != "skip")
        {
            bail!("Catalog requires extra filters that were not supplied");
        }
        let response: Response = self.json(catalog_page_url(addon, catalog, skip)?).await?;
        // Keep the raw response count for the provider's skip cursor. The HTTP
        // response already has a byte bound; do not truncate and silently skip titles.
        Ok(response.metas)
    }
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new() -> Result<Self> {
        Self::with_cache_dir(None)
    }

    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn with_cache_dir(directory: Option<std::path::PathBuf>) -> Result<Self> {
        Ok(Self(http_cache::HttpCache::new(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(25))
                .connect_timeout(Duration::from_secs(8))
                .user_agent("Scry/0.1")
                .build()?,
            directory,
        )))
    }

    pub fn invalidate_cache(&self) {
        self.0.invalidate();
    }

    #[must_use]
    pub fn background(&self) -> Self {
        Self(self.0.background())
    }

    async fn json<T: serde::de::DeserializeOwned + Clone + Send + Sync + 'static>(
        &self,
        url: Url,
    ) -> Result<T> {
        self.json_mode(url, true).await
    }

    async fn json_mode<T: serde::de::DeserializeOwned + Clone + Send + Sync + 'static>(
        &self,
        url: Url,
        allow_stale: bool,
    ) -> Result<T> {
        self.json_checked(url, allow_stale, |_| Ok(())).await
    }

    async fn json_checked<T, F>(&self, url: Url, allow_stale: bool, check: F) -> Result<T>
    where
        T: serde::de::DeserializeOwned + Clone + Send + Sync + 'static,
        F: Fn(&T) -> Result<()> + Send + Sync + 'static,
    {
        self.0
            .json_checked(url, 4 * 1024 * 1024, allow_stale, check)
            .await
    }

    /// # Errors
    /// Returns an error for an invalid manifest URL, failed request, or invalid manifest.
    pub async fn install(&self, input: &str) -> Result<Addon> {
        let url = manifest_url(input)?;
        let manifest = self
            .json_checked(url.clone(), true, |manifest: &Manifest| {
                if manifest.id.trim().is_empty() || manifest.name.trim().is_empty() {
                    bail!("Add-on manifest has no ID or name");
                }
                Ok(())
            })
            .await?;
        Ok(Addon {
            disabled: false,
            manifest_url: url.to_string(),
            manifest,
        })
    }

    /// # Errors
    /// Returns an error for unsupported filters, failed requests, or invalid catalog data.
    pub async fn catalog(
        &self,
        addon: &Addon,
        catalog: &Catalog,
        search: Option<&str>,
    ) -> Result<Vec<Meta>> {
        #[derive(Clone, Deserialize)]
        struct Response {
            metas: Vec<Meta>,
        }
        catalog.validate_search(search)?;
        let response: Response = self
            .json(resource_url(
                &addon.manifest_url,
                "catalog",
                &catalog.kind,
                &catalog.id,
                search,
            )?)
            .await?;
        Ok(response.metas.into_iter().take(100).collect())
    }

    /// # Errors
    /// Returns an error for an invalid resource URL, failed request, or invalid metadata.
    pub async fn meta(&self, addon: &Addon, kind: &str, id: &str) -> Result<Meta> {
        #[derive(Clone, Deserialize)]
        struct Response {
            meta: Meta,
        }
        let url = resource_url(&addon.manifest_url, "meta", kind, id, None)?;
        let (id, kind) = (id.to_owned(), kind.to_owned());
        Ok(self
            .json_checked(url, true, move |response: &Response| {
                if response.meta.id != id || response.meta.kind != kind {
                    bail!("Add-on metadata identity does not match the request");
                }
                Ok(())
            })
            .await?
            .meta)
    }

    /// # Errors
    /// Returns an error for an invalid resource URL, failed request, or invalid stream data.
    pub async fn streams(&self, addon: &Addon, kind: &str, id: &str) -> Result<Vec<Stream>> {
        #[derive(Clone, Deserialize)]
        struct Response {
            streams: Vec<Stream>,
        }
        Ok(self
            .json_mode::<Response>(
                resource_url(&addon.manifest_url, "stream", kind, id, None)?,
                false,
            )
            .await?
            .streams
            .into_iter()
            .take(100)
            .collect())
    }

    /// # Errors
    /// Returns a URL-free error for failed or invalid subtitle resources.
    pub async fn subtitles(
        &self,
        addon: &Addon,
        kind: &str,
        id: &str,
        hints: &serde_json::Value,
    ) -> Result<Vec<Subtitle>> {
        #[derive(Clone, Deserialize)]
        struct Response {
            subtitles: Vec<Subtitle>,
        }
        let mut url = resource_url(&addon.manifest_url, "subtitles", kind, id, None)?;
        let mut extras = Vec::new();
        for key in ["videoHash", "videoSize", "filename"] {
            let value = hints.get(key).and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
            if let Some(value) = value.filter(|s| !s.is_empty() && s.len() <= 1024) {
                let escaped: String =
                    url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
                extras.push(format!("{key}={}", escaped.replace('+', "%20")));
            }
        }
        if !extras.is_empty() {
            url.path_segments_mut()
                .map_err(|()| anyhow::anyhow!("Invalid subtitle resource"))?
                .pop()
                .push(id);
            url.set_path(&format!("{}/{}.json", url.path(), extras.join("&")));
        }
        Ok(self
            .json_mode::<Response>(url, false)
            .await?
            .subtitles
            .into_iter()
            .take(100)
            .filter(|s| !s.id.is_empty() && s.validate_url().is_ok())
            .collect())
    }

    /// # Errors
    /// Returns a private error for failed or oversized subtitle downloads.
    pub async fn subtitle_bytes(&self, subtitle: &Subtitle) -> Result<std::sync::Arc<Vec<u8>>> {
        Ok(self
            .0
            .get(subtitle.validate_url()?, 4 * 1024 * 1024, false)
            .await?
            .bytes)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Protocol fixtures and local HTTP test failures must fail the test immediately"
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subtitle_resources_keep_variants_and_reject_unsafe_urls() {
        let (url, server) = serve("200 OK", r#"{"subtitles":[{"id":"one","lang":"eng","url":"https://example.com/one.srt","label":"Full dialogue"},{"id":"two","lang":"eng","url":"https://example.com/two.ass"},{"id":"bad","lang":"eng","url":"file:///private"}]}"#).await;
        let addon: Addon = serde_json::from_value(serde_json::json!({"manifest_url":url.as_str(),"manifest":{"id":"subs","name":"Subtitles","resources":["subtitles"]}})).unwrap();
        let subtitles = Client::new()
            .unwrap()
            .subtitles(&addon, "series", "tt1:1:2", &serde_json::Value::Null)
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(subtitles.len(), 2);
        assert_eq!(subtitles[0].label, "Full dialogue");
        assert_eq!(subtitles[1].id, "two");
        let subtitle = Subtitle {
            id: "x".into(),
            lang: "eng".into(),
            label: String::new(),
            url: "https://user:secret@example.com/sub.srt".into(),
        };
        assert!(!format!("{:#}", subtitle.validate_url().unwrap_err()).contains("secret"));
    }

    async fn serve(status: &str, body: &str) -> (Url, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let received = socket.read(&mut request).await.unwrap();
            if received == 0 {
                return;
            }
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        (
            Url::parse(&format!("http://{address}/private-token/manifest.json")).unwrap(),
            server,
        )
    }

    async fn json_error(status: &str, body: &str) -> String {
        let (url, server) = serve(status, body).await;
        let client = Client::new().unwrap();
        let error = client.json::<Vec<Meta>>(url).await.unwrap_err();
        server.await.unwrap();
        format!("{error:#}")
    }

    #[tokio::test]
    async fn metadata_requires_the_requested_identity() {
        let client = Client::new().unwrap();
        for (id, kind, valid) in [
            ("film", "movie", true),
            ("other", "movie", false),
            ("film", "series", false),
        ] {
            let body = serde_json::json!({"meta":{"id":id,"type":kind,"name":"Film"}}).to_string();
            let (url, server) = serve("200 OK", &body).await;
            let addon: Addon = serde_json::from_value(serde_json::json!({
                "manifest_url":url.as_str(), "manifest":{"id":"test","name":"Test"}
            }))
            .unwrap();
            let result = client.meta(&addon, "movie", "film").await;
            server.await.unwrap();
            assert_eq!(result.is_ok(), valid, "{id}/{kind}");
            if let Err(error) = result {
                assert_eq!(
                    error.to_string(),
                    "Add-on metadata identity does not match the request"
                );
            }
        }
    }

    #[tokio::test]
    async fn response_errors_never_echo_private_values() {
        let error = json_error("200 OK", "\"https://private.example/secret-token\"").await;
        assert_eq!(error, "Invalid add-on response");
        let error = json_error("403 Forbidden", "https://private.example/secret-token").await;
        assert_eq!(error, "Add-on returned HTTP 403");
    }

    #[test]
    fn configured_paths_and_episode_ids_are_preserved() {
        let url = resource_url(
            "https://addon.example/config-token/manifest.json",
            "stream",
            "series",
            "tt123:2:4",
            None,
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://addon.example/config-token/stream/series/tt123:2:4.json"
        );
    }

    #[test]
    fn search_cannot_inject_a_path_or_query() {
        let url = resource_url(
            "https://addon.example/manifest.json",
            "catalog",
            "movie",
            "top",
            Some("a/b?x=1#z"),
        )
        .unwrap();
        assert!(url.as_str().ends_with("/top/search=a%2Fb%3Fx%3D1%23z.json"));
        assert!(url.query().is_none());
    }

    #[test]
    fn http_manifests_and_resources_are_supported() {
        let input = "http://gdrive2.billythebishop4.workers.dev/manifest.json";
        assert_eq!(manifest_url(input).unwrap().as_str(), input);
        assert_eq!(
            resource_url(input, "meta", "movie", "tt123", None)
                .unwrap()
                .as_str(),
            "http://gdrive2.billythebishop4.workers.dev/meta/movie/tt123.json"
        );
    }

    #[test]
    fn unsupported_manifest_schemes_and_logins_are_rejected() {
        for input in [
            "file:///manifest.json",
            "https://user:pass@example.com/manifest.json",
        ] {
            assert!(manifest_url(input).is_err());
        }
    }

    #[test]
    fn resource_filters_are_respected() {
        let addon: Addon = serde_json::from_value(serde_json::json!({"manifest_url":"https://a.example/manifest.json", "manifest":{"id":"a", "name":"a", "resources":[{"name":"stream", "types":["series"], "idPrefixes":["tt"]}]}})).unwrap();
        assert!(addon.supports("stream", "series", "tt123:1:2"));
        assert!(!addon.supports("stream", "movie", "tt123"));
        assert!(!addon.supports("stream", "series", "other"));
    }
}
