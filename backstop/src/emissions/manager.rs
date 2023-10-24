use cast::{i128, u64};
use fixed_point_math::FixedPoint;
use soroban_sdk::{panic_with_error, unwrap::UnwrapOptimized, vec, Address, Env, Vec};

use crate::{
    constants::{BACKSTOP_EPOCH, SCALAR_7},
    dependencies::TokenClient,
    errors::BackstopError,
    storage::{self, BackstopEmissionConfig, BackstopEmissionsData},
};

use super::update_emission_data;

/// Add a pool to the reward zone. If the reward zone is full, attempt to swap it with the pool to remove.
pub fn add_to_reward_zone(e: &Env, to_add: Address, to_remove: Address) {
    let mut reward_zone = storage::get_reward_zone(e);
    let max_rz_len = 10 + (i128(e.ledger().timestamp() - BACKSTOP_EPOCH) >> 23); // bit-shift 23 is ~97 day interval

    // ensure an entity in the reward zone cannot be included twice
    if reward_zone.contains(to_add.clone()) {
        panic_with_error!(e, BackstopError::BadRequest);
    }

    if max_rz_len > i128(reward_zone.len()) {
        // there is room in the reward zone. Add whatever
        // TODO: Once there is a defined limit of "backstop minimum", ensure it is reached!
        reward_zone.push_front(to_add.clone());
    } else {
        // don't allow rz modifications within 48 hours of the start of an emission cycle
        // if pools don't adopt their emissions within this time frame and get swapped, the tokens will be lost
        let next_distribution = storage::get_next_emission_cycle(e);
        if next_distribution != 0 && e.ledger().timestamp() < next_distribution - 5 * 24 * 60 * 60 {
            panic_with_error!(e, BackstopError::BadRequest);
        }

        // attempt to swap the "to_remove"
        // TODO: Once there is a defined limit of "backstop minimum", ensure it is reached!
        if storage::get_pool_balance(e, &to_add).tokens
            <= storage::get_pool_balance(e, &to_remove).tokens
        {
            panic_with_error!(e, BackstopError::InvalidRewardZoneEntry);
        }

        // swap to_add for to_remove
        let to_remove_index = reward_zone.first_index_of(to_remove.clone());
        match to_remove_index {
            Some(idx) => {
                reward_zone.set(idx, to_add.clone());
                storage::set_pool_eps(e, &to_remove, &0);
                // emissions data is not updated. Emissions will be set on the next emission cycle
            }
            None => panic_with_error!(e, BackstopError::InvalidRewardZoneEntry),
        }
    }

    storage::set_reward_zone(e, &reward_zone);
}

/// Update the backstop for the next emission cycle from the Emitter
#[allow(clippy::zero_prefixed_literal)]
pub fn update_emission_cycle(e: &Env) {
    if e.ledger().timestamp() < storage::get_next_emission_cycle(e) {
        panic_with_error!(e, BackstopError::BadRequest);
    }
    let next_distribution = e.ledger().timestamp() + 7 * 24 * 60 * 60;
    storage::set_next_emission_cycle(e, &next_distribution);

    let reward_zone = storage::get_reward_zone(e);
    let rz_len = reward_zone.len();
    let mut rz_tokens: Vec<i128> = vec![e];

    // TODO: Potential to assume optimization of backstop token balances ~= RZ tokens
    //       However, linear iteration over the RZ will still occur
    // fetch total tokens of BLND in the reward zone
    let mut total_tokens: i128 = 0;
    for rz_pool_index in 0..rz_len {
        let rz_pool = reward_zone.get(rz_pool_index).unwrap_optimized();
        let mut pool_balance = storage::get_pool_balance(e, &rz_pool);
        let net_deposits =
            pool_balance.tokens.clone() - pool_balance.convert_to_tokens(pool_balance.q4w.clone());
        rz_tokens.push_back(net_deposits);
        total_tokens += net_deposits;
    }

    let blnd_token_client = TokenClient::new(e, &storage::get_blnd_token(e));
    // store pools EPS and distribute emissions to backstop depositors
    for rz_pool_index in 0..rz_len {
        let rz_pool = reward_zone.get(rz_pool_index).unwrap_optimized();
        let cur_pool_tokens = rz_tokens.pop_front_unchecked();
        let share = cur_pool_tokens
            .fixed_div_floor(total_tokens, SCALAR_7)
            .unwrap_optimized();

        // store pool EPS and distribute pool's emissions via allowances to pool
        let pool_eps = share
            .fixed_mul_floor(0_3000000, SCALAR_7)
            .unwrap_optimized();
        let new_pool_emissions = pool_eps * 7 * 24 * 60 * 60;
        let current_allowance =
            blnd_token_client.allowance(&e.current_contract_address(), &rz_pool);
        blnd_token_client.approve(
            &e.current_contract_address(),
            &rz_pool,
            &(current_allowance + new_pool_emissions),
            &(e.ledger().sequence() + 17_280 * 30), // ~30 days: TODO: check phase 1 limits
        );
        storage::set_pool_eps(e, &rz_pool, &pool_eps);

        // distribute backstop depositor emissions
        let pool_backstop_eps = share
            .fixed_mul_floor(0_7000000, SCALAR_7)
            .unwrap_optimized();
        set_backstop_emission_config(
            e,
            &rz_pool,
            u64(pool_backstop_eps).unwrap_optimized(),
            next_distribution,
        );
    }
}

