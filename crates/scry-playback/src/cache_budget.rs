//! Size compressed read-ahead from observed packet density, not video resolution.
//! Keep ten seconds plus 25% VBR headroom, within the original 32 MiB ceiling.
const MIB: u64 = 1024 * 1024;
const UNIT: f64 = 1_048_576.0;
const MIN: u64 = 8 * MIB;
pub(super) const MAX: u64 = 32 * MIB;

#[derive(Clone, Copy)]
pub(super) struct CacheBudget {
    pub limit: u64,
    high_water: u64,
    measured: bool,
}

impl Default for CacheBudget {
    fn default() -> Self {
        Self {
            limit: MAX,
            high_water: MIN,
            measured: false,
        }
    }
}

impl CacheBudget {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "The rounded byte budget is clamped to 8–32 MiB before conversion"
    )]
    pub fn observe(&mut self, bytes: f64, seconds: f64) -> Option<u64> {
        if !bytes.is_finite() || !seconds.is_finite() || bytes <= 0.0 || seconds <= 0.0 {
            return None;
        }
        let wanted =
            ((bytes / seconds * 12.5 / UNIT).ceil() * UNIT).clamp(8.0 * UNIT, 32.0 * UNIT) as u64;
        // Do not shrink on startup's first few packets. After sizing, a short
        // buffer can indicate a bitrate burst: allow immediate growth instead.
        if seconds < 3.0 && (!self.measured || wanted <= self.limit) {
            return None;
        }
        self.measured = true;
        self.high_water = self.high_water.max(wanted);
        (self.high_water != self.limit).then_some(self.high_water)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncertain_startup_preserves_the_original_network_buffer() {
        let mut budget = CacheBudget::default();
        for (bytes, seconds) in [
            (0.0, 10.0),
            (1.0, 0.0),
            (f64::NAN, 10.0),
            (1.0, f64::INFINITY),
            (1.0, 0.1),
        ] {
            assert_eq!(budget.observe(bytes, seconds), None);
        }
        assert_eq!(budget.limit, MAX);
    }

    #[test]
    fn low_bitrate_saves_memory_but_keeps_vbr_headroom() {
        let mut budget = CacheBudget::default();
        assert_eq!(budget.observe(12.0 * UNIT, 10.0), Some(15 * MIB));
        budget.limit = 15 * MIB;
        assert_eq!(budget.observe(4.0 * UNIT, 10.0), None);
        let mut small = CacheBudget::default();
        assert_eq!(small.observe(UNIT, 10.0), Some(MIN));
    }

    #[test]
    fn bitrate_bursts_expand_even_when_the_smaller_cache_cannot_hold_three_seconds() {
        let mut budget = CacheBudget::default();
        budget.limit = budget.observe(UNIT, 10.0).unwrap_or(MAX);
        assert_eq!(budget.observe(6.0 * UNIT, 1.0), Some(MAX));
        budget.limit = MAX;
        assert_eq!(budget.observe(UNIT, 10.0), None);
    }
}
