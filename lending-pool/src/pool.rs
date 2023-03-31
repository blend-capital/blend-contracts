use crate::{
    dependencies::{BackstopClient, BlendTokenClient, TokenClient},
    emissions,
    errors::PoolError,
    reserve::Reserve,
    reserve_usage::ReserveUsage,
    storage::{self, PoolConfig, ReserveConfig, ReserveData, ReserveMetadata},
    user_data::UserAction,
    validator::{require_hf, require_util_under_cap},
};
use soroban_sdk::{Address, BytesN, Env, IntoVal};

/// Initialize the pool
pub fn execute_initialize(
    e: &Env,
    admin: &Address,
    oracle: &BytesN<32>,
    backstop_id: &BytesN<32>,
    backstop: &Address,
    bstop_rate: &u64,
    b_token_hash: &BytesN<32>,
    d_token_hash: &BytesN<32>
) -> Result<(), PoolError> {
    if storage::has_admin(e) {
        return Err(PoolError::AlreadyInitialized);
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
    storage::set_token_hashes(e, b_token_hash, d_token_hash);
    Ok(())
}

/// Initialize a reserve for the pool
pub fn initialize_reserve(e: &Env, from: &Address, asset: &BytesN<32>, metadata: &ReserveMetadata) -> Result<(), PoolError> {
    if storage::has_res(e, asset) {
        return Err(PoolError::AlreadyInitialized);
    }

    if from.clone() != storage::get_admin(e) {
        return Err(PoolError::NotAuthorized);
    }

    let (b_token_hash, d_token_hash) = storage::get_token_hashes(e);
    // force consistent d and b token addresses based on underlying asset
    let deployer = e.deployer();
    let mut b_token_salt: BytesN<32> = asset.clone();
    let mut d_token_salt: BytesN<32>  = asset.clone();
    b_token_salt.set(0, 0);
    d_token_salt.set(0, 1);
    let b_token_id = deployer.with_current_contract(&b_token_salt).deploy(&b_token_hash);
    let d_token_id = deployer.with_current_contract(&d_token_salt).deploy(&d_token_hash);

    let index = storage::push_res_list(e, asset);
    let reserve_config = ReserveConfig {
        b_token: b_token_id.clone(),
        d_token: d_token_id.clone(),
        index,
        decimals: metadata.decimals,
        c_factor: metadata.c_factor,
        l_factor: metadata.l_factor,
        util: metadata.util,
        max_util: metadata.max_util,
        r_one: metadata.r_one,
        r_two: metadata.r_two,
        r_three: metadata.r_three,
        reactivity: metadata.reactivity,
    };
    storage::set_res_config(e, asset, &reserve_config);
    let init_data = ReserveData {
        d_rate: 1_000_000_000,
        ir_mod: 1_000_000_000,
        d_supply: 0,
        b_supply: 0,
        last_block: e.ledger().sequence(),
    };
    storage::set_res_data(e, asset, &init_data);

    // initialize tokens
    let asset_client = TokenClient::new(e, asset);
    let asset_symbol = asset_client.symbol();

    let b_token_client = BlendTokenClient::new(e, &b_token_id);
    let mut b_token_symbol = asset_symbol.clone();
    b_token_symbol.insert_from_bytes(0, "b".into_val(e));
    let mut b_token_name = asset_symbol.clone();
    b_token_name.insert_from_bytes(0, "Blend supply token for ".into_val(e));
    b_token_client.initialize(&e.current_contract_address(), &7, &b_token_name, &b_token_symbol);
    b_token_client.init_asset(&e.current_contract_address(), &e.current_contract_id(), &asset, &index);

    let d_token_client = BlendTokenClient::new(e, &d_token_id);
    let mut d_token_symbol = asset_symbol.clone();
    d_token_symbol.insert_from_bytes(0, "d".into_val(e));
    let mut d_token_name = asset_symbol.clone();
    d_token_name.insert_from_bytes(0, "Blend debt token for ".into_val(e));
    d_token_client.initialize(&e.current_contract_address(), &7, &b_token_name, &b_token_symbol);
    d_token_client.init_asset(&e.current_contract_address(), &e.current_contract_id(), &asset, &index);

    Ok(())
}

/// Update a reserve in the pool
pub fn execute_update_reserve(e: &Env, from: &Address, asset: &BytesN<32>, metadata: &ReserveMetadata) -> Result<(), PoolError> {
    if from.clone() != storage::get_admin(e) {
        return Err(PoolError::NotAuthorized);
    }

    let pool_config = storage::get_pool_config(e);

    if pool_config.status == 2 {
        return Err(PoolError::InvalidPoolStatus);
    }

    let mut reserve = Reserve::load(&e, asset.clone());
    reserve.update_rates_and_mint_backstop(e, &pool_config)?;

    // only change metadata based configuration
    reserve.config.decimals = metadata.decimals;
    reserve.config.c_factor = metadata.c_factor;
    reserve.config.l_factor = metadata.l_factor;
    reserve.config.util = metadata.util;
    reserve.config.max_util = metadata.max_util;
    reserve.config.r_one = metadata.r_one;
    reserve.config.r_two = metadata.r_two;
    reserve.config.r_three = metadata.r_three;
    reserve.config.reactivity = metadata.reactivity;

    storage::set_res_config(e, asset, &reserve.config);
    reserve.set_data(e);

    Ok(())
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
        dependencies::{TokenClient, D_TOKEN_WASM, B_TOKEN_WASM},
        testutils::{create_mock_oracle, create_reserve, setup_reserve, create_token_contract},
    };

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as _, BytesN as _},
    };

    /***** Setup ******/

    #[test]
    fn test_initialize_reserve() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let bombadil = Address::random(&e);
        let sauron = Address::random(&e);
        let (asset_id_0, _) = create_token_contract(&e, &bombadil);
        let (asset_id_1, _) = create_token_contract(&e, &bombadil);

        let b_token_hash = e.install_contract_wasm(B_TOKEN_WASM);
        let d_token_hash = e.install_contract_wasm(D_TOKEN_WASM);

        let metadata = ReserveMetadata {
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_5000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        e.as_contract(&pool_id, || {
            storage::set_token_hashes(&e, &b_token_hash, &d_token_hash);
            storage::set_admin(&e, &bombadil);

            initialize_reserve(&e, &bombadil, &asset_id_0, &metadata).unwrap();

            // if already exists blocks
            let result = initialize_reserve(&e, &bombadil, &asset_id_0, &metadata);
            assert_eq!(result, Err(PoolError::AlreadyInitialized));

            // only admin
            let result = initialize_reserve(&e, &sauron, &asset_id_1, &metadata);
            assert_eq!(result, Err(PoolError::NotAuthorized));

            initialize_reserve(&e, &bombadil, &asset_id_1, &metadata).unwrap();

            let res_config_0 = storage::get_res_config(&e, &asset_id_0);
            let res_config_1 = storage::get_res_config(&e, &asset_id_1);
            assert_eq!(res_config_0.decimals, metadata.decimals);
            assert_eq!(res_config_0.c_factor, metadata.c_factor);
            assert_eq!(res_config_0.l_factor, metadata.l_factor);
            assert_eq!(res_config_0.util, metadata.util);
            assert_eq!(res_config_0.max_util, metadata.max_util);
            assert_eq!(res_config_0.r_one, metadata.r_one);
            assert_eq!(res_config_0.r_two, metadata.r_two);
            assert_eq!(res_config_0.r_three, metadata.r_three);
            assert_eq!(res_config_0.reactivity, metadata.reactivity);
            assert_eq!(res_config_0.index, 0);
            assert_eq!(res_config_1.index, 1);

            assert_ne!(res_config_0.b_token, res_config_1.b_token);
            assert_ne!(res_config_0.d_token, res_config_1.d_token);
        });
    }

    /***** Supply *****/

    #[test]
    fn test_supply() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let backstop_id = BytesN::<32>::random(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        let b_token_hash = e.install_contract_wasm(B_TOKEN_WASM);
        let d_token_hash = e.install_contract_wasm(D_TOKEN_WASM);
        e.as_contract(&pool_id, || {
            execute_initialize(&e, &bombadil, &oracle_id, &backstop_id, &backstop, &0_200_000_000, &b_token_hash, &d_token_hash).unwrap();
        });

        let metadata = ReserveMetadata {
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_5000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        let (asset_id_0, asset_0_client) = create_token_contract(&e, &bombadil);
        let (asset_id_1, asset_1_client) = create_token_contract(&e, &bombadil);
        e.as_contract(&pool_id, || {
            initialize_reserve(&e, &bombadil, &asset_id_0, &metadata).unwrap();
            initialize_reserve(&e, &bombadil, &asset_id_1, &metadata).unwrap();
        });

        oracle_client.set_price(&asset_id_0, &1_0000000);
        oracle_client.set_price(&asset_id_1, &1_0000000);
        asset_0_client.mint(&bombadil, &samwise, &500_0000000);
        asset_1_client.mint(&bombadil, &frodo, &500_0000000);

        e.as_contract(&pool_id, || {
            e.budget().reset();
            execute_supply(&e, &samwise, &asset_id_0, 100_0000000).unwrap();
            execute_supply(&e, &frodo, &asset_id_1, 500_0000000).unwrap();
            assert_eq!(400_0000000, asset_0_client.balance(&samwise));
            assert_eq!(0, asset_1_client.balance(&frodo));
            assert_eq!(100_0000000, asset_0_client.balance(&pool));
            assert_eq!(500_0000000, asset_1_client.balance(&pool));
        });
    }

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

            e.budget().reset();
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

            e.budget().reset();
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

            e.budget().reset();
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

            e.budget().reset();
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
