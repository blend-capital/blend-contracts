use soroban_sdk::{map, panic_with_error, unwrap::UnwrapOptimized, vec, Address, Env, Map, Vec};

use sep_40_oracle::{Asset, PriceFeedClient};

use crate::{
    errors::PoolError,
    storage::{self, PoolConfig},
};

use super::reserve::Reserve;

pub struct Pool {
    pub config: PoolConfig,
    pub reserves: Map<Address, Reserve>,
    reserves_to_store: Vec<Address>,
    price_decimals: Option<u32>,
    prices: Map<Address, i128>,
}

impl Pool {
    /// Load the Pool from the ledger
    pub fn load(e: &Env) -> Self {
        let pool_config = storage::get_pool_config(e);
        Pool {
            config: pool_config,
            reserves: map![e],
            reserves_to_store: vec![e],
            price_decimals: None,
            prices: map![e],
        }
    }

    /// Load a Reserve from the ledger and update to the current ledger timestamp. Returns
    /// a cached version if it exists.
    ///
    /// ### Arguments
    /// * asset - The address of the underlying asset
    pub fn load_reserve(&self, e: &Env, asset: &Address) -> Reserve {
        if let Some(reserve) = self.reserves.get(asset.clone()) {
            return reserve;
        }
        Reserve::load(e, &self.config, asset)
    }

    /// Cache the updated reserve in the pool.
    ///
    /// ### Arguments
    /// * reserve - The updated reserve
    /// * write - If the reserve needs to be written to the ledger
    pub fn cache_reserve(&mut self, reserve: Reserve, write: bool) {
        if !self.reserves_to_store.contains(&reserve.asset) && write {
            self.reserves_to_store.push_back(reserve.asset.clone());
        }
        self.reserves.set(reserve.asset.clone(), reserve);
    }

    /// Store the cached reserves to the ledger that need to be written.
    pub fn store_cached_reserves(&self, e: &Env) {
        for address in self.reserves_to_store.iter() {
            let reserve = self.reserves.get_unchecked(address);
            reserve.store(e);
        }
    }

    /// Require that the action does not violate the pool status, or panic.
    ///
    /// ### Arguments
    /// * `action_type` - The type of action being performed
    pub fn require_action_allowed(&self, e: &Env, action_type: u32) {
        // disable borrowing for any non-active pool and disable supplying for any frozen pool
        if (self.config.status > 0 && action_type == 4)
            || (self.config.status > 1 && (action_type == 2 || action_type == 0))
        {
            panic_with_error!(e, PoolError::InvalidPoolStatus);
        }
    }

    /// Load the decimals of the prices for the Pool's oracle. Returns a cached version if one
    /// already exists.
    pub fn load_price_decimals(&mut self, e: &Env) -> u32 {
        if let Some(decimals) = self.price_decimals {
            return decimals;
        }
        let oracle_client = PriceFeedClient::new(e, &self.config.oracle);
        let decimals = oracle_client.decimals();
        self.price_decimals = Some(decimals);
        decimals
    }

    /// Load a price from the Pool's oracle. Returns a cached version if one already exists.
    ///
    /// ### Arguments
    /// * asset - The address of the underlying asset
    ///
    /// ### Panics
    /// If the price is stale
    pub fn load_price(&mut self, e: &Env, asset: &Address) -> i128 {
        if let Some(price) = self.prices.get(asset.clone()) {
            return price;
        }
        let oracle_client = PriceFeedClient::new(e, &self.config.oracle);
        let oracle_asset = Asset::Stellar(asset.clone());
        let price_data = oracle_client.lastprice(&oracle_asset).unwrap_optimized();
        if price_data.timestamp + 24 * 60 * 60 < e.ledger().timestamp() {
            panic_with_error!(e, PoolError::StalePrice);
        }
        self.prices.set(asset.clone(), price_data.price);
        price_data.price
    }
}

