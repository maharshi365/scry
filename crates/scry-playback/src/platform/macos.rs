use std::path::PathBuf;

pub(crate) const LIBRARY_NAME: &str = "libmpv.2.dylib";

pub(crate) fn library_path() -> Option<PathBuf> {
    // Stable Homebrew opt symlinks work across upgrades on Apple Silicon and Intel.
    [
        "/opt/homebrew/opt/mpv/lib/libmpv.2.dylib",
        "/usr/local/opt/mpv/lib/libmpv.2.dylib",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

pub(crate) fn native_options() -> Vec<(&'static str, String)> {
    Vec::new()
}
