use serde::{Deserialize, Serialize};

use crate::core::field::FieldSnapshot;
use crate::core::possession::{LastdownPolicy, Possession};
use crate::core::state_machine::DriveStatus;
use crate::ids::{DriveId, GateId};
use crate::resource::{ContinuationReserve, ResourceVector};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraint {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityNeed {
    pub name: String,
    pub minimum: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceGate {
    pub gate_id: GateId,
    pub name: String,
    pub utility: u64,
    pub partial_utility: u64,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Objective {
    pub schema_version: u32,
    pub statement: String,
    pub constraints: Vec<Constraint>,
    pub required_capabilities: Vec<CapabilityNeed>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveBudget {
    pub granted: ResourceVector,
    pub consumed: ResourceVector,
    pub reserve: ContinuationReserve,
    /// Vector hold subtracted from remaining resources before ordinary work.
    pub reserve_hold: ResourceVector,
    pub lock_reserve: bool,
}

impl DriveBudget {
    pub fn remaining(&self) -> ResourceVector {
        self.granted.saturating_sub(self.consumed)
    }

    /// Resources an ordinary inference cycle is allowed to burn.
    pub fn ordinary(&self) -> ResourceVector {
        self.remaining().saturating_sub(self.reserve_hold)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressMark {
    PositiveAdvance,
    Neutral,
    Regression,
    Score,
}

/// What repeated possession marks should force the coordinator to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopSignal {
    None,
    Reassess,
    Rollback,
    Diagnose,
}

/// Two regressions escalate to rollback. Six possessions with no gate movement
/// escalate to diagnosis. Three neutral possessions ask for a strategy
/// reassessment. Diagnosis outranks reassessment once that longer streak is
/// present, and rollback outranks both.
pub fn loop_signal(marks: &[ProgressMark]) -> LoopSignal {
    let neutral = marks
        .iter()
        .filter(|mark| **mark == ProgressMark::Neutral)
        .count();
    let regression = marks
        .iter()
        .filter(|mark| **mark == ProgressMark::Regression)
        .count();
    let advanced = marks
        .iter()
        .any(|mark| matches!(mark, ProgressMark::PositiveAdvance | ProgressMark::Score));
    if regression >= 2 {
        LoopSignal::Rollback
    } else if marks.len() >= 6 && !advanced {
        LoopSignal::Diagnose
    } else if neutral >= 3 {
        LoopSignal::Reassess
    } else {
        LoopSignal::None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drive {
    pub schema_version: u32,
    pub drive_id: DriveId,
    pub generation: u64,
    pub objective: Objective,
    pub acceptance_gates: Vec<AcceptanceGate>,
    pub field: FieldSnapshot,
    pub possession: Option<Possession>,
    pub budget: DriveBudget,
    pub status: DriveStatus,
    pub attempt_used: bool,
    pub preserving: bool,
    pub failure: Option<String>,
    pub possession_marks: Vec<ProgressMark>,
}

impl Drive {
    pub fn required_gates_complete(&self) -> bool {
        self.acceptance_gates
            .iter()
            .filter(|gate| gate.required)
            .all(|gate| {
                self.field
                    .completed_gates
                    .iter()
                    .any(|completed| completed == &gate.gate_id)
            })
    }

    pub fn full_utility(&self) -> u64 {
        self.acceptance_gates.iter().map(|gate| gate.utility).sum()
    }

    pub fn banked_utility(&self) -> u64 {
        let completed: u64 = self
            .acceptance_gates
            .iter()
            .filter(|gate| {
                self.field
                    .completed_gates
                    .iter()
                    .any(|id| id == &gate.gate_id)
            })
            .map(|gate| gate.utility)
            .sum();
        let partial: u64 = self
            .acceptance_gates
            .iter()
            .filter(|gate| {
                self.field
                    .partial_gates
                    .iter()
                    .any(|id| id == &gate.gate_id)
            })
            .map(|gate| gate.partial_utility)
            .sum();
        completed.saturating_add(partial)
    }

    pub fn gates_remaining(&self) -> u32 {
        self.field.unresolved_gates.len() as u32 + self.field.partial_gates.len() as u32
    }

    pub fn policy(&self) -> LastdownPolicy {
        self.possession
            .as_ref()
            .map(|possession| possession.policy.clone())
            .unwrap_or_default()
    }
}
