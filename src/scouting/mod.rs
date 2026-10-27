//! Blind locker room.
//!
//! A candidate receives an unattributed field projection and returns a
//! schema-constrained distance. The projection carries evidence. It leaves
//! out rankings, predecessor distance, and model identity.

pub mod amnesia;
pub mod schema;
pub mod scout;

pub use amnesia::{candidate_packet, forbidden_hit_count, measure_amnesia, CandidatePacket};
pub use schema::{parse_scout_response, ScoutResponse};
pub use scout::{
    complete_scouts, request_scouts, AcceptedScout, ScoutBinding, ScoutInvitation, ScoutRequest,
};
