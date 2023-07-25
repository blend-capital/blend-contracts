use fixed_point_math::FixedPoint;
use soroban_sdk::{panic_with_error, unwrap::UnwrapOptimized, Env};

use crate::{constants::SCALAR_7, errors::PoolError, storage};

use super::{pool::Pool, Positions};

pub struct PositionData {
    /// The effective collateral balance denominated in the base asset
    pub collateral_base: i128,
    // The raw collateral balance demoninated in the base asset
    pub collateral_raw: i128,
    /// The effective liability balance denominated in the base asset
    pub liability_base: i128,
    // The raw liability balance demoninated in the base asset
    pub liability_raw: i128,
    /// The scalar for the base asset
    pub scalar: i128,
}

impl PositionData {
    /// Calculate the position data for a given set of of positions
    ///
    /// ### Arguments
    /// * pool - The pool
    /// * positions - The positions to calculate the health factor for
    pub fn calculate_from_positions(e: &Env, pool: &mut Pool, positions: &Positions) -> Self {
        let oracle_scalar = 10i128.pow(pool.load_price_decimals(e));

        let reserve_list = storage::get_res_list(e);
        let mut collateral_base = 0;
        let mut liability_base = 0;
        let mut collateral_raw = 0;
        let mut liability_raw = 0;
        for i in 0..reserve_list.len() {
            let b_token_balance = positions.get_collateral(i);
            let d_token_balance = positions.get_liabilities(i);
            if b_token_balance == 0 && d_token_balance == 0 {
                continue;
            }
            let reserve = pool.load_reserve(e, &reserve_list.get_unchecked(i));
            let asset_to_base = pool.load_price(e, &reserve.asset);

            if b_token_balance > 0 {
                // append users effective collateral to collateral_base
                let asset_collateral = reserve.to_effective_asset_from_b_token(b_token_balance);
                collateral_base += asset_to_base
                    .fixed_mul_floor(asset_collateral, reserve.scalar)
                    .unwrap_optimized();
                collateral_raw += asset_to_base
                    .fixed_mul_floor(
                        reserve.to_asset_from_b_token(b_token_balance),
                        reserve.scalar,
                    )
                    .unwrap_optimized();
            }

            if d_token_balance > 0 {
                // append users effective liability to liability_base
                let asset_liability = reserve.to_effective_asset_from_d_token(d_token_balance);
                liability_base += asset_to_base
                    .fixed_mul_floor(asset_liability, reserve.scalar)
                    .unwrap_optimized();
                liability_raw += asset_to_base
                    .fixed_mul_floor(
                        reserve.to_asset_from_d_token(d_token_balance),
                        reserve.scalar,
                    )
                    .unwrap_optimized();
            }
        }

        PositionData {
            collateral_base,
            collateral_raw,
            liability_base,
            liability_raw,
            scalar: oracle_scalar,
        }
    }

    /// Return the health factor as a ratio
    pub fn as_health_factor(&self) -> i128 {
        self.collateral_base
            .fixed_div_ceil(self.liability_base, self.scalar)
            .unwrap_optimized()
    }

    /// Check if the position data meets the minimum health factor, panic if not
    pub fn require_healthy(&self, e: &Env) {
        if self.liability_base == 0 {
            return;
        }

        // force user to have slightly more collateral than liabilities to prevent rounding errors
        let min_health_factor = self
            .scalar
            .fixed_mul_floor(1_0000100, SCALAR_7)
            .unwrap_optimized();
        if self.as_health_factor() < min_health_factor {
            panic_with_error!(e, PoolError::InvalidHf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{storage::PoolConfig, testutils};
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address,
    };

    #[test]
    fn test_calculate_from_positions() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths();

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);
        let (oracle, oracle_client) = testutils::create_mock_oracle(&e);

        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve_0 =
            testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_config.decimals = 9;
        reserve_config.c_factor = 0_8500000;
        reserve_config.l_factor = 0_8000000;
        reserve_data.b_supply = 100_000_000_000;
        reserve_data.d_supply = 70_000_000_000;
        reserve_data.b_rate = 1_100_000_000;
        reserve_data.d_rate = 1_150_000_000;
        reserve_config.index = 1;
        let reserve_1 =
            testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_config.decimals = 6;
        reserve_config.index = 2;
        reserve_data.b_supply = 10_000_000;
        reserve_data.d_supply = 5_000_000;
        reserve_data.b_rate = 1_001_100_000;
        reserve_data.d_rate = 1_001_200_000;
        let reserve_2 =
            testutils::create_reserve(&e, &pool, &underlying_2, &reserve_config, &reserve_data);
        oracle_client.set_price(&underlying_0, &1_0000000);
        oracle_client.set_price(&underlying_1, &2_5000000);
        oracle_client.set_price(&underlying_2, &1000_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 0,
            protocol_version: 1,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });
        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_200_000_000,
            status: 0,
        };

        let mut positions = Positions::env_default(&e, &samwise);
        e.as_contract(&pool, || {
            positions.add_collateral(&e, &reserve_0, 100_1234567);
            positions.add_liabilities(&e, &reserve_0, 1_5000000);
            positions.add_liabilities(&e, &reserve_1, 50_987_654_321);
            positions.add_supply(&e, &reserve_1, 120_987_654_321);
            positions.add_collateral(&e, &reserve_2, 0_250_000);
            storage::set_pool_config(&e, &pool_config);
            let mut pool = Pool::load(&e);
            let position_data = PositionData::calculate_from_positions(&e, &mut pool, &positions);
            assert_eq!(position_data.collateral_base, 262_7985925);
            assert_eq!(position_data.liability_base, 185_2368827);
            assert_eq!(position_data.collateral_raw, 350_3984567);
            assert_eq!(position_data.liability_raw, 148_0895061);
            assert_eq!(position_data.scalar, 1_0000000);
        });
    }

    #[test]
    fn test_require_healthy() {
        let e = Env::default();

        let position_data = PositionData {
            collateral_base: 9_1234567,
            collateral_raw: 12_0000000,
            liability_base: 9_1233333,
            liability_raw: 10_0000000,
            scalar: 1_0000000,
        };

        position_data.require_healthy(&e);
        // no panic
        assert!(true);
    }

    #[test]
    fn test_require_healthy_no_liabilites() {
        let e = Env::default();

        let position_data = PositionData {
            collateral_base: 9_1234567,
            collateral_raw: 12_0000000,
            liability_base: 0,
            liability_raw: 0,
            scalar: 1_0000000,
        };

        position_data.require_healthy(&e);
        // no panic
        assert!(true);
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(10))")]
    fn test_require_healthy_panics() {
        let e = Env::default();

        let position_data = PositionData {
            collateral_base: 9_1234567,
            collateral_raw: 12_0000000,
            liability_base: 9_1234567,
            liability_raw: 10_0000000,
            scalar: 1_0000000,
        };

        position_data.require_healthy(&e);
        // no panic
        assert!(true);
    }
}
