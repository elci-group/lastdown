use std::collections::BTreeMap;

use crate::core::drive::{Drive, LoopSignal};
use crate::core::possession::TrustDomain;
use crate::core::state_machine::{pressure_band, transition, DriveStatus, MachineEvent, Pressure};
use crate::error::{FailureClass, PhaseError, PuntError, ReserveError};
use crate::provider::BoundarySignal;
use crate::punt::continuation::ContinuationObject;
use crate::punt::protocol::PuntProtocol;
use crate::punt::replay::{resume_drive, ResumeRequest};
use crate::punt::store::ContinuationStore;
use crate::resource::{Probability, ResourceVector};
use crate::runway::decision::{
    is_hopeless, should_open_lastdown, Choice, DecisionInput, LastdownView,
};
use crate::runway::{decide, estimate, RunwayEstimate, RunwayInputs};
use crate::telemetry::{kinds, TelemetryEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpendClass {
    Ordinary,
    Continuation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Directive {
    Continue,
    EnteredRedzone,
    Attempt,
    FieldGoal,
    Reset,
    Punt,
    Score,
    Turnover,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub directive: Directive,
    pub estimate: RunwayEstimate,
    pub decision: Option<crate::runway::UtilityDecision>,
}

pub struct Sentinel<S> {
    protocol: PuntProtocol<S>,
    destination: TrustDomain,
}

impl<S: ContinuationStore> Sentinel<S> {
    pub fn new(store: S, destination: TrustDomain, now_ms: u64) -> Self {
        Self {
            protocol: PuntProtocol {
                store,
                log: crate::telemetry::TelemetryLog::default(),
                now_ms,
                ledger: crate::padagonia::FieldLedger::default(),
            },
            destination,
        }
    }

    pub fn set_now(&mut self, now_ms: u64) {
        self.protocol.now_ms = now_ms;
    }

    pub fn log(&self) -> &crate::telemetry::TelemetryLog {
        &self.protocol.log
    }

    pub fn store(&self) -> &S {
        &self.protocol.store
    }

    pub fn into_store(self) -> S {
        self.protocol.store
    }

    pub fn ledger(&self) -> &crate::padagonia::FieldLedger {
        &self.protocol.ledger
    }

    pub fn note_loop(&mut self, drive: &Drive, signal: LoopSignal) {
        let kind = match signal {
            LoopSignal::None => return,
            LoopSignal::Reassess => kinds::LOOP_REASSESS,
            LoopSignal::Rollback => kinds::LOOP_ROLLBACK,
            LoopSignal::Diagnose => kinds::LOOP_DIAGNOSE,
        };
        let parent = drive
            .field
            .parent
            .as_ref()
            .map(|field_id| field_id.as_str().to_string());
        let plan = crate::strategy::possession_plan(signal, parent.as_deref());
        self.emit(
            kind,
            drive,
            BTreeMap::from([("plan".to_string(), plan.label().to_string())]),
        );
    }

    pub fn start(&mut self, drive: &mut Drive) -> Result<(), PhaseError> {
        self.emit(kinds::DRIVE_CREATED, drive, BTreeMap::new());
        drive.status = transition(drive.status, MachineEvent::KickoffComplete)?;
        self.emit(kinds::POSSESSION_ASSIGNED, drive, BTreeMap::new());
        Ok(())
    }

    pub fn observe(
        &mut self,
        drive: &mut Drive,
        inputs: &RunwayInputs,
    ) -> Result<Observation, PhaseError> {
        if drive.status.is_terminal() {
            return Err(PhaseError::Terminal(drive.status));
        }
        if drive.required_gates_complete() {
            drive.status = transition(drive.status, MachineEvent::ObjectiveSatisfied)?;
            self.emit(kinds::SCORE_COMPLETED, drive, BTreeMap::new());
            let estimate = estimate(inputs);
            return Ok(Observation {
                directive: Directive::Score,
                estimate,
                decision: None,
            });
        }
        let estimate = estimate(inputs);
        let mut entered_redzone = false;
        let policy = drive.policy();
        let band = pressure_band(
            estimate.exhaustion_probability,
            policy.redzone_ppm,
            policy.lastdown_ppm,
        );
        match (drive.status, band) {
            (DriveStatus::Normal, Some(_)) => {
                drive.status = transition(drive.status, MachineEvent::PressureRedzone)?;
                self.emit(kinds::REDZONE_ENTERED, drive, BTreeMap::new());
                entered_redzone = true;
            }
            (DriveStatus::Redzone, None) => {
                drive.status = transition(drive.status, MachineEvent::RunwayRecovered)?;
            }
            _ => {}
        }
        let ordinary = estimate.executable_remaining;
        let fresh_ordinary = inputs.fresh_grant.saturating_sub(inputs.reserve_hold);
        let remaining_work = inputs
            .observed_per_gate
            .scale(u64::from(inputs.gates_remaining));
        let open = should_open_lastdown(&LastdownView {
            ordinary,
            fresh_ordinary,
            next_cost: inputs.next_cost,
            remaining_work,
            banked_utility: drive.banked_utility(),
            pressure_at_lastdown: matches!(band, Some(Pressure::Lastdown)),
        });
        if open
            && !matches!(
                drive.status,
                DriveStatus::Lastdown | DriveStatus::Punt | DriveStatus::Reset
            )
        {
            drive.status = transition(drive.status, MachineEvent::OpenLastdown)?;
            self.emit(kinds::LASTDOWN_ENTERED, drive, BTreeMap::new());
        }
        if drive.status == DriveStatus::Lastdown {
            let decision = self.decide_now(drive, inputs, &estimate);
            if is_hopeless(&self.decision_input(drive, inputs, &estimate), &decision) {
                self.turn_over(drive, FailureClass::ObjectiveUnreachable)?;
                return Ok(Observation {
                    directive: Directive::Turnover,
                    estimate,
                    decision: Some(decision),
                });
            }
            let choice = if decision.chosen == Choice::Attempt && drive.attempt_used {
                Choice::Punt
            } else {
                decision.chosen
            };
            let directive = match choice {
                Choice::Attempt => {
                    drive.attempt_used = true;
                    drive.status = transition(drive.status, MachineEvent::ChooseAttempt)?;
                    Directive::Attempt
                }
                Choice::FieldGoal => {
                    drive.status = transition(drive.status, MachineEvent::ChooseFieldGoal)?;
                    self.emit(kinds::FIELD_GOAL_COMPLETED, drive, BTreeMap::new());
                    Directive::FieldGoal
                }
                Choice::Reset => {
                    drive.status = transition(drive.status, MachineEvent::ChooseReset)?;
                    Directive::Reset
                }
                Choice::Punt => {
                    drive.status = transition(drive.status, MachineEvent::ChoosePunt)?;
                    Directive::Punt
                }
            };
            return Ok(Observation {
                directive,
                estimate,
                decision: Some(decision),
            });
        }
        Ok(Observation {
            directive: if entered_redzone {
                Directive::EnteredRedzone
            } else {
                Directive::Continue
            },
            estimate,
            decision: None,
        })
    }

    pub fn preserve(&mut self, drive: &mut Drive) -> Result<ContinuationObject, PuntError> {
        self.protocol.execute(drive, self.destination)
    }

    pub fn emergency(
        &mut self,
        drive: &mut Drive,
        signal: BoundarySignal,
    ) -> Result<ContinuationObject, PuntError> {
        if !drive.budget.remaining().covers(drive.budget.reserve_hold) {
            drive.failure = Some("reserve_unfunded".to_string());
            drive.status = transition(
                drive.status,
                MachineEvent::Unrecoverable(FailureClass::ReserveUnfunded),
            )
            .map_err(|_| PuntError::WrongPhase)?;
            self.emit(
                kinds::TURNOVER,
                drive,
                BTreeMap::from([("reason".to_string(), "reserve_unfunded".to_string())]),
            );
            return Err(PuntError::NotCommitted);
        }
        if !matches!(drive.status, DriveStatus::Punt | DriveStatus::Reset) {
            drive.status = transition(drive.status, MachineEvent::BoundaryPreserve)
                .map_err(|_| PuntError::WrongPhase)?;
        }
        self.emit(
            kinds::LASTDOWN_ENTERED,
            drive,
            BTreeMap::from([("signal".to_string(), signal.label().to_string())]),
        );
        self.preserve(drive)
    }

    pub fn resume(
        &mut self,
        drive_id: &crate::ids::DriveId,
        generation: u64,
        request: ResumeRequest,
    ) -> Result<Drive, PuntError> {
        let object = self
            .protocol
            .store
            .load_committed(drive_id.as_str(), generation)?;
        let drive = resume_drive(&object, request)?;
        self.emit(
            kinds::POSSESSION_TRANSFERRED,
            &drive,
            BTreeMap::from([("generation".to_string(), generation.to_string())]),
        );
        self.emit(kinds::POSSESSION_ASSIGNED, &drive, BTreeMap::new());
        Ok(drive)
    }

    pub fn note_advance(&mut self, drive: &Drive, mark: &str) {
        self.emit(
            kinds::FIELD_ADVANCED,
            drive,
            BTreeMap::from([("mark".to_string(), mark.to_string())]),
        );
    }

    pub fn note_denied(&mut self, drive: &Drive) {
        self.emit(kinds::INFERENCE_DENIED, drive, BTreeMap::new());
    }

    fn decide_now(
        &self,
        drive: &Drive,
        inputs: &RunwayInputs,
        estimate: &RunwayEstimate,
    ) -> crate::runway::UtilityDecision {
        decide(&self.decision_input(drive, inputs, estimate))
    }

    fn decision_input(
        &self,
        drive: &Drive,
        inputs: &RunwayInputs,
        estimate: &RunwayEstimate,
    ) -> DecisionInput {
        let policy = drive.policy();
        let p_score = Probability::from_ppm(
            1_000_000u32.saturating_sub(estimate.exhaustion_probability.ppm()),
        )
        .unwrap_or(Probability::ZERO);
        let p_preserve = if inputs.remaining.covers(inputs.reserve_hold) {
            Probability::from_ppm(980_000).unwrap_or(Probability::ONE)
        } else {
            Probability::ZERO
        };
        let fresh_ordinary = inputs.fresh_grant.saturating_sub(inputs.reserve_hold);
        let remaining_work = inputs
            .observed_per_gate
            .scale(u64::from(inputs.gates_remaining));
        let fits_fresh = fresh_ordinary.covers(remaining_work);
        let finish_ppm = if fits_fresh {
            policy.fresh_context_success_ppm
        } else {
            policy.fresh_context_success_ppm / 5
        };
        DecisionInput {
            full_utility: drive.full_utility(),
            banked_utility: drive.banked_utility(),
            p_score,
            p_preserve,
            p_finish_after_handoff: Probability::from_ppm(finish_ppm).unwrap_or(Probability::ZERO),
            min_attempt: Probability::from_ppm(policy.min_attempt_ppm).unwrap_or(Probability::ZERO),
            reset_bonus_ppm: policy.reset_bonus_ppm,
        }
    }

    fn turn_over(&mut self, drive: &mut Drive, class: FailureClass) -> Result<(), PhaseError> {
        drive.status = transition(drive.status, MachineEvent::Unrecoverable(class))?;
        drive.failure = Some("objective_unreachable".to_string());
        self.emit(
            kinds::TURNOVER,
            drive,
            BTreeMap::from([("reason".to_string(), "objective_unreachable".to_string())]),
        );
        Ok(())
    }

    fn emit(&mut self, kind: &str, drive: &Drive, detail: BTreeMap<String, String>) {
        self.protocol.log.push(TelemetryEvent::new(
            kind,
            self.protocol.now_ms,
            drive.drive_id.clone(),
            Some(drive.field.field_id.clone()),
            detail,
        ));
    }
}

pub fn authorise(
    drive: &Drive,
    class: SpendClass,
    cost: &ResourceVector,
) -> Result<(), ReserveError> {
    match class {
        SpendClass::Ordinary => {
            if !matches!(
                drive.status,
                DriveStatus::Normal | DriveStatus::Redzone | DriveStatus::Attempt
            ) {
                return Err(ReserveError::WrongPhase);
            }
            let ordinary = drive.budget.ordinary();
            if ordinary.is_zero() || !ordinary.covers(*cost) {
                return Err(ReserveError::ReserveBoundary);
            }
            Ok(())
        }
        SpendClass::Continuation => {
            if !drive.preserving
                && !matches!(
                    drive.status,
                    DriveStatus::Punt | DriveStatus::Reset | DriveStatus::FieldGoal
                )
            {
                return Err(ReserveError::WrongPhase);
            }
            if !drive.budget.remaining().covers(*cost) {
                return Err(ReserveError::ReserveShort);
            }
            Ok(())
        }
    }
}
