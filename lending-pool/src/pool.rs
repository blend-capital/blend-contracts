use crate::{
    dependencies::{BackstopClient, TokenClient},
    emissions,
    errors::PoolError,
    reserve::Reserve,
    reserve_usage::ReserveUsage,
    storage::{self, PoolConfig, ReserveConfig, ReserveData},
    user_data::UserAction,
    validator::{require_hf, require_util_under_cap},
};
use soroban_sdk::{Address, BytesN, Env};

/// Initialize the pool
pub fn execute_initialize(
    e: &Env,
    admin: &Address,
    oracle: &BytesN<32>,
    backstop_id: &BytesN<32>,
    backstop: &Address,
    bstop_rate: &u64,
) {
    if storage::has_admin(e) {
        panic!("already initialized")
    }

    storage::set_admin(e, admin);
    storage::set_backstop(e, backstop_id);
    storage::set_backstop_address(e, backstop);
    storage::set_pool_config(
        e,
        &PoolConfig {
            oracle: oracle.clone(),
            bstop_rate: bstop_rate.clone(),
            status: 1,
        },
    );
}

// @dev: This function will be reworked - used for testing purposes
/// Initialize a reserve for the pool
pub fn initialize_reserve(e: &Env, from: &Address, asset: &BytesN<32>, config: &ReserveConfig) {
    if storage::has_res(e, asset) {
        panic!("already initialized")
    }

    if from.clone() != storage::get_admin(e) {
        panic!("not authorized")
    }

    storage::set_res_config(e, asset, config);
    let init_data = ReserveData {
        d_rate: 1_000_000_000,
        ir_mod: 1_000_000_000,
        d_supply: 0,
        b_supply: 0,
        last_block: e.ledger().sequence(),
    };
    storage::set_res_data(e, asset, &init_data);
}

/// Perform a supply of "asset" from "from" for "amount" into the pool
///
/// Returns the number of b_tokens minted
pub fn execute_supply(
    e: &Env,
    from: &Address,
    asset: &BytesN<32>,
    amount: i128,
) -> Result<i128, PoolError> {
    let pool_config = storage::get_pool_config(e);

    if pool_config.status == 2 {
        return Err(PoolError::InvalidPoolStatus);
    }

    let mut reserve = Reserve::load(&e, asset.clone());
    reserve.pre_action(&e, &pool_config, 1, from.clone())?;

    let to_mint = reserve.to_b_token(e, amount.clone());
    if storage::has_auction(e, &0, &from) {
        let user_action = UserAction {
            asset: asset.clone(),
            b_token_delta: to_mint,
            d_token_delta: 0,
        };
        require_hf(&e, &pool_config, &from, &user_action)?;
        storage::del_auction(e, &0, &from);
    }

    TokenClient::new(&e, asset).xfer(from, &e.current_contract_address(), &amount);
    TokenClient::new(&e, &reserve.config.b_token).mint(
        &e.current_contract_address(),
        &from,
        &to_mint,
    );

    let mut user_config = ReserveUsage::new(storage::get_user_config(e, from));
    if !user_config.is_supply(reserve.config.index) {
        user_config.set_supply(reserve.config.index, true);
        storage::set_user_config(e, from, &user_config.config);
    }

    reserve.add_supply(&to_mint);
    reserve.set_data(&e);

    Ok(to_mint)
}

