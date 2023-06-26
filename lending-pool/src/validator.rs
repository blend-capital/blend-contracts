use soroban_sdk::{panic_with_error, Env};

use crate::errors::PoolError;

/// Require that an incoming amount is not negative
///
/// ### Arguments
/// * `amount` - The amount to check
///
/// ### Panics
/// If the number is negative
pub fn require_nonnegative(e: &Env, amount: &i128) {
    if amount.is_negative() {
        panic_with_error!(e, PoolError::NegativeAmount);
    }
}

// #[cfg(test)]
// mod tests {
//     use soroban_sdk::testutils::Address as _;

//     use crate::dependencies::TokenClient;
//     use crate::storage;
//     use crate::testutils::{create_mock_oracle, create_reserve, setup_reserve};

//     use super::*;
// }
