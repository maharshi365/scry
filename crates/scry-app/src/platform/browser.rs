use anyhow::{Result, bail};
use url::Url;

/// Validate a web URL before asking Rune to open the default browser.
///
/// # Errors
/// Returns an error unless the URL is HTTP or HTTPS, has a host, and has no credentials.
pub fn validate_browser_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| anyhow::anyhow!("Invalid browser URL"))?;
    if value.contains('\0')
        || !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("Only HTTP and HTTPS browser URLs without credentials are supported");
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_launch_rejects_local_files_commands_and_credentials() {
        for value in [
            "file:///C:/Windows",
            "javascript:alert(1)",
            "https://user:secret@example.com/",
            "https://example.com/\0",
        ] {
            assert!(validate_browser_url(value).is_err());
        }
        let trailer = "https://www.youtube.com/watch?v=FVI84Dfx2-I";
        assert_eq!(
            validate_browser_url(trailer).ok().as_ref().map(Url::as_str),
            Some(trailer)
        );
    }
}
