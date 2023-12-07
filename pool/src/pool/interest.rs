use cast::i128;
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{unwrap::UnwrapOptimized, Env};

use crate::{
    constants::{SCALAR_7, SCALAR_9, SECONDS_PER_YEAR},
    storage::ReserveConfig,
};

/// Calculates the loan accrual ratio for the Reserve based on the current utilization and
/// rate modifier for the reserve.
///
/// ### Arguments
/// * `config` - The Reserve config to calculate an accrual for
/// * `cur_util` - The current utilization rate of the reserve (7 decimals)
/// * `ir_mod` - The current interest rate modifier of the reserve (9 decimals)
/// * `last_block` - The last block an accrual was performed
///
/// ### Returns
/// * (i128, i128) - (accrual amount scaled to 9 decimal places, new interest rate modifier scaled to 9 decimal places)
#[allow(clippy::zero_prefixed_literal)]
pub fn calc_accrual(
    e: &Env,
    config: &ReserveConfig,
    cur_util: i128,
    ir_mod: i128,
    last_time: u64,
) -> (i128, i128) {
    let cur_ir: i128;
    let target_util: i128 = i128(config.util);
    if cur_util <= target_util {
        let util_scalar = cur_util
            .fixed_div_ceil(target_util, SCALAR_7)
            .unwrap_optimized();
        let base_rate = util_scalar
            .fixed_mul_ceil(i128(config.r_one), SCALAR_7)
            .unwrap_optimized()
            + 0_0100000;

        cur_ir = base_rate
            .fixed_mul_ceil(ir_mod, SCALAR_9)
            .unwrap_optimized();
    } else if cur_util <= 0_9500000 {
        let util_scalar = (cur_util - target_util)
            .fixed_div_ceil(0_9500000 - target_util, SCALAR_7)
            .unwrap_optimized();
        let base_rate = util_scalar
            .fixed_mul_ceil(i128(config.r_two), SCALAR_7)
            .unwrap_optimized()
            + i128(config.r_one)
            + 0_0100000;

        cur_ir = base_rate
            .fixed_mul_ceil(ir_mod, SCALAR_9)
            .unwrap_optimized();
    } else {
        let util_scalar = (cur_util - 0_9500000)
            .fixed_div_ceil(0_0500000, SCALAR_7)
            .unwrap_optimized();
        let extra_rate = util_scalar
            .fixed_mul_ceil(i128(config.r_three), SCALAR_7)
            .unwrap_optimized();

        let intersection = ir_mod
            .fixed_mul_ceil(i128(config.r_two + config.r_one + 0_0100000), SCALAR_9)
            .unwrap_optimized();
        cur_ir = extra_rate + intersection;
    }

    // update rate_modifier
    // scale delta blocks and util dif to 9 decimals
    let delta_time_scaled = i128(e.ledger().timestamp() - last_time) * SCALAR_9;
    let util_dif_scaled = (cur_util - target_util) * 100;
    let new_ir_mod: i128;
    if util_dif_scaled >= 0 {
        // rate modifier increasing
        let util_error = delta_time_scaled
            .fixed_mul_floor(util_dif_scaled, SCALAR_9)
            .unwrap_optimized();
        let rate_dif = util_error
            .fixed_mul_floor(i128(config.reactivity), SCALAR_9)
            .unwrap_optimized();
        let next_ir_mod = ir_mod + rate_dif;
        if next_ir_mod > 10_000_000_000 {
            new_ir_mod = 10_000_000_000;
        } else {
            new_ir_mod = next_ir_mod;
        }
    } else {
        // rate modifier decreasing
        let util_error = delta_time_scaled
            .fixed_mul_ceil(util_dif_scaled, SCALAR_9)
            .unwrap_optimized();
        let rate_dif = util_error
            .fixed_mul_ceil(i128(config.reactivity), SCALAR_9)
            .unwrap_optimized();
        let next_ir_mod = ir_mod + rate_dif;
        if next_ir_mod < 0_100_000_000 {
            new_ir_mod = 0_100_000_000;
        } else {
            new_ir_mod = next_ir_mod;
        }
    }

    // calc accrual amount over blocks
    let time_weight = delta_time_scaled / SECONDS_PER_YEAR;
    (
        1_000_000_000
            + time_weight
                .fixed_mul_ceil(cur_ir * 100, SCALAR_9)
                .unwrap_optimized(),
        new_ir_mod,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Ledger, LedgerInfo};

    #[test]
    fn test_calc_accrual_util_under_target() {
        let e = Env::default();

        let reserve_config = ReserveConfig {
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_7500000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 0_000_002_000,
            index: 0,
        };
        let ir_mod: i128 = 1_000_000_000;

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

        let (accrual, ir_mod) = calc_accrual(&e, &reserve_config, 0_6565656, ir_mod, 0);

        assert_eq!(accrual, 1_000_000_853);
        assert_eq!(ir_mod, 0_999_906_566);
    }

    #[test]
    fn test_calc_accrual_util_over_target() {
        let e = Env::default();

        let reserve_config = ReserveConfig {
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_7500000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 0_000_002_000,
            index: 0,
        };
        let ir_mod: i128 = 1_000_000_000;

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

        let (accrual, ir_mod) = calc_accrual(&e, &reserve_config, 0_7979797, ir_mod, 0);

        assert_eq!(accrual, 1_000_002_853);
        assert_eq!(ir_mod, 1_000_047_979);
    }

    #[test]
    fn test_calc_accrual_util_over_95() {
        let e = Env::default();

        let reserve_config = ReserveConfig {
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_7500000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 0_000_002_000,
            index: 0,
        };
        let ir_mod: i128 = 1_000_000_000;

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

        let (accrual, ir_mod) = calc_accrual(&e, &reserve_config, 0_9696969, ir_mod, 0);

        assert_eq!(accrual, 1_000_018_247);
        assert_eq!(ir_mod, 1_000_219_696);
    }

    #[test]
    fn test_calc_ir_mod_over_limit() {
        let e = Env::default();

        let reserve_config = ReserveConfig {
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_7500000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 0_000_002_000,
            index: 0,
        };
        let ir_mod: i128 = 9_997_000_000;

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 10000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let (_accrual, ir_mod) = calc_accrual(&e, &reserve_config, 0_9696969, ir_mod, 0);

        assert_eq!(ir_mod, 10_000_000_000);
    }

    #[test]
    fn test_calc_ir_mod_under_limit() {
        let e = Env::default();

        let reserve_config = ReserveConfig {
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_7500000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 0_000_002_000,
            index: 0,
        };
        let ir_mod: i128 = 0_150_000_000;

        e.ledger().set(LedgerInfo {
            timestamp: 10000 * 5,
            protocol_version: 20,
            sequence_number: 10000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let (_accrual, ir_mod) = calc_accrual(&e, &reserve_config, 0_2020202, ir_mod, 0);

        assert_eq!(ir_mod, 0_100_000_000);
    }

    #[test]
    fn test_calc_accrual_rounds_up() {
        let e = Env::default();

        let reserve_config = ReserveConfig {
            decimals: 7,
            c_factor: 0_7500000,
            l_factor: 0_7500000,
            util: 0_7500000,
            max_util: 0_9500000,
            r_one: 0_0500000,
            r_two: 0_5000000,
            r_three: 1_5000000,
            reactivity: 0_000_002_000,
            index: 0,
        };
        let ir_mod: i128 = 0_100_000_000;

        e.ledger().set(LedgerInfo {
            timestamp: 501,
            protocol_version: 20,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let (accrual, ir_mod) = calc_accrual(&e, &reserve_config, 0_0500000, ir_mod, 500);

        assert_eq!(accrual, 1_000_000_001);
        assert_eq!(ir_mod, 0_100_000_000);
    }
}
