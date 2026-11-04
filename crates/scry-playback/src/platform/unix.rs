use std::path::PathBuf;

pub(crate) const LIBRARY_NAME: &str = "libmpv.so.2";

#[expect(
    clippy::unnecessary_wraps,
    reason = "The shared platform interface allows no system library path on Windows and macOS"
)]
pub(crate) fn library_path() -> Option<PathBuf> {
    Some(LIBRARY_NAME.into())
}

pub(crate) fn native_options() -> &'static [(&'static str, &'static str)] {
    &[]
}
