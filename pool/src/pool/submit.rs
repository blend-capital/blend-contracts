use sep_41_token::TokenClient;
use soroban_sdk::{panic_with_error, Address, Env, Vec};

use crate::PoolError;

use super::{
    actions::{build_actions_from_request, Request},
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
    if from == &e.current_contract_address()
        || spender == &e.current_contract_address()
        || to == &e.current_contract_address()
    {
        panic_with_error!(e, &PoolError::BadRequest);
    }
    let mut pool = Pool::load(e);

    let (actions, new_from_state, check_health) =
        build_actions_from_request(e, &mut pool, from, requests);

    // panics if the new positions set does not meet the health factor requirement
    // min is 1.0000100 to prevent rounding errors
    if check_health
        && new_from_state.has_liabilities()
        && PositionData::calculate_from_positions(e, &mut pool, &new_from_state.positions)
            .is_hf_under(1_0000100)
    {
        panic_with_error!(e, PoolError::InvalidHf);
    }

    // transfer tokens from sender to pool
    for (address, amount) in actions.spender_transfer.iter() {
        TokenClient::new(e, &address).transfer(spender, &e.current_contract_address(), &amount);
    }

    // store updated info to ledger
    pool.store_cached_reserves(e);
    new_from_state.store(e);

    // transfer tokens from pool to "to"
    for (address, amount) in actions.pool_transfer.iter() {
        TokenClient::new(e, &address).transfer(&e.current_contract_address(), to, &amount);
    }

    new_from_state.positions
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::{self, PoolConfig},
        testutils, RequestType,
    };

    use super::*;
    use sep_40_oracle::testutils::Asset;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec, Symbol,
    };

    #[test]
    fn test_submit() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);
        let merry = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, underlying_0_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, underlying_1_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        underlying_0_client.mint(&frodo, &16_0000000);

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0.clone()),
                Asset::Stellar(underlying_1.clone()),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 1_0000000, 5_0000000]);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            e.mock_all_auths_allowing_non_root_auth();
            storage::set_pool_config(&e, &pool_config);

            let pre_pool_balance_0 = underlying_0_client.balance(&pool);
            let pre_pool_balance_1 = underlying_1_client.balance(&pool);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::SupplyCollateral as u32,
                    address: underlying_0,
                    amount: 15_0000000,
                },
                Request {
                    request_type: RequestType::Borrow as u32,
                    address: underlying_1,
                    amount: 1_5000000,
                },
            ];
            let positions = execute_submit(&e, &samwise, &frodo, &merry, requests);

            assert_eq!(positions.liabilities.len(), 1);
            assert_eq!(positions.collateral.len(), 1);
            assert_eq!(positions.supply.len(), 0);
            assert_eq!(positions.collateral.get_unchecked(0), 14_9999884);
            assert_eq!(positions.liabilities.get_unchecked(1), 1_4999983);

            assert_eq!(
                underlying_0_client.balance(&pool),
                pre_pool_balance_0 + 15_0000000
            );
            assert_eq!(
                underlying_1_client.balance(&pool),
                pre_pool_balance_1 - 1_5000000
            );

            assert_eq!(underlying_0_client.balance(&frodo), 1_0000000);
            assert_eq!(underlying_1_client.balance(&merry), 1_5000000);
        });
    }

    #[test]
    fn test_submit_no_liabilities_does_not_load_oracle() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e); // will fail if executed against

        let (underlying_0, underlying_0_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, underlying_1_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        underlying_0_client.mint(&frodo, &16_0000000);
        underlying_1_client.mint(&frodo, &10_0000000);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            e.mock_all_auths_allowing_non_root_auth();
            storage::set_pool_config(&e, &pool_config);

            let pre_pool_balance_0 = underlying_0_client.balance(&pool);
            let pre_pool_balance_1 = underlying_1_client.balance(&pool);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::SupplyCollateral as u32,
                    address: underlying_0,
                    amount: 15_0000000,
                },
                // force check_health to true
                Request {
                    request_type: RequestType::Borrow as u32,
                    address: underlying_1.clone(),
                    amount: 1_5000000,
                },
                Request {
                    request_type: RequestType::Repay as u32,
                    address: underlying_1,
                    amount: 1_5000001,
                },
            ];
            let positions = execute_submit(&e, &samwise, &frodo, &frodo, requests);

            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 1);
            assert_eq!(positions.supply.len(), 0);
            assert_eq!(positions.collateral.get_unchecked(0), 14_9999884);

            assert_eq!(
                underlying_0_client.balance(&pool),
                pre_pool_balance_0 + 15_0000000
            );
            assert_eq!(
                underlying_1_client.balance(&pool),
                pre_pool_balance_1 + 1 // repayment rounded against user
            );

            assert_eq!(underlying_0_client.balance(&frodo), 1_0000000);
            assert_eq!(underlying_1_client.balance(&frodo), 10_0000000 - 1);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1205)")]
    fn test_submit_requires_healhty() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);
        let merry = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, underlying_0_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        underlying_0_client.mint(&frodo, &16_0000000);

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(underlying_0.clone()),
                Asset::Stellar(underlying_1.clone()),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 1_0000000, 5_0000000]);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::SupplyCollateral as u32,
                    address: underlying_0,
                    amount: 15_0000000,
                },
                Request {
                    request_type: RequestType::Borrow as u32,
                    address: underlying_1,
                    amount: 1_7500000,
                },
            ];
            execute_submit(&e, &samwise, &frodo, &merry, requests);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1200)")]
    fn test_submit_from_is_not_self() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, underlying_0_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        underlying_0_client.mint(&samwise, &16_0000000);

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![&e, Asset::Stellar(underlying_0.clone())],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 1_0000000]);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            e.mock_all_auths_allowing_non_root_auth();
            storage::set_pool_config(&e, &pool_config);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::SupplyCollateral as u32,
                    address: underlying_0,
                    amount: 15_0000000,
                },
            ];
            execute_submit(&e, &pool, &samwise, &samwise, requests);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1200)")]
    fn test_submit_spender_is_not_self() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, underlying_0_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        underlying_0_client.mint(&samwise, &16_0000000);

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![&e, Asset::Stellar(underlying_0.clone())],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 1_0000000]);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            e.mock_all_auths_allowing_non_root_auth();
            storage::set_pool_config(&e, &pool_config);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::SupplyCollateral as u32,
                    address: underlying_0,
                    amount: 15_0000000,
                },
            ];
            execute_submit(&e, &samwise, &pool, &samwise, requests);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1200)")]
    fn test_submit_to_is_not_self() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, underlying_0_client) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        underlying_0_client.mint(&samwise, &16_0000000);

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![&e, Asset::Stellar(underlying_0.clone())],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 1_0000000]);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            e.mock_all_auths_allowing_non_root_auth();
            storage::set_pool_config(&e, &pool_config);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::SupplyCollateral as u32,
                    address: underlying_0,
                    amount: 15_0000000,
                },
            ];
            execute_submit(&e, &samwise, &samwise, &pool, requests);
        });
    }
}
