//! Display formatting, independent of the visual system.

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
    value.map_or_else(
        || "—:—".into(),
        |n| {
            let s = n.max(0.0) as u64;
            format!("{}:{:02}", s / 60, s % 60)
        },
    )
}
