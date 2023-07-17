use crate::{dependencies::TokenClient, storage};
use soroban_sdk::{unwrap::UnwrapOptimized, Address, Env, Map, Vec};

use super::{
    actions::{build_actions_from_request, Action, Request},
    health_factor::PositionData,
    pool::Pool,
    Positions,
};

/// Execute a set of updates for a user against the pool.
///
/// ### Arguments
/// * from - The address of the user whose positions are being modified
/// * spender - The address of the user who is sending tokens to the pool
/// * to - The address of the user who is receiving tokens from the pool
/// * requests - A vec of requests to be processed
///
/// ### Panics
/// If the request is unable to be fully executed
pub fn execute_submit(
    e: &Env,
    from: &Address,
    spender: &Address,
    to: &Address,
    requests: Vec<Request>,
) -> Positions {
    let mut pool = Pool::load(e);
    let mut pool_actions: Map<Address, Action> = Map::new(&e);
    let (check_health, pool_actions) = build_actions_from_request(e, &mut pool, &from, requests);
    let from_positions = storage::get_user_positions(e, &from);

    if check_health {
        // panics if the new positions set does not meet the health factor requirement
        PositionData::calculate_from_positions(e, &mut pool, &from_positions).require_healthy(e);
    }

    // TODO: Is this reentrancy guard necessary?
    // transfer tokens into the pool
    for (asset, action) in pool_actions.iter_unchecked() {
        if action.tokens_in > 0 {
            TokenClient::new(e, &asset).transfer(
                &spender,
                &e.current_contract_address(),
                &action.tokens_in,
            );
        }
    }

    // store updated info to ledger
    pool.store_cached_reserves(e);

    // transfer tokens out of the pool
    for (asset, action) in pool_actions.iter_unchecked() {
        if action.tokens_out > 0 {
            TokenClient::new(e, &asset).transfer(
                &e.current_contract_address(),
                &to,
                &action.tokens_out,
            );
        }
    }

    from_positions
}

#[cfg(test)]
mod tests {
    use crate::{storage::PoolConfig, testutils};

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec,
    };

    #[test]
    fn test_submit() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);
        let merry = Address::random(&e);
        let pool = Address::random(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_1, underlying_1_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let (underlying_2, underlying_2_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        underlying_1_client.mint(&frodo, &16_0000000);

        oracle_client.set_price(&underlying_1, &1_0000000);
        oracle_client.set_price(&underlying_2, &5_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
        });
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            let pre_pool_balance_1 = underlying_1_client.balance(&pool);
            let pre_pool_balance_2 = underlying_2_client.balance(&pool);

            let requests = vec![
                &e,
                Request {
                    request_type: 2,
                    reserve_index: 0,
                    amount: 15_0000000,
                },
                Request {
                    request_type: 4,
                    reserve_index: 1,
                    amount: 1_5000000,
                },
            ];
            let positions = execute_submit(&e, &samwise, &frodo, &merry, requests);

            assert_eq!(positions.liabilities.len(), 1);
            assert_eq!(positions.collateral.len(), 1);
            assert_eq!(positions.supply.len(), 0);
            assert_eq!(positions.get_collateral(0), 14_9999884);
            assert_eq!(positions.get_liabilities(1), 1_4999983);

            assert_eq!(
                underlying_1_client.balance(&pool),
                pre_pool_balance_1 + 15_0000000
            );
            assert_eq!(
                underlying_2_client.balance(&pool),
                pre_pool_balance_2 - 1_5000000
            );

            assert_eq!(underlying_1_client.balance(&frodo), 1_0000000);
            assert_eq!(underlying_2_client.balance(&merry), 1_5000000);
        });
    }

    #[test]
    #[should_panic(expected = "ContractError(10)")]
    fn test_submit_requires_healhty() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let frodo = Address::random(&e);
        let merry = Address::random(&e);
        let pool = Address::random(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_1, underlying_1_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        underlying_1_client.mint(&frodo, &16_0000000);

        oracle_client.set_price(&underlying_1, &1_0000000);
        oracle_client.set_price(&underlying_2, &5_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
        });
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            let requests = vec![
                &e,
                Request {
                    request_type: 2,
                    reserve_index: 0,
                    amount: 15_0000000,
                },
                Request {
                    request_type: 4,
                    reserve_index: 1,
                    amount: 1_7500000,
                },
            ];
            execute_submit(&e, &samwise, &frodo, &merry, requests);
        });
    }
}
