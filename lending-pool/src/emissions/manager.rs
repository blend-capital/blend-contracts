use crate::{
    errors::PoolError,
    reserve::Reserve,
    reserve_usage::ReserveUsage,
    storage::{self, PoolEmissionConfig, ReserveEmissionsConfig, ReserveEmissionsData},
};
use fixed_point_math::FixedPoint;
use soroban_sdk::{contracttype, map, Address, Env, Map, Symbol, Vec};

use super::distributor;

// Types

/// Metadata for a pool's reserve emission configuration
#[contracttype]
pub struct ReserveEmissionMetadata {
    res_index: u32,
    res_type: u32,
    share: u64,
}

/// Get emissions information for a reserve
pub fn get_reserve_emissions(
    e: &Env,
    asset: &Address,
    token_type: u32,
) -> Result<Option<(ReserveEmissionsConfig, ReserveEmissionsData)>, PoolError> {
    if token_type > 1 {
        return Err(PoolError::BadRequest);
    }

    let res_list = storage::get_res_list(e);
    if let Some(res_index) = res_list.first_index_of(asset) {
        let res_token_index = res_index * 3 + token_type;
        if storage::has_res_emis_data(e, &res_token_index) {
            return Ok(Some((
                storage::get_res_emis_config(e, &res_token_index).unwrap(),
                storage::get_res_emis_data(e, &res_token_index).unwrap(),
            )));
        }
        return Ok(None);
    }

    Err(PoolError::BadRequest)
}

/// Updates the pool's emissions for the next emission cycle
///
/// Needs to be run each time a new emission cycle starts
///
/// ### Errors
/// If update has already been run for this emission cycle
pub fn update_emissions(e: &Env, next_exp: u64, pool_eps: u64) -> Result<u64, PoolError> {
    let mut pool_config = storage::get_pool_emission_config(e);
    if next_exp <= pool_config.last_time {
        return Err(PoolError::BadRequest);
    }

    let pool_reserve_usage = ReserveUsage::new(pool_config.config);

    let pool_emissions = storage::get_pool_emissions(e);
    let reserve_count = storage::get_res_list(e);
    for i in 0..reserve_count.len() {
        let res_asset_address = reserve_count.get_unchecked(i).unwrap();
        if pool_reserve_usage.is_liability(i) {
            let key = ReserveUsage::liability_key(i);
            update_reserve_emission_data(e, res_asset_address.clone(), 0, key)?;

            let res_eps_share = pool_emissions.get_unchecked(key).unwrap();
            update_reserve_emission_config(e, key, next_exp, pool_eps, res_eps_share);
        }
        if pool_reserve_usage.is_supply(i) {
            let key = ReserveUsage::supply_key(i);
            update_reserve_emission_data(e, res_asset_address.clone(), 1, key)?;

            let res_eps_share = pool_emissions.get_unchecked(key).unwrap();
            update_reserve_emission_config(e, key, next_exp, pool_eps, res_eps_share);
        }
    }

    pool_config.last_time = next_exp;
    storage::set_pool_emission_config(e, &pool_config);
    Ok(next_exp)
}

fn update_reserve_emission_data(
    e: &Env,
    res_asset_address: Address,
    res_type: u32,
    res_token_id: u32,
) -> Result<(), PoolError> {
    if storage::has_res_emis_data(e, &res_token_id) {
        // data exists - update it with old config
        let reserve = Reserve::load(&e, res_asset_address);
        distributor::update_emission_data(&e, &reserve, res_type)?;
        Ok(())
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
        Ok(())
    }
}

fn update_reserve_emission_config(
    e: &Env,
    key: u32,
    expiration: u64,
    pool_eps: u64,
    eps_share: u64,
) {
    let new_res_eps = eps_share.fixed_mul_floor(pool_eps, 1_0000000).unwrap();
    let new_reserve_emis_config = ReserveEmissionsConfig {
        expiration,
        eps: new_res_eps,
    };

    storage::set_res_emis_config(e, &key, &new_reserve_emis_config);
    e.events().publish(
        (Symbol::new(&e, "e_config"),),
        (key, new_res_eps, expiration),
    )
}

