use crate::{
    errors::PoolError,
    storage::{self, ReserveEmissionsConfig, ReserveEmissionsData},
};
use fixed_point_math::FixedPoint;
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

/// Get emissions information for a reserve
pub fn get_reserve_emissions(
    e: &Env,
    asset: &Address,
    token_type: u32,
) -> Option<(ReserveEmissionsConfig, ReserveEmissionsData)> {
    if token_type > 1 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let res_list = storage::get_res_list(e);
    if let Some(res_index) = res_list.first_index_of(asset) {
        let res_token_index = res_index * 2 + token_type;
        if storage::has_res_emis_data(e, &res_token_index) {
            return Some((
                storage::get_res_emis_config(e, &res_token_index).unwrap_optimized(),
                storage::get_res_emis_data(e, &res_token_index).unwrap_optimized(),
            ));
        }
        return None;
    }

    panic_with_error!(e, PoolError::BadRequest);
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

/// Updates the pool's emissions for the next emission cycle
///
/// Needs to be run each time a new emission cycle starts
///
/// Returns the new expiration timestamp
///
/// ### Panics
/// If update has already been run for this emission cycle
pub fn update_emissions_cycle(e: &Env, next_exp: u64, pool_eps: u64) -> u64 {
    let cur_exp = storage::get_pool_emissions_expiration(e);
    if next_exp <= cur_exp {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let pool_emissions = storage::get_pool_emissions(e);
    let reserve_list = storage::get_res_list(e);
    for (res_token_id, res_eps_share) in pool_emissions.iter() {
        let reserve_index = res_token_id / 2;
        let res_asset_address = reserve_list.get_unchecked(reserve_index);
        // update emissions data first to use the previous config until the current ledger timestamp
        update_reserve_emission_data(e, &res_asset_address, res_token_id);
        update_reserve_emission_config(e, res_token_id, next_exp, pool_eps, res_eps_share);
    }

    storage::set_pool_emissions_expiration(e, &next_exp);
    next_exp
}

fn update_reserve_emission_data(e: &Env, asset: &Address, res_token_id: u32) {
    if storage::has_res_emis_data(e, &res_token_id) {
        // data exists - update it with old config
        let reserve_config = storage::get_res_config(e, asset);
        let reserve_data = storage::get_res_data(e, asset);
        let supply = match res_token_id % 2 {
            0 => reserve_data.d_supply,
            1 => reserve_data.b_supply,
            _ => panic_with_error!(e, PoolError::BadRequest),
        };
        let mut emission_data = distributor::update_emission_data(
            e,
            res_token_id,
            supply,
            10i128.pow(reserve_config.decimals),
        )
        .unwrap(); // will always return a result
        if emission_data.last_time != e.ledger().timestamp() {
            // force the emission data to be updated to the current timestamp
            emission_data.last_time = e.ledger().timestamp();
            storage::set_res_emis_data(e, &res_token_id, &emission_data);
        }
    } else {
        // no data exists yet - first time this reserve token will get emission
        storage::set_res_emis_data(
            e,
            &res_token_id,
            &ReserveEmissionsData {
                index: 0,
                last_time: e.ledger().timestamp(),
            },
        );
    }
}

fn update_reserve_emission_config(
    e: &Env,
    res_token_id: u32,
    expiration: u64,
    pool_eps: u64,
    eps_share: u64,
) {
    let new_res_eps = eps_share
        .fixed_mul_floor(pool_eps, 1_0000000)
        .unwrap_optimized();
    let new_reserve_emis_config = ReserveEmissionsConfig {
        expiration,
        eps: new_res_eps,
    };

    storage::set_res_emis_config(e, &res_token_id, &new_reserve_emis_config);
    e.events().publish(
        (Symbol::new(e, "e_config"),),
        (res_token_id, new_res_eps, expiration),
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

    /********** update emissions cycle ********/

    #[test]
    fn test_update_emissions_cycle_no_emitted_reserves_does_nothing() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let pool = Address::random(&e);
        let bombadil = Address::random(&e);

        let next_exp = 1500604800;
        let pool_eps = 0_5000000;
        let pool_emissions: Map<u32, u64> = map![&e];

        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.as_contract(&pool, || {
            storage::set_pool_emissions(&e, &pool_emissions);

            update_emissions_cycle(&e, next_exp, pool_eps);

            assert_eq!(storage::get_pool_emissions_expiration(&e), next_exp);

            assert!(storage::get_res_emis_config(&e, &0).is_none());
            assert!(storage::get_res_emis_config(&e, &1).is_none());
            assert!(storage::get_res_emis_config(&e, &2).is_none());
            assert!(storage::get_res_emis_config(&e, &3).is_none());
        });
    }

    #[test]
    fn test_update_emissions_cycle_sets_reserve_emission_when_emitting_both() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let pool = Address::random(&e);
        let bombadil = Address::random(&e);

        let next_exp = 1500604800;
        let pool_eps = 0_5000000;
        let pool_emissions: Map<u32, u64> = map![
            &e,
            (2, 0_7500000), // reserve_1 liability
            (3, 0_2500000)  // reserve_1 supply
        ];

        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_data.last_time = 1499900000;
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);
        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        e.as_contract(&pool, || {
            storage::set_pool_emissions(&e, &pool_emissions);

            update_emissions_cycle(&e, next_exp, pool_eps);

            assert_eq!(storage::get_pool_emissions_expiration(&e), next_exp);

            assert!(storage::get_res_emis_config(&e, &0).is_none());
            assert!(storage::get_res_emis_config(&e, &1).is_none());
            assert!(storage::get_res_emis_config(&e, &4).is_none());
            assert!(storage::get_res_emis_config(&e, &5).is_none());

            let r_1_l_config = storage::get_res_emis_config(&e, &2).unwrap_optimized();
            let r_1_s_config = storage::get_res_emis_config(&e, &3).unwrap_optimized();
            assert_eq!(r_1_l_config.expiration, next_exp);
            assert_eq!(r_1_l_config.eps, 0_3750000);
            assert_eq!(r_1_s_config.expiration, next_exp);
            assert_eq!(r_1_s_config.eps, 0_1250000);

            // verify empty data was created for both
            let r_1_l_data = storage::get_res_emis_data(&e, &2).unwrap_optimized();
            let r_1_s_data = storage::get_res_emis_data(&e, &3).unwrap_optimized();
            assert_eq!(r_1_l_data.index, 0);
            assert_eq!(r_1_l_data.last_time, 1500000000);
            assert_eq!(r_1_s_data.index, 0);
            assert_eq!(r_1_s_data.last_time, 1500000000);
        });
    }

    #[test]
    fn test_update_emissions_cycle_sets_reserve_emission_config_and_data() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = Address::random(&e);
        let bombadil = Address::random(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let next_exp = 1500604800;
        let pool_eps = 0_5000000;
        let pool_emissions: Map<u32, u64> = map![
            &e,
            (0, 0_2500000), // reserve_0 liabilities
            (5, 0_7500000)  // reserve_1 supply
        ];

        let old_r_l_0_config = ReserveEmissionsConfig {
            eps: 0_2000000,
            expiration: 1500000100,
        };
        let old_r_l_0_data = ReserveEmissionsData {
            index: 100,
            last_time: 1499980000,
        };
        let old_r_s_2_config = ReserveEmissionsConfig {
            eps: 0_3000000,
            expiration: 1500000100,
        };
        let old_r_s_2_data = ReserveEmissionsData {
            index: 500,
            last_time: 1499980000,
        };

        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_data.last_time = 1499900000;
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);
        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        reserve_data.b_supply = 100_0000000;
        reserve_data.d_supply = 50_0000000;
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        e.as_contract(&pool, || {
            storage::set_pool_emissions(&e, &pool_emissions);
            storage::set_res_emis_config(&e, &0, &old_r_l_0_config);
            storage::set_res_emis_data(&e, &0, &old_r_l_0_data);
            storage::set_res_emis_config(&e, &5, &old_r_s_2_config);
            storage::set_res_emis_data(&e, &5, &old_r_s_2_data);

            let result = update_emissions_cycle(&e, next_exp, pool_eps);

            assert_eq!(storage::get_pool_emissions_expiration(&e), next_exp);
            assert_eq!(result, next_exp);

            assert!(storage::get_res_emis_config(&e, &1).is_none());
            assert!(storage::get_res_emis_config(&e, &2).is_none());
            assert!(storage::get_res_emis_config(&e, &3).is_none());
            assert!(storage::get_res_emis_config(&e, &4).is_none());

            let r_0_l_config = storage::get_res_emis_config(&e, &0).unwrap_optimized();
            let r_2_s_config = storage::get_res_emis_config(&e, &5).unwrap_optimized();
            assert_eq!(r_0_l_config.expiration, next_exp);
            assert_eq!(r_0_l_config.eps, 0_1250000);
            assert_eq!(r_2_s_config.expiration, next_exp);
            assert_eq!(r_2_s_config.eps, 0_3750000);

            let r_0_l_data = storage::get_res_emis_data(&e, &0).unwrap_optimized();
            let r_2_s_data = storage::get_res_emis_data(&e, &5).unwrap_optimized();
            assert_eq!(r_0_l_data.index, 533333433);
            assert_eq!(r_0_l_data.last_time, 1500000000);
            assert_eq!(r_2_s_data.index, 600000500);
            assert_eq!(r_2_s_data.last_time, 1500000000);
        });
    }

    #[test]
    fn test_update_emissions_cycle_all_data_set_to_ledger_timestamp() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = Address::random(&e);
        let bombadil = Address::random(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500100000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let next_exp = 1500704800;
        let pool_eps = 0_5000000;
        let pool_emissions: Map<u32, u64> = map![
            &e,
            (0, 0_2500000), // reserve_0 liabilities
            (5, 0_7500000)  // reserve_1 supply
        ];

        let old_r_l_0_config = ReserveEmissionsConfig {
            eps: 0_2000000,
            expiration: 1500000100,
        };
        let old_r_l_0_data = ReserveEmissionsData {
            index: 100,
            last_time: 1500000200,
        };
        let old_r_s_2_config = ReserveEmissionsConfig {
            eps: 0_3000000,
            expiration: 1500000100,
        };
        let old_r_s_2_data = ReserveEmissionsData {
            index: 500,
            last_time: 1500000100,
        };

        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_data.last_time = 1499900000;
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);
        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        reserve_data.b_supply = 100_0000000;
        reserve_data.d_supply = 50_0000000;
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        e.as_contract(&pool, || {
            storage::set_pool_emissions(&e, &pool_emissions);
            storage::set_res_emis_config(&e, &0, &old_r_l_0_config);
            storage::set_res_emis_data(&e, &0, &old_r_l_0_data);
            storage::set_res_emis_config(&e, &5, &old_r_s_2_config);
            storage::set_res_emis_data(&e, &5, &old_r_s_2_data);

            let result = update_emissions_cycle(&e, next_exp, pool_eps);

            assert_eq!(storage::get_pool_emissions_expiration(&e), next_exp);
            assert_eq!(result, next_exp);

            assert!(storage::get_res_emis_config(&e, &1).is_none());
            assert!(storage::get_res_emis_config(&e, &2).is_none());
            assert!(storage::get_res_emis_config(&e, &3).is_none());
            assert!(storage::get_res_emis_config(&e, &4).is_none());

            let r_0_l_config = storage::get_res_emis_config(&e, &0).unwrap_optimized();
            let r_2_s_config = storage::get_res_emis_config(&e, &5).unwrap_optimized();
            assert_eq!(r_0_l_config.expiration, next_exp);
            assert_eq!(r_0_l_config.eps, 0_1250000);
            assert_eq!(r_2_s_config.expiration, next_exp);
            assert_eq!(r_2_s_config.eps, 0_3750000);

            // should not accrue any value to index due to already passing the last expiration
            let r_0_l_data = storage::get_res_emis_data(&e, &0).unwrap_optimized();
            let r_2_s_data = storage::get_res_emis_data(&e, &5).unwrap_optimized();
            assert_eq!(r_0_l_data.index, 100);
            assert_eq!(r_0_l_data.last_time, 1500100000);
            assert_eq!(r_2_s_data.index, 500);
            assert_eq!(r_2_s_data.last_time, 1500100000);
        });
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "ContractError(2)")]
    fn test_update_emissions_cycle_panics_if_already_updated() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let pool = Address::random(&e);
        let bombadil = Address::random(&e);

        let next_exp = 1500604800;
        let pool_eps = 0_5000000;
        let pool_emissions: Map<u32, u64> = map![&e, (2, 0_7500000), (3, 0_2500000)];

        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_data.last_time = 1499900000;
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);
        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);
        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        reserve_data.b_supply = 100_0000000;
        reserve_data.d_supply = 50_0000000;
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        e.as_contract(&pool, || {
            storage::set_pool_emissions_expiration(&e, &1500604800);
            storage::set_pool_emissions(&e, &pool_emissions);

            update_emissions_cycle(&e, next_exp, pool_eps);
        });
    }

    /********** set_pool_emissions **********/

    #[test]
    fn test_set_pool_emissions() {
        let e = Env::default();
        e.budget().reset_unlimited();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let pool = Address::random(&e);
        let bombadil = Address::random(&e);

        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
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
    #[should_panic]
    //#[should_panic(expected = "ContractError(2)")]
    fn test_set_pool_emissions_panics_if_over_100() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let pool = Address::random(&e);
        let bombadil = Address::random(&e);

        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
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
            storage::set_pool_emissions_expiration(&e, &1000);
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
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let pool = Address::random(&e);
        let bombadil = Address::random(&e);

        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
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
