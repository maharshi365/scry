use anyhow::{Result, bail};
use url::Url;

fn browser_url(value: &str) -> Result<Url> {
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

/// Open a web URL with the operating system's default browser.
///
/// # Errors
/// Returns an error for an unsafe URL or if the browser cannot be launched.
pub fn open_browser(value: &str) -> Result<()> {
    let url = browser_url(value)?;
    #[cfg(windows)]
    {
        super::windows::open_browser(url.as_str())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        })
        .arg(url.as_str())
        .spawn()?;
        Ok(())
    }
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
            assert!(browser_url(value).is_err());
        }
        let trailer = "https://www.youtube.com/watch?v=FVI84Dfx2-I";
        assert_eq!(
            browser_url(trailer).ok().as_ref().map(Url::as_str),
            Some(trailer)
        );
    }
}
