use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum DTokenError {
    NegativeNumber = 1,
    AlreadyInitialized = 2,
    InsufficientBLND = 3,
    NotAuthorized = 4,
    OverflowError = 5,
    BalanceError = 6,
    InvalidCaller = 7,
    InvalidNonce = 8,
}