#[cfg(test)]
mod tests {
    use sep_40_oracle::testutils::Asset;
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Symbol,
    };

    use crate::{storage::ReserveData, testutils};

    use super::*;

    #[test]
    fn test_reserve_cache() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 123456 * 5,
            protocol_version: 20,
            sequence_number: 123456,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let mut pool = Pool::load(&e);
            let reserve = pool.load_reserve(&e, &underlying);
            pool.cache_reserve(reserve.clone(), true);

            // delete the reserve data from the ledger to ensure it is loaded from the cache
            storage::set_res_data(
                &e,
                &underlying,
                &ReserveData {
                    b_rate: 0,
                    d_rate: 0,
                    ir_mod: 0,
                    b_supply: 0,
                    d_supply: 0,
                    last_time: 0,
                    backstop_credit: 0,
                },
            );

            let new_reserve = pool.load_reserve(&e, &underlying);
            assert_eq!(new_reserve.d_rate, reserve.d_rate);

            // store all cached reserves and verify the data is updated
            pool.store_cached_reserves(&e);
            let new_reserve_data = storage::get_res_data(&e, &underlying);
            assert_eq!(new_reserve_data.d_rate, reserve.d_rate);
        });
    }

    #[test]
    fn test_reserve_cache_stores_only_marked() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 123456 * 5,
            protocol_version: 20,
            sequence_number: 123456,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        let mut reserve_1 = testutils::default_reserve(&e);
        reserve_1.index = 1;
        let mut reserve_2 = testutils::default_reserve(&e);
        reserve_2.index = 2;

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let mut pool = Pool::load(&e);
            pool.cache_reserve(reserve_2.clone(), true);
            pool.cache_reserve(reserve_1.clone(), true);

            // verify a duplicate cache takes the most recently cached
            reserve_1.d_rate = 123;
            pool.cache_reserve(reserve_1.clone(), true);

            let reserve = pool.load_reserve(&e, &underlying);
            pool.cache_reserve(reserve.clone(), false);

            // delete the reserve data from the ledger to ensure it is loaded from the cache
            storage::set_res_data(
                &e,
                &underlying,
                &ReserveData {
                    b_rate: 0,
                    d_rate: 0,
                    ir_mod: 0,
                    b_supply: 0,
                    d_supply: 0,
                    last_time: 0,
                    backstop_credit: 0,
                },
            );

            let new_reserve = pool.load_reserve(&e, &underlying);
            assert_eq!(new_reserve.d_rate, reserve.d_rate);

            // store all cached reserves and verify the unmarked one was not updated
            pool.store_cached_reserves(&e);
            let new_reserve_data = storage::get_res_data(&e, &underlying);
            assert_eq!(new_reserve_data.d_rate, 0);
            let new_reserve_data = storage::get_res_data(&e, &reserve_1.asset);
            assert_eq!(new_reserve_data.d_rate, 123);
            let new_reserve_data = storage::get_res_data(&e, &reserve_2.asset);
            assert_eq!(new_reserve_data.d_rate, reserve_2.d_rate);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #11)")]
    fn test_require_action_allowed_borrow_while_on_ice_panics() {
        let e = Env::default();

        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 1,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);

            pool.require_action_allowed(&e, 4);
        });
    }

    #[test]
    fn test_require_action_allowed_borrow_while_active() {
        let e = Env::default();

        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);

            pool.require_action_allowed(&e, 4);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #11)")]
    fn test_require_action_allowed_supply_while_frozen() {
        let e = Env::default();

        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);

            pool.require_action_allowed(&e, 0);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #11)")]
    fn test_require_action_allowed_supply_collateral_while_frozen() {
        let e = Env::default();

        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);

            pool.require_action_allowed(&e, 2);
        });
    }

    #[test]
    fn test_require_action_allowed_can_withdrawal_and_repay_while_frozen() {
        let e = Env::default();

        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);

            pool.require_action_allowed(&e, 5);
            pool.require_action_allowed(&e, 1);
            pool.require_action_allowed(&e, 3);
            // no panic
            assert!(true);
        });
    }

    #[test]
    fn test_load_price_decimals() {
        let e = Env::default();
        e.mock_all_auths();

        let pool = testutils::create_pool(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);
        oracle_client.set_data(
            &Address::generate(&e),
            &Asset::Stellar(Address::generate(&e)),
            &vec![&e, Asset::Stellar(Address::generate(&e))],
            &7,
            &300,
        );
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let mut pool = Pool::load(&e);

            let decimals = pool.load_price_decimals(&e);
            assert_eq!(decimals, 7);
        });
    }

    #[test]
    fn test_load_price() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        let bombadil = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let asset_0 = Address::generate(&e);
        let asset_1 = Address::generate(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);

        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![
                &e,
                Asset::Stellar(asset_0.clone()),
                Asset::Stellar(asset_1.clone()),
            ],
            &7,
            &300,
        );
        oracle_client.set_price_stable(&vec![&e, 123, 456]);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let mut pool = Pool::load(&e);

            let price = pool.load_price(&e, &asset_0);
            assert_eq!(price, 123);

            let price = pool.load_price(&e, &asset_1);
            assert_eq!(price, 456);

            // verify the price is cached
            oracle_client.set_price_stable(&vec![&e, 789, 101112]);
            let price = pool.load_price(&e, &asset_0);
            assert_eq!(price, 123);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #30)")]
    fn test_load_price_panics_if_stale() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 1000 + 24 * 60 * 60 + 1,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let asset = Address::generate(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);
        oracle_client.set_data(
            &bombadil,
            &Asset::Other(Symbol::new(&e, "USD")),
            &vec![&e, Asset::Stellar(asset.clone())],
            &7,
            &300,
        );
        oracle_client.set_price(&vec![&e, 123], &1000);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let mut pool = Pool::load(&e);

            pool.load_price(&e, &asset);
            assert!(false);
        });
    }
}
