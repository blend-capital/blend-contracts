/// Fixed-point scalar for 7 decimal numbers
pub const SCALAR_7: i128 = 1_0000000;

/// The approximate deployment time in seconds since epoch of the backstop module. This is NOT the
/// actual deployment time and should not be considered accruate. It is only used to determine reward
/// zone size on ~90 day intervals, starting at 10 on or before April 15th, 2024 00:00:00 UTC.
pub const BACKSTOP_EPOCH: u64 = 1713139200;
