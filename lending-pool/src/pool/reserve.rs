use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, unwrap::UnwrapOptimized};

use crate::{
    constants::{SCALAR_7, SCALAR_9},
    dependencies::TokenClient,
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
    pub decimals: u32,         // decimals used for balances
    pub max_util: u32,         // the maximum utilization rate for the reserve
    pub last_time: u64,        // the last block the data was updated
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
            decimals: reserve_config.decimals,
            max_util: reserve_config.max_util,
            last_time: reserve_data.last_time,
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

        let cur_util = reserve.utilization();
        let (loan_accrual, new_ir_mod) = calc_accrual(
            e,
            &reserve_config,
            cur_util,
            reserve.ir_mod,
            reserve.last_time,
        );
        reserve.ir_mod = new_ir_mod;

        // credit the backstop underlying from the accrued interest based on the backstop rate
        if pool_config.bstop_rate > 0 {
            let backstop_rate = i128(pool_config.bstop_rate);
            let b_accrual = (loan_accrual - SCALAR_9)
                .fixed_mul_floor(cur_util, SCALAR_7)
                .unwrap_optimized();
            let bstop_amount = reserve
                .total_supply()
                .fixed_mul_floor(b_accrual, SCALAR_9)
                .unwrap_optimized()
                .fixed_mul_floor(backstop_rate, SCALAR_9)
                .unwrap_optimized();
            reserve.backstop_credit += bstop_amount;
        }

        reserve.d_rate = loan_accrual
            .fixed_mul_ceil(reserve.d_rate, SCALAR_9)
            .unwrap_optimized();

        if reserve.b_supply != 0 {
            // TODO: Is it safe to calculate b_rate from accrual? If any unexpected token loss occurs
            //       the transfer rate will become unrecoverable.
            let pre_update_supply = reserve.total_supply();
            let pre_update_b_rate = reserve.b_rate;
            let token_bal = TokenClient::new(e, &asset).balance(&e.current_contract_address());
            reserve.b_rate = (reserve.total_liabilities() + token_bal - reserve.backstop_credit)
                .fixed_div_floor(reserve.b_supply, 10i128.pow(reserve.decimals))
                .unwrap_optimized();

            // credit the backstop underlying from the accrued interest based on the backstop rate
            let b_rate_accrual = reserve.b_rate - pre_update_b_rate;
            if pool_config.bstop_rate > 0 && b_rate_accrual > 0 {
                reserve.backstop_credit += reserve.to_asset_from_b_token(
                    pre_update_supply
                        .fixed_mul_floor(b_rate_accrual, 10i128.pow(reserve.decimals))
                        .unwrap_optimized()
                        .fixed_mul_floor(i128(pool_config.bstop_rate), SCALAR_9)
                        .unwrap_optimized(),
                );
            }
        }

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
        if self.decimals < 7 {
            (self.total_liabilities() * 10i128.pow(7 - self.decimals))
                .fixed_div_floor(self.total_supply(), 10i128.pow(self.decimals))
                .unwrap_optimized()
        } else {
            self.total_liabilities()
                .fixed_div_floor(self.total_supply(), 10i128.pow(self.decimals))
                .unwrap_optimized()
                / (10i128.pow(self.decimals - 7))
        }
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
        d_tokens.fixed_mul_ceil(self.d_rate, SCALAR_9).unwrap_optimized()
    }

    /// Convert b_tokens to the corresponding asset value
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of tokens to convert
    pub fn to_asset_from_b_token(&self, b_tokens: i128) -> i128 {
        b_tokens
            .fixed_mul_floor(self.b_rate, 10i128.pow(self.decimals))
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
        amount.fixed_div_ceil(self.d_rate, SCALAR_9).unwrap_optimized()
    }

    /// Convert asset tokens to the corresponding d token value - rounding down
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_d_token_down(&self, amount: i128) -> i128 {
        amount.fixed_div_floor(self.d_rate, SCALAR_9).unwrap_optimized()
    }

    /// Convert asset tokens to the corresponding b token value - round up
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_b_token_up(&self, amount: i128) -> i128 {
        amount.fixed_div_ceil(self.b_rate, SCALAR_9).unwrap_optimized()
    }

    /// Convert asset tokens to the corresponding b token value - round down
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_b_token_down(&self, amount: i128) -> i128 {
        amount.fixed_div_floor(self.b_rate, SCALAR_9).unwrap_optimized()
    }
}
