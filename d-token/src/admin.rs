use soroban_sdk::{panic_with_error, Address, Env};

use crate::{errors::TokenError, storage};

/// Require "addr" is the pool the token belongs too. This function should be called
/// if any admin level actions are taken.
///
/// ### Arguments
/// * `addr` - The address to test
///
/// ### Errors
/// If the given address is not the pool the token belongs to
pub fn require_is_pool(e: &Env, addr: &Address) {
    let admin = storage::read_pool(e);
    if admin != addr.clone() {
        panic_with_error!(e, TokenError::UnauthorizedError);
    }
}

#[cfg(test)]
mod tests {
    use soroban_sdk::testutils::Address as _;

    use super::*;

    #[test]
    fn test_require_pool() {
        let e = Env::default();

        let token_address = Address::random(&e);

        let pool = Address::random(&e);

        e.as_contract(&token_address, || {
            storage::write_pool(&e, &pool);

            require_is_pool(&e, &pool);
        });
    }

    #[test]
    #[should_panic]
    fn test_require_pool_not_pool_panics() {
        let e = Env::default();

        let token_address = Address::random(&e);

        let pool = Address::random(&e);
        let not_pool = Address::random(&e);

        e.as_contract(&token_address, || {
            storage::write_pool(&e, &pool);

            require_is_pool(&e, &not_pool);
        });
    }
}
