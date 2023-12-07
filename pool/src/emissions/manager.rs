use crate::{
    constants::SCALAR_7,
    dependencies::BackstopClient,
    errors::PoolError,
    storage::{self, ReserveEmissionsConfig, ReserveEmissionsData},
};
use cast::{i128, u64};
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{
    contracttype, map, panic_with_error, unwrap::UnwrapOptimized, Address, Env, Map, Symbol, Vec,
};

use super::distributor;

// Types

/// Metadata for a pool's reserve emission configuration
#[contracttype]
pub struct ReserveEmissionMetadata {
    pub res_index: u32,
    pub res_type: u32,
    pub share: u64,
}

/// Set the pool emissions
///
/// These will not be applied until the next `update_emissions` is run
///
/// ### Arguments
/// * `res_emission_metadata` - A vector of `ReserveEmissionMetadata` that details each reserve token's share
///                             if the total pool eps
///
/// ### Panics
/// If the total share of the pool eps from the reserves is over 1
pub fn set_pool_emissions(e: &Env, res_emission_metadata: Vec<ReserveEmissionMetadata>) {
    let mut pool_emissions: Map<u32, u64> = map![e];
    let mut total_share = 0;

    let reserve_list = storage::get_res_list(e);
    for metadata in res_emission_metadata {
        let key = metadata.res_index * 2 + metadata.res_type;
        if metadata.res_type > 1 || reserve_list.get(metadata.res_index).is_none() {
            panic_with_error!(e, PoolError::BadRequest);
        }
        pool_emissions.set(key, metadata.share);
        total_share += metadata.share;
    }

    if total_share > 1_0000000 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    storage::set_pool_emissions(e, &pool_emissions);
}

/// Consume emitted tokens from the backstop and distribute them to reserves
///
/// Returns the number of new tokens distributed for emissions
///
/// ### Panics
/// If update has already been run for this emission cycle
pub fn gulp_emissions(e: &Env) -> i128 {
    let backstop = storage::get_backstop(e);
    let new_emissions =
        BackstopClient::new(e, &backstop).gulp_pool_emissions(&e.current_contract_address());
    do_gulp_emissions(e, new_emissions);
    new_emissions
}

fn do_gulp_emissions(e: &Env, new_emissions: i128) {
    // ensure enough tokens are being emitted to avoid rounding issues
    if new_emissions < SCALAR_7 {
        panic_with_error!(e, PoolError::BadRequest)
    }
    let pool_emissions = storage::get_pool_emissions(e);
    let reserve_list = storage::get_res_list(e);
    for (res_token_id, res_eps_share) in pool_emissions.iter() {
        let reserve_index = res_token_id / 2;
        let res_asset_address = reserve_list.get_unchecked(reserve_index);
        let new_reserve_emissions = i128(res_eps_share)
            .fixed_mul_floor(new_emissions, SCALAR_7)
            .unwrap_optimized();
        update_reserve_emission_config(e, &res_asset_address, res_token_id, new_reserve_emissions);
    }
}

fn update_reserve_emission_config(
    e: &Env,
    asset: &Address,
    res_token_id: u32,
    new_reserve_emissions: i128,
) {
    let mut tokens_left_to_emit = new_reserve_emissions;
    if let Some(emis_config) = storage::get_res_emis_config(e, &res_token_id) {
        // data exists - update it with old config
        let reserve_config = storage::get_res_config(e, asset);
        let reserve_data = storage::get_res_data(e, asset);
        let supply = match res_token_id % 2 {
            0 => reserve_data.d_supply,
            1 => reserve_data.b_supply,
            _ => panic_with_error!(e, PoolError::BadRequest),
        };
        let mut emission_data = distributor::update_emission_data_with_config(
            e,
            res_token_id,
            supply,
            10i128.pow(reserve_config.decimals),
            &emis_config,
        );
        if emission_data.last_time != e.ledger().timestamp() {
            // force the emission data to be updated to the current timestamp
            emission_data.last_time = e.ledger().timestamp();
            storage::set_res_emis_data(e, &res_token_id, &emission_data);
        }
        // determine the amount of tokens not emitted from the last config
        if emis_config.expiration > e.ledger().timestamp() {
            let time_since_last_emission = emis_config.expiration - e.ledger().timestamp();
            let tokens_since_last_emission = i128(emis_config.eps * time_since_last_emission);
            tokens_left_to_emit += tokens_since_last_emission;
        }
    } else {
        // no config or data exists yet - first time this reserve token will get emission
        storage::set_res_emis_data(
            e,
            &res_token_id,
            &ReserveEmissionsData {
                index: 0,
                last_time: e.ledger().timestamp(),
            },
        );
    }
    let expiration = e.ledger().timestamp() + 7 * 24 * 60 * 60;
    let eps = u64(tokens_left_to_emit / (7 * 24 * 60 * 60)).unwrap_optimized();
    let new_reserve_emis_config = ReserveEmissionsConfig { expiration, eps };
    storage::set_res_emis_config(e, &res_token_id, &new_reserve_emis_config);

    e.events().publish(
        (Symbol::new(e, "reserve_emission_update"),),
        (res_token_id, eps, expiration),
    )
}

