//! Nominal, effective, and observed distance, recorded per route.
//!
//! Field distance is the remaining gate count. It is one objective value.
//! Yards-to-go stays on the assessment for a model and a field. The log has
//! no task-level yards-to-go slot.

use serde::{Deserialize, Serialize};

use crate::coordinator::{effective_ytg, CalibrationSample, CalibrationTable, YtgFactors};

pub const BOAST_KEY: &str = "boast@2026-10";
pub const PLAIN_KEY: &str = "plain@2026-10";
pub const TASK_CLASS: &str = "coding";
pub const TOOLSET: &str = "cargo";
pub const BOAST_NOMINAL: u64 = 2_000;
pub const BOAST_OBSERVED: u64 = 8_000;
pub const PLAIN_NOMINAL: u64 = 7_500;
pub const PLAIN_OBSERVED: u64 = 7_700;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Nominal,
    Effective,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssessmentRecord {
    pub field_id: String,
    pub route_id: String,
    pub model_key: String,
    pub task_class: String,
    pub field_distance_gates: u32,
    pub nominal_ytg_milli: u64,
    pub effective_ytg_milli: u64,
    pub observed_ytg_milli: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssessmentLog {
    records: Vec<AssessmentRecord>,
}

impl AssessmentLog {
    pub fn push(&mut self, record: AssessmentRecord) {
        self.records.push(record);
    }

    pub fn records(&self) -> &[AssessmentRecord] {
        &self.records
    }

    pub fn observe(&mut self, field_id: &str, route_id: &str, observed_ytg_milli: u64) -> bool {
        let Some(record) = self.records.iter_mut().find(|record| {
            record.field_id == field_id
                && record.route_id == route_id
                && record.observed_ytg_milli.is_none()
        }) else {
            return false;
        };
        record.observed_ytg_milli = Some(observed_ytg_milli);
        true
    }

    pub fn mean_absolute_error(&self, kind: ErrorKind) -> Option<u64> {
        let mut count = 0u64;
        let mut total = 0u64;
        for record in &self.records {
            let Some(observed) = record.observed_ytg_milli else {
                continue;
            };
            let predicted = match kind {
                ErrorKind::Nominal => record.nominal_ytg_milli,
                ErrorKind::Effective => record.effective_ytg_milli,
            };
            total = total.saturating_add(predicted.abs_diff(observed));
            count += 1;
        }
        total.checked_div(count)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceOutcome {
    pub effective_mae: u64,
    pub nominal_mae: u64,
    pub overconfident_nominal: u64,
    pub overconfident_effective: u64,
    pub calibrated_nominal: u64,
    pub calibrated_effective: u64,
}

pub fn reference_table() -> CalibrationTable {
    let mut table = CalibrationTable::default();
    for _ in 0..3 {
        table.record(CalibrationSample {
            model_key: BOAST_KEY.to_string(),
            task_class: TASK_CLASS.to_string(),
            toolset: TOOLSET.to_string(),
            nominal_ytg_milli: BOAST_NOMINAL,
            observed_ytg_milli: BOAST_OBSERVED,
        });
    }
    table.record(CalibrationSample {
        model_key: PLAIN_KEY.to_string(),
        task_class: TASK_CLASS.to_string(),
        toolset: TOOLSET.to_string(),
        nominal_ytg_milli: PLAIN_NOMINAL,
        observed_ytg_milli: PLAIN_OBSERVED,
    });
    table
}

fn predict(table: &CalibrationTable, model_key: &str, nominal: u64) -> u64 {
    let error = table
        .error_milli(model_key, TASK_CLASS, TOOLSET)
        .unwrap_or(0);
    let variance = table
        .spread_milli(model_key, TASK_CLASS, TOOLSET)
        .unwrap_or(0);
    let confidence = table.confidence_ppm(model_key, TASK_CLASS, TOOLSET);
    effective_ytg(&YtgFactors {
        nominal_ytg_milli: nominal,
        historical_error_milli: error,
        variance_milli: variance,
        calibration_confidence_ppm: confidence,
        recovery_cost_milli: 0,
        provider_reliability_ppm: 1_000_000,
        domain_suitability_ppm: 1_000_000,
        capability_headroom_ppm: 1_000_000,
    })
    .effective_ytg_milli
}

/// A fixed history where effective distance is closer to the later observation
/// than the raw nominal distance, and the nominal ranking flips.
pub fn reference_outcome() -> ReferenceOutcome {
    let table = reference_table();
    let overconfident_effective = predict(&table, BOAST_KEY, BOAST_NOMINAL);
    let calibrated_effective = predict(&table, PLAIN_KEY, PLAIN_NOMINAL);
    let mut log = AssessmentLog::default();
    log.push(AssessmentRecord {
        field_id: "fld-ref".to_string(),
        route_id: "route-boast".to_string(),
        model_key: BOAST_KEY.to_string(),
        task_class: TASK_CLASS.to_string(),
        field_distance_gates: 4,
        nominal_ytg_milli: BOAST_NOMINAL,
        effective_ytg_milli: overconfident_effective,
        observed_ytg_milli: None,
    });
    log.push(AssessmentRecord {
        field_id: "fld-ref".to_string(),
        route_id: "route-plain".to_string(),
        model_key: PLAIN_KEY.to_string(),
        task_class: TASK_CLASS.to_string(),
        field_distance_gates: 4,
        nominal_ytg_milli: PLAIN_NOMINAL,
        effective_ytg_milli: calibrated_effective,
        observed_ytg_milli: None,
    });
    let _ = log.observe("fld-ref", "route-boast", BOAST_OBSERVED);
    let _ = log.observe("fld-ref", "route-plain", PLAIN_OBSERVED);
    ReferenceOutcome {
        effective_mae: log
            .mean_absolute_error(ErrorKind::Effective)
            .unwrap_or(u64::MAX),
        nominal_mae: log
            .mean_absolute_error(ErrorKind::Nominal)
            .unwrap_or(u64::MAX),
        overconfident_nominal: BOAST_NOMINAL,
        overconfident_effective,
        calibrated_nominal: PLAIN_NOMINAL,
        calibrated_effective,
    }
}
