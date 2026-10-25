pub mod platform;

#[must_use]
pub fn project_dir(cache: bool) -> Option<std::path::PathBuf> {
    #[cfg(feature = "profiling")]
    if let Some(root) = std::env::var_os("SCRY_PROFILE_ROOT") {
        return Some(std::path::PathBuf::from(root).join(if cache { "cache" } else { "data" }));
    }
    project_dir_with_root(cache, std::env::var_os("SCRY_DATA_ROOT").as_deref())
}

fn project_dir_with_root(
    cache: bool,
    root: Option<&std::ffi::OsStr>,
) -> Option<std::path::PathBuf> {
    if let Some(root) = root.filter(|root| !root.is_empty()) {
        return Some(std::path::PathBuf::from(root).join(if cache { "cache" } else { "data" }));
    }
    directories::ProjectDirs::from("app", "Scry", "Scry").map(|dirs| {
        if cache {
            dirs.cache_dir()
        } else {
            dirs.data_local_dir()
        }
        .to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::project_dir_with_root;
    use std::{ffi::OsStr, path::PathBuf};

    #[test]
    fn custom_root_separates_development_data_and_cache() {
        let root = OsStr::new("development-profile");
        assert_eq!(
            project_dir_with_root(false, Some(root)),
            Some(PathBuf::from(root).join("data"))
        );
        assert_eq!(
            project_dir_with_root(true, Some(root)),
            Some(PathBuf::from(root).join("cache"))
        );
    }

    #[test]
    fn empty_root_keeps_default_storage() {
        for cache in [false, true] {
            assert_eq!(
                project_dir_with_root(cache, Some(OsStr::new(""))),
                project_dir_with_root(cache, None)
            );
        }
    }
}