#[cfg(test)]
mod tests {
    use crate::testutils;

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        unwrap::UnwrapOptimized,
        vec, Address,
    };

    /********** gulp_emissions ********/

    #[test]
    fn test_gulp_emissions_no_pool_emissions_does_nothing() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);

        let new_emissions: i128 = 302_400_0000000;
        let pool_emissions: Map<u32, u64> = map![&e];

        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.as_contract(&pool, || {
            storage::set_pool_emissions(&e, &pool_emissions);

            do_gulp_emissions(&e, new_emissions);

            assert!(storage::get_res_emis_config(&e, &0).is_none());
            assert!(storage::get_res_emis_config(&e, &1).is_none());
            assert!(storage::get_res_emis_config(&e, &2).is_none());
            assert!(storage::get_res_emis_config(&e, &3).is_none());
        });
    }

    #[test]
    fn test_gulp_emissions() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);

        let new_emissions: i128 = 302_400_0000000;
        let pool_emissions: Map<u32, u64> = map![
            &e,
            (0, 0_2000000), // reserve_0 liability
            (2, 0_5500000), // reserve_1 liability
            (3, 0_2500000)  // reserve_1 supply
        ];

        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_data.last_time = 1499900000;
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);
        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        // setup reserve_0 liability to have emissions remaining
        let old_r_0_l_config = ReserveEmissionsConfig {
            eps: 0_1500000,
            expiration: 1500000200,
        };
        let old_r_0_l_data = ReserveEmissionsData {
            index: 99999,
            last_time: 1499980000,
        };

        // setup reserve_1 liability to have no emissions

        // steup reserve_1 supply to have emissions expired
        let old_r_1_s_config = ReserveEmissionsConfig {
            eps: 0_3500000,
            expiration: 1499990000,
        };
        let old_r_1_s_data = ReserveEmissionsData {
            index: 11111,
            last_time: 1499990000,
        };
        e.as_contract(&pool, || {
            storage::set_pool_emissions(&e, &pool_emissions);
            storage::set_res_emis_config(&e, &0, &old_r_0_l_config);
            storage::set_res_emis_data(&e, &0, &old_r_0_l_data);
            storage::set_res_emis_config(&e, &3, &old_r_1_s_config);
            storage::set_res_emis_data(&e, &3, &old_r_1_s_data);

            do_gulp_emissions(&e, new_emissions);

            assert!(storage::get_res_emis_config(&e, &1).is_none());
            assert!(storage::get_res_emis_config(&e, &4).is_none());
            assert!(storage::get_res_emis_config(&e, &5).is_none());

            // verify reserve_0 liability leftover emissions were carried over
            let r_0_l_config = storage::get_res_emis_config(&e, &0).unwrap_optimized();
            let r_0_l_data = storage::get_res_emis_data(&e, &0).unwrap_optimized();
            assert_eq!(r_0_l_config.expiration, 1500000000 + 7 * 24 * 60 * 60);
            assert_eq!(r_0_l_config.eps, 0_1000496);
            assert_eq!(r_0_l_data.index, 99999 + 40 * SCALAR_7);
            assert_eq!(r_0_l_data.last_time, 1500000000);

            // verify reserve_1 liability initialized emissions
            let r_1_l_config = storage::get_res_emis_config(&e, &2).unwrap_optimized();
            let r_1_l_data = storage::get_res_emis_data(&e, &2).unwrap_optimized();
            assert_eq!(r_1_l_config.expiration, 1500000000 + 7 * 24 * 60 * 60);
            assert_eq!(r_1_l_config.eps, 0_2750000);
            assert_eq!(r_1_l_data.index, 0);
            assert_eq!(r_1_l_data.last_time, 1500000000);

            // verify reserve_1 supply updated reserve data to the correct timestamp
            let r_1_s_config = storage::get_res_emis_config(&e, &3).unwrap_optimized();
            let r_1_s_data = storage::get_res_emis_data(&e, &3).unwrap_optimized();
            assert_eq!(r_1_s_config.expiration, 1500000000 + 7 * 24 * 60 * 60);
            assert_eq!(r_1_s_config.eps, 0_1250000);
            assert_eq!(r_1_s_data.index, 11111);
            assert_eq!(r_1_s_data.last_time, 1500000000);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_gulp_emissions_too_small() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);

        let new_emissions: i128 = 1000000;
        let pool_emissions: Map<u32, u64> = map![
            &e,
            (0, 0_2000000), // reserve_0 liability
            (2, 0_5500000), // reserve_1 liability
            (3, 0_2500000)  // reserve_1 supply
        ];

        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_data.last_time = 1499900000;
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);
        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        // setup reserve_0 liability to have emissions remaining
        let old_r_0_l_config = ReserveEmissionsConfig {
            eps: 0_1500000,
            expiration: 1500000200,
        };
        let old_r_0_l_data = ReserveEmissionsData {
            index: 99999,
            last_time: 1499980000,
        };

        // setup reserve_1 liability to have no emissions

        // steup reserve_1 supply to have emissions expired
        let old_r_1_s_config = ReserveEmissionsConfig {
            eps: 0_3500000,
            expiration: 1499990000,
        };
        let old_r_1_s_data = ReserveEmissionsData {
            index: 11111,
            last_time: 1499990000,
        };
        e.as_contract(&pool, || {
            storage::set_pool_emissions(&e, &pool_emissions);
            storage::set_res_emis_config(&e, &0, &old_r_0_l_config);
            storage::set_res_emis_data(&e, &0, &old_r_0_l_data);
            storage::set_res_emis_config(&e, &3, &old_r_1_s_config);
            storage::set_res_emis_data(&e, &3, &old_r_1_s_data);

            do_gulp_emissions(&e, new_emissions);
        });
    }

    /********** set_pool_emissions **********/

    #[test]
    fn test_set_pool_emissions() {
        let e = Env::default();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);

        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);
        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);
        let (underlying_3, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_3, &reserve_config, &reserve_data);

        let pool_emissions: Map<u32, u64> = map![&e, (2, 0_7500000),];
        let res_emission_metadata: Vec<ReserveEmissionMetadata> = vec![
            &e,
            ReserveEmissionMetadata {
                res_index: 0,
                res_type: 1,
                share: 0_3500000,
            },
            ReserveEmissionMetadata {
                res_index: 3,
                res_type: 0,
                share: 0_6500000,
            },
        ];

        e.as_contract(&pool, || {
            storage::set_pool_emissions(&e, &pool_emissions);

            set_pool_emissions(&e, res_emission_metadata);

            let new_pool_emissions = storage::get_pool_emissions(&e);
            assert_eq!(new_pool_emissions.len(), 2);
            assert_eq!(new_pool_emissions.get(1).unwrap_optimized(), 0_3500000);
            assert_eq!(new_pool_emissions.get(6).unwrap_optimized(), 0_6500000);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_set_pool_emissions_panics_if_over_100() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);

        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);
        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);
        let (underlying_3, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_3, &reserve_config, &reserve_data);

        let pool_emissions: Map<u32, u64> = map![&e, (2, 0_7500000),];
        let res_emission_metadata: Vec<ReserveEmissionMetadata> = vec![
            &e,
            ReserveEmissionMetadata {
                res_index: 0,
                res_type: 1,
                share: 0_3500000,
            },
            ReserveEmissionMetadata {
                res_index: 3,
                res_type: 0,
                share: 0_6500001,
            },
        ];

        e.as_contract(&pool, || {
            storage::set_pool_emissions(&e, &pool_emissions);

            set_pool_emissions(&e, res_emission_metadata);
        });
    }

    #[test]
    fn test_set_pool_emissions_ok_if_under_100() {
        let e = Env::default();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 20,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);

        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);
        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);
        let (underlying_3, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_3, &reserve_config, &reserve_data);

        let pool_emissions: Map<u32, u64> = map![&e, (2, 0_7500000),];
        let res_emission_metadata: Vec<ReserveEmissionMetadata> = vec![
            &e,
            ReserveEmissionMetadata {
                res_index: 0,
                res_type: 1,
                share: 0_3400000,
            },
            ReserveEmissionMetadata {
                res_index: 3,
                res_type: 0,
                share: 0_6500000,
            },
        ];

        e.as_contract(&pool, || {
            storage::set_pool_emissions(&e, &pool_emissions);

            set_pool_emissions(&e, res_emission_metadata);

            let new_pool_emissions = storage::get_pool_emissions(&e);
            assert_eq!(new_pool_emissions.len(), 2);
            assert_eq!(new_pool_emissions.get(1).unwrap_optimized(), 0_3400000);
            assert_eq!(new_pool_emissions.get(6).unwrap_optimized(), 0_6500000);
        });
    }
}
