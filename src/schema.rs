/// Version of every persisted Lastdown object in this crate.
pub const SCHEMA_VERSION: u32 = 1;

/// Magic that opens a drive continuation object.
pub const CONTINUATION_MAGIC: &str = "LASTDOWN/1";

/// Nominal yards used as the scale reference for completion cost.
///
/// 10.000 yards, stored as milli-yards. A nominal value equal to this
/// reference does not inflate or shrink the observed per-gate cost.
pub const YTG_REFERENCE_MILLI: u32 = 10_000;
