use soroban_sdk::{panic_with_error, Address, Env};

use crate::{dependencies::PoolClient, errors::TokenError, storage};

/// Requires that the user does not currently user their b_tokens
/// as collateral.
///
/// Panics if the user is marked as collateralizing their b_tokens.
pub fn require_noncollateralized(e: &Env, user: &Address) {
    let pool_address = storage::read_pool(e);
    let reserve_asset = storage::read_asset(e);
    let pool_client = PoolClient::new(e, &pool_address);
    let user_config = pool_client.config(user);

    // Pulled from `/lending-pool/src/reserve_usage.rs#L83-L86`
    let to_res_shift = reserve_asset.res_index * 3;
    if (user_config >> to_res_shift) & 0b100 == 0 {
        // the reserve is used as collateral
        panic_with_error!(e, TokenError::UnauthorizedError)
    }
}

#[cfg(test)]
mod tests {
    use soroban_sdk::testutils::Address as _;

    use super::*;
    use crate::{dependencies::POOL_WASM, storage::Asset};

    fn create_lending_pool(e: &Env) -> (Address, PoolClient) {
        let contract_address = Address::random(e);
        e.register_contract_wasm(&contract_address, POOL_WASM);
        let client = PoolClient::new(e, &contract_address);
        (contract_address, client)
    }

    #[test]
    fn test_require_noncollateralized() {
        let e = Env::default();
        let token_address = Address::random(&e);

        let res_index = 7;
        let (pool_address, pool_client) = create_lending_pool(&e);

        let samwise = Address::random(&e);

        pool_client.set_collat(&samwise, &res_index, &false);
        e.as_contract(&token_address, || {
            storage::write_pool(&e, &pool_address);
            storage::write_asset(
                &e,
                &Asset {
                    res_index,
                    id: Address::random(&e),
                },
            );

            require_noncollateralized(&e, &samwise);
        });
    }

    #[test]
    #[should_panic]
    fn test_require_noncollateralized_panics_if_collateralized() {
        let e = Env::default();
        let token_address = Address::random(&e);

        let res_index = 7;
        let (pool_address, pool_client) = create_lending_pool(&e);

        let samwise = Address::random(&e);

        pool_client.set_collat(&samwise, &res_index, &true);
        e.as_contract(&token_address, || {
            storage::write_pool(&e, &pool_address);
            storage::write_asset(
                &e,
                &Asset {
                    res_index,
                    id: Address::random(&e),
                },
            );

            require_noncollateralized(&e, &samwise);
        });
    }
}
