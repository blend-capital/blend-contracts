use crate::{
    dependencies::TokenClient, errors::PoolError, reserve::Reserve, reserve_usage::ReserveUsage,
    storage, user_data::UserAction, validator::require_hf,
};
use soroban_sdk::{Address, BytesN, Env};

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

#[cfg(test)]
mod tests {
    use crate::{
        auctions::AuctionData,
        dependencies::{TokenClient, B_TOKEN_WASM, D_TOKEN_WASM},
        pool::{execute_borrow, execute_initialize, initialize_reserve},
        storage::{PoolConfig, ReserveMetadata},
        testutils::{create_mock_oracle, create_reserve, create_token_contract, setup_reserve},
    };

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as _, BytesN as _},
    };

    #[test]
    fn test_supply() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let backstop_id = BytesN::<32>::random(&e);
        let blnd_id = BytesN::<32>::random(&e);
        let usdc_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        let b_token_hash = e.install_contract_wasm(B_TOKEN_WASM);
        let d_token_hash = e.install_contract_wasm(D_TOKEN_WASM);
        e.as_contract(&pool_id, || {
            execute_initialize(
                &e,
                &bombadil,
                &oracle_id,
                &0_200_000_000,
                &backstop_id,
                &b_token_hash,
                &d_token_hash,
                &blnd_id,
                &usdc_id,
            )
            .unwrap();
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
            e.budget().reset_unlimited();
            execute_supply(&e, &samwise, &asset_id_0, 100_0000000).unwrap();
            execute_supply(&e, &frodo, &asset_id_1, 500_0000000).unwrap();
            assert_eq!(400_0000000, asset_0_client.balance(&samwise));
            assert_eq!(0, asset_1_client.balance(&frodo));
            assert_eq!(100_0000000, asset_0_client.balance(&pool));
            assert_eq!(500_0000000, asset_1_client.balance(&pool));
            let user_config = ReserveUsage::new(storage::get_user_config(&e, &samwise));
            assert!(user_config.is_collateral(0));
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

    #[test]
    fn test_pool_supply_checks_status() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_supply = 0;
        reserve_0.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &1_0000000);

        let asset_0_client = TokenClient::new(&e, &reserve_0.asset);
        asset_0_client.mint(&bombadil, &samwise, &500_0000000);

        let mut pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 1,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);
            e.budget().reset_unlimited();

            // can supply on ice
            execute_supply(&e, &samwise, &reserve_0.asset, 100_0000000).unwrap();
            assert_eq!(400_0000000, asset_0_client.balance(&samwise));
            assert_eq!(100_0000000, asset_0_client.balance(&pool));

            // can't supply if frozen
            pool_config.status = 2;
            storage::set_pool_config(&e, &pool_config);
            let result = execute_supply(&e, &samwise, &reserve_0.asset, 100_0000000);
            assert_eq!(result, Err(PoolError::InvalidPoolStatus));
        });
    }
}
