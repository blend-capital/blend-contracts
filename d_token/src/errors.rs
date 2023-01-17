use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum DTokenError {
    NegativeNumber = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    OverflowError = 4,
    BalanceError = 5,
}
