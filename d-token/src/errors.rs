use soroban_sdk::contracterror;

// Use the same error numbers as the built-in token contract.
// -> https://github.com/stellar/rs-soroban-env/blob/main/soroban-env-host/src/native_contract/contract_error.rs
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TokenError {
    NotImplemented = 999,
    InternalError = 1,
    AlreadyInitializedError = 3,

    UnauthorizedError = 4,

    NegativeAmountError = 8,
    AllowanceError = 9,
    BalanceError = 10,
    BalanceDeauthorizedError = 11,
    OverflowError = 12,
    TrustlineMissingError = 13,
}
