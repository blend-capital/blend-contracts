use crate::{
    dependencies::TokenClient,
    errors::PoolError,
    reserve::Reserve,
    reserve_usage::ReserveUsage,
    storage,
    user_data::UserAction,
    validator::{require_hf, require_nonnegative, require_util_under_cap},
};
use soroban_sdk::{Address, BytesN, Env};

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
    require_nonnegative(amount)?;
    let pool_config = storage::get_pool_config(e);

    if pool_config.status > 0 {
        return Err(PoolError::InvalidPoolStatus);
    }

    if storage::has_auction(e, &0, &from) {
        return Err(PoolError::AuctionInProgress);
    }

    let mut reserve = Reserve::load(&e, asset.clone());
    reserve.pre_action(&e, &pool_config, 0, from.clone())?;

    let mut to_mint = reserve.to_d_token_up(amount);
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

#[cfg(test)]
mod tests {
    use crate::{
        auctions::AuctionData,
        dependencies::TokenClient,
        pool::execute_supply,
        storage::PoolConfig,
        testutils::{create_mock_oracle, create_reserve, setup_reserve},
    };

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as _, BytesN as _},
    };

    #[test]
    fn test_borrow_negative_amount() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_supply = 0;
        reserve_0.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &1_0000000);

        let asset_0_client = TokenClient::new(&e, &reserve_0.asset);
        asset_0_client.mint(&bombadil, &frodo, &500_0000000); // for samwise to borrow

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            execute_supply(&e, &frodo, &reserve_0.asset, 100_0000000).unwrap();

            let result = execute_borrow(&e, &samwise, &reserve_0.asset, -1, &samwise);
            assert_eq!(result, Err(PoolError::NegativeAmount));
        });
    }

    #[test]
    fn test_borrow_no_collateral() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_supply = 0;
        reserve_0.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &1_0000000);

        let asset_0_client = TokenClient::new(&e, &reserve_0.asset);
        asset_0_client.mint(&bombadil, &frodo, &500_0000000); // for samwise to borrow

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            execute_supply(&e, &frodo, &reserve_0.asset, 100_0000000).unwrap();

            let result = execute_borrow(&e, &samwise, &reserve_0.asset, 1, &samwise);
            assert_eq!(result, Err(PoolError::InvalidHf));
        });
    }

    #[test]
    fn test_borrow_checks_hf() {
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
        oracle_client.set_price(&reserve_0.asset, &2_0000000);
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
            execute_supply(&e, &frodo, &reserve_1.asset, 100_0000000).unwrap();
            execute_supply(&e, &samwise, &reserve_0.asset, 50_0000000).unwrap();

            // borrow - fail HF check
            let result = execute_borrow(&e, &samwise, &reserve_1.asset, 56_2500000, &samwise);
            assert_eq!(result, Err(PoolError::InvalidHf));

            // borrow - pass HF check
            execute_borrow(&e, &samwise, &reserve_1.asset, 56_2490000, &samwise).unwrap();
            assert_eq!(asset_1_client.balance(&samwise), 56_2490000);
            assert_eq!(asset_1_client.balance(&pool), 100_0000000 - 56_2490000);
            assert_eq!(
                TokenClient::new(&e, &reserve_1.config.d_token).balance(&samwise),
                56_2490000
            );
            let user_config = ReserveUsage::new(storage::get_user_config(&e, &samwise));
            assert!(user_config.is_liability(1));
        });
    }

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

    #[test]
    fn test_borrow_rounds_d_tokens_up() {
        let e = Env::default();
        e.budget().reset_unlimited();

        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let sauron = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_supply = 1;
        reserve_0.data.b_supply = 8;
        reserve_0.data.d_rate = 1_250000000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let (_oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &1_0000000);

        let asset_0_client = TokenClient::new(&e, &reserve_0.asset);
        asset_0_client.mint(&bombadil, &pool, &7_0000000); // supplied by samwise
        asset_0_client.mint(&bombadil, &samwise, &1_0000000); //borrowed by samwise
        let b_token0_client = TokenClient::new(&e, &reserve_0.config.b_token);
        b_token0_client.mint(&pool, &samwise, &8_0000000); //supplied by samwise
        let d_token0_client = TokenClient::new(&e, &reserve_0.config.d_token);
        d_token0_client.mint(&pool, &samwise, &1_0000000); //borrowed by samwise
        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_supply = 0;
        reserve_1.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_1.asset, &1_0000000);

        let asset_1_client = TokenClient::new(&e, &reserve_1.asset);
        asset_1_client.mint(&bombadil, &samwise, &10_0000000); //collateral for samwise
        asset_1_client.incr_allow(&samwise, &pool, &i128::MAX);
        asset_1_client.mint(&bombadil, &sauron, &10_0000000); //collateral for sauron
        asset_1_client.incr_allow(&sauron, &pool, &i128::MAX);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            //supply as collateral
            execute_supply(&e, &sauron, &reserve_1.asset, 10_0000000).unwrap();
            execute_supply(&e, &samwise, &reserve_1.asset, 10_0000000).unwrap();

            //borrow - unrounded
            let result =
                execute_borrow(&e, &samwise, &reserve_0.asset, 1_0000000, &samwise).unwrap();
            assert_eq!(result, 8000000);
            assert_eq!(2_0000000, asset_0_client.balance(&samwise));
            assert_eq!(result + 1_0000000, d_token0_client.balance(&samwise));

            // borrow - rounded
            let result = execute_borrow(&e, &sauron, &reserve_0.asset, 2, &sauron).unwrap();
            assert_eq!(result, 2);
            assert_eq!(result, asset_0_client.balance(&sauron));
            assert_eq!(result, d_token0_client.balance(&sauron));
        });
    }
    #[test]
    fn test_pool_borrow_checks_status() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_supply = 0;
        reserve_0.data.b_supply = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);

        let (oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &2_0000000);

        let asset_0_client = TokenClient::new(&e, &reserve_0.asset);
        asset_0_client.mint(&bombadil, &samwise, &500_0000000);

        let mut pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            execute_supply(&e, &samwise, &reserve_0.asset, 50_0000000).unwrap();

            // can't borrow on ice
            pool_config.status = 1;
            storage::set_pool_config(&e, &pool_config);
            let result = execute_borrow(&e, &samwise, &reserve_0.asset, 1_0000000, &samwise);
            assert_eq!(result, Err(PoolError::InvalidPoolStatus));

            // can't borrow if frozen
            pool_config.status = 2;
            storage::set_pool_config(&e, &pool_config);
            let result = execute_borrow(&e, &samwise, &reserve_0.asset, 1_0000000, &samwise);
            assert_eq!(result, Err(PoolError::InvalidPoolStatus));
        });
    }
}
