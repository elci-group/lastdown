//! Deterministic possession routing.
//!
//! The coordinator is ordinary code. It reads scout assessments, calibration
//! history, and the route table. Candidates do not receive the explanation.

pub mod calibration;
pub mod effective_ytg;
pub mod routing;

pub use calibration::{signed_delta, CalibrationSample, CalibrationTable};
pub use effective_ytg::{apply_error, effective_ytg, EffectiveYtg, YtgFactors};
pub use routing::{
    headroom_ppm, select_route, RouteContext, RouteDecision, RoutePolicy, RoutedScout,
};
