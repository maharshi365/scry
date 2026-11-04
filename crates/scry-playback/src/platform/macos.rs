use std::path::{Path, PathBuf};

pub(crate) const LIBRARY_NAME: &str = "libmpv.2.dylib";

pub(crate) fn library_path() -> Option<PathBuf> {
    // Stable Homebrew opt symlinks work across upgrades on Apple Silicon and Intel.
    [
        "/opt/homebrew/opt/mpv/lib/libmpv.2.dylib",
        "/usr/local/opt/mpv/lib/libmpv.2.dylib",
    ]
    .into_iter()
    .find(|path| Path::new(path).is_file())
    .map(PathBuf::from)
}

pub(crate) fn native_options() -> &'static [(&'static str, &'static str)] {
    &[]
}
