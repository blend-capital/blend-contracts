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

//     use soroban_sdk::{U256, Bytes};

//     use super::*;

//     #[test]
//     fn test_require_nonnegative() {
//         let e = Env::default();
//         let val1 = i128::MAX / 3;

//         let val1_as_bytes = Bytes::from_array(&e, &val1.to_be_bytes().concat([]));
//         let val2: U256 = U256::from_be_bytes(&e, &val1_as_bytes);
//         let as_bytes = val2.to_be_bytes();
//         assert_eq!(as_bytes, val1_as_bytes)
//     }
// }
