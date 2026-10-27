//! Lastdown keeps a protected continuation reserve and punts a drive before
//! that reserve is spent.
//!
//! A committed continuation is enough for a fresh context to resume. The
//! field ledger records epistemic claims. Blind scouting asks candidates for
//! a nominal distance without showing them rankings or a predecessor's
//! assessment. The coordinator turns those answers into effective distance
//! and assigns one route. Scalar gate distance stays authoritative for replay.

#![forbid(unsafe_code)]

pub mod clock;
pub mod coordinator;
pub mod core;
pub mod error;
pub mod ids;
pub mod learning;
pub mod padagonia;
pub mod provider;
pub mod punt;
pub mod resource;
pub mod runway;
pub mod schema;
pub mod scouting;
pub mod secrets;
pub mod security;
pub mod sentinel;
pub mod sim;
pub mod strategy;
pub mod telemetry;
pub mod vectoral;

pub use clock::ManualClock;
pub use core::{Drive, DriveStatus, FieldSnapshot, ModelRoute, Possession};
pub use error::{PhaseError, PuntError, ReserveError};
pub use padagonia::FieldLedger;
pub use resource::{ContinuationReserve, Probability, ResourceVector};
pub use runway::{estimate, RunwayEstimate, RunwayInputs};
pub use schema::{CONTINUATION_MAGIC, SCHEMA_VERSION};
pub use sentinel::{authorise, Directive, Sentinel, SpendClass};
