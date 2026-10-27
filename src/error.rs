use crate::core::state_machine::DriveStatus;
use crate::provider::BoundarySignal;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdError {
    #[error("identifier is empty or contains whitespace")]
    Empty,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RangeError {
    #[error("probability {0} exceeds 1000000 ppm")]
    Probability(u32),
    #[error("score {0} exceeds 1000")]
    Score(u16),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FieldError {
    #[error("field invariant failed: {0}")]
    Invariant(&'static str),
    #[error("unknown gate {0}")]
    UnknownGate(String),
    #[error("artifact digest does not match its body")]
    DigestMismatch,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PhaseError {
    #[error("drive is already terminal ({0})")]
    Terminal(DriveStatus),
    #[error("illegal transition from {from} on {event}")]
    Illegal {
        from: DriveStatus,
        event: &'static str,
    },
    /// Provider exhaustion is an orchestration boundary. It is not, by itself,
    /// a task failure, so it cannot enter `TURNOVER`.
    #[error("provider exhaustion cannot be recorded as a turnover")]
    ExhaustionIsNotTurnover,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReserveError {
    #[error("ordinary execution would consume the continuation reserve")]
    ReserveBoundary,
    #[error("continuation spend is not available in this phase")]
    WrongPhase,
    #[error("remaining resources do not cover the continuation spend")]
    ReserveShort,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PuntError {
    #[error("possession stays with the incumbent until the continuation commits")]
    NotCommitted,
    #[error("continuation integrity check failed")]
    Integrity,
    #[error("disclosure policy refused the destination")]
    DisclosureDenied,
    #[error("schema version {0} is not supported")]
    Schema(u32),
    #[error("field check failed: {0}")]
    Field(&'static str),
    #[error("store error: {0}")]
    Store(String),
    #[error("replay mismatch: {0}")]
    Replay(&'static str),
    #[error("punt requires a drive that is preparing a handoff")]
    WrongPhase,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FixtureError {
    #[error("fixture schema {0} is not supported")]
    Schema(u32),
    #[error("fixture is inconsistent: {0}")]
    Invariant(&'static str),
    #[error("fixture io: {0}")]
    Io(String),
    #[error("fixture json: {0}")]
    Json(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    FieldUnrecoverable,
    IntegrityFailure,
    DisclosureDenied,
    ReserveUnfunded,
    ObjectiveUnreachable,
    /// Rejected by the state machine. Present so callers cannot smuggle a
    /// provider death into `TURNOVER` by renaming it.
    ProviderExhausted,
}

impl FailureClass {
    pub fn from_signal(signal: BoundarySignal) -> Self {
        match signal {
            BoundarySignal::RateLimited
            | BoundarySignal::ContextOverflow
            | BoundarySignal::Timeout
            | BoundarySignal::QuotaRejected
            | BoundarySignal::ProviderTerminated
            | BoundarySignal::MalformedOutput
            | BoundarySignal::PartialToolExecution => Self::ProviderExhausted,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScoutError {
    #[error("scout schema {0} is not supported")]
    Schema(u32),
    #[error("scout response could not be read")]
    Malformed,
    #[error("scout response is outside the requested range: {0}")]
    Range(&'static str),
    #[error("scout response is internally inconsistent: {0}")]
    Inconsistent(&'static str),
    #[error("candidate packet contained competitive metadata")]
    Leak,
    #[error("scouting is not available from this phase")]
    WrongPhase,
    #[error("unknown candidate")]
    UnknownCandidate,
    #[error("no scout was accepted")]
    NoneAccepted,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error("no eligible receiver")]
    NoneEligible,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    #[error("ledger entry is immutable")]
    Immutable,
    #[error("ledger entry is missing")]
    Missing,
    #[error("ledger io: {0}")]
    Io(String),
    #[error("ledger has no commit marker")]
    NotCommitted,
    #[error("seal check failed")]
    Seal,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SealError {
    #[error("seal check failed")]
    Authentication,
    #[error("seal bytes are not valid")]
    Encoding,
}
