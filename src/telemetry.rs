use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{DriveId, FieldId};
use crate::schema::SCHEMA_VERSION;

pub mod kinds {
    pub const DRIVE_CREATED: &str = "drive.created";
    pub const POSSESSION_ASSIGNED: &str = "possession.assigned";
    pub const FIELD_ADVANCED: &str = "field.advanced";
    pub const REDZONE_ENTERED: &str = "redzone.entered";
    pub const LASTDOWN_ENTERED: &str = "lastdown.entered";
    pub const PUNT_PREPARED: &str = "punt.prepared";
    pub const PUNT_COMMITTED: &str = "punt.committed";
    pub const SCOUT_REQUESTED: &str = "scout.requested";
    pub const SCOUT_RETURNED: &str = "scout.returned";
    pub const ROUTE_CALCULATED: &str = "route.calculated";
    pub const LOOP_REASSESS: &str = "loop.reassess";
    pub const LOOP_ROLLBACK: &str = "loop.rollback";
    pub const LOOP_DIAGNOSE: &str = "loop.diagnose";
    pub const POSSESSION_TRANSFERRED: &str = "possession.transferred";
    pub const FIELD_GOAL_COMPLETED: &str = "field_goal.completed";
    pub const SCORE_COMPLETED: &str = "score.completed";
    pub const TURNOVER: &str = "turnover";
    pub const INFERENCE_DENIED: &str = "inference.denied";
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub schema_version: u32,
    pub kind: String,
    pub at_ms: u64,
    pub drive_id: DriveId,
    pub field_id: Option<FieldId>,
    pub detail: BTreeMap<String, String>,
}

impl TelemetryEvent {
    pub fn new(
        kind: &str,
        at_ms: u64,
        drive_id: DriveId,
        field_id: Option<FieldId>,
        detail: BTreeMap<String, String>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            kind: kind.to_string(),
            at_ms,
            drive_id,
            field_id,
            detail,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryLog {
    events: Vec<TelemetryEvent>,
}

impl TelemetryLog {
    pub fn push(&mut self, event: TelemetryEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[TelemetryEvent] {
        &self.events
    }

    pub fn kinds(&self) -> Vec<&str> {
        self.events
            .iter()
            .map(|event| event.kind.as_str())
            .collect()
    }

    pub fn contains(&self, kind: &str) -> bool {
        self.events.iter().any(|event| event.kind == kind)
    }
}
