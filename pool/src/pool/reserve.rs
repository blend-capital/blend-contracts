use cast::i128;
use sep_41_token::TokenClient;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{contracttype, panic_with_error, unwrap::UnwrapOptimized, Address, Env};

use crate::{
    constants::{SCALAR_7, SCALAR_9},
    errors::PoolError,
    storage::{self, PoolConfig, ReserveData},
};

use super::interest::calc_accrual;

#[derive(Clone)]
#[contracttype]
pub struct Reserve {
    pub asset: Address,        // the underlying asset address
    pub index: u32,            // the reserve index in the pool
    pub l_factor: u32,         // the liability factor for the reserve
    pub c_factor: u32,         // the collateral factor for the reserve
    pub max_util: u32,         // the maximum utilization rate for the reserve
    pub last_time: u64,        // the last block the data was updated
    pub scalar: i128,          // scalar used for balances
    pub d_rate: i128,          // the conversion rate from dToken to underlying (9 decimals)
    pub b_rate: i128,          // the conversion rate from bToken to underlying (9 decimals)
    pub ir_mod: i128,          // the interest rate curve modifier
    pub b_supply: i128,        // the total supply of b tokens
    pub d_supply: i128,        // the total supply of d tokens
    pub backstop_credit: i128, // the total amount of underlying tokens owed to the backstop
}

impl Reserve {
    /// Load a Reserve from the ledger and update to the current ledger timestamp.
    ///
    /// **NOTE**: This function is not cached, and should be called from the Pool.
    ///
    /// ### Arguments
    /// * pool_config - The pool configuration
    /// * asset - The address of the underlying asset
    ///
    /// ### Panics
    /// Panics if the asset is not supported, if emissions cannot be updated, or if the reserve
    /// cannot be updated to the current ledger timestamp.
    pub fn load(e: &Env, pool_config: &PoolConfig, asset: &Address) -> Reserve {
        let reserve_config = storage::get_res_config(e, asset);
        let reserve_data = storage::get_res_data(e, asset);
        let mut reserve = Reserve {
            asset: asset.clone(),
            index: reserve_config.index,
            l_factor: reserve_config.l_factor,
            c_factor: reserve_config.c_factor,
            max_util: reserve_config.max_util,
            last_time: reserve_data.last_time,
            scalar: 10i128.pow(reserve_config.decimals),
            d_rate: reserve_data.d_rate,
            b_rate: reserve_data.b_rate,
            ir_mod: reserve_data.ir_mod,
            b_supply: reserve_data.b_supply,
            d_supply: reserve_data.d_supply,
            backstop_credit: reserve_data.backstop_credit,
        };

        // short circuit if the reserve has already been updated this ledger
        if e.ledger().timestamp() == reserve.last_time {
            return reserve;
        }

        if reserve.b_supply == 0 {
            reserve.last_time = e.ledger().timestamp();
            return reserve;
        }

        let cur_util = reserve.utilization();
        let (loan_accrual, new_ir_mod) = calc_accrual(
            e,
            &reserve_config,
            cur_util,
            reserve.ir_mod,
            reserve.last_time,
        );
        reserve.ir_mod = new_ir_mod;

        reserve.d_rate = loan_accrual
            .fixed_mul_ceil(reserve.d_rate, SCALAR_9)
            .unwrap_optimized();

        // TODO: Is it safe to calculate b_rate from accrual? If any unexpected token loss occurs
        //       the transfer rate will become unrecoverable.
        let pre_update_supply = reserve.total_supply();
        let token_bal = TokenClient::new(e, asset).balance(&e.current_contract_address());

        // credit the backstop underlying from the accrued interest based on the backstop rate
        let accrued_supply =
            reserve.total_liabilities() + token_bal - reserve.backstop_credit - pre_update_supply;
        if pool_config.bstop_rate > 0 && accrued_supply > 0 {
            let new_backstop_credit = accrued_supply
                .fixed_mul_floor(i128(pool_config.bstop_rate), SCALAR_9)
                .unwrap_optimized();
            reserve.backstop_credit += new_backstop_credit;
        }

        reserve.b_rate = (reserve.total_liabilities() + token_bal - reserve.backstop_credit)
            .fixed_div_floor(reserve.b_supply, SCALAR_9)
            .unwrap_optimized();
        reserve.last_time = e.ledger().timestamp();
        reserve
    }

    /// Store the updated reserve to the ledger.
    pub fn store(&self, e: &Env) {
        let reserve_data = ReserveData {
            d_rate: self.d_rate,
            b_rate: self.b_rate,
            ir_mod: self.ir_mod,
            b_supply: self.b_supply,
            d_supply: self.d_supply,
            backstop_credit: self.backstop_credit,
            last_time: self.last_time,
        };
        storage::set_res_data(e, &self.asset, &reserve_data);
    }

