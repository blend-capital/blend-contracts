use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
/// Error codes for the backstop contract. Common errors are codes that match up with the built-in
/// contracts error reporting. Backstop specific errors start at 1000.
pub enum BackstopError {
    // Common Errors
    InternalError = 1,
    AlreadyInitializedError = 3,

    UnauthorizedError = 4,

    NegativeAmountError = 8,
    BalanceError = 10,
    OverflowError = 12,

    // Backstop
    BadRequest = 1000,
    NotExpired = 1001,
    InvalidRewardZoneEntry = 1002,
    InsufficientFunds = 1003,
    NotPool = 1004,
    InvalidShareMintAmount = 1005,
    InvalidTokenWithdrawAmount = 1006,
    TooManyQ4WEntries = 1007,
}
