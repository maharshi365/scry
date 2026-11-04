use std::path::PathBuf;

pub(crate) const LIBRARY_NAME: &str = "libmpv-2.dll";

pub(crate) fn library_path() -> Option<PathBuf> {
    None
}

pub(crate) fn native_options() -> &'static [(&'static str, &'static str)] {
    // Keep both native renderers on the same API with D3D11 decode interop.
    &[("gpu-api", "d3d11"), ("gpu-context", "d3d11")]
}
