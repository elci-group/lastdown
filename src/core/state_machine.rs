use serde::{Deserialize, Serialize};

use crate::error::{FailureClass, PhaseError};
use crate::resource::Probability;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveStatus {
    Kickoff,
    Normal,
    Redzone,
    Lastdown,
    Attempt,
    FieldGoal,
    Reset,
    Punt,
    BlindScout,
    Route,
    Score,
    Turnover,
}

impl DriveStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::FieldGoal | Self::Score | Self::Turnover)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Kickoff => "kickoff",
            Self::Normal => "normal",
            Self::Redzone => "redzone",
            Self::Lastdown => "lastdown",
            Self::Attempt => "attempt",
            Self::FieldGoal => "field_goal",
            Self::Reset => "reset",
            Self::Punt => "punt",
            Self::BlindScout => "blind_scout",
            Self::Route => "route",
            Self::Score => "score",
            Self::Turnover => "turnover",
        }
    }
}

impl std::fmt::Display for DriveStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineEvent {
    KickoffComplete,
    ObjectiveSatisfied,
    /// Predicted exhaustion entered the redzone band and is still below the
    /// lastdown band. The incumbent keeps possession.
    PressureRedzone,
    /// Runway recovered inside redzone.
    RunwayRecovered,
    /// The reserve boundary, or a fresh-context advantage, opened the decision.
    OpenLastdown,
    ChooseAttempt,
    ChooseFieldGoal,
    ChooseReset,
    ChoosePunt,
    /// A provider boundary arrived while the reserve is still intact.
    /// Possession moves to the punt protocol. This is not a turnover.
    BoundaryPreserve,
    BeginBlindScout,
    ScoutComplete,
    AssignPossession,
    Unrecoverable(FailureClass),
}

impl MachineEvent {
    pub fn label(self) -> &'static str {
        match self {
            Self::KickoffComplete => "kickoff_complete",
            Self::ObjectiveSatisfied => "objective_satisfied",
            Self::PressureRedzone => "pressure_redzone",
            Self::RunwayRecovered => "runway_recovered",
            Self::OpenLastdown => "open_lastdown",
            Self::ChooseAttempt => "choose_attempt",
            Self::ChooseFieldGoal => "choose_field_goal",
            Self::ChooseReset => "choose_reset",
            Self::ChoosePunt => "choose_punt",
            Self::BoundaryPreserve => "boundary_preserve",
            Self::BeginBlindScout => "begin_blind_scout",
            Self::ScoutComplete => "scout_complete",
            Self::AssignPossession => "assign_possession",
            Self::Unrecoverable(class) => match class {
                FailureClass::FieldUnrecoverable => "unrecoverable_field",
                FailureClass::IntegrityFailure => "unrecoverable_integrity",
                FailureClass::DisclosureDenied => "unrecoverable_disclosure",
                FailureClass::ReserveUnfunded => "unrecoverable_reserve",
                FailureClass::ObjectiveUnreachable => "unrecoverable_objective",
                FailureClass::ProviderExhausted => "unrecoverable_provider",
            },
        }
    }
}

pub fn transition(from: DriveStatus, event: MachineEvent) -> Result<DriveStatus, PhaseError> {
    if let MachineEvent::Unrecoverable(FailureClass::ProviderExhausted) = event {
        return Err(PhaseError::ExhaustionIsNotTurnover);
    }
    if from.is_terminal() {
        return Err(PhaseError::Terminal(from));
    }
    let next = match (from, event) {
        (DriveStatus::Kickoff, MachineEvent::KickoffComplete) => DriveStatus::Normal,
        (
            DriveStatus::Normal
            | DriveStatus::Redzone
            | DriveStatus::Lastdown
            | DriveStatus::Attempt,
            MachineEvent::ObjectiveSatisfied,
        ) => DriveStatus::Score,
        (DriveStatus::Normal, MachineEvent::PressureRedzone) => DriveStatus::Redzone,
        (DriveStatus::Redzone, MachineEvent::RunwayRecovered) => DriveStatus::Normal,
        (DriveStatus::Redzone, MachineEvent::PressureRedzone) => DriveStatus::Redzone,
        (DriveStatus::Normal | DriveStatus::Redzone, MachineEvent::OpenLastdown) => {
            DriveStatus::Lastdown
        }
        (DriveStatus::Lastdown, MachineEvent::ChooseAttempt) => DriveStatus::Attempt,
        (DriveStatus::Lastdown, MachineEvent::ChooseFieldGoal) => DriveStatus::FieldGoal,
        (DriveStatus::Lastdown, MachineEvent::ChooseReset) => DriveStatus::Reset,
        (DriveStatus::Lastdown, MachineEvent::ChoosePunt) => DriveStatus::Punt,
        (DriveStatus::Attempt, MachineEvent::OpenLastdown) => DriveStatus::Lastdown,
        (
            DriveStatus::Normal
            | DriveStatus::Redzone
            | DriveStatus::Lastdown
            | DriveStatus::Attempt,
            MachineEvent::BoundaryPreserve,
        ) => DriveStatus::Punt,
        (DriveStatus::Punt, MachineEvent::BeginBlindScout) => DriveStatus::BlindScout,
        (DriveStatus::BlindScout, MachineEvent::ScoutComplete) => DriveStatus::Route,
        (
            DriveStatus::Punt | DriveStatus::Reset | DriveStatus::Route,
            MachineEvent::AssignPossession,
        ) => DriveStatus::Normal,
        (
            DriveStatus::Normal
            | DriveStatus::Redzone
            | DriveStatus::Lastdown
            | DriveStatus::Attempt
            | DriveStatus::Punt
            | DriveStatus::Reset,
            MachineEvent::Unrecoverable(_),
        ) => DriveStatus::Turnover,
        _ => {
            return Err(PhaseError::Illegal {
                from,
                event: event.label(),
            });
        }
    };
    Ok(next)
}

/// Redzone and lastdown thresholds are probabilities, not token counts.
pub fn pressure_band(
    exhaustion: Probability,
    redzone_ppm: u32,
    lastdown_ppm: u32,
) -> Option<Pressure> {
    let ppm = exhaustion.ppm();
    if ppm >= lastdown_ppm {
        Some(Pressure::Lastdown)
    } else if ppm >= redzone_ppm {
        Some(Pressure::Redzone)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pressure {
    Redzone,
    Lastdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_exhaustion_is_not_a_turnover() {
        let error = transition(
            DriveStatus::Normal,
            MachineEvent::Unrecoverable(FailureClass::ProviderExhausted),
        );
        assert_eq!(error, Err(PhaseError::ExhaustionIsNotTurnover));
    }

    #[test]
    fn boundary_preserve_enters_punt() {
        let next = transition(DriveStatus::Redzone, MachineEvent::BoundaryPreserve).unwrap();
        assert_eq!(next, DriveStatus::Punt);
    }

    #[test]
    fn redzone_can_recover() {
        let next = transition(DriveStatus::Redzone, MachineEvent::RunwayRecovered).unwrap();
        assert_eq!(next, DriveStatus::Normal);
    }

    #[test]
    fn score_is_terminal() {
        let error = transition(DriveStatus::Score, MachineEvent::KickoffComplete);
        assert!(matches!(error, Err(PhaseError::Terminal(_))));
    }
}
