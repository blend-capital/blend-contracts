use soroban_sdk::{panic_with_error, Address, Env};

use crate::{dependencies::PoolClient, errors::TokenError, storage};

/// Requires that the user does not currently user their b_tokens
/// as collateral.
///
/// Panics if the user is marked as collateralizing their b_tokens.
pub fn require_noncollateralized(e: &Env, user: &Address) {
    let pool_id = storage::read_pool_id(e);
    let reserve_asset = storage::read_asset(e);
    let pool_client = PoolClient::new(e, &pool_id);
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
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _},
        Bytes, BytesN,
    };

    use super::*;
    use crate::{dependencies::POOL_WASM, storage::Asset};

    fn create_lending_pool(e: &Env) -> (BytesN<32>, PoolClient) {
        let contract_id = BytesN::<32>::random(e);
        e.register_contract_wasm(&contract_id, POOL_WASM);
        let client = PoolClient::new(e, &contract_id);
        (contract_id, client)
    }

    #[test]
    fn test_require_noncollateralized() {
        let e = Env::default();
        let token_id = BytesN::<32>::random(&e);

        let res_index = 7;
        let (pool_id, pool_client) = create_lending_pool(&e);
        let pool = Address::from_contract_id(&e, &pool_id);

        let samwise = Address::random(&e);

        pool_client.set_collat(&samwise, &res_index, &false);
        e.as_contract(&token_id, || {
            storage::write_pool(&e, &pool);
            storage::write_pool_id(&e, &pool_id);
            storage::write_asset(
                &e,
                &Asset {
                    res_index,
                    id: BytesN::<32>::random(&e),
                },
            );

            require_noncollateralized(&e, &samwise);
        });
    }

    #[test]
    #[should_panic]
    fn test_require_noncollateralized_panics_if_collateralized() {
        let e = Env::default();
        let token_id = BytesN::<32>::random(&e);

        let res_index = 7;
        let (pool_id, pool_client) = create_lending_pool(&e);
        let pool = Address::from_contract_id(&e, &pool_id);

        let samwise = Address::random(&e);

        pool_client.set_collat(&samwise, &res_index, &true);
        e.as_contract(&token_id, || {
            storage::write_pool(&e, &pool);
            storage::write_pool_id(&e, &pool_id);
            storage::write_asset(
                &e,
                &Asset {
                    res_index,
                    id: BytesN::<32>::random(&e),
                },
            );

            require_noncollateralized(&e, &samwise);
        });
    }
}
