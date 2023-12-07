use crate::{
    errors::PoolError,
    storage::{self, PoolConfig, ReserveConfig, ReserveData},
};
use soroban_sdk::{panic_with_error, Address, Env, Symbol};

use super::pool::Pool;

/// Initialize the pool
///
/// Panics if the pool is already initialized or the arguments are invalid
#[allow(clippy::too_many_arguments)]
pub fn execute_initialize(
    e: &Env,
    admin: &Address,
    name: &Symbol,
    oracle: &Address,
    bstop_rate: &u64,
    backstop_address: &Address,
    blnd_id: &Address,
    usdc_id: &Address,
) {
    if storage::has_admin(e) {
        panic_with_error!(e, PoolError::AlreadyInitialized);
    }

    // ensure backstop is [0,1)
    if *bstop_rate >= 1_000_000_000 {
        panic_with_error!(e, PoolError::InvalidPoolInitArgs);
    }

    storage::set_admin(e, admin);
    storage::set_name(e, name);
    storage::set_backstop(e, backstop_address);
    storage::set_pool_config(
        e,
        &PoolConfig {
            oracle: oracle.clone(),
            bstop_rate: *bstop_rate,
            status: 1,
        },
    );
    storage::set_blnd_token(e, blnd_id);
    storage::set_usdc_token(e, usdc_id);
}

/// Update the pool
pub fn execute_update_pool(e: &Env, backstop_take_rate: u64) {
    // ensure backstop is [0,1)
    if backstop_take_rate >= 1_000_000_000 {
        panic_with_error!(e, PoolError::BadRequest);
    }
    let mut pool_config = storage::get_pool_config(e);
    pool_config.bstop_rate = backstop_take_rate;
    storage::set_pool_config(e, &pool_config);
}

/// Initialize a reserve for the pool
pub fn initialize_reserve(e: &Env, asset: &Address, config: &ReserveConfig) -> u32 {
    if storage::has_res(e, asset) {
        panic_with_error!(e, PoolError::AlreadyInitialized);
    }

    require_valid_reserve_metadata(e, config);
    let index = storage::push_res_list(e, asset);

    let reserve_config = ReserveConfig {
        index,
        decimals: config.decimals,
        c_factor: config.c_factor,
        l_factor: config.l_factor,
        util: config.util,
        max_util: config.max_util,
        r_one: config.r_one,
        r_two: config.r_two,
        r_three: config.r_three,
        reactivity: config.reactivity,
    };
    storage::set_res_config(e, asset, &reserve_config);
    let init_data = ReserveData {
        b_rate: 1_000_000_000,
        d_rate: 1_000_000_000,
        ir_mod: 1_000_000_000,
        d_supply: 0,
        b_supply: 0,
        last_time: e.ledger().timestamp(),
        backstop_credit: 0,
    };
    storage::set_res_data(e, asset, &init_data);
    index
}

/// Update a reserve in the pool
pub fn execute_update_reserve(e: &Env, asset: &Address, config: &ReserveConfig) {
    require_valid_reserve_metadata(e, config);

    let pool = Pool::load(e);
    if pool.config.status == 2 {
        panic_with_error!(e, PoolError::InvalidPoolStatus);
    }

    // accrue and store reserve data to the ledger
    let reserve = pool.load_reserve(e, asset);
    reserve.store(e);

    // force index to remain constant and only allow metadata based changes
    let mut new_config = config.clone();
    new_config.index = reserve.index;

    storage::set_res_config(e, asset, &new_config);
}

#[allow(clippy::zero_prefixed_literal)]
fn require_valid_reserve_metadata(e: &Env, metadata: &ReserveConfig) {
    if metadata.decimals > 18
        || metadata.c_factor > 1_0000000
        || metadata.l_factor > 1_0000000
        || metadata.util > 0_9500000
        || (metadata.max_util > 1_0000000 || metadata.max_util <= metadata.util)
        || (metadata.r_one > metadata.r_two || metadata.r_two > metadata.r_three)
        || (metadata.reactivity > 0_0005000)
    {
        panic_with_error!(e, PoolError::InvalidReserveMetadata);
    }
}

