use std::collections::BTreeMap;

use crate::core::drive::{Drive, ProgressMark};
use crate::core::field::advance_mark;
use crate::core::state_machine::DriveStatus;
use crate::error::PuntError;
use crate::padagonia::FieldLedger;
use crate::punt::continuation::{
    assemble, canonical_bytes, disclosure_allows, leaks_competitive_metadata, redact_field,
    validate,
};
use crate::punt::replay::operationally_equivalent;
use crate::punt::store::ContinuationStore;
use crate::telemetry::{kinds, TelemetryEvent, TelemetryLog};

/// Transactional punt. Possession is released only after the prepared package
/// round-trips through the store and matches the frozen field.
pub struct PuntProtocol<S> {
    pub store: S,
    pub log: TelemetryLog,
    pub now_ms: u64,
    pub ledger: FieldLedger,
}

impl<S: ContinuationStore> PuntProtocol<S> {
    pub fn execute(
        &mut self,
        drive: &mut Drive,
        destination: crate::core::possession::TrustDomain,
    ) -> Result<crate::punt::continuation::ContinuationObject, PuntError> {
        let field_goal = drive.status == DriveStatus::FieldGoal;
        if field_goal {
            drive.status = DriveStatus::Punt;
        }
        if !matches!(drive.status, DriveStatus::Punt | DriveStatus::Reset) {
            return Err(PuntError::WrongPhase);
        }
        drive.preserving = true;
        let before = drive.field.clone();
        let mut sealed_field = drive.field.clone();
        sealed_field.freeze_side_effects();
        sealed_field.refresh_repository_dirty();
        let redactions = redact_field(&mut sealed_field);
        if let Err(error) = sealed_field.check_invariants() {
            drive.preserving = false;
            return Err(PuntError::Field(match error {
                crate::error::FieldError::DigestMismatch => "digest",
                crate::error::FieldError::Invariant(message) => message,
                crate::error::FieldError::UnknownGate(_) => "gate",
            }));
        }
        if !disclosure_allows(&sealed_field, destination) {
            self.abort(drive);
            return Err(PuntError::DisclosureDenied);
        }
        let object = match assemble(drive, &sealed_field, redactions, self.now_ms) {
            Ok(object) => object,
            Err(error) => {
                drive.preserving = false;
                return Err(error);
            }
        };
        let bytes = match canonical_bytes(&object) {
            Ok(bytes) => bytes,
            Err(error) => {
                drive.preserving = false;
                return Err(error);
            }
        };
        if leaks_competitive_metadata(&bytes) {
            drive.preserving = false;
            return Err(PuntError::Integrity);
        }
        if let Err(error) = self.store.prepare(&object) {
            drive.preserving = false;
            return Err(error);
        }
        self.emit(
            kinds::PUNT_PREPARED,
            drive,
            BTreeMap::from([("generation".to_string(), drive.generation.to_string())]),
        );
        let read_back = match self
            .store
            .load_prepared(drive.drive_id.as_str(), drive.generation)
        {
            Ok(object) => object,
            Err(error) => {
                self.abort(drive);
                return Err(error);
            }
        };
        if let Err(error) = validate(&read_back) {
            self.abort(drive);
            return Err(error);
        }
        let reconstructed = match crate::punt::continuation::reconstruct_field(&read_back) {
            Ok(field) => field,
            Err(error) => {
                self.abort(drive);
                return Err(error);
            }
        };
        if !operationally_equivalent(&sealed_field, &reconstructed) {
            self.abort(drive);
            return Err(PuntError::Replay("round trip"));
        }
        if let Err(error) = self.store.commit(drive.drive_id.as_str(), drive.generation) {
            self.abort(drive);
            return Err(error);
        }
        self.ledger.record(&sealed_field);
        let mark = match advance_mark(&before, &sealed_field) {
            crate::core::field::AdvanceMark::PositiveAdvance => ProgressMark::PositiveAdvance,
            crate::core::field::AdvanceMark::Neutral => ProgressMark::Neutral,
            crate::core::field::AdvanceMark::Regression => ProgressMark::Regression,
        };
        // A possession that completes gates relative to where it started is
        // positive even when this snapshot's parent already held them.
        let started = drive
            .possession
            .as_ref()
            .map(|possession| possession.started_completed)
            .unwrap_or(0);
        let mark = if sealed_field.completed_gates.len() > started {
            ProgressMark::PositiveAdvance
        } else {
            mark
        };
        drive.possession_marks.push(mark);
        drive.field = sealed_field;
        drive.possession = None;
        drive.preserving = false;
        drive.generation = drive.generation.saturating_add(1);
        if field_goal {
            drive.status = DriveStatus::FieldGoal;
        }
        self.emit(
            kinds::PUNT_COMMITTED,
            drive,
            BTreeMap::from([("mark".to_string(), format!("{mark:?}"))]),
        );
        Ok(object)
    }

    fn abort(&mut self, drive: &mut Drive) {
        drive.preserving = false;
        if matches!(drive.status, DriveStatus::Punt | DriveStatus::Reset) {
            drive.status = DriveStatus::Lastdown;
        }
    }

    fn emit(&mut self, kind: &str, drive: &Drive, detail: BTreeMap<String, String>) {
        self.log.push(TelemetryEvent::new(
            kind,
            self.now_ms,
            drive.drive_id.clone(),
            Some(drive.field.field_id.clone()),
            detail,
        ));
    }
}
