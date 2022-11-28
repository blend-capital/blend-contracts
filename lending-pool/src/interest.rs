use soroban_sdk::Env;

use crate::storage::ReserveConfig;

// TODO: Fixed Point Math lib
fn mul_7(a: &u64, b: &u64) -> u64 {
    mul_div_u64(a, b, &1_0000000)
}

fn div_7(a: &u64, b: &u64) -> u64 {
    mul_div_u64(a, &1_0000000, b)
}

fn mul_9(a: &u64, b: &u64) -> u64 {
    mul_div_u64(a, b, &1_000_000_000)
}

fn _div_9(a: &u64, b: &u64) -> u64 {
    mul_div_u64(a, &1_000_000_000, b)
}

fn mul_div_u64(a: &u64, b: &u64, scalar: &u64) -> u64 {
    ((a.clone() as u128 * b.clone() as u128) / (scalar.clone() as u128)) as u64
}

const BLOCKS_PER_YEAR: u64 = 6307200;

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
/// * (u64, u64) - (accrual amount scaled to 9 decimal places, new interest rate modifier scaled to 9 decimal places)
pub fn calc_accrual(
    e: &Env,
    config: &ReserveConfig,
    cur_util: u64,
    ir_mod: u64,
    last_block: u32,
) -> (u64, u64) {
    let cur_ir: u64;
    if cur_util <= config.util.clone().into() {
        let base_rate =
            mul_7(&div_7(&cur_util, &config.util.into()), &config.r_one.into()) + 0_0100000;
        cur_ir = mul_9(&ir_mod, &base_rate);
    } else if cur_util <= 0_9500000 {
        let base_rate = mul_7(
            &div_7(
                &(cur_util - config.util as u64),
                &(0_9500000 - config.util as u64),
            ),
            &config.r_two.into(),
        ) + config.r_one as u64
            + 0_0100000;
        cur_ir = mul_9(&ir_mod, &base_rate);
    } else {
        let extra_rate = mul_7(
            &div_7(&(cur_util - 0_9500000), &0_0500000),
            &config.r_three.into(),
        );
        let intersection = mul_9(&ir_mod, &((config.r_two + config.r_one + 0_0100000) as u64));
        cur_ir = extra_rate + intersection;
    }

    // update rate_modifier
    let delta_blocks = (e.ledger().sequence() - last_block) as u64;
    let target_util = config.util as u64;
    let new_ir_mod: u64;
    if cur_util >= target_util {
        // rate modifier increasing
        let util_error = mul_9(
            &(delta_blocks * 1_000_000_000),
            &((cur_util - target_util) * 100),
        );
        let rate_dif = mul_9(&util_error, &(config.reactivity.into()));
        let next_ir_mod = ir_mod + rate_dif;
        if next_ir_mod > 10_000_000_000 {
            new_ir_mod = 10_000_000_000;
        } else {
            new_ir_mod = next_ir_mod;
        }
    } else {
        // rate modifier decreasing
        let util_error = mul_9(
            &(delta_blocks * 1_000_000_000),
            &((target_util - cur_util) * 100),
        );
        let rate_dif = mul_9(&util_error, &(config.reactivity.into()));
        if ir_mod <= rate_dif + 0_100_000_000 {
            new_ir_mod = 0_100_000_000;
        } else {
            new_ir_mod = ir_mod - rate_dif;
        }
    }

    // calc accrual amount over blocks
    // TODO: Check if decimals are enough - manually doing 9 decimal fixed point math
    let block_weight = (delta_blocks * 1_000_000_000) / BLOCKS_PER_YEAR;
    (
        1_000_000_000 + (cur_ir * 100 * block_weight) / 1_000_000_000,
        new_ir_mod,
    )
}

#[cfg(test)]
mod tests {
    use crate::{
        reserve::Reserve,
        storage::{ReserveConfig, ReserveData},
        testutils::generate_contract_id,
    };

    use super::*;
    use soroban_sdk::testutils::{Ledger, LedgerInfo};

    #[test]
    fn test_calc_accrual_util_under_target() {
        let e = Env::default();
        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000, // 10e-5
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_100_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 90_0000000,
                d_supply: 65_0000000,
                last_block: 0,
            },
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let (accrual, ir_mod) =
            calc_accrual(&e, &reserve.config, 0_6565656, reserve.data.ir_mod, 0);

        assert_eq!(accrual, 1_000_000_852);
        assert_eq!(ir_mod, 0_999_906_566);
    }

    #[test]
    fn test_calc_accrual_util_over_target() {
        let e = Env::default();
        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000, // 10e-5
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_100_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 90_0000000,
                d_supply: 79_0000000,
                last_block: 0,
            },
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let (accrual, ir_mod) =
            calc_accrual(&e, &reserve.config, 0_7979797, reserve.data.ir_mod, 0);

        assert_eq!(accrual, 1_000_002_852);
        assert_eq!(ir_mod, 1_000_047_979);
    }

    #[test]
    fn test_calc_accrual_util_over_95() {
        let e = Env::default();
        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000, // 10e-5
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_100_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 90_0000000,
                d_supply: 96_0000000,
                last_block: 0,
            },
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let (accrual, ir_mod) =
            calc_accrual(&e, &reserve.config, 0_9696969, reserve.data.ir_mod, 0);

        assert_eq!(accrual, 1_000_018_246);
        assert_eq!(ir_mod, 1_000_219_696);
    }

    #[test]
    fn test_calc_ir_mod_over_limit() {
        let e = Env::default();
        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_5500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000, // 10e-5
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_100_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 9_997_000_000,
                b_supply: 90_0000000,
                d_supply: 96_0000000,
                last_block: 0,
            },
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 10000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let (_accrual, ir_mod) =
            calc_accrual(&e, &reserve.config, 0_9696969, reserve.data.ir_mod, 0);

        assert_eq!(ir_mod, 10_000_000_000);
    }

    #[test]
    fn test_calc_ir_mod_under_limit() {
        let e = Env::default();
        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 0,
                util: 0_8500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000, // 10e-5
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_100_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 0_150_000_000,
                b_supply: 90_0000000,
                d_supply: 20_0000000,
                last_block: 0,
            },
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 10000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let (_accrual, ir_mod) =
            calc_accrual(&e, &reserve.config, 0_2020202, reserve.data.ir_mod, 0);

        assert_eq!(ir_mod, 0_100_000_000);
    }
}
