use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{Address, Env};

use crate::{
    constants::SCALAR_7,
    errors::PoolError,
    reserve::Reserve,
    storage::PoolConfig,
    user_data::{UserAction, UserData},
};

/// Require that user is currently healthy given an incoming actions.
///
/// ### Arguments
/// * `oracle` - The oracle address
/// * `user` - The user to check
/// * `user_action` - An incoming user action
///
/// ### Errors
/// If the user's health factor is not at least 1.05
pub fn require_hf(
    e: &Env,
    pool_config: &PoolConfig,
    user: &Address,
    user_action: &UserAction,
) -> Result<(), PoolError> {
    let account_data = UserData::load(e, pool_config, user, &user_action);
    // Note: User is required to have at least 5% excess collateral in order to undertake an action that would reduce their health factor
    let collateral_required = account_data
        .liability_base
        .clone()
        .fixed_mul_ceil(1_0500000, SCALAR_7)
        .unwrap();
    if collateral_required > account_data.collateral_base && account_data.liability_base != 0 {
        return Err(PoolError::InvalidHf);
    }
    Ok(())
}

/// Require that an incoming action does not exceed the utilization cap for the reserve
///
/// ### Arguments
/// * `reserve` - The reserve
/// * `user_action` - An incoming user action
///
/// ### Errors
/// If the action causes the reserve's utilization to exceed the cap
pub fn require_util_under_cap(
    e: &Env,
    reserve: &mut Reserve,
    user_action: &UserAction,
) -> Result<(), PoolError> {
    let mut user_action_supply: i128 = 0;
    let mut user_action_liabilities: i128 = 0;
    if user_action.b_token_delta != 0 {
        user_action_supply = reserve.to_asset_from_b_token(e, user_action.b_token_delta);
    }
    if user_action.d_token_delta != 0 {
        user_action_liabilities = reserve.to_asset_from_d_token(user_action.d_token_delta);
    }
    let temp = reserve.total_liabilities() + user_action_liabilities;
    let temp2 = reserve.total_supply(e) + user_action_supply;
    let util = (reserve.total_liabilities() + user_action_liabilities)
        .fixed_div_floor(reserve.total_supply(e) + user_action_supply, SCALAR_7)
        .unwrap();
    if util > i128(reserve.config.max_util) {
        return Err(PoolError::InvalidUtilRate);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use soroban_sdk::testutils::Address as _;

    use crate::dependencies::TokenClient;
    use crate::storage;
    use crate::testutils::{
        create_mock_oracle, create_reserve, generate_contract_id, setup_reserve,
    };

    use super::*;

    #[test]
    fn test_require_hf() {
        let e = Env::default();
        let pool_id = generate_contract_id(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &10_0000000);
        oracle_client.set_price(&reserve_1.asset, &5_0000000);

        // setup user (collateralize reserve 0 and borrow reserve 1)
        let collateral_amount = 25_0000000;
        let liability_amount = 26_0000000;
        let additional_liability = 1_0000000;
        e.as_contract(&pool_id, || {
            storage::set_user_config(&e, &samwise, &0x000000000000000A);

            TokenClient::new(&e, &reserve_0.config.b_token).mint(
                &e.current_contract_address(),
                &samwise,
                &collateral_amount,
            );
            TokenClient::new(&e, &reserve_1.config.d_token).mint(
                &e.current_contract_address(),
                &samwise,
                &liability_amount,
            );
        });

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        let mut user_action = UserAction {
            asset: reserve_1.asset.clone(),
            d_token_delta: additional_liability,
            b_token_delta: 0,
        };
        e.as_contract(&pool_id, || {
            let result = require_hf(&e, &pool_config, &samwise, &user_action);
            assert_eq!(result, Err(PoolError::InvalidHf));
        });

        user_action.d_token_delta = 0_5000000;
        e.as_contract(&pool_id, || {
            let result = require_hf(&e, &pool_config, &samwise, &user_action);
            assert_eq!(result, Ok(()));
        });
    }

    #[test]
    fn test_require_utilization_under_cap() {
        let e = Env::default();
        let pool_id = generate_contract_id(&e);

        let bombadil = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);

        let mut user_action = UserAction {
            asset: reserve_0.asset.clone(),
            d_token_delta: 20_0000000,
            b_token_delta: 0,
        };
        e.as_contract(&pool_id, || {
            let result = require_util_under_cap(&e, &mut reserve_0, &user_action);
            assert_eq!(result, Ok(()));
        });

        user_action.d_token_delta = 20_0000100;
        e.as_contract(&pool_id, || {
            let result = require_util_under_cap(&e, &mut reserve_0, &user_action);
            assert_eq!(result, Err(PoolError::InvalidUtilRate));
        });
    }
}