    /// Fetch the current utilization rate for the reserve normalized to 7 decimals
    pub fn utilization(&self) -> i128 {
        self.total_liabilities()
            .fixed_div_floor(self.total_supply(), SCALAR_7)
            .unwrap_optimized()
    }

    /// Require that the utilization rate is below the maximum allowed, or panic.
    pub fn require_utilization_below_max(&self, e: &Env) {
        if self.utilization() > i128(self.max_util) {
            panic_with_error!(e, PoolError::InvalidUtilRate)
        }
    }

    /// Fetch the total liabilities for the reserve in underlying tokens
    pub fn total_liabilities(&self) -> i128 {
        self.to_asset_from_d_token(self.d_supply)
    }

    /// Fetch the total supply for the reserve in underlying tokens
    pub fn total_supply(&self) -> i128 {
        self.to_asset_from_b_token(self.b_supply)
    }

    /********** Conversion Functions **********/

    /// Convert d_tokens to the corresponding asset value
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of tokens to convert
    pub fn to_asset_from_d_token(&self, d_tokens: i128) -> i128 {
        d_tokens
            .fixed_mul_ceil(self.d_rate, SCALAR_9)
            .unwrap_optimized()
    }

    /// Convert b_tokens to the corresponding asset value
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of tokens to convert
    pub fn to_asset_from_b_token(&self, b_tokens: i128) -> i128 {
        b_tokens
            .fixed_mul_floor(self.b_rate, SCALAR_9)
            .unwrap_optimized()
    }

    /// Convert d_tokens to their corresponding effective asset value. This
    /// takes into account the liability factor.
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of tokens to convert
    pub fn to_effective_asset_from_d_token(&self, d_tokens: i128) -> i128 {
        let assets = self.to_asset_from_d_token(d_tokens);
        assets
            .fixed_div_ceil(i128(self.l_factor), SCALAR_7)
            .unwrap_optimized()
    }

    /// Convert b_tokens to the corresponding effective asset value. This
    /// takes into account the collateral factor.
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of tokens to convert
    pub fn to_effective_asset_from_b_token(&self, b_tokens: i128) -> i128 {
        let assets = self.to_asset_from_b_token(b_tokens);
        assets
            .fixed_mul_floor(i128(self.c_factor), SCALAR_7)
            .unwrap_optimized()
    }

    /// Convert asset tokens to the corresponding d token value - rounding up
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_d_token_up(&self, amount: i128) -> i128 {
        amount
            .fixed_div_ceil(self.d_rate, SCALAR_9)
            .unwrap_optimized()
    }

    /// Convert asset tokens to the corresponding d token value - rounding down
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_d_token_down(&self, amount: i128) -> i128 {
        amount
            .fixed_div_floor(self.d_rate, SCALAR_9)
            .unwrap_optimized()
    }

    /// Convert asset tokens to the corresponding b token value - round up
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_b_token_up(&self, amount: i128) -> i128 {
        amount
            .fixed_div_ceil(self.b_rate, SCALAR_9)
            .unwrap_optimized()
    }

    /// Convert asset tokens to the corresponding b token value - round down
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_b_token_down(&self, amount: i128) -> i128 {
        amount
            .fixed_div_floor(self.b_rate, SCALAR_9)
            .unwrap_optimized()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutils;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};

