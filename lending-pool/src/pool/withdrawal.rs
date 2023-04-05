use crate::{
    dependencies::TokenClient, errors::PoolError, reserve::Reserve, reserve_usage::ReserveUsage,
    storage, user_data::UserAction, validator::require_hf,
};
use soroban_sdk::{Address, BytesN, Env};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auctions::AuctionData,
        dependencies::TokenClient,
        pool::{execute_borrow, execute_supply},
        storage::PoolConfig,
        testutils::{create_mock_oracle, create_reserve, setup_reserve},
    };
    use soroban_sdk::{
        map,
        testutils::{Address as _, BytesN as _},
    };

    #[test]
    fn test_pool_withdrawal_checks_status() {
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

            // can withdrawal if frozen
            pool_config.status = 2;
            storage::set_pool_config(&e, &pool_config);
            execute_withdraw(&e, &samwise, &reserve_0.asset, 10_0000000, &samwise).unwrap();
            assert_eq!(asset_0_client.balance(&samwise), 460_0000000);
            assert_eq!(asset_0_client.balance(&pool), 40_0000000);
            assert_eq!(
                TokenClient::new(&e, &reserve_0.config.b_token).balance(&samwise),
                40_0000000
            );
        });
    }

    #[test]
    fn test_pool_withdrawal() {
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
        oracle_client.set_price(&reserve_0.asset, &2_0000000);

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
            execute_supply(&e, &samwise, &reserve_0.asset, 50_0000000).unwrap();

            // partially withdrawal
            execute_withdraw(&e, &samwise, &reserve_0.asset, 10_0000000, &samwise).unwrap();
            assert_eq!(asset_0_client.balance(&samwise), 460_0000000);
            assert_eq!(asset_0_client.balance(&pool), 40_0000000);
            assert_eq!(
                TokenClient::new(&e, &reserve_0.config.b_token).balance(&samwise),
                40_0000000
            );
            let config = ReserveUsage::new(storage::get_user_config(&e, &samwise));
            assert!(config.is_collateral(0));

            // fully withdrawal
            execute_withdraw(&e, &samwise, &reserve_0.asset, i128::MAX, &samwise).unwrap();
            assert_eq!(asset_0_client.balance(&samwise), 500_0000000);
            assert_eq!(asset_0_client.balance(&pool), 0);
            assert_eq!(
                TokenClient::new(&e, &reserve_0.config.b_token).balance(&samwise),
                0
            );
            let config = ReserveUsage::new(storage::get_user_config(&e, &samwise));
            assert!(!config.is_collateral(0));
        });
    }

    #[test]
    #[should_panic]
    fn test_withdrawal_no_supply() {
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

            execute_withdraw(&e, &samwise, &reserve_0.asset, 1, &samwise).unwrap();
        });
    }

    #[test]
    fn test_pool_withdrawal_checks_hf() {
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
            execute_borrow(&e, &samwise, &reserve_1.asset, 45_0000000, &samwise).unwrap();

            // withdrawal - fail HF check
            let result = execute_withdraw(&e, &samwise, &reserve_0.asset, 10_1000000, &samwise);
            assert_eq!(result, Err(PoolError::InvalidHf));

            // withdrawal - pass HF check
            execute_withdraw(&e, &samwise, &reserve_0.asset, 9_9000000, &samwise).unwrap();
            assert_eq!(asset_0_client.balance(&samwise), 450_0000000 + 9_9000000);
            assert_eq!(asset_0_client.balance(&pool), 50_0000000 - 9_9000000);
            assert_eq!(
                TokenClient::new(&e, &reserve_0.config.b_token).balance(&samwise),
                50_0000000 - 9_9000000
            );
        });
    }

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
}
