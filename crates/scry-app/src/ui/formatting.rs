//! Display formatting, independent of the visual system.

pub(super) fn build_version(version: &str) -> String {
    let version = version
        .split_once('+')
        .map_or(version, |(version, _)| version);
    version.split_once("-dev.").map_or_else(
        || format!("v{version}"),
        |(release, build)| format!("v{release} · Build {build}"),
    )
}

pub(super) fn excerpt(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let mut result: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Display whole nonnegative seconds with saturating float-to-integer conversion"
)]
pub(super) fn clock(value: Option<f64>) -> String {
    value.filter(|n| n.is_finite()).map_or_else(
        || "—:—".into(),
        |n| {
            let s = n.max(0.0) as u64;
            format!("{}:{:02}", s / 60, s % 60)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_versions_use_readable_release_and_build_labels() {
        assert_eq!(build_version("0.1.0"), "v0.1.0");
        assert_eq!(build_version("0.1.0-dev.42+abc1234"), "v0.1.0 · Build 42");
        assert_eq!(build_version("0.2.0-rc.1+abc1234"), "v0.2.0-rc.1");
    }

    #[test]
    fn clock_displays_unknown_for_non_finite_samples() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(clock(Some(value)), "—:—", "sample {value}");
        }
    }

    #[test]
    fn clock_preserves_finite_sample_bounds() {
        assert_eq!(clock(None), "—:—");
        assert_eq!(clock(Some(-1.0)), "0:00");
        assert_eq!(clock(Some(125.9)), "2:05");
        assert_eq!(clock(Some(f64::MAX)), "307445734561825860:15");
    }

    #[test]
    fn excerpt_counts_unicode_characters_without_splitting_utf8() {
        assert_eq!(excerpt("é猫🎬end", 3), "é猫🎬…");
        assert_eq!(excerpt("é猫🎬", 3), "é猫🎬");
        assert_eq!(excerpt("猫", 0), "…");
        assert_eq!(excerpt("", 0), "");
    }
}
