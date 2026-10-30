//! Video output configuration and render-context lifetime.
mod software;
pub use software::Surface as FrameSurface;

use anyhow::{Result, bail};
use std::{ffi::c_void, path::Path};

/// Select a video output independently of the operating system.
#[derive(Clone)]
pub enum PlaybackTarget {
    NativeWindow(u64),
    DecodeOnly,
    Software(FrameSurface),
}

impl PlaybackTarget {
    pub(crate) fn cache_budget(&self) -> Option<crate::cache_budget::CacheBudget> {
        match self {
            Self::NativeWindow(_) => Some(crate::cache_budget::CacheBudget::default()),
            Self::DecodeOnly | Self::Software(_) => None,
        }
    }

    pub(crate) fn options(&self) -> Result<Vec<(&'static str, String)>> {
        Ok(match self {
            Self::NativeWindow(window) => {
                let mut options = vec![
                    ("wid", window.to_string()),
                    ("vo", VideoRenderer::from_env()?.name().into()),
                    ("hwdec", "auto-safe".into()),
                ];
                options.extend(crate::platform::native_options());
                options
            }
            Self::DecodeOnly => vec![
                ("vo", "null".into()),
                ("ao", "null".into()),
                ("hwdec", "no".into()),
            ],
            Self::Software(_) => vec![("vo", "libmpv".into()), ("hwdec", "auto-safe".into())],
        })
    }
}

pub(crate) struct Output(Backend);

enum Backend {
    Passive,
    Software(software::Renderer),
}

impl Output {
    // Player keeps the initialized client alive until this output is dropped.
    pub(crate) fn start(target: PlaybackTarget, handle: *mut c_void, path: &Path) -> Result<Self> {
        match target {
            PlaybackTarget::Software(surface) => Ok(Self(Backend::Software(
                software::Renderer::start(handle, path, surface)?,
            ))),
            PlaybackTarget::NativeWindow(_) | PlaybackTarget::DecodeOnly => {
                Ok(Self(Backend::Passive))
            }
        }
    }

    pub(crate) fn is_finished(&self) -> bool {
        match &self.0 {
            Backend::Passive => false,
            Backend::Software(renderer) => renderer.is_finished(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum VideoRenderer {
    Gpu,
    GpuNext,
}

impl VideoRenderer {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "gpu" => Ok(Self::Gpu),
            "gpu-next" => Ok(Self::GpuNext),
            _ => bail!("Scry_VIDEO_RENDERER must be gpu or gpu-next"),
        }
    }

    fn from_env() -> Result<Self> {
        match std::env::var("Scry_VIDEO_RENDERER") {
            Ok(value) => Self::parse(&value),
            // gpu saved working set but increased private commit in matched probes.
            Err(std::env::VarError::NotPresent) => Ok(Self::GpuNext),
            Err(_) => bail!("Scry_VIDEO_RENDERER must be gpu or gpu-next"),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Gpu => "gpu",
            Self::GpuNext => "gpu-next",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn renderer_setting_accepts_only_supported_names() -> Result<()> {
        assert_eq!(VideoRenderer::parse("gpu")?, VideoRenderer::Gpu);
        assert_eq!(VideoRenderer::parse("gpu-next")?, VideoRenderer::GpuNext);
        for value in ["", "GPU", "gpu ", "auto", "private-input"] {
            let error = VideoRenderer::parse(value)
                .err()
                .context("Invalid renderer accepted")?;
            assert_eq!(
                error.to_string(),
                "Scry_VIDEO_RENDERER must be gpu or gpu-next"
            );
        }
        Ok(())
    }

    #[test]
    fn software_output_does_not_disable_audio() -> Result<()> {
        let target = PlaybackTarget::Software(FrameSurface::new(|_, _, _| {}));
        let options = target.options()?;
        assert!(
            options
                .iter()
                .any(|(key, value)| *key == "vo" && value == "libmpv")
        );
        assert!(!options.iter().any(|(key, _)| *key == "ao"));
        assert!(target.cache_budget().is_none());
        Ok(())
    }

    #[test]
    fn decode_only_disables_audio_video_and_hardware_decode() -> Result<()> {
        let options = PlaybackTarget::DecodeOnly.options()?;
        for option in [("vo", "null"), ("ao", "null"), ("hwdec", "no")] {
            assert!(
                options
                    .iter()
                    .any(|(key, value)| *key == option.0 && value == option.1)
            );
        }
        Ok(())
    }
}
