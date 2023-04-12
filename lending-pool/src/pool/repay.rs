use crate::{
    dependencies::TokenClient, errors::PoolError, reserve::Reserve, reserve_usage::ReserveUsage,
    storage, user_data::UserAction, validator::require_hf,
};
use soroban_sdk::{Address, BytesN, Env};

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
        to_burn = reserve.to_d_token_down(amount);
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

#[cfg(test)]
mod tests {
    use std::println;

    use crate::{
        auctions::AuctionData,
        dependencies::TokenClient,
        pool::{execute_borrow, execute_supply},
        storage::PoolConfig,
        testutils::{create_mock_oracle, create_reserve, setup_reserve},
    };

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as _, BytesN as _},
    };

    #[test]
    #[should_panic]
    fn test_repay_no_liability() {
        // TODO: better error handling on token transfer failures
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
        asset_0_client.mint(&bombadil, &frodo, &500_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();
            execute_supply(&e, &frodo, &reserve_0.asset, 100_0000000).unwrap();

            // should panic
            execute_repay(&e, &samwise, &reserve_0.asset, 1, &samwise).unwrap();
        });
    }

    #[test]
    fn test_repay() {
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
        let res_1_d_token_client = TokenClient::new(&e, &reserve_1.config.d_token);

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
            execute_borrow(&e, &samwise, &reserve_1.asset, 50_0000000, &samwise).unwrap();

            // repay partial
            execute_repay(&e, &samwise, &reserve_1.asset, 20_0000000, &samwise).unwrap();
            assert_eq!(asset_1_client.balance(&samwise), 30_0000000);
            assert_eq!(asset_1_client.balance(&pool), 100_0000000 - 30_0000000);
            assert_eq!(res_1_d_token_client.balance(&samwise), 30_0000000);
            let config = ReserveUsage::new(storage::get_user_config(&e, &samwise));
            assert!(config.is_liability(1));

            // repay all
            execute_repay(&e, &samwise, &reserve_1.asset, i128::MAX, &samwise).unwrap();
            assert_eq!(asset_1_client.balance(&samwise), 0);
            assert_eq!(asset_1_client.balance(&pool), 100_0000000);
            assert_eq!(res_1_d_token_client.balance(&samwise), 0);
            let config = ReserveUsage::new(storage::get_user_config(&e, &samwise));
            assert!(!config.is_liability(1));
        });
    }

    #[test]
    fn test_repay_rounds_d_tokens_down() {
        let e = Env::default();
        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let sauron = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_supply = 2_0000002;
        reserve_0.data.b_supply = 8_0000000;
        reserve_0.data.d_rate = 1_250000000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let (_oracle_id, oracle_client) = create_mock_oracle(&e);
        oracle_client.set_price(&reserve_0.asset, &1_0000000);

        let asset_0_client = TokenClient::new(&e, &reserve_0.asset);
        asset_0_client.mint(&bombadil, &pool, &6_1999998); // supplied by samwise
        asset_0_client.mint(&bombadil, &samwise, &(1_8000000 + 7000000)); //borrowed by samwise + extra for repayment
        asset_0_client.mint(&bombadil, &sauron, &(2 + 3)); //2 borrowed by sauron + extra for repayment
        let b_token0_client = TokenClient::new(&e, &reserve_0.config.b_token);
        b_token0_client.mint(&pool, &samwise, &8_0000000); //supplied by samwise
        let d_token0_client = TokenClient::new(&e, &reserve_0.config.d_token);
        d_token0_client.mint(&pool, &samwise, &2_0000000); //borrowed by samwise
        d_token0_client.mint(&pool, &sauron, &2); //borrowed by sauron
        e.budget().reset_unlimited();

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
            println!("in test");
            storage::set_pool_config(&e, &pool_config);

            e.budget().reset_unlimited();

            //supply as collateral
            execute_supply(&e, &sauron, &reserve_1.asset, 10_0000000).unwrap();
            execute_supply(&e, &samwise, &reserve_1.asset, 10_0000000).unwrap();

            //repay - unrounded
            let result =
                execute_repay(&e, &samwise, &reserve_0.asset, 1_0000000, &samwise).unwrap();
            assert_eq!(result, 8000000);
            assert_eq!(8000000 + 7000000, asset_0_client.balance(&samwise));
            assert_eq!(1_2000000, d_token0_client.balance(&samwise));
            let result2 =
                execute_repay(&e, &samwise, &reserve_0.asset, i128::MAX, &samwise).unwrap();
            assert_eq!(result2, 1_2000000);
            assert_eq!(0, asset_0_client.balance(&samwise));
            assert_eq!(1_2000000 - result2, d_token0_client.balance(&samwise));

            // borrow - rounded
            let result3 = execute_repay(&e, &sauron, &reserve_0.asset, 1, &sauron).unwrap();
            assert_eq!(result3, 0);
            assert_eq!(4, asset_0_client.balance(&sauron));
            assert_eq!(2, d_token0_client.balance(&sauron));
            let result4 = execute_repay(&e, &sauron, &reserve_0.asset, 2, &sauron).unwrap();
            assert_eq!(result4, 1);
            assert_eq!(2, asset_0_client.balance(&sauron));
            assert_eq!(1, d_token0_client.balance(&sauron));
            let result4 = execute_repay(&e, &sauron, &reserve_0.asset, i128::MAX, &sauron).unwrap();
            assert_eq!(result4, 1);
            assert_eq!(0, asset_0_client.balance(&sauron));
            assert_eq!(0, d_token0_client.balance(&sauron));
        });
    }

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
