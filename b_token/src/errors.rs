use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TokenError {
    NegativeNumber = 1,
    AlreadyInitialized = 2,
    AllowanceError = 3,
    NotAuthorized = 4,
    OverflowError = 5,
    BalanceError = 6,
    InvalidCaller = 7,
    InvalidNonce = 8,
    InvalidAdmin = 9,
    TokenCollateralized = 10,
}
