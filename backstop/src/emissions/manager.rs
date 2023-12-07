use cast::{i128, u64};
use sep_41_token::TokenClient;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{panic_with_error, unwrap::UnwrapOptimized, vec, Address, Env, Vec};

use crate::{
    backstop::{load_pool_backstop_data, require_pool_above_threshold},
    constants::{BACKSTOP_EPOCH, SCALAR_7},
    dependencies::EmitterClient,
    errors::BackstopError,
    storage::{self, BackstopEmissionConfig, BackstopEmissionsData},
    PoolBalance,
};

use super::distributor::update_emission_data_with_config;

/// Add a pool to the reward zone. If the reward zone is full, attempt to swap it with the pool to remove.
pub fn add_to_reward_zone(e: &Env, to_add: Address, to_remove: Address) {
    let mut reward_zone = storage::get_reward_zone(e);
    let max_rz_len = 10 + (i128(e.ledger().timestamp() - BACKSTOP_EPOCH) >> 23); // bit-shift 23 is ~97 day interval

    // ensure an entity in the reward zone cannot be included twice
    if reward_zone.contains(to_add.clone()) {
        panic_with_error!(e, BackstopError::BadRequest);
    }

    // enusre to_add has met the minimum backstop deposit threshold
    // NOTE: "to_add" can only carry a pool balance if it is a deployed pool from the factory
    let pool_data = load_pool_backstop_data(e, &to_add);
    if !require_pool_above_threshold(&pool_data) {
        panic_with_error!(e, BackstopError::InvalidRewardZoneEntry);
    }

    if max_rz_len > i128(reward_zone.len()) {
        // there is room in the reward zone. Add "to_add".
        reward_zone.push_front(to_add.clone());
    } else {
        // swap to_add for to_remove
        let to_remove_index = reward_zone.first_index_of(to_remove.clone());
        match to_remove_index {
            Some(idx) => {
                // verify distribute was run recently to prevent "to_remove" from losing excess emissions
                // @dev: resource constraints prevent us from distributing on reward zone changes
                let last_distribution = storage::get_last_distribution_time(e);
                if last_distribution < e.ledger().timestamp() - 24 * 60 * 60 {
                    panic_with_error!(e, BackstopError::BadRequest);
                }

                // Verify "to_add" has a higher backstop deposit that "to_remove"
                if pool_data.tokens <= storage::get_pool_balance(e, &to_remove).tokens {
                    panic_with_error!(e, BackstopError::InvalidRewardZoneEntry);
                }
                reward_zone.set(idx, to_add.clone());
            }
            None => panic_with_error!(e, BackstopError::InvalidRewardZoneEntry),
        }
    }

    storage::set_reward_zone(e, &reward_zone);
}

