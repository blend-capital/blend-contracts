use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BackstopError {
    BadRequest = 1,
    InvalidBalance = 2,
    NotExpired = 3,
    InvalidRewardZoneEntry = 4,
}
