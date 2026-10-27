//! Interpretable effective yards-to-go.
//!
//! ```text
//! calibrated = nominal + historical_error
//! blended    = calibrated + variance * (1 - calibration_confidence)
//! stretched  = blended / provider_reliability
//! effective  = stretched / (domain_suitability * capability_headroom) + recovery_cost
//! ```
//!
//! Every factor is parts per million except the milli-yard terms. One million
//! ppm is a neutral multiplier. The constituents stay on the record so a route
//! can be recomputed from the same inputs.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YtgFactors {
    pub nominal_ytg_milli: u64,
    pub historical_error_milli: i64,
    pub variance_milli: u64,
    pub calibration_confidence_ppm: u32,
    pub recovery_cost_milli: u64,
    pub provider_reliability_ppm: u32,
    pub domain_suitability_ppm: u32,
    pub capability_headroom_ppm: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveYtg {
    pub nominal_ytg_milli: u64,
    pub historical_error_milli: i64,
    pub variance_milli: u64,
    pub calibration_confidence_ppm: u32,
    pub recovery_cost_milli: u64,
    pub provider_reliability_ppm: u32,
    pub domain_suitability_ppm: u32,
    pub capability_headroom_ppm: u32,
    pub calibrated_ytg_milli: u64,
    pub blended_ytg_milli: u64,
    pub fit_ppm: u32,
    pub effective_ytg_milli: u64,
}

pub fn apply_error(nominal: u64, error_milli: i64) -> u64 {
    let adjusted = if error_milli >= 0 {
        nominal.saturating_add(u64::try_from(error_milli).unwrap_or(u64::MAX))
    } else {
        nominal.saturating_sub(error_milli.unsigned_abs())
    };
    if nominal > 0 {
        adjusted.max(1)
    } else {
        adjusted
    }
}

fn clamp_ppm(value: u32) -> u32 {
    value.min(1_000_000)
}

pub fn effective_ytg(factors: &YtgFactors) -> EffectiveYtg {
    let confidence = clamp_ppm(factors.calibration_confidence_ppm);
    let reliability = clamp_ppm(factors.provider_reliability_ppm).max(1);
    let domain = clamp_ppm(factors.domain_suitability_ppm);
    let headroom = clamp_ppm(factors.capability_headroom_ppm);
    let calibrated = apply_error(factors.nominal_ytg_milli, factors.historical_error_milli);
    let uncertainty = factors
        .variance_milli
        .saturating_mul(u64::from(1_000_000 - confidence))
        / 1_000_000;
    let blended = calibrated.saturating_add(uncertainty);
    let stretched = blended.saturating_mul(1_000_000) / u64::from(reliability);
    let fit_wide = u64::from(domain).saturating_mul(u64::from(headroom)) / 1_000_000;
    let fit_ppm = u32::try_from(fit_wide).unwrap_or(u32::MAX).max(1);
    let after_fit = stretched.saturating_mul(1_000_000) / u64::from(fit_ppm);
    let effective = after_fit.saturating_add(factors.recovery_cost_milli).max(1);
    EffectiveYtg {
        nominal_ytg_milli: factors.nominal_ytg_milli,
        historical_error_milli: factors.historical_error_milli,
        variance_milli: factors.variance_milli,
        calibration_confidence_ppm: confidence,
        recovery_cost_milli: factors.recovery_cost_milli,
        provider_reliability_ppm: reliability,
        domain_suitability_ppm: domain,
        capability_headroom_ppm: headroom,
        calibrated_ytg_milli: calibrated,
        blended_ytg_milli: blended,
        fit_ppm,
        effective_ytg_milli: effective,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral(nominal: u64) -> YtgFactors {
        YtgFactors {
            nominal_ytg_milli: nominal,
            historical_error_milli: 0,
            variance_milli: 0,
            calibration_confidence_ppm: 0,
            recovery_cost_milli: 0,
            provider_reliability_ppm: 1_000_000,
            domain_suitability_ppm: 1_000_000,
            capability_headroom_ppm: 1_000_000,
        }
    }

    #[test]
    fn neutral_factors_preserve_nominal_distance() {
        let effective = effective_ytg(&neutral(10_000));
        assert_eq!(effective.effective_ytg_milli, 10_000);
    }

    #[test]
    fn positive_error_and_low_reliability_stretch_distance() {
        let mut factors = neutral(10_000);
        factors.historical_error_milli = 5_000;
        assert_eq!(effective_ytg(&factors).effective_ytg_milli, 15_000);
        factors.historical_error_milli = 0;
        factors.provider_reliability_ppm = 500_000;
        assert_eq!(effective_ytg(&factors).effective_ytg_milli, 20_000);
        factors.provider_reliability_ppm = 1_000_000;
        factors.domain_suitability_ppm = 500_000;
        assert_eq!(effective_ytg(&factors).effective_ytg_milli, 20_000);
        factors.domain_suitability_ppm = 1_000_000;
        factors.recovery_cost_milli = 7;
        assert_eq!(effective_ytg(&factors).effective_ytg_milli, 10_007);
    }

    #[test]
    fn confidence_shrinks_the_variance_pad() {
        let mut factors = neutral(10_000);
        factors.variance_milli = 1_000;
        assert_eq!(effective_ytg(&factors).effective_ytg_milli, 11_000);
        factors.calibration_confidence_ppm = 1_000_000;
        assert_eq!(effective_ytg(&factors).effective_ytg_milli, 10_000);
    }
}