/// Assign emissions from the Emitter to backstops and pools in the reward zone
#[allow(clippy::zero_prefixed_literal)]
pub fn gulp_emissions(e: &Env) -> i128 {
    let emitter = storage::get_emitter(e);
    let emitter_last_distribution =
        EmitterClient::new(&e, &emitter).get_last_distro(&e.current_contract_address());
    let last_distribution = storage::get_last_distribution_time(e);

    // ensure enough time has passed between the last emitter distribution and gulp_emissions
    // to prevent excess rounding issues
    if emitter_last_distribution <= (last_distribution + 60 * 60) {
        panic_with_error!(e, BackstopError::BadRequest);
    }
    storage::set_last_distribution_time(e, &emitter_last_distribution);
    let new_emissions = i128(emitter_last_distribution - last_distribution) * SCALAR_7; // emitter releases 1 token per second
    let total_backstop_emissions = new_emissions
        .fixed_mul_floor(0_7000000, SCALAR_7)
        .unwrap_optimized();
    let total_pool_emissions = new_emissions
        .fixed_mul_floor(0_3000000, SCALAR_7)
        .unwrap_optimized();

    let reward_zone = storage::get_reward_zone(e);
    let rz_len = reward_zone.len();
    let mut rz_balance: Vec<PoolBalance> = vec![e];

    // TODO: Potential to assume optimization of backstop token balances ~= RZ tokens
    //       However, linear iteration over the RZ will still occur
    // fetch total tokens of BLND in the reward zone
    let mut total_non_queued_tokens: i128 = 0;
    for rz_pool_index in 0..rz_len {
        let rz_pool = reward_zone.get(rz_pool_index).unwrap_optimized();
        let pool_balance = storage::get_pool_balance(e, &rz_pool);
        total_non_queued_tokens += pool_balance.non_queued_tokens();
        rz_balance.push_back(pool_balance);
    }

    // store pools EPS and distribute emissions to backstop depositors
    for rz_pool_index in 0..rz_len {
        let rz_pool = reward_zone.get(rz_pool_index).unwrap_optimized();
        let cur_pool_balance = rz_balance.pop_front_unchecked();
        let cur_pool_non_queued_tokens = cur_pool_balance.non_queued_tokens();
        let share = cur_pool_non_queued_tokens
            .fixed_div_floor(total_non_queued_tokens, SCALAR_7)
            .unwrap_optimized();

        // store pool EPS and distribute pool's emissions via allowances to pool
        let new_pool_emissions = share
            .fixed_mul_floor(total_pool_emissions, SCALAR_7)
            .unwrap_optimized();
        let current_emissions = storage::get_pool_emissions(e, &rz_pool);
        storage::set_pool_emissions(e, &rz_pool, current_emissions + new_pool_emissions);

        // distribute backstop depositor emissions
        let new_pool_backstop_tokens = share
            .fixed_mul_floor(total_backstop_emissions, SCALAR_7)
            .unwrap_optimized();
        set_backstop_emission_config(e, &rz_pool, &cur_pool_balance, new_pool_backstop_tokens);
    }
    new_emissions
}

/// Consume pool emissions approve them to be transferred by the pool
pub fn gulp_pool_emissions(e: &Env, pool_id: &Address) -> i128 {
    let pool_emissions = storage::get_pool_emissions(e, pool_id);
    if pool_emissions == 0 {
        panic_with_error!(e, BackstopError::BadRequest);
    }

    let blnd_token_client = TokenClient::new(e, &storage::get_blnd_token(e));
    let current_allowance = blnd_token_client.allowance(&e.current_contract_address(), pool_id);
    let new_tokens = current_allowance + pool_emissions;
    let new_seq = e.ledger().sequence() + 17_280 * 30; // ~30 days: TODO: check phase 1 limits
    blnd_token_client.approve(
        &e.current_contract_address(),
        pool_id,
        &new_tokens,
        &new_seq, // ~30 days: TODO: check phase 1 limits
    );
    storage::set_pool_emissions(e, pool_id, 0);
    pool_emissions
}

