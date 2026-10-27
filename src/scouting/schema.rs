//! Schema-constrained nominal distance returned by a candidate.
//!
//! The wire field is `distance_milli`. The coordinator stores that number as
//! the candidate's nominal yards-to-go. The invitation uses answer bounds so
//! the request itself does not carry a yards-to-go key.

use serde::{Deserialize, Serialize};

use crate::error::ScoutError;
use crate::schema::SCHEMA_VERSION;
use crate::scouting::scout::ScoutInvitation;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoutResponse {
    pub schema_version: u32,
    pub field_id: String,
    pub candidate_id: String,
    pub distance_milli: u64,
    pub success_ppm: u32,
    pub confidence_ppm: u32,
    pub resource_tokens: u64,
    pub blocking_unknowns: Vec<String>,
    pub tool_requirements: Vec<String>,
    pub continuation_tokens: u64,
}

pub fn parse_scout_response(bytes: &[u8]) -> Result<ScoutResponse, ScoutError> {
    serde_json::from_slice(bytes).map_err(|_| ScoutError::Malformed)
}

pub fn check_response(
    invitation: &ScoutInvitation,
    response: &ScoutResponse,
) -> Result<(), ScoutError> {
    if response.schema_version != SCHEMA_VERSION {
        return Err(ScoutError::Schema(response.schema_version));
    }
    if response.field_id != invitation.field_id || response.candidate_id != invitation.candidate_id
    {
        return Err(ScoutError::Inconsistent("identity"));
    }
    if response.candidate_id.is_empty() {
        return Err(ScoutError::Inconsistent("candidate"));
    }
    if response.success_ppm > 1_000_000 {
        return Err(ScoutError::Range("success"));
    }
    if response.confidence_ppm > 1_000_000 {
        return Err(ScoutError::Range("confidence"));
    }
    if response.distance_milli < invitation.answer_min_milli
        || response.distance_milli > invitation.answer_max_milli
    {
        return Err(ScoutError::Range("distance"));
    }
    if !response.blocking_unknowns.is_empty() && response.confidence_ppm == 1_000_000 {
        return Err(ScoutError::Inconsistent("certainty with unknowns"));
    }
    if response.distance_milli == 0 {
        if response.success_ppm != 1_000_000
            || !response.blocking_unknowns.is_empty()
            || response.continuation_tokens != 0
        {
            return Err(ScoutError::Inconsistent("zero distance"));
        }
        return Ok(());
    }
    if response.resource_tokens == 0 || response.continuation_tokens == 0 {
        return Err(ScoutError::Inconsistent("resources"));
    }
    if response.continuation_tokens > response.resource_tokens {
        return Err(ScoutError::Inconsistent("continuation above resources"));
    }
    if response.success_ppm == 1_000_000
        && response.distance_milli == invitation.answer_max_milli
        && invitation.answer_max_milli > invitation.answer_min_milli
    {
        return Err(ScoutError::Inconsistent("certainty at the far bound"));
    }
    if response.success_ppm == 0 && response.blocking_unknowns.is_empty() {
        return Err(ScoutError::Inconsistent("failure without unknowns"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scouting::amnesia::CandidatePacket;
    use crate::scouting::scout::ScoutInvitation;

    fn invitation() -> ScoutInvitation {
        ScoutInvitation {
            schema_version: SCHEMA_VERSION,
            candidate_id: "cand-a".to_string(),
            field_id: "fld".to_string(),
            task_class: "coding".to_string(),
            answer_min_milli: 1,
            answer_max_milli: 20_000,
            packet: CandidatePacket {
                schema_version: SCHEMA_VERSION,
                field_id: "fld".to_string(),
                objective: "land it".to_string(),
                constraints: Vec::new(),
                completed_gates: Vec::new(),
                partial_gates: Vec::new(),
                unresolved_gates: vec!["g".to_string()],
                facts: Vec::new(),
                artifacts: Vec::new(),
                contradictions: Vec::new(),
                tests: Vec::new(),
                blockers: Vec::new(),
            },
        }
    }

    fn response() -> ScoutResponse {
        ScoutResponse {
            schema_version: SCHEMA_VERSION,
            field_id: "fld".to_string(),
            candidate_id: "cand-a".to_string(),
            distance_milli: 4_000,
            success_ppm: 700_000,
            confidence_ppm: 600_000,
            resource_tokens: 2_000,
            blocking_unknowns: Vec::new(),
            tool_requirements: vec!["cargo".to_string()],
            continuation_tokens: 400,
        }
    }

    #[test]
    fn rejects_schema_range_and_inconsistent_answers() {
        let invite = invitation();
        assert!(check_response(&invite, &response()).is_ok());
        let extra = br#"{"schema_version":1,"field_id":"fld","candidate_id":"cand-a","distance_milli":10,"success_ppm":1,"confidence_ppm":1,"resource_tokens":10,"blocking_unknowns":[],"tool_requirements":[],"continuation_tokens":1,"extra":1}"#;
        assert_eq!(parse_scout_response(extra), Err(ScoutError::Malformed));
        let mut far = response();
        far.distance_milli = 50_000;
        assert_eq!(
            check_response(&invite, &far),
            Err(ScoutError::Range("distance"))
        );
        let mut certain = response();
        certain.blocking_unknowns.push("api".to_string());
        certain.confidence_ppm = 1_000_000;
        assert!(matches!(
            check_response(&invite, &certain),
            Err(ScoutError::Inconsistent(_))
        ));
        let mut open = invitation();
        open.answer_min_milli = 0;
        let mut zero = response();
        zero.distance_milli = 0;
        zero.success_ppm = 0;
        zero.continuation_tokens = 0;
        assert!(matches!(
            check_response(&open, &zero),
            Err(ScoutError::Inconsistent(_))
        ));
    }
}
