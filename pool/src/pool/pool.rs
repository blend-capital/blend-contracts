use soroban_sdk::{map, panic_with_error, unwrap::UnwrapOptimized, vec, Address, Env, Map, Vec};

use sep_40_oracle::{Asset, PriceFeedClient};

use crate::{
    errors::PoolError,
    storage::{self, PoolConfig},
    Positions,
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
    /// * store - If the reserve is expected to be stored to the ledger
    pub fn load_reserve(&mut self, e: &Env, asset: &Address, store: bool) -> Reserve {
        if store && !self.reserves_to_store.contains(asset) {
            self.reserves_to_store.push_back(asset.clone());
        }

        if let Some(reserve) = self.reserves.get(asset.clone()) {
            return reserve;
        } else {
            Reserve::load(e, &self.config, asset)
        }
    }

    /// Cache the updated reserve in the pool.
    ///
    /// ### Arguments
    /// * reserve - The updated reserve
    pub fn cache_reserve(&mut self, reserve: Reserve) {
        self.reserves.set(reserve.asset.clone(), reserve);
    }

    /// Store the cached reserves to the ledger that need to be written.
    pub fn store_cached_reserves(&self, e: &Env) {
        for address in self.reserves_to_store.iter() {
            let reserve = self
                .reserves
                .get(address)
                .unwrap_or_else(|| panic_with_error!(e, PoolError::InternalReserveNotFound));
            reserve.store(e);
        }
    }

    /// Require that the action does not violate the pool status, or panic.
    ///
    /// ### Arguments
    /// * `action_type` - The type of action being performed
    pub fn require_action_allowed(&self, e: &Env, action_type: u32) {
        // disable borrowing or auction cancellation for any non-active pool and disable supplying for any frozen pool
        if (self.config.status > 1 && (action_type == 4 || action_type == 9))
            || (self.config.status > 3 && (action_type == 2 || action_type == 0))
        {
            panic_with_error!(e, PoolError::InvalidPoolStatus);
        }
    }

    /// Require that a position does not violate the maximum number of positions, or panic.
    ///
    /// ### Arguments
    /// * `positions` - The user's positions
    /// * `previous_num` - The number of positions the user previously had
    ///
    /// ### Panics
    /// If the user has more positions than the maximum allowed and they are not
    /// decreasing their number of positions
    pub fn require_under_max(&self, e: &Env, positions: &Positions, previous_num: u32) {
        let new_num = positions.effective_count();
        if new_num > previous_num && self.config.max_positions < new_num {
            panic_with_error!(e, PoolError::MaxPositionsExceeded)
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

    use crate::{pool::User, storage::ReserveData, testutils};

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
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let mut pool = Pool::load(&e);
            let reserve = pool.load_reserve(&e, &underlying, true);
            pool.cache_reserve(reserve.clone());

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

            let new_reserve = pool.load_reserve(&e, &underlying, true);
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
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        reserve_config.index = 1;
        reserve_data.d_rate = 1_001_000_000;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        reserve_config.index = 2;
        reserve_data.d_rate = 1_002_000_000;
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let mut pool = Pool::load(&e);
            let reserve_0 = pool.load_reserve(&e, &underlying_0, false);
            let mut reserve_1 = pool.load_reserve(&e, &underlying_1, true);
            let mut reserve_2 = pool.load_reserve(&e, &underlying_2, true);
            reserve_2.d_rate = 456;
            pool.cache_reserve(reserve_0.clone());
            pool.cache_reserve(reserve_1.clone());
            pool.cache_reserve(reserve_2.clone());

            // verify a duplicate cache takes the most recently cached
            reserve_1.d_rate = 123;
            pool.cache_reserve(reserve_1.clone());

            // verify reloading without store flag still stores reserve
            let _ = pool.load_reserve(&e, &underlying_2, false);

            // delete the reserve data from the ledger to ensure it is loaded from the cache
            storage::set_res_data(
                &e,
                &underlying_0,
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

            let new_reserve = pool.load_reserve(&e, &underlying_0, false);
            assert_eq!(new_reserve.d_rate, reserve_0.d_rate);

            // store all cached reserves and verify the temp one was not stored
            pool.store_cached_reserves(&e);
            let new_reserve_data = storage::get_res_data(&e, &underlying_0);
            assert_eq!(new_reserve_data.d_rate, 0);
            let new_reserve_data = storage::get_res_data(&e, &reserve_1.asset);
            assert_eq!(new_reserve_data.d_rate, 123);
            let new_reserve_data = storage::get_res_data(&e, &reserve_2.asset);
            assert_eq!(new_reserve_data.d_rate, 456);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1209)")]
    fn test_reserve_cache_panics_if_missing_reserve_to_store() {
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
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        reserve_config.index = 1;
        reserve_data.d_rate = 1_001_000_000;
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        reserve_config.index = 2;
        reserve_data.d_rate = 1_002_000_000;
        testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let mut pool = Pool::load(&e);
            let reserve_0 = pool.load_reserve(&e, &underlying_0, false);
            let mut reserve_1 = pool.load_reserve(&e, &underlying_1, true);
            let mut reserve_2 = pool.load_reserve(&e, &underlying_2, true);
            reserve_1.b_rate = 123;
            reserve_2.d_rate = 456;
            pool.cache_reserve(reserve_0.clone());
            pool.cache_reserve(reserve_1.clone());
            // pool.cache_reserve(reserve_2.clone());

            pool.store_cached_reserves(&e);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1206)")]
    fn test_require_action_allowed_borrow_while_on_ice_panics() {
        let e = Env::default();

        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 2,
            max_positions: 2,
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
            bstop_rate: 0_2000000,
            status: 1,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);

            pool.require_action_allowed(&e, 4);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1206)")]
    fn test_require_action_allowed_cancel_liquidation_while_on_ice_panics() {
        let e = Env::default();

        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 2,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);

            pool.require_action_allowed(&e, 9);
        });
    }

    #[test]
    fn test_require_action_allowed_cancel_liquidation_while_active() {
        let e = Env::default();

        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 1,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);

            pool.require_action_allowed(&e, 9);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1206)")]
    fn test_require_action_allowed_supply_while_frozen() {
        let e = Env::default();

        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 4,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let pool = Pool::load(&e);

            pool.require_action_allowed(&e, 0);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1206)")]
    fn test_require_action_allowed_supply_collateral_while_frozen() {
        let e = Env::default();

        let pool = testutils::create_pool(&e);
        let oracle = Address::generate(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 4,
            max_positions: 2,
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
            bstop_rate: 0_2000000,
            status: 4,
            max_positions: 2,
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
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
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
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
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
    #[should_panic(expected = "Error(Contract, #1210)")]
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
            max_entry_ttl: 3110400,
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
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let mut pool = Pool::load(&e);

            pool.load_price(&e, &asset);
            assert!(false);
        });
    }

    #[test]
    fn test_require_under_max_empty() {
        let e = Env::default();
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let mut reserve_0 = testutils::default_reserve(&e);
        let (oracle, _) = testutils::create_mock_oracle(&e);
        let mut user = User {
            address: samwise.clone(),
            positions: Positions::env_default(&e),
        };
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let prev_positions = user.positions.effective_count();

            let pool = Pool::load(&e);
            user.add_collateral(&e, &mut reserve_0, 1);

            pool.require_under_max(&e, &user.positions, prev_positions);
        });
    }

    #[test]
    fn test_require_under_max_ignores_supply() {
        let e = Env::default();
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let mut reserve_0 = testutils::default_reserve(&e);
        let mut reserve_1 = testutils::default_reserve(&e);
        reserve_1.index = 1;

        let (oracle, _) = testutils::create_mock_oracle(&e);
        let mut user = User {
            address: samwise.clone(),
            positions: Positions::env_default(&e),
        };
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            user.add_supply(&e, &mut reserve_0, 42);
            user.add_supply(&e, &mut reserve_1, 42);
            user.add_collateral(&e, &mut reserve_1, 1);
            let prev_positions = user.positions.effective_count();

            let pool = Pool::load(&e);
            user.add_liabilities(&e, &mut reserve_1, 2);

            pool.require_under_max(&e, &user.positions, prev_positions);
        });
    }

    #[test]
    fn test_require_under_max_allows_decreasing_change() {
        let e = Env::default();
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let mut reserve_0 = testutils::default_reserve(&e);
        let mut reserve_1 = testutils::default_reserve(&e);
        reserve_1.index = 1;

        let (oracle, _) = testutils::create_mock_oracle(&e);
        let mut user = User {
            address: samwise.clone(),
            positions: Positions::env_default(&e),
        };
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            user.add_collateral(&e, &mut reserve_0, 42);
            user.add_collateral(&e, &mut reserve_1, 42);
            user.add_liabilities(&e, &mut reserve_0, 123);
            user.add_liabilities(&e, &mut reserve_1, 123);
            let prev_positions = user.positions.effective_count();

            let pool = Pool::load(&e);
            user.remove_collateral(&e, &mut reserve_1, 42);

            pool.require_under_max(&e, &user.positions, prev_positions);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1208)")]
    fn test_require_under_max_panics_if_over() {
        let e = Env::default();
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let mut reserve_0 = testutils::default_reserve(&e);
        let mut reserve_1 = testutils::default_reserve(&e);
        reserve_1.index = 1;

        let mut user = User {
            address: samwise.clone(),
            positions: Positions::env_default(&e),
        };
        let (oracle, _) = testutils::create_mock_oracle(&e);
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            user.add_collateral(&e, &mut reserve_0, 123);
            user.add_liabilities(&e, &mut reserve_0, 789);
            let prev_positions = user.positions.effective_count();

            let pool = Pool::load(&e);
            user.add_liabilities(&e, &mut reserve_1, 42);

            pool.require_under_max(&e, &user.positions, prev_positions);
        });
    }
}