/// Perform a withdraw of "asset" from "from" of "amount" to "to"
///
/// Returns the number of b_tokens burnt
pub fn execute_withdraw(
    e: &Env,
    from: &Address,
    asset: &BytesN<32>,
    amount: i128,
    to: &Address,
) -> Result<i128, PoolError> {
    let pool_config = storage::get_pool_config(e);

    if storage::has_auction(e, &0, &from) {
        return Err(PoolError::AuctionInProgress);
    }

    let mut reserve = Reserve::load(&e, asset.clone());
    reserve.pre_action(&e, &pool_config, 1, from.clone())?;

    let mut to_burn: i128;
    let to_return: i128;
    let b_token_client = TokenClient::new(&e, &reserve.config.b_token);
    if amount == i128::MAX {
        // if they input i128::MAX as the burn amount, burn 100% of their holdings
        to_burn = b_token_client.balance(&from);
        to_return = reserve.to_asset_from_b_token(e, to_burn);
    } else {
        to_burn = reserve.to_b_token(e, amount);
        if to_burn == 0 {
            to_burn = 1
        };
        to_return = amount;
    }

    let user_action = UserAction {
        asset: asset.clone(),
        b_token_delta: -to_burn,
        d_token_delta: 0,
    };
    require_hf(&e, &pool_config, &from, &user_action)?;

    b_token_client.clawback(&e.current_contract_address(), &from, &to_burn);

    TokenClient::new(&e, asset).xfer(&e.current_contract_address(), &to, &to_return);

    let mut user_config = ReserveUsage::new(storage::get_user_config(e, from));
    if b_token_client.balance(&from) == 0 {
        user_config.set_supply(reserve.config.index, false);
        storage::set_user_config(e, from, &user_config.config);
    }

    reserve.remove_supply(&to_burn);
    reserve.set_data(&e);

    Ok(to_burn)
}

/// Perform a borrow of "asset" from the pool of "amount" to "to" with the liabilities tracked to "from"
///
/// Returns the number of d_tokens minted
pub fn execute_borrow(
    e: &Env,
    from: &Address,
    asset: &BytesN<32>,
    amount: i128,
    to: &Address,
) -> Result<i128, PoolError> {
    let pool_config = storage::get_pool_config(e);

    if pool_config.status > 0 {
        return Err(PoolError::InvalidPoolStatus);
    }

    if storage::has_auction(e, &0, &from) {
        return Err(PoolError::AuctionInProgress);
    }

    let mut reserve = Reserve::load(&e, asset.clone());
    reserve.pre_action(&e, &pool_config, 0, from.clone())?;

    let mut to_mint = reserve.to_d_token(amount);
    if to_mint == 0 {
        to_mint = 1
    };
    let user_action = UserAction {
        asset: asset.clone(),
        b_token_delta: 0,
        d_token_delta: to_mint,
    };
    require_util_under_cap(e, &mut reserve, &user_action)?;
    require_hf(&e, &pool_config, &from, &user_action)?;

    TokenClient::new(&e, &reserve.config.d_token).mint(
        &e.current_contract_address(),
        &from,
        &to_mint,
    );
    TokenClient::new(&e, asset).xfer(&e.current_contract_address(), &to, &amount);

    let mut user_config = ReserveUsage::new(storage::get_user_config(e, from));
    if !user_config.is_liability(reserve.config.index) {
        user_config.set_liability(reserve.config.index, true);
        storage::set_user_config(e, from, &user_config.config);
    }

    reserve.add_liability(&to_mint);
    reserve.set_data(&e);

    Ok(to_mint)
}

/// Perform a repayment of "asset" from "from" for "amount" to be credited for "on_behalf_of"
///
/// Returns the number of d_tokens burnt
pub fn execute_repay(
    e: &Env,
    from: &Address,
    asset: &BytesN<32>,
    amount: i128,
    on_behalf_of: &Address,
) -> Result<i128, PoolError> {
    let pool_config = storage::get_pool_config(e);

    let mut reserve = Reserve::load(&e, asset.clone());
    reserve.pre_action(&e, &pool_config, 0, from.clone())?;

    let d_token_client = TokenClient::new(&e, &reserve.config.d_token);
    let to_burn: i128;
    let to_repay: i128;
    if amount == i128::MAX {
        // if they input i128::MAX as the repay amount, burn 100% of their holdings
        to_burn = d_token_client.balance(&from);
        to_repay = reserve.to_asset_from_d_token(to_burn);
    } else {
        to_burn = reserve.to_d_token(amount);
        to_repay = amount;
    }
    if storage::has_auction(e, &0, &from) {
        let user_action = UserAction {
            asset: asset.clone(),
            b_token_delta: 0,
            d_token_delta: -to_burn,
        };
        require_hf(&e, &pool_config, &from, &user_action)?;
        storage::del_auction(e, &0, &from);
    }

    TokenClient::new(e, &reserve.asset).xfer(from, &e.current_contract_address(), &to_repay);
    d_token_client.clawback(&e.current_contract_address(), on_behalf_of, &to_burn);

    let mut user_config = ReserveUsage::new(storage::get_user_config(e, from));
    if d_token_client.balance(&from) == 0 {
        user_config.set_liability(reserve.config.index, false);
        storage::set_user_config(e, from, &user_config.config);
    }

    reserve.remove_liability(&to_burn);
    reserve.set_data(&e);

    Ok(to_burn)
}

