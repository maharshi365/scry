pub mod platform;

#[must_use]
pub fn project_dir(cache: bool) -> Option<std::path::PathBuf> {
    #[cfg(feature = "profiling")]
    if let Some(root) = std::env::var_os("SCRY_PROFILE_ROOT") {
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
