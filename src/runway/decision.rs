use serde::{Deserialize, Serialize};

use crate::resource::{scale_by_probability, Probability};
use crate::schema::SCHEMA_VERSION;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    Attempt,
    FieldGoal,
    Reset,
    Punt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionInput {
    pub full_utility: u64,
    pub banked_utility: u64,
    pub p_score: Probability,
    pub p_preserve: Probability,
    pub p_finish_after_handoff: Probability,
    pub min_attempt: Probability,
    pub reset_bonus_ppm: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtilityDecision {
    pub schema_version: u32,
    pub u_score: u64,
    pub u_fieldgoal: u64,
    pub u_reset: u64,
    pub u_punt: u64,
    pub p_score: Probability,
    pub p_preserve: Probability,
    pub score_disqualified: bool,
    pub chosen: Choice,
}

/// Four utilities. A score whose probability sits under `min_attempt` is
/// disqualified even when the raw product looks large. That is the hail-mary ban.
pub fn decide(input: &DecisionInput) -> UtilityDecision {
    let score_disqualified = input.p_score < input.min_attempt;
    let u_score = if score_disqualified {
        0
    } else {
        scale_by_probability(input.p_score, input.full_utility)
    };
    let u_fieldgoal = input.banked_utility;
    let remaining = input.full_utility.saturating_sub(input.banked_utility);
    let after = scale_by_probability(input.p_finish_after_handoff, remaining)
        .saturating_add(input.banked_utility);
    let u_punt = scale_by_probability(input.p_preserve, after);
    let u_reset = u_punt
        .saturating_mul(1_000_000u64.saturating_add(u64::from(input.reset_bonus_ppm)))
        / 1_000_000;
    let mut chosen = Choice::Punt;
    let mut best = u_punt;
    if u_reset > best {
        chosen = Choice::Reset;
        best = u_reset;
    }
    if u_fieldgoal > best {
        chosen = Choice::FieldGoal;
        best = u_fieldgoal;
    }
    if u_score > best {
        chosen = Choice::Attempt;
    }
    UtilityDecision {
        schema_version: SCHEMA_VERSION,
        u_score,
        u_fieldgoal,
        u_reset,
        u_punt,
        p_score: input.p_score,
        p_preserve: input.p_preserve,
        score_disqualified,
        chosen,
    }
}

pub fn is_hopeless(input: &DecisionInput, decision: &UtilityDecision) -> bool {
    input.banked_utility == 0
        && decision.u_score == 0
        && decision.u_fieldgoal == 0
        && decision.u_punt == 0
        && decision.u_reset == 0
}

/// LASTDOWN opens only when another ordinary cycle is unsafe, or when the
/// banked field can move to a fresh grant that can finish and this possession
/// cannot. A possession that can still cover the remainder keeps the ball.
pub fn should_open_lastdown(view: &LastdownView) -> bool {
    if view.ordinary.covers(view.remaining_work) {
        return false;
    }
    if !view.ordinary.covers(view.next_cost) {
        return true;
    }
    view.pressure_at_lastdown
        && view.banked_utility > 0
        && view.fresh_ordinary.covers(view.remaining_work)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastdownView {
    pub ordinary: crate::resource::ResourceVector,
    pub fresh_ordinary: crate::resource::ResourceVector,
    pub next_cost: crate::resource::ResourceVector,
    pub remaining_work: crate::resource::ResourceVector,
    pub banked_utility: u64,
    pub pressure_at_lastdown: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ppm(value: u32) -> Probability {
        Probability::from_ppm(value).unwrap()
    }

    #[test]
    fn hail_mary_is_disqualified() {
        let banned = decide(&DecisionInput {
            full_utility: 1_000,
            banked_utility: 0,
            p_score: ppm(400_000),
            p_preserve: ppm(980_000),
            p_finish_after_handoff: ppm(200_000),
            min_attempt: ppm(550_000),
            reset_bonus_ppm: 0,
        });
        assert!(banned.score_disqualified);
        assert_eq!(banned.chosen, Choice::Punt);
        let allowed = decide(&DecisionInput {
            min_attempt: ppm(100_000),
            ..DecisionInput {
                full_utility: 1_000,
                banked_utility: 0,
                p_score: ppm(400_000),
                p_preserve: ppm(980_000),
                p_finish_after_handoff: ppm(200_000),
                min_attempt: ppm(100_000),
                reset_bonus_ppm: 0,
            }
        });
        assert_eq!(allowed.chosen, Choice::Attempt);
    }

    #[test]
    fn expired_deadline_takes_the_field_goal() {
        let decision = decide(&DecisionInput {
            full_utility: 100,
            banked_utility: 40,
            p_score: Probability::ZERO,
            p_preserve: ppm(980_000),
            p_finish_after_handoff: Probability::ZERO,
            min_attempt: ppm(550_000),
            reset_bonus_ppm: 0,
        });
        assert_eq!(decision.chosen, Choice::FieldGoal);
        assert_eq!(decision.u_fieldgoal, 40);
        assert_eq!(decision.u_punt, 39);
    }

    #[test]
    fn reset_bonus_must_beat_the_punt() {
        let decision = decide(&DecisionInput {
            full_utility: 1_000,
            banked_utility: 0,
            p_score: ppm(400_000),
            p_preserve: ppm(980_000),
            p_finish_after_handoff: ppm(200_000),
            min_attempt: ppm(550_000),
            reset_bonus_ppm: 100_000,
        });
        assert_eq!(decision.chosen, Choice::Reset);
        assert!(decision.u_reset > decision.u_punt);
    }
}
