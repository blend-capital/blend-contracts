use cast::i128;
use fixed_point_math::{FixedPoint, STROOP};
use soroban_sdk::Env;

use crate::{
    constants::{BLOCKS_PER_YEAR, SCALAR_9},
    storage::ReserveConfig,
};

/// Calculates the loan accrual ratio for the Reserve based on the current utilization and
/// rate modifier for the reserve.
///
/// ### Arguments
/// * `config` - The Reserve config to calculate an accrual for
/// * `cur_util` - The current utilization rate of the reserve
/// * `ir_mod` - The current interest rate modifier of the reserve
/// * `last_block` - The last block an accrual was performed
///
/// ### Returns
/// * (i128, i128) - (accrual amount scaled to 9 decimal places, new interest rate modifier scaled to 9 decimal places)
pub fn calc_accrual(
    e: &Env,
    config: &ReserveConfig,
    cur_util: i128,
    ir_mod: i128,
    last_block: u32,
) -> (i128, i128) {
    let cur_ir: i128;
    let target_util: i128 = i128(config.util);
    if cur_util <= target_util {
        let util_scalar = cur_util.fixed_div_ceil(target_util, i128(STROOP)).unwrap();
        let base_rate = util_scalar
            .fixed_mul_ceil(i128(config.r_one), i128(STROOP))
            .unwrap()
            + 0_0100000;

        cur_ir = base_rate.fixed_mul_ceil(ir_mod, SCALAR_9).unwrap();
    } else if cur_util <= 0_9500000 {
        let util_scalar = (cur_util - target_util)
            .fixed_div_ceil(0_9500000 - target_util, i128(STROOP))
            .unwrap();
        let base_rate = util_scalar
            .fixed_mul_ceil(i128(config.r_two), i128(STROOP))
            .unwrap()
            + i128(config.r_one)
            + 0_0100000;

        cur_ir = base_rate.fixed_mul_ceil(ir_mod, SCALAR_9).unwrap();
    } else {
        let util_scalar = (cur_util - 0_9500000)
            .fixed_div_ceil(0_0500000, i128(STROOP))
            .unwrap();
        let extra_rate = util_scalar
            .fixed_mul_ceil(i128(config.r_three), i128(STROOP))
            .unwrap();

        let intersection = ir_mod
            .fixed_mul_ceil(i128(config.r_two + config.r_one + 0_0100000), SCALAR_9)
            .unwrap();
        cur_ir = extra_rate + intersection;
    }

    // update rate_modifier
    // scale delta blocks and util dif to 9 decimals
    let delta_blocks_scaled = i128(e.ledger().sequence() - last_block) * SCALAR_9;
    let util_dif_scaled = (cur_util - target_util) * 100;
    let new_ir_mod: i128;
    if util_dif_scaled >= 0 {
        // rate modifier increasing
        let util_error = delta_blocks_scaled
            .fixed_mul_floor(util_dif_scaled, SCALAR_9)
            .unwrap();
        let rate_dif = util_error
            .fixed_mul_floor(i128(config.reactivity), SCALAR_9)
            .unwrap();
        let next_ir_mod = ir_mod + rate_dif;
        if next_ir_mod > 10_000_000_000 {
            new_ir_mod = 10_000_000_000;
        } else {
            new_ir_mod = next_ir_mod;
        }
    } else {
        // rate modifier decreasing
        let util_error = delta_blocks_scaled
            .fixed_mul_ceil(util_dif_scaled, SCALAR_9)
            .unwrap();
        let rate_dif = util_error
            .fixed_mul_ceil(i128(config.reactivity), SCALAR_9)
            .unwrap();
        let next_ir_mod = ir_mod + rate_dif;
        if next_ir_mod < 0_100_000_000 {
            new_ir_mod = 0_100_000_000;
        } else {
            new_ir_mod = next_ir_mod;
        }
    }

    // calc accrual amount over blocks
    let block_weight = delta_blocks_scaled / BLOCKS_PER_YEAR;
    (
        1_000_000_000 + block_weight.fixed_mul_ceil(cur_ir * 100, SCALAR_9).unwrap(),
        new_ir_mod,
    )
}

#[cfg(test)]
mod tests {
    use crate::{
        testutils::{create_reserve},
    };

    use super::*;
    use soroban_sdk::testutils::{Ledger, LedgerInfo};

    #[test]
    fn test_calc_accrual_util_under_target() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_100_000_000);
        reserve.data.d_rate = 1_000_000_000;
        reserve.data.b_supply = 90_0000000;
        reserve.data.d_supply = 65_0000000;

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let (accrual, ir_mod) =
            calc_accrual(&e, &reserve.config, 0_6565656, i128(reserve.data.ir_mod), 0);

        assert_eq!(accrual, 1_000_000_853);
        assert_eq!(ir_mod, 0_999_906_566);
    }

    #[test]
    fn test_calc_accrual_util_over_target() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_100_000_000);
        reserve.data.d_rate = 1_000_000_000;
        reserve.data.b_supply = 90_0000000;
        reserve.data.d_supply = 79_0000000;

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let (accrual, ir_mod) =
            calc_accrual(&e, &reserve.config, 0_7979797, i128(reserve.data.ir_mod), 0);

        assert_eq!(accrual, 1_000_002_853);
        assert_eq!(ir_mod, 1_000_047_979);
    }

    #[test]
    fn test_calc_accrual_util_over_95() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_100_000_000);
        reserve.data.d_rate = 1_000_000_000;
        reserve.data.b_supply = 90_0000000;
        reserve.data.d_supply = 96_0000000;

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let (accrual, ir_mod) =
            calc_accrual(&e, &reserve.config, 0_9696969, i128(reserve.data.ir_mod), 0);

        assert_eq!(accrual, 1_000_018_247);
        assert_eq!(ir_mod, 1_000_219_696);
    }

    #[test]
    fn test_calc_ir_mod_over_limit() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_100_000_000);
        reserve.data.d_rate = 1_000_000_000;
        reserve.data.ir_mod = 9_997_000_000;
        reserve.data.b_supply = 90_0000000;
        reserve.data.d_supply = 96_0000000;

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 10000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let (_accrual, ir_mod) =
            calc_accrual(&e, &reserve.config, 0_9696969, i128(reserve.data.ir_mod), 0);

        assert_eq!(ir_mod, 10_000_000_000);
    }

    #[test]
    fn test_calc_ir_mod_under_limit() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_100_000_000);
        reserve.data.d_rate = 1_000_000_000;
        reserve.data.ir_mod = 0_150_000_000;
        reserve.data.b_supply = 90_0000000;
        reserve.data.d_supply = 20_0000000;

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 10000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let (_accrual, ir_mod) =
            calc_accrual(&e, &reserve.config, 0_2020202, i128(reserve.data.ir_mod), 0);

        assert_eq!(ir_mod, 0_100_000_000);
    }

    #[test]
    fn test_calc_accrual_rounds_up() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_100_000_000);
        reserve.data.d_rate = 1_000_000_000;
        reserve.data.ir_mod = 0_100_000_000;
        reserve.data.b_supply = 90_0000000;
        reserve.data.d_supply = 20_0000000;

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let (accrual, ir_mod) = calc_accrual(
            &e,
            &reserve.config,
            0_0500000,
            i128(reserve.data.ir_mod),
            99,
        );

        assert_eq!(accrual, 1_000_000_001);
        assert_eq!(ir_mod, 0_100_000_000);
    }
}