    #[test]
    fn test_load_reserve() {
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
        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_data.d_rate = 1_345_678_123;
        reserve_data.b_rate = 1_123_456_789;
        reserve_data.d_supply = 65_0000000;
        reserve_data.b_supply = 99_0000000;
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let reserve = Reserve::load(&e, &pool_config, &underlying);

            // (accrual: 1_002_957_369, util: .7864352)
            assert_eq!(reserve.d_rate, 1_349_657_792);
            assert_eq!(reserve.b_rate, 1_125_547_121);
            assert_eq!(reserve.ir_mod, 1_044_981_440);
            assert_eq!(reserve.d_supply, 65_0000000);
            assert_eq!(reserve.b_supply, 99_0000000);
            assert_eq!(reserve.backstop_credit, 0_0517357);
            assert_eq!(reserve.last_time, 617280);
        });
    }

    #[test]
    fn test_load_reserve_zero_supply() {
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
        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_data.d_rate = 0;
        reserve_data.b_rate = 0;
        reserve_data.d_supply = 0;
        reserve_data.b_supply = 0;
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let reserve = Reserve::load(&e, &pool_config, &underlying);

            // (accrual: 1_002_957_369, util: .7864352)q
            assert_eq!(reserve.d_rate, 0);
            assert_eq!(reserve.b_rate, 0);
            assert_eq!(reserve.ir_mod, 1_000_000_000);
            assert_eq!(reserve.d_supply, 0);
            assert_eq!(reserve.b_supply, 0);
            assert_eq!(reserve.backstop_credit, 0);
            assert_eq!(reserve.last_time, 617280);
        });
    }

    #[test]
    fn test_load_reserve_zero_bstop_rate() {
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
        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_data.d_rate = 1_345_678_123;
        reserve_data.b_rate = 1_123_456_789;
        reserve_data.d_supply = 65_0000000;
        reserve_data.b_supply = 99_0000000;
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let reserve = Reserve::load(&e, &pool_config, &underlying);

            // (accrual: 1_002_957_369, util: .7864352)
            assert_eq!(reserve.d_rate, 1_349_657_792);
            assert_eq!(reserve.b_rate, 1_126_069_704);
            assert_eq!(reserve.ir_mod, 1_044_981_440);
            assert_eq!(reserve.d_supply, 65_0000000);
            assert_eq!(reserve.b_supply, 99_0000000);
            assert_eq!(reserve.backstop_credit, 0);
            assert_eq!(reserve.last_time, 617280);
        });
    }

    #[test]
    fn test_store() {
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
        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_data.d_rate = 1_345_678_123;
        reserve_data.b_rate = 1_123_456_789;
        reserve_data.d_supply = 65_0000000;
        reserve_data.b_supply = 99_0000000;
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            let reserve = Reserve::load(&e, &pool_config, &underlying);
            reserve.store(&e);

            let reserve_data = storage::get_res_data(&e, &underlying);

            // (accrual: 1_002_957_369, util: .7864352)
            assert_eq!(reserve_data.d_rate, 1_349_657_792);
            assert_eq!(reserve_data.b_rate, 1_125_547_121);
            assert_eq!(reserve_data.ir_mod, 1_044_981_440);
            assert_eq!(reserve_data.d_supply, 65_0000000);
            assert_eq!(reserve_data.b_supply, 99_0000000);
            assert_eq!(reserve_data.backstop_credit, 0_0517357);
            assert_eq!(reserve_data.last_time, 617280);
        });
    }

    #[test]
    fn test_utilization() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.d_rate = 1_345_678_123;
        reserve.b_rate = 1_123_456_789;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;

        let result = reserve.utilization();

        assert_eq!(result, 0_7864352);
    }

    #[test]
    fn test_require_utilization_below_max_pass() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;

        reserve.require_utilization_below_max(&e);
        // no panic
        assert!(true);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #12)")]
    fn test_require_utilization_under_max_panic() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.b_supply = 100_0000000;
        reserve.d_supply = 95_0000100;

        reserve.require_utilization_below_max(&e);
    }

    /***** Token Transfer Math *****/

    #[test]
    fn test_to_asset_from_d_token() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.d_rate = 1_321_834_961;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;

        let result = reserve.to_asset_from_d_token(1_1234567);

        assert_eq!(result, 1_4850244);
    }

    #[test]
    fn test_to_asset_from_b_token() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.b_rate = 1_321_834_961;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;

        let result = reserve.to_asset_from_b_token(1_1234567);

        assert_eq!(result, 1_4850243);
    }

    #[test]
    fn test_to_effective_asset_from_d_token() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.d_rate = 1_321_834_961;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;
        reserve.l_factor = 1_1000000;

        let result = reserve.to_effective_asset_from_d_token(1_1234567);

        assert_eq!(result, 1_3500222);
    }

    #[test]
    fn test_to_effective_asset_from_b_token() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.b_rate = 1_321_834_961;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;
        reserve.c_factor = 0_8500000;

        let result = reserve.to_effective_asset_from_b_token(1_1234567);

        assert_eq!(result, 1_2622706);
    }

    #[test]
    fn test_total_liabilities() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.d_rate = 1_823_912_692;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;

        let result = reserve.total_liabilities();

        assert_eq!(result, 118_5543250);
    }

    #[test]
    fn test_total_supply() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.b_rate = 1_823_912_692;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;

        let result = reserve.total_supply();

        assert_eq!(result, 180_5673565);
    }

    #[test]
    fn test_to_d_token_up() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.d_rate = 1_321_834_961;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;

        let result = reserve.to_d_token_up(1_4850243);

        assert_eq!(result, 1_1234567);
    }

    #[test]
    fn test_to_d_token_down() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.d_rate = 1_321_834_961;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;

        let result = reserve.to_d_token_down(1_4850243);

        assert_eq!(result, 1_1234566);
    }

    #[test]
    fn test_to_b_token_up() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.b_rate = 1_321_834_961;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;

        let result = reserve.to_b_token_up(1_4850243);

        assert_eq!(result, 1_1234567);
    }

    #[test]
    fn test_to_b_token_down() {
        let e = Env::default();

        let mut reserve = testutils::default_reserve(&e);
        reserve.b_rate = 1_321_834_961;
        reserve.b_supply = 99_0000000;
        reserve.d_supply = 65_0000000;

        let result = reserve.to_b_token_down(1_4850243);

        assert_eq!(result, 1_1234566);
    }
}