/// Set the pool emissions
///
/// These will not be applied until the next `update_emissions` is run
///
/// ### Arguments
/// * `res_emission_metadata` - A vector of `ReserveEmissionMetadata` that details each reserve token's share
///                             if the total pool eps
///
/// ### Errors
/// If the total share of the pool eps from the reserves is over 1
pub fn set_pool_emissions(
    e: &Env,
    res_emission_metadata: Vec<ReserveEmissionMetadata>,
) -> Result<(), PoolError> {
    let cur_pool_emis_config = storage::get_pool_emission_config(e);

    let mut pool_config: ReserveUsage = ReserveUsage { config: 0 };
    let mut pool_emissions: Map<u32, u64> = map![&e];
    let mut total_share = 0;

    for res_emission in res_emission_metadata {
        let metadata = res_emission.unwrap();
        let key: u32;
        if metadata.res_type == 0 {
            pool_config.set_liability(metadata.res_index, true);
            key = ReserveUsage::liability_key(metadata.res_index);
        } else {
            pool_config.set_supply(metadata.res_index, true);
            key = ReserveUsage::supply_key(metadata.res_index);
        }
        pool_emissions.set(key, metadata.share);
        total_share += metadata.share;
    }

    if total_share > 1_0000000 {
        return Err(PoolError::BadRequest);
    }

    storage::set_pool_emission_config(
        e,
        &PoolEmissionConfig {
            config: pool_config.config,
            last_time: cur_pool_emis_config.last_time,
        },
    );
    storage::set_pool_emissions(e, &pool_emissions);
    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::{
        constants::SCALAR_7,
        testutils::{create_reserve, setup_reserve},
    };

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        vec, Address,
    };

    /********** Update Emissions **********/

    #[test]
    fn test_update_emissions_no_emitted_reserves_does_nothing() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let pool_id = Address::random(&e);
        let bombadil = Address::random(&e);

        let next_exp = 1500604800;
        let pool_eps = 0_5000000;
        let pool_emission_config = PoolEmissionConfig {
            last_time: 0,
            config: 0,
        };
        let pool_emissions: Map<u32, u64> = map![&e];

        let mut reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);

        e.as_contract(&pool_id, || {
            storage::set_pool_emission_config(&e, &pool_emission_config);
            storage::set_pool_emissions(&e, &pool_emissions);

            update_emissions(&e, next_exp, pool_eps).unwrap();

            let new_config = storage::get_pool_emission_config(&e);
            assert_eq!(new_config.last_time, next_exp);

            assert!(storage::get_res_emis_config(&e, &ReserveUsage::liability_key(0)).is_none());
            assert!(storage::get_res_emis_config(&e, &ReserveUsage::supply_key(0)).is_none());
            assert!(storage::get_res_emis_config(&e, &ReserveUsage::liability_key(1)).is_none());
            assert!(storage::get_res_emis_config(&e, &ReserveUsage::supply_key(1)).is_none());
        });
    }

    #[test]
    fn test_update_emissions_sets_reserve_emission_when_emitting_both() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let pool_id = Address::random(&e);
        let bombadil = Address::random(&e);

        let next_exp = 1500604800;
        let pool_eps = 0_5000000;
        let pool_emission_config = PoolEmissionConfig {
            last_time: 0,
            config: 0b000_011_000,
        };
        let pool_emissions: Map<u32, u64> = map![
            &e,
            (ReserveUsage::liability_key(1), 0_7500000),
            (ReserveUsage::supply_key(1), 0_2500000)
        ];

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.last_time = 1499900000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        reserve_1.data.last_time = 1499900000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let mut reserve_2 = create_reserve(&e);
        reserve_2.config.index = 2;
        reserve_2.data.last_time = 1499900000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        e.as_contract(&pool_id, || {
            storage::set_pool_emission_config(&e, &pool_emission_config);
            storage::set_pool_emissions(&e, &pool_emissions);

            let result = update_emissions(&e, next_exp, pool_eps).unwrap();

            let new_config = storage::get_pool_emission_config(&e);
            assert_eq!(new_config.last_time, next_exp);
            assert_eq!(result, next_exp);

            assert!(storage::get_res_emis_config(&e, &ReserveUsage::liability_key(0)).is_none());
            assert!(storage::get_res_emis_config(&e, &ReserveUsage::supply_key(0)).is_none());
            assert!(storage::get_res_emis_config(&e, &ReserveUsage::liability_key(2)).is_none());
            assert!(storage::get_res_emis_config(&e, &ReserveUsage::supply_key(2)).is_none());

            let r_1_l_config =
                storage::get_res_emis_config(&e, &ReserveUsage::liability_key(1)).unwrap();
            let r_1_s_config =
                storage::get_res_emis_config(&e, &ReserveUsage::supply_key(1)).unwrap();
            assert_eq!(r_1_l_config.expiration, next_exp);
            assert_eq!(r_1_l_config.eps, 0_3750000);
            assert_eq!(r_1_s_config.expiration, next_exp);
            assert_eq!(r_1_s_config.eps, 0_1250000);

            // verify empty data was created for both
            let r_1_l_data =
                storage::get_res_emis_data(&e, &ReserveUsage::liability_key(1)).unwrap();
            let r_1_s_data = storage::get_res_emis_data(&e, &ReserveUsage::supply_key(1)).unwrap();
            assert_eq!(r_1_l_data.index, 0);
            assert_eq!(r_1_l_data.last_time, 1500000000);
            assert_eq!(r_1_s_data.index, 0);
            assert_eq!(r_1_s_data.last_time, 1500000000);
        });
    }

    #[test]
    fn test_update_emissions_sets_reserve_emission_config_and_data() {
        let e = Env::default();
        e.mock_all_auths();

        let pool_id = Address::random(&e);
        let bombadil = Address::random(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let next_exp = 1500604800;
        let pool_eps = 0_5000000;
        let pool_emission_config = PoolEmissionConfig {
            last_time: 0,
            config: 0b010_000_001,
        };
        let pool_emissions: Map<u32, u64> = map![
            &e,
            (ReserveUsage::liability_key(0), 0_2500000),
            (ReserveUsage::supply_key(2), 0_7500000)
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

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.last_time = 1499900000;
        reserve_0.data.b_supply = 100_0000000;
        reserve_0.data.d_supply = 50_0000000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        reserve_1.data.last_time = 1499900000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let mut reserve_2 = create_reserve(&e);
        reserve_2.config.index = 2;
        reserve_2.data.last_time = 1499900000;
        reserve_2.data.b_supply = 100_0000000;
        reserve_2.data.d_supply = 50_0000000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        e.as_contract(&pool_id, || {
            storage::set_pool_emission_config(&e, &pool_emission_config);
            storage::set_pool_emissions(&e, &pool_emissions);
            storage::set_res_emis_config(&e, &ReserveUsage::liability_key(0), &old_r_l_0_config);
            storage::set_res_emis_data(&e, &ReserveUsage::liability_key(0), &old_r_l_0_data);
            storage::set_res_emis_config(&e, &ReserveUsage::supply_key(2), &old_r_s_2_config);
            storage::set_res_emis_data(&e, &ReserveUsage::supply_key(2), &old_r_s_2_data);

            let result = update_emissions(&e, next_exp, pool_eps).unwrap();

            let new_config = storage::get_pool_emission_config(&e);
            assert_eq!(new_config.last_time, next_exp);
            assert_eq!(result, next_exp);

            assert!(storage::get_res_emis_config(&e, &ReserveUsage::supply_key(0)).is_none());
            assert!(storage::get_res_emis_config(&e, &ReserveUsage::liability_key(1)).is_none());
            assert!(storage::get_res_emis_config(&e, &ReserveUsage::supply_key(1)).is_none());
            assert!(storage::get_res_emis_config(&e, &ReserveUsage::liability_key(2)).is_none());

            let r_0_l_config =
                storage::get_res_emis_config(&e, &ReserveUsage::liability_key(0)).unwrap();
            let r_2_s_config =
                storage::get_res_emis_config(&e, &ReserveUsage::supply_key(2)).unwrap();
            assert_eq!(r_0_l_config.expiration, next_exp);
            assert_eq!(r_0_l_config.eps, 0_1250000);
            assert_eq!(r_2_s_config.expiration, next_exp);
            assert_eq!(r_2_s_config.eps, 0_3750000);

            let r_0_l_data =
                storage::get_res_emis_data(&e, &ReserveUsage::liability_key(0)).unwrap();
            let r_2_s_data = storage::get_res_emis_data(&e, &ReserveUsage::supply_key(2)).unwrap();
            assert_eq!(r_0_l_data.index, 800000100);
            assert_eq!(r_0_l_data.last_time, 1500000000);
            assert_eq!(r_2_s_data.index, 600000500);
            assert_eq!(r_2_s_data.last_time, 1500000000);
        });
    }
    #[test]
    fn test_update_emissions_updates_correctly_year_gap() {
        let e = Env::default();
        e.mock_all_auths();

        let pool_id = Address::random(&e);
        let bombadil = Address::random(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let next_exp = 1500604800;
        let pool_eps = 0_5000000;
        let pool_emission_config = PoolEmissionConfig {
            last_time: 0,
            config: 0b010_000_001,
        };
        let pool_emissions: Map<u32, u64> = map![
            &e,
            (ReserveUsage::liability_key(0), 0_2500000),
            (ReserveUsage::supply_key(2), 0_7500000)
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

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.last_time = 1499900000;
        reserve_0.data.b_supply = 100_0000000;
        reserve_0.data.d_supply = 50_0000000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        reserve_1.data.last_time = 1499900000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let mut reserve_2 = create_reserve(&e);
        reserve_2.config.index = 2;
        reserve_2.data.last_time = 1499900000;
        reserve_2.data.b_supply = 100_0000000;
        reserve_2.data.d_supply = 50_0000000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        e.as_contract(&pool_id, || {
            e.budget().reset_unlimited();
            storage::set_pool_emission_config(&e, &pool_emission_config);
            storage::set_pool_emissions(&e, &pool_emissions);
            storage::set_res_emis_config(&e, &ReserveUsage::liability_key(0), &old_r_l_0_config);
            storage::set_res_emis_data(&e, &ReserveUsage::liability_key(0), &old_r_l_0_data);
            storage::set_res_emis_config(&e, &ReserveUsage::supply_key(2), &old_r_s_2_config);
            storage::set_res_emis_data(&e, &ReserveUsage::supply_key(2), &old_r_s_2_data);

            let result = update_emissions(&e, next_exp, pool_eps).unwrap();

            let new_config = storage::get_pool_emission_config(&e);
            assert_eq!(new_config.last_time, next_exp);
            assert_eq!(result, next_exp);

            let r_0_l_config =
                storage::get_res_emis_config(&e, &ReserveUsage::liability_key(0)).unwrap();
            let r_2_s_config =
                storage::get_res_emis_config(&e, &ReserveUsage::supply_key(2)).unwrap();
            assert_eq!(r_0_l_config.expiration, next_exp);
            assert_eq!(r_0_l_config.eps, 0_1250000);
            assert_eq!(r_2_s_config.expiration, next_exp);
            assert_eq!(r_2_s_config.eps, 0_3750000);

            let r_0_l_data =
                storage::get_res_emis_data(&e, &ReserveUsage::liability_key(0)).unwrap();
            let r_2_s_data = storage::get_res_emis_data(&e, &ReserveUsage::supply_key(2)).unwrap();
            assert_eq!(r_0_l_data.index, 800000100);
            assert_eq!(r_0_l_data.last_time, 1500000000);
            assert_eq!(r_2_s_data.index, 600000500);
            assert_eq!(r_2_s_data.last_time, 1500000000);

            let next_exp_1 = next_exp + 604800;
            e.ledger().set(LedgerInfo {
                timestamp: 1500000000 + 604800,
                protocol_version: 1,
                sequence_number: 20100 + 120960,
                network_id: Default::default(),
                base_reserve: 10,
            });
            let result = update_emissions(&e, next_exp_1, pool_eps).unwrap();

            let new_config = storage::get_pool_emission_config(&e);
            assert_eq!(new_config.last_time, next_exp_1);
            assert_eq!(result, next_exp_1);

            let r_0_l_config =
                storage::get_res_emis_config(&e, &ReserveUsage::liability_key(0)).unwrap();
            let r_2_s_config =
                storage::get_res_emis_config(&e, &ReserveUsage::supply_key(2)).unwrap();
            assert_eq!(r_0_l_config.expiration, next_exp_1);
            assert_eq!(r_0_l_config.eps, 0_1250000);
            assert_eq!(r_2_s_config.expiration, next_exp_1);
            assert_eq!(r_2_s_config.eps, 0_3750000);

            let r_0_l_data =
                storage::get_res_emis_data(&e, &ReserveUsage::liability_key(0)).unwrap();
            let r_2_s_data = storage::get_res_emis_data(&e, &ReserveUsage::supply_key(2)).unwrap();
            assert_eq!(r_0_l_data.last_time, 1500000000 + 604800);
            assert_eq!(
                r_0_l_data.index,
                800000100 + 604800 * 0_1250000 * SCALAR_7 / 50_0000000
            );
            assert_eq!(
                r_2_s_data.index,
                600000500 + 604800 * 0_3750000 * SCALAR_7 / 100_0000000
            );
            assert_eq!(r_2_s_data.last_time, 1500000000 + 604800);
            let next_exp_2 = next_exp + 604800 + 31536000;
            e.ledger().set(LedgerInfo {
                timestamp: 1500000000 + 604800 + 31536000,
                protocol_version: 1,
                sequence_number: 20100 + 120960 + 6308000,
                network_id: Default::default(),
                base_reserve: 10,
            });
            let result = update_emissions(&e, next_exp_2, pool_eps).unwrap();
            let new_config = storage::get_pool_emission_config(&e);
            assert_eq!(new_config.last_time, next_exp_2);
            assert_eq!(result, next_exp_2);

            let r_0_l_config =
                storage::get_res_emis_config(&e, &ReserveUsage::liability_key(0)).unwrap();
            let r_2_s_config =
                storage::get_res_emis_config(&e, &ReserveUsage::supply_key(2)).unwrap();
            assert_eq!(r_0_l_config.expiration, next_exp_2);
            assert_eq!(r_0_l_config.eps, 0_1250000);
            assert_eq!(r_2_s_config.expiration, next_exp_2);
            assert_eq!(r_2_s_config.eps, 0_3750000);

            let r_0_l_data =
                storage::get_res_emis_data(&e, &ReserveUsage::liability_key(0)).unwrap();
            let r_2_s_data = storage::get_res_emis_data(&e, &ReserveUsage::supply_key(2)).unwrap();
            assert_eq!(r_0_l_data.last_time, 1500000000 + 604800 + 31536000);
            assert_eq!(
                r_0_l_data.index,
                800000100
                    + 604800 * 0_1250000 * SCALAR_7 / 50_0000000
                    + 604800 * 0_1250000 * SCALAR_7 / 50_0000000 //+ 31536000 * 0_1250000 * SCALAR_7 / 50_0000000
            );
            assert_eq!(r_2_s_data.last_time, 1500000000 + 604800 + 31536000); //

            assert_eq!(
                r_2_s_data.index,
                600000500
                    + 604800 * 0_3750000 * SCALAR_7 / 100_0000000
                    + 604800 * 0_3750000 * SCALAR_7 / 100_0000000 //+ 31536000 * 0_3750000 * SCALAR_7 / 100_0000000
            );

            let next_exp_3 = next_exp + 604800 + 31536000 + 604800;
            e.ledger().set(LedgerInfo {
                timestamp: 1500000000 + 604800 + 31536000 + 604800,
                protocol_version: 1,
                sequence_number: 20100 + 120960 + 6308000 + 120960,
                network_id: Default::default(),
                base_reserve: 10,
            });
            let result = update_emissions(&e, next_exp_3, pool_eps).unwrap();
            let new_config = storage::get_pool_emission_config(&e);
            assert_eq!(new_config.last_time, next_exp_3);
            assert_eq!(result, next_exp_3);

            let r_0_l_config =
                storage::get_res_emis_config(&e, &ReserveUsage::liability_key(0)).unwrap();
            let r_2_s_config =
                storage::get_res_emis_config(&e, &ReserveUsage::supply_key(2)).unwrap();
            assert_eq!(r_0_l_config.expiration, next_exp_3);
            assert_eq!(r_0_l_config.eps, 0_1250000);
            assert_eq!(r_2_s_config.expiration, next_exp_3);
            assert_eq!(r_2_s_config.eps, 0_3750000);

            let r_0_l_data =
                storage::get_res_emis_data(&e, &ReserveUsage::liability_key(0)).unwrap();
            let r_2_s_data = storage::get_res_emis_data(&e, &ReserveUsage::supply_key(2)).unwrap();
            assert_eq!(
                r_0_l_data.last_time,
                1500000000 + 604800 + 604800 + 31536000 //+ 604800
            );
            assert_eq!(
                r_0_l_data.index,
                800000100
                    + 604800 * 0_1250000 * SCALAR_7 / 50_0000000
                    + 604800 * 0_1250000 * SCALAR_7 / 50_0000000
                    + 604800 * 0_1250000 * SCALAR_7 / 50_0000000 //+  * 0_1250000 * SCALAR_7 / 50_0000000
            );
            assert_eq!(
                r_2_s_data.last_time,
                1500000000 + 604800 + 604800 + 31536000
            ); // 604800

            assert_eq!(
                r_2_s_data.index,
                600000500
                    + 604800 * 0_3750000 * SCALAR_7 / 100_0000000
                    + 604800 * 0_3750000 * SCALAR_7 / 100_0000000
                    + 604800 * 0_3750000 * SCALAR_7 / 100_0000000 //+  * 0_1250000 * SCALAR_7 / 50_0000000
            );
        });
    }

    #[test]
    fn test_update_emissions_panics_if_already_updated() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let pool_id = Address::random(&e);
        let bombadil = Address::random(&e);

        let next_exp = 1500604800;
        let pool_eps = 0_5000000;
        let pool_emission_config = PoolEmissionConfig {
            last_time: 1500604800,
            config: 0b000_011_000,
        };
        let pool_emissions: Map<u32, u64> = map![
            &e,
            (ReserveUsage::liability_key(1), 0_7500000),
            (ReserveUsage::supply_key(1), 0_2500000)
        ];

        let mut reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let mut reserve_2 = create_reserve(&e);
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        e.as_contract(&pool_id, || {
            storage::set_pool_emission_config(&e, &pool_emission_config);
            storage::set_pool_emissions(&e, &pool_emissions);

            let result = update_emissions(&e, next_exp, pool_eps);
            match result {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    PoolError::BadRequest => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    /********** Set Pool Emissions **********/

    #[test]
    fn test_set_pool_emissions() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let pool_id = Address::random(&e);

        let pool_emission_config = PoolEmissionConfig {
            last_time: 1000,
            config: 0b000_011_000,
        };
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

        e.as_contract(&pool_id, || {
            storage::set_pool_emission_config(&e, &pool_emission_config);
            storage::set_pool_emissions(&e, &pool_emissions);

            set_pool_emissions(&e, res_emission_metadata).unwrap();

            let new_pool_emission_config = storage::get_pool_emission_config(&e);
            assert_eq!(
                new_pool_emission_config.last_time,
                pool_emission_config.last_time
            );
            assert_eq!(new_pool_emission_config.config, 0b001_000_000_010);
            let new_pool_emissions = storage::get_pool_emissions(&e);
            assert_eq!(new_pool_emissions.len(), 2);
            assert_eq!(
                new_pool_emissions
                    .get(ReserveUsage::supply_key(0))
                    .unwrap()
                    .unwrap(),
                0_3500000
            );
            assert_eq!(
                new_pool_emissions
                    .get(ReserveUsage::liability_key(3))
                    .unwrap()
                    .unwrap(),
                0_6500000
            );
        });
    }

    #[test]
    fn test_set_pool_emissions_panics_if_over_100() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let pool_id = Address::random(&e);

        let pool_emission_config = PoolEmissionConfig {
            last_time: 1000,
            config: 0b000_011_000,
        };
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

        e.as_contract(&pool_id, || {
            storage::set_pool_emission_config(&e, &pool_emission_config);
            storage::set_pool_emissions(&e, &pool_emissions);

            let result = set_pool_emissions(&e, res_emission_metadata);
            match result {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    PoolError::BadRequest => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    #[test]
    fn test_set_pool_emissions_ok_if_under_100() {
        let e = Env::default();
        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 20100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let pool_id = Address::random(&e);

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

        e.as_contract(&pool_id, || {
            storage::set_pool_emissions(&e, &pool_emissions);

            set_pool_emissions(&e, res_emission_metadata).unwrap();

            let new_pool_emission_config = storage::get_pool_emission_config(&e);
            assert_eq!(new_pool_emission_config.last_time, 0);
            assert_eq!(new_pool_emission_config.config, 0b001_000_000_010);
            let new_pool_emissions = storage::get_pool_emissions(&e);
            assert_eq!(new_pool_emissions.len(), 2);
            assert_eq!(
                new_pool_emissions
                    .get(ReserveUsage::supply_key(0))
                    .unwrap()
                    .unwrap(),
                0_3400000
            );
            assert_eq!(
                new_pool_emissions
                    .get(ReserveUsage::liability_key(3))
                    .unwrap()
                    .unwrap(),
                0_6500000
            );
        });
    }
}