/// Set a new EPS for the backstop
pub fn set_backstop_emission_config(e: &Env, pool_id: &Address, eps: u64, expiration: u64) {
    if storage::has_backstop_emis_config(e, pool_id) {
        // a previous config exists - update with old config before setting new EPS
        let pool_balance = storage::get_pool_balance(e, pool_id);
        let mut emission_data = update_emission_data(e, pool_id, &pool_balance).unwrap_optimized();
        if emission_data.last_time != e.ledger().timestamp() {
            // force the emission data to be updated to the current timestamp
            emission_data.last_time = e.ledger().timestamp();
            storage::set_backstop_emis_data(e, pool_id, &emission_data);
        }
    } else {
        // first time the pool's backstop is receiving emissions - ensure data is written
        storage::set_backstop_emis_data(
            e,
            pool_id,
            &BackstopEmissionsData {
                index: 0,
                last_time: e.ledger().timestamp(),
            },
        );
    }
    let backstop_emis_config = BackstopEmissionConfig { expiration, eps };
    storage::set_backstop_emis_config(e, pool_id, &backstop_emis_config);
}

#[cfg(test)]
mod tests {

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec, BytesN, Vec,
    };

    use crate::{
        backstop::PoolBalance,
        storage::BackstopEmissionConfig,
        testutils::{self, create_backstop},
    };

    /********** update_emission_cycle **********/

    #[test]
    fn test_update_emission_cycle_happy_path() {
        let e = Env::default();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let backstop = create_backstop(&e);
        let (_, blnd_token_client) = testutils::create_blnd_token(&e, &backstop, &bombadil);
        let pool_1 = Address::random(&e);
        let pool_2 = Address::random(&e);
        let pool_3 = Address::random(&e);
        let reward_zone: Vec<Address> = vec![&e, pool_1.clone(), pool_2.clone(), pool_3.clone()];

        let pool_1_emissions_config = BackstopEmissionConfig {
            expiration: BACKSTOP_EPOCH,
            eps: 0_1000000,
        };
        let pool_1_emissions_data = BackstopEmissionsData {
            index: 887766,
            last_time: BACKSTOP_EPOCH - 12345,
        };
        e.as_contract(&backstop, || {
            storage::set_next_emission_cycle(&e, &BACKSTOP_EPOCH);
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_backstop_emis_config(&e, &pool_1, &pool_1_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1, &pool_1_emissions_data);
            storage::set_pool_balance(
                &e,
                &pool_1,
                &PoolBalance {
                    tokens: 300_000_0000000,
                    shares: 300_000_0000000,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &pool_2,
                &PoolBalance {
                    tokens: 200_000_0000000,
                    shares: 200_000_0000000,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &pool_3,
                &PoolBalance {
                    tokens: 500_000_0000000,
                    shares: 500_000_0000000,
                    q4w: 0,
                },
            );
            blnd_token_client.approve(&backstop, &pool_1, &100_123_0000000, &1000000);

            update_emission_cycle(&e);

            assert_eq!(
                storage::get_next_emission_cycle(&e),
                BACKSTOP_EPOCH + 7 * 24 * 60 * 60
            );
            assert_eq!(
                storage::get_pool_balance(&e, &pool_1).tokens,
                300_000_0000000
            );
            assert_eq!(
                storage::get_pool_balance(&e, &pool_2).tokens,
                200_000_0000000
            );
            assert_eq!(
                storage::get_pool_balance(&e, &pool_3).tokens,
                500_000_0000000
            );
            assert_eq!(storage::get_pool_eps(&e, &pool_1), 0_0900000);
            assert_eq!(storage::get_pool_eps(&e, &pool_2), 0_0600000);
            assert_eq!(storage::get_pool_eps(&e, &pool_3), 0_1500000);
            assert_eq!(
                blnd_token_client.allowance(&backstop, &pool_1),
                154_555_0000000
            );
            assert_eq!(
                blnd_token_client.allowance(&backstop, &pool_2),
                36_288_0000000
            );
            assert_eq!(
                blnd_token_client.allowance(&backstop, &pool_3),
                90_720_0000000
            );
            let new_pool_1_config =
                storage::get_backstop_emis_config(&e, &pool_1).unwrap_optimized();
            let new_pool_1_data = storage::get_backstop_emis_data(&e, &pool_1).unwrap_optimized();
            assert_eq!(new_pool_1_config.eps, 0_2100000);
            assert_eq!(
                new_pool_1_config.expiration,
                BACKSTOP_EPOCH + 7 * 24 * 60 * 60
            );
            // old config applied up to block timestamp
            assert_eq!(new_pool_1_data.index, 928916);
            assert_eq!(new_pool_1_data.last_time, BACKSTOP_EPOCH);
            let new_pool_2_config =
                storage::get_backstop_emis_config(&e, &pool_2).unwrap_optimized();
            let new_pool_2_data = storage::get_backstop_emis_data(&e, &pool_2).unwrap_optimized();
            assert_eq!(new_pool_2_config.eps, 0_1400000);
            assert_eq!(
                new_pool_2_config.expiration,
                BACKSTOP_EPOCH + 7 * 24 * 60 * 60
            );
            assert_eq!(new_pool_2_data.index, 0);
            assert_eq!(new_pool_2_data.last_time, BACKSTOP_EPOCH);
            let new_pool_3_config =
                storage::get_backstop_emis_config(&e, &pool_3).unwrap_optimized();
            let new_pool_3_data = storage::get_backstop_emis_data(&e, &pool_3).unwrap_optimized();
            assert_eq!(new_pool_3_config.eps, 0_3500000);
            assert_eq!(
                new_pool_3_config.expiration,
                BACKSTOP_EPOCH + 7 * 24 * 60 * 60
            );
            assert_eq!(new_pool_3_data.index, 0);
            assert_eq!(new_pool_3_data.last_time, BACKSTOP_EPOCH);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_update_emission_cycle_too_early() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let pool_1 = Address::random(&e);
        let pool_2 = Address::random(&e);
        let pool_3 = Address::random(&e);
        let reward_zone: Vec<Address> = vec![&e, pool_1.clone(), pool_2.clone(), pool_3.clone()];

        e.as_contract(&backstop_id, || {
            storage::set_next_emission_cycle(&e, &(BACKSTOP_EPOCH + 1));
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_pool_balance(
                &e,
                &pool_1,
                &PoolBalance {
                    tokens: 300_000_0000000,
                    shares: 300_000_0000000,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &pool_2,
                &PoolBalance {
                    tokens: 200_000_0000000,
                    shares: 200_000_0000000,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &pool_3,
                &PoolBalance {
                    tokens: 500_000_0000000,
                    shares: 500_000_0000000,
                    q4w: 0,
                },
            );

            update_emission_cycle(&e);
        });
    }

    /********** add_to_reward_zone **********/

    #[test]
    fn test_add_to_rz_empty_adds_pool() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            base_reserve: 10,
            network_id: Default::default(),
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::random(&e);

        e.as_contract(&backstop_id, || {
            add_to_reward_zone(
                &e,
                to_add.clone(),
                Address::from_contract_id(&BytesN::from_array(&e, &[0u8; 32])),
            );
            let actual_rz = storage::get_reward_zone(&e);
            let expected_rz: Vec<Address> = vec![&e, to_add];
            assert_eq!(actual_rz, expected_rz);
        });
    }

    #[test]
    fn test_add_to_rz_increases_size_over_time() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH + (1 << 23),
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::random(&e);
        let mut reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            add_to_reward_zone(
                &e,
                to_add.clone(),
                Address::from_contract_id(&BytesN::from_array(&e, &[0u8; 32])),
            );
            let actual_rz = storage::get_reward_zone(&e);
            reward_zone.push_front(to_add);
            assert_eq!(actual_rz, reward_zone);
        });
    }
    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_add_to_rz_takes_floor_for_size() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH + (1 << 23) - 1,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::random(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            add_to_reward_zone(
                &e,
                to_add.clone(),
                Address::from_contract_id(&BytesN::from_array(&e, &[0u8; 32])),
            );
        });
    }

    #[test]
    fn test_add_to_rz_swap_happy_path() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::random(&e);
        let to_remove = Address::random(&e);
        let mut reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            to_remove.clone(), // index 7
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_next_emission_cycle(&e, &(BACKSTOP_EPOCH + 5 * 24 * 60 * 60));
            storage::set_pool_eps(&e, &to_remove, &1);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 50,
                    tokens: 100,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &to_remove,
                &PoolBalance {
                    shares: 50,
                    tokens: 99,
                    q4w: 0,
                },
            );

            add_to_reward_zone(&e, to_add.clone(), to_remove.clone());

            let remove_eps = storage::get_pool_eps(&e, &to_remove);
            assert_eq!(remove_eps, 0);
            let actual_rz = storage::get_reward_zone(&e);
            assert_eq!(actual_rz.len(), 10);
            reward_zone.set(7, to_add);
            assert_eq!(actual_rz, reward_zone);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_add_to_rz_swap_not_enough_tokens() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::random(&e);
        let to_remove = Address::random(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            to_remove.clone(), // index 7
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone.clone());
            storage::set_next_emission_cycle(&e, &(BACKSTOP_EPOCH + 24 * 60 * 60));
            storage::set_pool_eps(&e, &to_remove, &1);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 50,
                    tokens: 100,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &to_remove,
                &PoolBalance {
                    shares: 50,
                    tokens: 100,
                    q4w: 0,
                },
            );

            add_to_reward_zone(&e, to_add.clone(), to_remove);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_add_to_rz_to_remove_not_in_rz() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::random(&e);
        let to_remove = Address::random(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_next_emission_cycle(&e, &(BACKSTOP_EPOCH + 24 * 60 * 60));
            storage::set_pool_eps(&e, &to_remove, &1);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 50,
                    tokens: 100,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &to_remove,
                &PoolBalance {
                    shares: 50,
                    tokens: 99,
                    q4w: 0,
                },
            );

            add_to_reward_zone(&e, to_add.clone(), to_remove);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_add_to_rz_swap_too_soon_to_distribution() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::random(&e);
        let to_remove = Address::random(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            to_remove.clone(), // index 7
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_next_emission_cycle(&e, &(BACKSTOP_EPOCH + 5 * 24 * 60 * 60 + 1));
            storage::set_pool_eps(&e, &to_remove, &1);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 50,
                    tokens: 100,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &to_remove,
                &PoolBalance {
                    shares: 50,
                    tokens: 99,
                    q4w: 0,
                },
            );

            add_to_reward_zone(&e, to_add, to_remove);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_add_to_rz_already_exists_panics() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::random(&e);
        let to_remove = Address::random(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::random(&e),
            to_remove.clone(),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            Address::random(&e),
            to_add.clone(),
            Address::random(&e),
            Address::random(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_next_emission_cycle(&e, &(BACKSTOP_EPOCH + 5 * 24 * 60 * 60));
            storage::set_pool_eps(&e, &to_remove, &1);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 50,
                    tokens: 100,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &to_remove,
                &PoolBalance {
                    shares: 50,
                    tokens: 99,
                    q4w: 0,
                },
            );

            add_to_reward_zone(&e, to_add.clone(), to_remove.clone());
        });
    }
}