/// Update the pool status
pub fn set_pool_status(e: &Env, admin: &Address, pool_status: u32) -> Result<(), PoolError> {
    if admin.clone() != storage::get_admin(e) {
        return Err(PoolError::NotAuthorized);
    }

    let mut pool_config = storage::get_pool_config(e);
    pool_config.status = pool_status;
    storage::set_pool_config(e, &pool_config);

    Ok(())
}

// Update the pool emission information from the backstop
pub fn update_pool_emissions(e: &Env) -> Result<u64, PoolError> {
    let backstop_id = storage::get_backstop(e);
    let backstop_client = BackstopClient::new(e, &backstop_id);
    let next_exp = backstop_client.next_dist();
    let pool_eps = backstop_client.pool_eps(&e.current_contract_id()) as u64;
    emissions::update_emissions(e, next_exp, pool_eps)
}

#[cfg(test)]
mod tests {
    use crate::{
        auctions::AuctionData,
        dependencies::TokenClient,
        testutils::{create_mock_oracle, create_reserve, setup_reserve},
    };

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as _, BytesN as _},
    };

    /***** Supply *****/

    #[test]
    fn test_supply_user_being_liquidated() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_supply = 0;
        reserve_0.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_supply = 0;
        reserve_1.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &1_0000000);
        oracle_client.set_price(&reserve_1.asset, &1_0000000);

        let asset_0_client = TokenClient::new(&e, &reserve_0.asset);
        let asset_1_client = TokenClient::new(&e, &reserve_1.asset);
        asset_0_client.mint(&bombadil, &samwise, &500_0000000);
        asset_1_client.mint(&bombadil, &frodo, &500_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            execute_supply(&e, &frodo, &reserve_1.asset, 500_0000000).unwrap(); // for samwise to borrow
            execute_supply(&e, &samwise, &reserve_0.asset, 100_0000000).unwrap();
            execute_borrow(&e, &samwise, &reserve_1.asset, 50_0000000, &samwise).unwrap();
            assert_eq!(400_0000000, asset_0_client.balance(&samwise));
            assert_eq!(50_0000000, asset_1_client.balance(&samwise));
            assert_eq!(100_0000000, asset_0_client.balance(&pool));
            assert_eq!(450_0000000, asset_1_client.balance(&pool));

            // adjust prices to put samwise underwater
            oracle_client.set_price(&reserve_1.asset, &2_0000000);

            // mock a created liquidation auction
            storage::set_auction(
                &e,
                &0,
                &samwise,
                &AuctionData {
                    bid: map![&e],
                    lot: map![&e],
                    block: e.ledger().sequence(),
                },
            );

            let result = execute_supply(&e, &samwise, &reserve_0.asset, 50_0000000);
            assert_eq!(result, Err(PoolError::InvalidHf));

            execute_supply(&e, &samwise, &reserve_0.asset, 100_0000000).unwrap();
            assert_eq!(300_0000000, asset_0_client.balance(&samwise));
            assert_eq!(50_0000000, asset_1_client.balance(&samwise));
            assert_eq!(200_0000000, asset_0_client.balance(&pool));
            assert_eq!(450_0000000, asset_1_client.balance(&pool));
            assert_eq!(false, storage::has_auction(&e, &0, &samwise));
        });
    }

    /***** Withdraw *****/

    #[test]
    fn test_withdraw_user_being_liquidated() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_supply = 0;
        reserve_0.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_supply = 0;
        reserve_1.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &1_0000000);
        oracle_client.set_price(&reserve_1.asset, &1_0000000);

        let asset_0_client = TokenClient::new(&e, &reserve_0.asset);
        asset_0_client.mint(&bombadil, &samwise, &500_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            execute_supply(&e, &samwise, &reserve_0.asset, 100_0000000).unwrap();
            assert_eq!(400_0000000, asset_0_client.balance(&samwise));
            assert_eq!(100_0000000, asset_0_client.balance(&pool));

            // mock a created liquidation auction
            storage::set_auction(
                &e,
                &0,
                &samwise,
                &AuctionData {
                    bid: map![&e],
                    lot: map![&e],
                    block: e.ledger().sequence(),
                },
            );

            let result = execute_withdraw(&e, &samwise, &reserve_0.asset, 100_0000000, &samwise);
            assert_eq!(result, Err(PoolError::AuctionInProgress));
        });
    }

    /***** Borrow *****/

    #[test]
    fn test_borrow_user_being_liquidated() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_supply = 0;
        reserve_0.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_supply = 0;
        reserve_1.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &1_0000000);
        oracle_client.set_price(&reserve_1.asset, &1_0000000);

        let asset_0_client = TokenClient::new(&e, &reserve_0.asset);
        let asset_1_client = TokenClient::new(&e, &reserve_1.asset);
        asset_0_client.mint(&bombadil, &samwise, &500_0000000);
        asset_1_client.mint(&bombadil, &pool, &500_0000000); // for samwise to borrow

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            execute_supply(&e, &samwise, &reserve_0.asset, 100_0000000).unwrap();
            assert_eq!(400_0000000, asset_0_client.balance(&samwise));
            assert_eq!(100_0000000, asset_0_client.balance(&pool));

            // mock a created liquidation auction
            storage::set_auction(
                &e,
                &0,
                &samwise,
                &AuctionData {
                    bid: map![&e],
                    lot: map![&e],
                    block: e.ledger().sequence(),
                },
            );

            let result = execute_borrow(&e, &samwise, &reserve_0.asset, 50_0000000, &samwise);
            assert_eq!(result, Err(PoolError::AuctionInProgress));
        });
    }

    /***** Repay *****/

    #[test]
    fn test_repay_user_being_liquidated() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_supply = 0;
        reserve_0.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_supply = 0;
        reserve_1.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &1_0000000);
        oracle_client.set_price(&reserve_1.asset, &1_0000000);

        let asset_0_client = TokenClient::new(&e, &reserve_0.asset);
        let asset_1_client = TokenClient::new(&e, &reserve_1.asset);
        asset_0_client.mint(&bombadil, &samwise, &500_0000000);
        asset_1_client.mint(&bombadil, &frodo, &500_0000000); // for samwise to borrow

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            execute_supply(&e, &frodo, &reserve_1.asset, 500_0000000).unwrap(); // for samwise to borrow
            execute_supply(&e, &samwise, &reserve_0.asset, 100_0000000).unwrap();
            execute_borrow(&e, &samwise, &reserve_1.asset, 50_0000000, &samwise).unwrap();
            assert_eq!(400_0000000, asset_0_client.balance(&samwise));
            assert_eq!(50_0000000, asset_1_client.balance(&samwise));
            assert_eq!(100_0000000, asset_0_client.balance(&pool));
            assert_eq!(450_0000000, asset_1_client.balance(&pool));

            // adjust prices to put samwise underwater
            oracle_client.set_price(&reserve_1.asset, &2_0000000);

            // mock a created liquidation auction
            storage::set_auction(
                &e,
                &0,
                &samwise,
                &AuctionData {
                    bid: map![&e],
                    lot: map![&e],
                    block: e.ledger().sequence(),
                },
            );

            let result = execute_repay(&e, &samwise, &reserve_1.asset, 10_0000000, &samwise);
            assert_eq!(result, Err(PoolError::InvalidHf));

            execute_repay(&e, &samwise, &reserve_1.asset, 25_0000000, &samwise).unwrap();
            assert_eq!(400_0000000, asset_0_client.balance(&samwise));
            assert_eq!(25_0000000, asset_1_client.balance(&samwise));
            assert_eq!(100_0000000, asset_0_client.balance(&pool));
            assert_eq!(475_0000000, asset_1_client.balance(&pool));
            assert_eq!(false, storage::has_auction(&e, &0, &samwise));
        });
    }
}
