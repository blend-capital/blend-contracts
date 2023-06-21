use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{panic_with_error, Env};

use crate::{constants::SCALAR_7, dependencies::OracleClient, errors::PoolError, storage};

use super::{pool::Pool, Positions};

pub struct PositionData {
    /// The effective collateral balance denominated in the base asset
    pub collateral_base: i128,
    /// The effective liability balance denominated in the base asset
    pub liability_base: i128,
    /// The decimals of the base asset
    pub decimals: u32,
}

impl PositionData {
    /// Calculate the position data for a given set of of positions
    ///
    /// ### Arguments
    /// * pool - The pool
    /// * positions - The positions to calculate the health factor for
    pub fn calculate_from_positions(e: &Env, pool: &mut Pool, positions: &Positions) -> Self {
        let oracle_client = OracleClient::new(e, &pool.config.oracle);
        let oracle_decimals = oracle_client.decimals();

        let reserve_list = storage::get_res_list(e);
        let mut collateral_base = 0;
        let mut liability_base = 0;
        for i in 0..reserve_list.len() {
            let b_token_balance = positions.get_collateral(i);
            let d_token_balance = positions.get_liabilities(i);
            if b_token_balance == 0 && d_token_balance == 0 {
                continue;
            }
            let reserve = pool.load_reserve(e, &reserve_list.get_unchecked(i).unwrap());
            let asset_to_base = i128(oracle_client.get_price(&reserve.asset));

            if b_token_balance > 0 {
                // append users effective collateral to collateral_base
                let asset_collateral = reserve.to_effective_asset_from_b_token(b_token_balance);
                collateral_base += asset_to_base
                    .fixed_mul_floor(asset_collateral, 10i128.pow(reserve.decimals))
                    .unwrap();
            }

            if d_token_balance > 0 {
                // append users effective liability to liability_base
                let asset_liability = reserve.to_effective_asset_from_d_token(d_token_balance);
                liability_base += asset_to_base
                    .fixed_mul_floor(asset_liability, 10i128.pow(reserve.decimals))
                    .unwrap();
            }
        }

        PositionData {
            collateral_base,
            liability_base,
            decimals: oracle_decimals,
        }
    }

    /// Return the health factor as a ratio
    pub fn as_health_factor(&self) -> i128 {
        self.collateral_base
            .fixed_div_ceil(self.liability_base, 10i128.pow(self.decimals))
            .unwrap()
    }

    /// Check if the position data meets the minimum health factor, panic if not
    pub fn require_healthy(&self, e: &Env) {
        // force user to have slightly more collateral than liabilities to prevent rounding errors
        let min_health_factor = 10i128
            .pow(self.decimals)
            .fixed_mul_floor(1_0000100, SCALAR_7)
            .unwrap();
        if self.as_health_factor() < min_health_factor {
            panic_with_error!(e, PoolError::InvalidHf);
        }
    }
}