#[cfg(test)]
mod tests {
    use crate::testutils;

    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};

    #[test]
    fn test_execute_initialize() {
        let e = Env::default();
        let pool = testutils::create_pool(&e);

        let admin = Address::generate(&e);
        let name = Symbol::new(&e, "pool_name");
        let oracle = Address::generate(&e);
        let bstop_rate = 0_100_000_000u64;
        let backstop_address = Address::generate(&e);
        let blnd_id = Address::generate(&e);
        let usdc_id = Address::generate(&e);

        e.as_contract(&pool, || {
            execute_initialize(
                &e,
                &admin,
                &name,
                &oracle,
                &bstop_rate,
                &backstop_address,
                &blnd_id,
                &usdc_id,
            );

            assert_eq!(storage::get_admin(&e), admin);
            let pool_config = storage::get_pool_config(&e);
            assert_eq!(pool_config.oracle, oracle);
            assert_eq!(pool_config.bstop_rate, bstop_rate);
            assert_eq!(pool_config.status, 1);
            assert_eq!(storage::get_backstop(&e), backstop_address);
            assert_eq!(storage::get_blnd_token(&e), blnd_id);
            assert_eq!(storage::get_usdc_token(&e), usdc_id);
        });
    }

    #[test]
    fn test_execute_update_pool() {
        let e = Env::default();
        let pool = testutils::create_pool(&e);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            // happy path
            execute_update_pool(&e, 0_200_000_000u64);
            let new_pool_config = storage::get_pool_config(&e);
            assert_eq!(new_pool_config.bstop_rate, 0_200_000_000u64);
            assert_eq!(new_pool_config.oracle, pool_config.oracle);
            assert_eq!(new_pool_config.status, pool_config.status);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_execute_update_pool_validates() {
        let e = Env::default();
        let pool = testutils::create_pool(&e);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            execute_update_pool(&e, 1_000_000_000u64);
        });
    }

    #[test]
    fn test_initialize_reserve() {
        let e = Env::default();
        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);

        let (asset_id_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (asset_id_1, _) = testutils::create_token_contract(&e, &bombadil);

        let metadata = ReserveConfig {
            index: 0,
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_5000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        e.as_contract(&pool, || {
            initialize_reserve(&e, &asset_id_0, &metadata);

            initialize_reserve(&e, &asset_id_1, &metadata);
            let res_config_0 = storage::get_res_config(&e, &asset_id_0);
            let res_config_1 = storage::get_res_config(&e, &asset_id_1);
            assert_eq!(res_config_0.decimals, metadata.decimals);
            assert_eq!(res_config_0.c_factor, metadata.c_factor);
            assert_eq!(res_config_0.l_factor, metadata.l_factor);
            assert_eq!(res_config_0.util, metadata.util);
            assert_eq!(res_config_0.max_util, metadata.max_util);
            assert_eq!(res_config_0.r_one, metadata.r_one);
            assert_eq!(res_config_0.r_two, metadata.r_two);
            assert_eq!(res_config_0.r_three, metadata.r_three);
            assert_eq!(res_config_0.reactivity, metadata.reactivity);
            assert_eq!(res_config_0.index, 0);
            assert_eq!(res_config_1.index, 1);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #3)")]
    fn test_initialize_reserve_blocks_duplicates() {
        let e = Env::default();
        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);
        let (asset_id, _) = testutils::create_token_contract(&e, &bombadil);

        let metadata = ReserveConfig {
            index: 0,
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_5000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        e.as_contract(&pool, || {
            initialize_reserve(&e, &asset_id, &metadata);
            let res_config = storage::get_res_config(&e, &asset_id);
            assert_eq!(res_config.index, 0);
            initialize_reserve(&e, &asset_id, &metadata);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_initialize_reserve_validates_metadata() {
        let e = Env::default();
        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);
        let (asset_id, _) = testutils::create_token_contract(&e, &bombadil);

        let metadata = ReserveConfig {
            index: 0,
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 1_0000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        e.as_contract(&pool, || {
            initialize_reserve(&e, &asset_id, &metadata);
            let res_config = storage::get_res_config(&e, &asset_id);
            assert_eq!(res_config.index, 0);
            initialize_reserve(&e, &asset_id, &metadata);
        });
    }

    #[test]
    fn test_execute_update_reserve() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 500,
            protocol_version: 20,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        let new_metadata = ReserveConfig {
            index: 99,
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_7777777,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 105,
        };

        e.ledger().set(LedgerInfo {
            timestamp: 10000,
            protocol_version: 20,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            let res_config_old = storage::get_res_config(&e, &underlying);

            execute_update_reserve(&e, &underlying, &new_metadata);
            let res_config_updated = storage::get_res_config(&e, &underlying);
            assert_eq!(res_config_updated.decimals, new_metadata.decimals);
            assert_eq!(res_config_updated.c_factor, new_metadata.c_factor);
            assert_eq!(res_config_updated.l_factor, new_metadata.l_factor);
            assert_eq!(res_config_updated.util, new_metadata.util);
            assert_eq!(res_config_updated.max_util, new_metadata.max_util);
            assert_eq!(res_config_updated.r_one, new_metadata.r_one);
            assert_eq!(res_config_updated.r_two, new_metadata.r_two);
            assert_eq!(res_config_updated.r_three, new_metadata.r_three);
            assert_eq!(res_config_updated.reactivity, new_metadata.reactivity);
            assert_eq!(res_config_updated.index, res_config_old.index);

            // validate interest was accrued
            let res_data = storage::get_res_data(&e, &underlying);
            assert!(res_data.d_rate > 1_000_000_000);
            assert!(res_data.backstop_credit > 0);
            assert_eq!(res_data.last_time, 10000);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_execute_update_reserve_validates_metadata() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 500,
            protocol_version: 20,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let pool = testutils::create_pool(&e);
        let bombadil = Address::generate(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        let new_metadata = ReserveConfig {
            index: 99,
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 1_0777777,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 105,
        };

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            execute_update_reserve(&e, &underlying, &new_metadata);
        });
    }

    #[test]
    fn test_validate_reserve_metadata() {
        let e = Env::default();

        // valid
        let metadata = ReserveConfig {
            index: 0,
            decimals: 18,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_5000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        require_valid_reserve_metadata(&e, &metadata);
        // no panic
        assert!(true);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_validate_reserve_metadata_validates_decimals() {
        let e = Env::default();

        let metadata = ReserveConfig {
            index: 0,
            decimals: 19,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_5000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        require_valid_reserve_metadata(&e, &metadata);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_validate_reserve_metadata_validates_c_factor() {
        let e = Env::default();

        let metadata = ReserveConfig {
            index: 0,
            decimals: 18,
            c_factor: 1_0000001,
            l_factor: 0_7500000,
            util: 0_5000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        require_valid_reserve_metadata(&e, &metadata);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_validate_reserve_metadata_validates_l_factor() {
        let e = Env::default();

        let metadata = ReserveConfig {
            index: 0,
            decimals: 18,
            c_factor: 0_7500000,
            l_factor: 1_0000001,
            util: 0_5000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        require_valid_reserve_metadata(&e, &metadata);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_validate_reserve_metadata_validates_util() {
        let e = Env::default();

        let metadata = ReserveConfig {
            index: 0,
            decimals: 18,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 1_0000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        require_valid_reserve_metadata(&e, &metadata);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_validate_reserve_metadata_validates_max_util() {
        let e = Env::default();

        let metadata = ReserveConfig {
            index: 0,
            decimals: 18,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_5000000,
            max_util: 1_0000001,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        require_valid_reserve_metadata(&e, &metadata);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_validate_reserve_metadata_validates_r_order() {
        let e = Env::default();

        let metadata = ReserveConfig {
            index: 0,
            decimals: 18,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_5000000,
            max_util: 0_9500000,
            r_one: 0_5000001,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 100,
        };
        require_valid_reserve_metadata(&e, &metadata);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_validate_reserve_metadata_validates_reactivity() {
        let e = Env::default();

        let metadata = ReserveConfig {
            index: 0,
            decimals: 18,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_5000000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 5001,
        };
        require_valid_reserve_metadata(&e, &metadata);
    }
}