/// Set a new EPS for the backstop
pub fn set_backstop_emission_config(
    e: &Env,
    pool_id: &Address,
    pool_balance: &PoolBalance,
    new_tokens: i128,
) {
    let mut tokens_left_to_emit = new_tokens;
    if let Some(emis_config) = storage::get_backstop_emis_config(e, pool_id) {
        // a previous config exists - update with old config before setting new EPS
        let mut emission_data =
            update_emission_data_with_config(e, pool_id, &pool_balance, &emis_config);
        if emission_data.last_time != e.ledger().timestamp() {
            // force the emission data to be updated to the current timestamp
            emission_data.last_time = e.ledger().timestamp();
            storage::set_backstop_emis_data(e, pool_id, &emission_data);
        }
        // determine the amount of tokens not emitted from the last config
        if emis_config.expiration > e.ledger().timestamp() {
            let time_since_last_emission = emis_config.expiration - e.ledger().timestamp();
            let tokens_since_last_emission = i128(emis_config.eps * time_since_last_emission);
            tokens_left_to_emit += tokens_since_last_emission;
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
    let expiration = e.ledger().timestamp() + 7 * 24 * 60 * 60;
    let eps = u64(tokens_left_to_emit / (7 * 24 * 60 * 60)).unwrap_optimized();
    let backstop_emis_config = BackstopEmissionConfig { expiration, eps };
    storage::set_backstop_emis_config(e, pool_id, &backstop_emis_config);
}

#[cfg(test)]
mod tests {

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec, Vec,
    };

    use crate::{
        backstop::PoolBalance,
        storage::BackstopEmissionConfig,
        testutils::{create_backstop, create_blnd_token, create_emitter},
    };

    /********** gulp_emissions **********/

    #[test]
    fn test_gulp_emissions() {
        let e = Env::default();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop = create_backstop(&e);
        let emitter_distro_time = BACKSTOP_EPOCH - 10;
        create_emitter(
            &e,
            &backstop,
            &Address::generate(&e),
            &Address::generate(&e),
            emitter_distro_time,
        );
        let pool_1 = Address::generate(&e);
        let pool_2 = Address::generate(&e);
        let pool_3 = Address::generate(&e);
        let reward_zone: Vec<Address> = vec![&e, pool_1.clone(), pool_2.clone(), pool_3.clone()];

        // setup pool 1 to have ongoing emissions
        let pool_1_emissions_config = BackstopEmissionConfig {
            expiration: BACKSTOP_EPOCH + 1000,
            eps: 0_1000000,
        };
        let pool_1_emissions_data = BackstopEmissionsData {
            index: 887766,
            last_time: BACKSTOP_EPOCH - 12345,
        };

        // setup pool 2 to have expired emissions
        let pool_2_emissions_config = BackstopEmissionConfig {
            expiration: BACKSTOP_EPOCH - 12345,
            eps: 0_0500000,
        };
        let pool_2_emissions_data = BackstopEmissionsData {
            index: 453234,
            last_time: BACKSTOP_EPOCH - 12345,
        };
        // setup pool 3 to have no emissions
        e.as_contract(&backstop, || {
            storage::set_last_distribution_time(&e, &(emitter_distro_time - 7 * 24 * 60 * 60));
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_backstop_emis_config(&e, &pool_1, &pool_1_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1, &pool_1_emissions_data);
            storage::set_pool_emissions(&e, &pool_1, 100_123_0000000);
            storage::set_backstop_emis_config(&e, &pool_2, &pool_2_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_2, &pool_2_emissions_data);
            storage::set_pool_balance(
                &e,
                &pool_1,
                &PoolBalance {
                    tokens: 300_000_0000000,
                    shares: 200_000_0000000,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &pool_2,
                &PoolBalance {
                    tokens: 200_000_0000000,
                    shares: 150_000_0000000,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &pool_3,
                &PoolBalance {
                    tokens: 500_000_0000000,
                    shares: 600_000_0000000,
                    q4w: 0,
                },
            );
            // blnd_token_client.approve(&backstop, &pool_1, &100_123_0000000, &1000000);

            gulp_emissions(&e);

            assert_eq!(storage::get_last_distribution_time(&e), emitter_distro_time);
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
            assert_eq!(storage::get_pool_emissions(&e, &pool_1), 154_555_0000000);
            assert_eq!(storage::get_pool_emissions(&e, &pool_2), 36_288_0000000);
            assert_eq!(storage::get_pool_emissions(&e, &pool_3), 90_720_0000000);

            // validate backstop emissions
            let new_pool_1_config =
                storage::get_backstop_emis_config(&e, &pool_1).unwrap_optimized();
            let new_pool_1_data = storage::get_backstop_emis_data(&e, &pool_1).unwrap_optimized();
            assert_eq!(new_pool_1_config.eps, 0_2101653);
            assert_eq!(
                new_pool_1_config.expiration,
                BACKSTOP_EPOCH + 7 * 24 * 60 * 60
            );
            assert_eq!(new_pool_1_data.index, 949491);
            assert_eq!(new_pool_1_data.last_time, BACKSTOP_EPOCH);

            let new_pool_2_config =
                storage::get_backstop_emis_config(&e, &pool_2).unwrap_optimized();
            let new_pool_2_data = storage::get_backstop_emis_data(&e, &pool_2).unwrap_optimized();
            assert_eq!(new_pool_2_config.eps, 0_1400000);
            assert_eq!(
                new_pool_2_config.expiration,
                BACKSTOP_EPOCH + 7 * 24 * 60 * 60
            );
            assert_eq!(new_pool_2_data.index, 453234);
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
    fn test_gulp_emissions_too_soon() {
        let e = Env::default();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop = create_backstop(&e);
        let emitter_distro_time = BACKSTOP_EPOCH - 10;
        create_emitter(
            &e,
            &backstop,
            &Address::generate(&e),
            &Address::generate(&e),
            emitter_distro_time,
        );
        let pool_1 = Address::generate(&e);
        let pool_2 = Address::generate(&e);
        let pool_3 = Address::generate(&e);
        let reward_zone: Vec<Address> = vec![&e, pool_1.clone(), pool_2.clone(), pool_3.clone()];

        // setup pool 1 to have ongoing emissions
        let pool_1_emissions_config = BackstopEmissionConfig {
            expiration: BACKSTOP_EPOCH + 1000,
            eps: 0_1000000,
        };
        let pool_1_emissions_data = BackstopEmissionsData {
            index: 887766,
            last_time: BACKSTOP_EPOCH - 12345,
        };

        // setup pool 2 to have expired emissions
        let pool_2_emissions_config = BackstopEmissionConfig {
            expiration: BACKSTOP_EPOCH - 12345,
            eps: 0_0500000,
        };
        let pool_2_emissions_data = BackstopEmissionsData {
            index: 453234,
            last_time: BACKSTOP_EPOCH - 12345,
        };
        // setup pool 3 to have no emissions
        e.as_contract(&backstop, || {
            storage::set_last_distribution_time(&e, &(emitter_distro_time - 59 * 60));
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_backstop_emis_config(&e, &pool_1, &pool_1_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_1, &pool_1_emissions_data);
            storage::set_pool_emissions(&e, &pool_1, 100_123_0000000);
            storage::set_backstop_emis_config(&e, &pool_2, &pool_2_emissions_config);
            storage::set_backstop_emis_data(&e, &pool_2, &pool_2_emissions_data);
            storage::set_pool_balance(
                &e,
                &pool_1,
                &PoolBalance {
                    tokens: 300_000_0000000,
                    shares: 200_000_0000000,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &pool_2,
                &PoolBalance {
                    tokens: 200_000_0000000,
                    shares: 150_000_0000000,
                    q4w: 0,
                },
            );
            storage::set_pool_balance(
                &e,
                &pool_3,
                &PoolBalance {
                    tokens: 500_000_0000000,
                    shares: 600_000_0000000,
                    q4w: 0,
                },
            );

            gulp_emissions(&e);
        });
    }

    /********** gulp_pool_emissions **********/

    #[test]
    fn test_gulp_pool_emissions() {
        let e = Env::default();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let backstop = create_backstop(&e);
        let pool_1 = Address::generate(&e);
        let (_, blnd_token_client) = create_blnd_token(&e, &backstop, &bombadil);

        e.as_contract(&backstop, || {
            storage::set_pool_emissions(&e, &pool_1, 100_123_0000000);

            gulp_pool_emissions(&e, &pool_1);

            assert_eq!(storage::get_pool_emissions(&e, &pool_1), 0);
            assert_eq!(
                blnd_token_client.allowance(&e.current_contract_address(), &pool_1),
                100_123_0000000
            );
        });
    }

    #[test]
    fn test_gulp_pool_emissions_has_allowance() {
        let e = Env::default();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let backstop = create_backstop(&e);
        let pool_1 = Address::generate(&e);
        let (_, blnd_token_client) = create_blnd_token(&e, &backstop, &bombadil);

        e.as_contract(&backstop, || {
            blnd_token_client.approve(&backstop, &pool_1, &1234567, &1000);

            storage::set_pool_emissions(&e, &pool_1, 123_0000000);

            gulp_pool_emissions(&e, &pool_1);

            assert_eq!(storage::get_pool_emissions(&e, &pool_1), 0);
            assert_eq!(
                blnd_token_client.allowance(&e.current_contract_address(), &pool_1),
                123_1234567
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_gulp_pool_emissions_no_emissions() {
        let e = Env::default();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let backstop = create_backstop(&e);
        let pool_1 = Address::generate(&e);
        create_blnd_token(&e, &backstop, &bombadil);

        e.as_contract(&backstop, || {
            gulp_pool_emissions(&e, &pool_1);
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
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::generate(&e);

        e.as_contract(&backstop_id, || {
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_000_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_lp_token_val(&e, &(5_0000000, 0_1000000));

            add_to_reward_zone(&e, to_add.clone(), Address::generate(&e));
            let actual_rz = storage::get_reward_zone(&e);
            let expected_rz: Vec<Address> = vec![&e, to_add];
            assert_eq!(actual_rz, expected_rz);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_add_to_rz_empty_pool_under_backstop_threshold() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            base_reserve: 10,
            network_id: Default::default(),
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::generate(&e);

        e.as_contract(&backstop_id, || {
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 100_000_0000000,
                    tokens: 75_000_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_lp_token_val(&e, &(5_0000000, 0_1000000));

            add_to_reward_zone(&e, to_add.clone(), Address::generate(&e));
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
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::generate(&e);
        let mut reward_zone: Vec<Address> = vec![
            &e,
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_000_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_lp_token_val(&e, &(5_0000000, 0_1000000));

            add_to_reward_zone(&e, to_add.clone(), Address::generate(&e));
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
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::generate(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_000_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_lp_token_val(&e, &(5_0000000, 0_1000000));

            add_to_reward_zone(&e, to_add.clone(), Address::generate(&e));
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
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::generate(&e);
        let to_remove = Address::generate(&e);
        let mut reward_zone: Vec<Address> = vec![
            &e,
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            to_remove.clone(), // index 7
            Address::generate(&e),
            Address::generate(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_last_distribution_time(&e, &(BACKSTOP_EPOCH - 1 * 24 * 60 * 60));
            storage::set_pool_emissions(&e, &to_remove, 1);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_001_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_pool_balance(
                &e,
                &to_remove,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_000_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_lp_token_val(&e, &(5_0000000, 0_1000000));

            add_to_reward_zone(&e, to_add.clone(), to_remove.clone());

            let remove_eps = storage::get_pool_emissions(&e, &to_remove);
            assert_eq!(remove_eps, 1);
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
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::generate(&e);
        let to_remove = Address::generate(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            to_remove.clone(), // index 7
            Address::generate(&e),
            Address::generate(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_last_distribution_time(&e, &(BACKSTOP_EPOCH - 1 * 24 * 60 * 60));
            storage::set_pool_emissions(&e, &to_remove, 1);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_000_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_pool_balance(
                &e,
                &to_remove,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_000_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_lp_token_val(&e, &(5_0000000, 0_1000000));

            add_to_reward_zone(&e, to_add.clone(), to_remove);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_add_to_rz_swap_distribution_too_long_ago() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: BACKSTOP_EPOCH,
            protocol_version: 20,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::generate(&e);
        let to_remove = Address::generate(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            to_remove.clone(), // index 7
            Address::generate(&e),
            Address::generate(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_last_distribution_time(&e, &(BACKSTOP_EPOCH - 1 * 24 * 60 * 60 - 1));
            storage::set_pool_emissions(&e, &to_remove, 1);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_001_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_pool_balance(
                &e,
                &to_remove,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_000_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_lp_token_val(&e, &(5_0000000, 0_1000000));

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
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::generate(&e);
        let to_remove = Address::generate(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_last_distribution_time(&e, &(BACKSTOP_EPOCH - 24 * 60 * 60));
            storage::set_pool_emissions(&e, &to_remove, 1);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_001_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_pool_balance(
                &e,
                &to_remove,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_000_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_lp_token_val(&e, &(5_0000000, 0_1000000));

            add_to_reward_zone(&e, to_add.clone(), to_remove);
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
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let backstop_id = create_backstop(&e);
        let to_add = Address::generate(&e);
        let to_remove = Address::generate(&e);
        let reward_zone: Vec<Address> = vec![
            &e,
            Address::generate(&e),
            to_remove.clone(),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            Address::generate(&e),
            to_add.clone(),
            Address::generate(&e),
            Address::generate(&e),
        ];

        e.as_contract(&backstop_id, || {
            storage::set_reward_zone(&e, &reward_zone);
            storage::set_last_distribution_time(&e, &(BACKSTOP_EPOCH - 24 * 60 * 60));
            storage::set_pool_emissions(&e, &to_remove, 1);
            storage::set_pool_balance(
                &e,
                &to_add,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_001_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_pool_balance(
                &e,
                &to_remove,
                &PoolBalance {
                    shares: 90_000_0000000,
                    tokens: 100_000_0000000,
                    q4w: 1_000_0000000,
                },
            );
            storage::set_lp_token_val(&e, &(5_0000000, 0_1000000));

            add_to_reward_zone(&e, to_add.clone(), to_remove.clone());
        });
    }
}
