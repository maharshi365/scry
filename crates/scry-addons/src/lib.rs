use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Addon {
    pub manifest_url: String,
    pub manifest: Manifest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
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
pub struct Meta {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub poster: Option<String>,
    pub background: Option<String>,
    pub year: String,
    pub genres: Vec<String>,
    pub videos: Vec<Video>,
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
            #[serde(default, rename = "releaseInfo", deserialize_with = "release_info")]
            release: String,
            #[serde(default, deserialize_with = "release_info")]
            year: String,
            #[serde(default, deserialize_with = "optional_genres")]
            genres: Vec<String>,
            #[serde(default)]
            videos: Vec<Video>,
        }
        let fields = Fields::deserialize(deserializer)?;
        Ok(Self {
            id: fields.id,
            kind: fields.kind,
            name: fields.name,
            description: fields.description,
            poster: fields.poster,
            background: fields.background,
            year: if fields.release.is_empty() {
                fields.year
            } else {
                fields.release
            },
            genres: fields.genres,
            videos: fields.videos,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Video {
    pub id: String,
    #[serde(default, deserialize_with = "optional_text")]
    pub title: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
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
pub struct Client(reqwest::Client);

fn optional_text<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
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
/// Returns an error if the input is not a valid HTTPS add-on manifest URL.
pub fn manifest_url(input: &str) -> Result<Url> {
    let input = input.trim();
    let input = input
        .strip_prefix("stremio://")
        .map_or_else(|| input.to_owned(), |rest| format!("https://{rest}"));
    let mut url = Url::parse(&input).context("Enter an HTTPS add-on manifest URL")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("Add-ons must use HTTPS and cannot contain login credentials");
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
                && r.get("types")
                    .and_then(|v| v.as_array())
                    .is_none_or(|types| types.iter().any(|t| t.as_str() == Some(kind)))
                && r.get("idPrefixes")
                    .and_then(|v| v.as_array())
                    .is_none_or(|prefixes| {
                        prefixes
                            .iter()
                            .any(|p| p.as_str().is_some_and(|p| id.starts_with(p)))
                    })
        })
    }
}

impl Client {
    /// # Errors
    /// Returns an error if the HTTPS client cannot be initialized.
    pub fn new() -> Result<Self> {
        Ok(Self(
            reqwest::Client::builder()
                .https_only(true)
                .timeout(Duration::from_secs(25))
                .connect_timeout(Duration::from_secs(8))
                .user_agent("Scry/0.1")
                .build()?,
        ))
    }

    async fn json<T: serde::de::DeserializeOwned>(&self, url: Url) -> Result<T> {
        // Never include reqwest errors: they can contain private manifest path tokens.
        let response = self
            .0
            .get(url)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Add-on request failed or timed out"))?;
        if !response.status().is_success() {
            bail!("Add-on returned HTTP {}", response.status().as_u16());
        }
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("Add-on response interrupted"))?
        {
            if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
                bail!("Add-on response exceeds 4 MiB");
            }
            bytes.extend_from_slice(&chunk);
        }
        // Serde errors can echo an invalid string value, including private URLs.
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("Invalid add-on response"))
    }

    /// # Errors
    /// Returns an error for an invalid manifest URL, failed request, or invalid manifest.
    pub async fn install(&self, input: &str) -> Result<Addon> {
        let url = manifest_url(input)?;
        let manifest: Manifest = self.json(url.clone()).await?;
        if manifest.id.trim().is_empty() || manifest.name.trim().is_empty() {
            bail!("Add-on manifest has no ID or name");
        }
        Ok(Addon {
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
        #[derive(Deserialize)]
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
        #[derive(Deserialize)]
        struct Response {
            meta: Meta,
        }
        Ok(self
            .json::<Response>(resource_url(&addon.manifest_url, "meta", kind, id, None)?)
            .await?
            .meta)
    }

    /// # Errors
    /// Returns an error for an invalid resource URL, failed request, or invalid stream data.
    pub async fn streams(&self, addon: &Addon, kind: &str, id: &str) -> Result<Vec<Stream>> {
        #[derive(Deserialize)]
        struct Response {
            streams: Vec<Stream>,
        }
        Ok(self
            .json::<Response>(resource_url(&addon.manifest_url, "stream", kind, id, None)?)
            .await?
            .streams
            .into_iter()
            .take(100)
            .collect())
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Protocol fixtures and local HTTP test failures must fail the test immediately"
)]
mod tests {
    use super::*;

    async fn json_error(status: &str, body: &str) -> String {
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
        // HTTP is allowed only on this private test client, not Client::new().
        let client = Client(reqwest::Client::builder().no_proxy().build().unwrap());
        let error = client
            .json::<Vec<Meta>>(
                Url::parse(&format!("http://{address}/private-token/manifest.json")).unwrap(),
            )
            .await
            .unwrap_err();
        server.await.unwrap();
        format!("{error:#}")
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
    fn insecure_manifests_are_rejected() {
        for input in [
            "http://example.com/manifest.json",
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
