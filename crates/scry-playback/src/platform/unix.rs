use std::path::PathBuf;

pub(crate) const LIBRARY_NAME: &str = "libmpv.so.2";

pub(crate) fn library_path() -> Option<PathBuf> {
    None
}

pub(crate) fn native_options() -> Vec<(&'static str, String)> {
    Vec::new()
}
