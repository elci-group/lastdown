use serde::{Deserialize, Serialize};

/// A boundary the incumbent hit or is about to hit.
///
/// The signal names the condition. It does not name a vendor. Routing and
/// recovery code must keep working if every vendor string in the process
/// changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundarySignal {
    RateLimited,
    ContextOverflow,
    Timeout,
    QuotaRejected,
    ProviderTerminated,
    MalformedOutput,
    PartialToolExecution,
}

impl BoundarySignal {
    pub fn label(self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::ContextOverflow => "context_overflow",
            Self::Timeout => "timeout",
            Self::QuotaRejected => "quota_rejected",
            Self::ProviderTerminated => "provider_terminated",
            Self::MalformedOutput => "malformed_output",
            Self::PartialToolExecution => "partial_tool_execution",
        }
    }
}
