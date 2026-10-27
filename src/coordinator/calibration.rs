//! Per-model calibration history.
//!
//! A versioned model key is the identity. `model-x@2026-10` and
//! `model-x@2027-01` do not share samples. Task class and toolset are part
//! of the key as well. There is no vendor branch.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalibrationSample {
    pub model_key: String,
    pub task_class: String,
    pub toolset: String,
    pub nominal_ytg_milli: u64,
    pub observed_ytg_milli: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CalibrationTable {
    samples: Vec<CalibrationSample>,
}

impl CalibrationTable {
    pub fn record(&mut self, sample: CalibrationSample) {
        self.samples.push(sample);
    }

    pub fn error_milli(&self, model_key: &str, task_class: &str, toolset: &str) -> Option<i64> {
        let errors = self.errors(model_key, task_class, toolset);
        if errors.is_empty() {
            return None;
        }
        let sum: i128 = errors.iter().copied().map(i128::from).sum();
        Some(i64::try_from(sum / errors.len() as i128).unwrap_or(0))
    }

    /// Mean absolute deviation of the signed error, in milli-yards.
    pub fn spread_milli(&self, model_key: &str, task_class: &str, toolset: &str) -> Option<u64> {
        let errors = self.errors(model_key, task_class, toolset);
        let mean = self.error_milli(model_key, task_class, toolset)?;
        if errors.is_empty() {
            return None;
        }
        let mut acc = 0i128;
        for error in errors {
            let delta = i128::from(error) - i128::from(mean);
            acc += delta.abs();
        }
        Some(
            u64::try_from(acc / self.matching(model_key, task_class, toolset).count() as i128)
                .unwrap_or(0),
        )
    }

    /// More history raises confidence, up to eight samples and 900_000 ppm.
    pub fn confidence_ppm(&self, model_key: &str, task_class: &str, toolset: &str) -> u32 {
        let count = self.matching(model_key, task_class, toolset).count().min(8) as u32;
        count.saturating_mul(112_500)
    }

    fn matching<'a>(
        &'a self,
        model_key: &'a str,
        task_class: &'a str,
        toolset: &'a str,
    ) -> impl Iterator<Item = &'a CalibrationSample> + 'a {
        self.samples.iter().filter(move |sample| {
            sample.model_key == model_key
                && sample.task_class == task_class
                && sample.toolset == toolset
        })
    }

    fn errors(&self, model_key: &str, task_class: &str, toolset: &str) -> Vec<i64> {
        self.matching(model_key, task_class, toolset)
            .map(|sample| signed_delta(sample.nominal_ytg_milli, sample.observed_ytg_milli))
            .collect()
    }
}

pub fn signed_delta(nominal: u64, observed: u64) -> i64 {
    if observed >= nominal {
        i64::try_from(observed - nominal).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(nominal - observed).unwrap_or(i64::MAX)
    }
}
