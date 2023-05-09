use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{Address, BytesN, Env, Symbol};

use crate::{
    constants::{SCALAR_7, SCALAR_9},
    dependencies::TokenClient,
    emissions,
    errors::PoolError,
    interest::calc_accrual,
    storage::{self, PoolConfig, ReserveConfig, ReserveData},
};

pub struct Reserve {
    pub asset: BytesN<32>,
    pub config: ReserveConfig,
    pub data: ReserveData,
    pub b_rate: Option<i128>,
}

impl Reserve {
    /// Load a reserve
    ///
    /// ### Arguments
    /// * `asset` - The contract address for the reserve asset
    ///
    /// ### Panics
    /// If the `asset` is not a reserve
    pub fn load(e: &Env, asset: BytesN<32>) -> Reserve {
        let config = storage::get_res_config(&e, &asset);
        let data = storage::get_res_data(&e, &asset);
        Reserve {
            asset,
            config,
            data,
            b_rate: None,
        }
    }

    /// Performs updates to the reserve before an action has taken place, including accruing interest
    /// and managing emissions.
    ///
    /// Does not write ReserveData to the ledger. This must be written later once
    /// the action has been completed.
    ///
    /// ### Arguments
    /// * `res_token_type` - The reserve token being acted against (0 for d_token / 1 for b_token)
    /// * `user` - The user performing the action
    ///
    /// ### Errors
    /// This function will return an error if the emission or rate update fails
    pub fn pre_action(
        &mut self,
        e: &Env,
        pool_config: &PoolConfig,
        res_token_type: u32,
        user: Address,
    ) -> Result<(), PoolError> {
        emissions::update_reserve(e, &self, res_token_type, &user)?;

        let to_mint = self.update_rates(e, pool_config.bstop_rate);

        if to_mint > 0 {
            let backstop = Address::from_contract_id(e, &storage::get_backstop(e));
            TokenClient::new(&e, &self.config.b_token).mint(
                &e.current_contract_address(),
                &backstop,
                &to_mint,
            );
        }

        Ok(())
    }

    /// Update the reserve rates based on the current chain state and mint any tokens due to the backstop.
    ///
    /// Returns the amount of b_tokens to mint to the backstop module
    ///
    /// Does not store reserve data back to ledger
    ///
    /// @ dev - Do not use if any b or d token balances will be adjusted for a user, use `pre_action` instead
    ///
    /// TODO: Fix backstop emissions issues
    pub fn update_rates_and_mint_backstop(
        &mut self,
        e: &Env,
        pool_config: &PoolConfig,
    ) -> Result<(), PoolError> {
        let to_mint = self.update_rates(e, pool_config.bstop_rate);

        if to_mint > 0 {
            let backstop = Address::from_contract_id(e, &storage::get_backstop(e));
            TokenClient::new(&e, &self.config.b_token).mint(
                &e.current_contract_address(),
                &backstop,
                &to_mint,
            );
        }

        Ok(())
    }

    /// Update the reserve rates based on the current chain state, where no action is due to be taken
    /// against the reserve.
    ///
    /// Returns the amount of b_tokens to mint to the backstop module
    ///
    /// Does not store reserve data back to ledger
    ///
    /// @dev - Do not write data to chain without minting backstop module, or use
    ///        `update_rates_and_mint_backstop` if no action is being taken, and
    ///        `pre_action` if an action is being taken
    pub fn update_rates(&mut self, e: &Env, bstop_rate: u64) -> i128 {
        // if updating has already happened this block, don't repeat
        if e.ledger().timestamp() == self.data.last_time {
            return 0;
        }
        let total_supply = self.total_supply(e);
        // if the reserve does not have any supply, don't update rates
        // but accrue the block as this was a valid interaction
        if total_supply == 0 {
            self.data.last_time = e.ledger().timestamp();
            return 0;
        }

        // accrue interest to current block
        let cur_util = self
            .total_liabilities()
            .fixed_div_floor(total_supply, SCALAR_7)
            .unwrap();
        let (loan_accrual, new_ir_mod) = calc_accrual(
            e,
            &self.config,
            cur_util,
            self.data.ir_mod,
            self.data.last_time,
        );
        let mut bstop_amount: i128 = 0;
        if bstop_rate > 0 {
            let backstop_rate = i128(bstop_rate);
            let b_accrual = (loan_accrual - SCALAR_9)
                .fixed_mul_floor(i128(cur_util), SCALAR_7)
                .unwrap();
            bstop_amount = b_accrual
                .fixed_mul_floor(total_supply, SCALAR_9)
                .unwrap()
                .fixed_mul_floor(backstop_rate, SCALAR_9)
                .unwrap();
            self.add_supply(&bstop_amount);
        }

        self.data.d_rate = loan_accrual
            .fixed_mul_ceil(self.data.d_rate, SCALAR_9)
            .unwrap();
        self.data.ir_mod = new_ir_mod;
        self.b_rate = None;

        self.data.last_time = e.ledger().timestamp();
        e.events().publish(
            (Symbol::new(&e, "updt_rate"), self.asset.clone()),
            (self.data.d_rate, self.data.ir_mod),
        );
        bstop_amount
    }

    /// Adds tokens to the total b token supply
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of b_tokens minted
    pub fn add_supply(&mut self, b_tokens: &i128) {
        self.data.b_supply += b_tokens;
    }

    /// Removes tokens from the total b token supply
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of b_tokens burnt
    pub fn remove_supply(&mut self, b_tokens: &i128) {
        // rust underflow protection will error if b_tokens is too large
        self.data.b_supply -= b_tokens;
    }

    /// Adds tokens to the total d token supply
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of d_tokens minted
    pub fn add_liability(&mut self, d_tokens: &i128) {
        self.data.d_supply += d_tokens;
    }

    /// Removes tokens from the total d token supply
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of d_tokens burnt
    pub fn remove_liability(&mut self, d_tokens: &i128) {
        self.data.d_supply -= d_tokens;
    }

    /// Persist reserve data to ledger
    pub fn set_data(&self, e: &Env) {
        storage::set_res_data(e, &self.asset, &self.data);
    }

    // ***** Conversion functions *****

    /// Fetch the total liabilities for the reserve
    pub fn total_liabilities(&self) -> i128 {
        self.to_asset_from_d_token(self.data.d_supply)
    }

    /// Fetch the total supply for the reserve
    pub fn total_supply(&mut self, e: &Env) -> i128 {
        self.to_asset_from_b_token(e, self.data.b_supply)
    }

    /// Convert d_tokens to the corresponding asset value
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of tokens to convert
    pub fn to_asset_from_d_token(&self, d_tokens: i128) -> i128 {
        self.data.d_rate.fixed_mul_ceil(d_tokens, SCALAR_9).unwrap()
    }

    /// Convert b_tokens to the corresponding asset value
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of tokens to convert
    pub fn to_asset_from_b_token(&mut self, e: &Env, b_tokens: i128) -> i128 {
        self.get_b_rate(e)
            .fixed_mul_floor(b_tokens, SCALAR_9)
            .unwrap()
    }

    /// Convert d_tokens to their corresponding effective asset value. This
    /// takes into account the liability factor.
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of tokens to convert
    pub fn to_effective_asset_from_d_token(&self, d_tokens: i128) -> i128 {
        let assets = self.to_asset_from_d_token(d_tokens);
        assets
            .fixed_div_ceil(i128(self.config.l_factor), SCALAR_7)
            .unwrap()
    }

    /// Convert b_tokens to the corresponding effective asset value. This
    /// takes into account the collateral factor.
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of tokens to convert
    pub fn to_effective_asset_from_b_token(&mut self, e: &Env, b_tokens: i128) -> i128 {
        let assets = self.to_asset_from_b_token(e, b_tokens);
        assets
            .fixed_mul_floor(i128(self.config.c_factor), SCALAR_7)
            .unwrap()
    }

    /// Convert asset tokens to the corresponding d token value - rounding up
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_d_token_up(&self, amount: i128) -> i128 {
        amount.fixed_div_ceil(self.data.d_rate, SCALAR_9).unwrap()
    }

    /// Convert asset tokens to the corresponding d token value - rounding down
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_d_token_down(&self, amount: i128) -> i128 {
        amount.fixed_div_floor(self.data.d_rate, SCALAR_9).unwrap()
    }

    /// Convert asset tokens to the corresponding b token value - round up
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_b_token_up(&mut self, e: &Env, amount: i128) -> i128 {
        amount.fixed_div_ceil(self.get_b_rate(e), SCALAR_9).unwrap()
    }

    /// Convert asset tokens to the corresponding b token value - round down
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_b_token_down(&mut self, e: &Env, amount: i128) -> i128 {
        amount
            .fixed_div_floor(self.get_b_rate(e), SCALAR_9)
            .unwrap()
    }

    /// Fetch or calculate the `b_token_rate` based on outstanding tokens
    pub fn get_b_rate(&mut self, e: &Env) -> i128 {
        match self.b_rate {
            Some(rate) => rate,
            None => {
                let b_rate: i128;
                if self.data.b_supply == 0 {
                    b_rate = 1_000_000_000;
                } else {
                    let token_bal =
                        TokenClient::new(e, &self.asset).balance(&e.current_contract_address());
                    // TODO: Should b_token_rates take into account "partial" tokens? Getting rounded away in `total_liabilities`
                    b_rate = (self.total_liabilities() + token_bal)
                        .fixed_div_floor(self.data.b_supply, SCALAR_9)
                        .unwrap();
                }
                self.b_rate = Some(b_rate);
                b_rate
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testutils::{create_reserve, generate_contract_id, setup_reserve};

    use super::*;
    use soroban_sdk::testutils::{Address as AddressTestTrait, Ledger, LedgerInfo};

    /***** Pre Action *****/

    #[test]
    fn test_pre_action() {
        let e = Env::default();

        let pool_id = generate_contract_id(&e);
        let backstop_id = generate_contract_id(&e);
        let backstop = &Address::from_contract_id(&e, &backstop_id);
        let oracle_id = generate_contract_id(&e);

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_123_456_789);
        reserve.data.d_rate = 1_345_678_123;
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve);

        let b_token_client = TokenClient::new(&e, &reserve.config.b_token);

        e.ledger().set(LedgerInfo {
            timestamp: 123456 * 5,
            protocol_version: 1,
            sequence_number: 123456,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_200_000_000,
            status: 0,
        };

        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);
            storage::set_res_config(&e, &reserve.asset, &reserve.config);
            storage::set_res_data(&e, &reserve.asset, &reserve.data);

            reserve.pre_action(&e, &pool_config, 0, samwise).unwrap(); // (accrual: 1_002_957_369, util: .7864352)

            // assert_eq!(reserve.data.b_rate, 1_125_547_118);
            assert_eq!(reserve.data.d_rate, 1_349_657_792);
            assert_eq!(reserve.data.ir_mod, 1_044_981_440);
            assert_eq!(reserve.data.last_time, 617280);
            assert_eq!(b_token_client.balance(&backstop), 0_051_735_6);
        });
    }

    /***** Update State *****/

    #[test]
    fn test_update_state_same_block_skips() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;
        reserve.data.last_time = 123;

        e.ledger().set(LedgerInfo {
            timestamp: 123,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let to_mint = reserve.update_rates(&e, 0_200_000_000);

        assert_eq!(reserve.data.d_rate, 1_000_000_000);
        assert_eq!(reserve.data.ir_mod, 1_000_000_000);
        assert_eq!(reserve.data.last_time, 123);
        assert_eq!(to_mint, 0);
    }

    #[test]
    fn test_update_state_no_supply_skips() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.b_supply = 0;
        reserve.data.d_supply = 0;
        reserve.data.last_time = 100;

        e.ledger().set(LedgerInfo {
            timestamp: 123 * 5,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let to_mint = reserve.update_rates(&e, 0_200_000_000);

        assert_eq!(reserve.data.d_rate, 1_000_000_000);
        assert_eq!(reserve.data.ir_mod, 1_000_000_000);
        assert_eq!(reserve.data.last_time, 123 * 5);
        assert_eq!(to_mint, 0);
    }

    #[test]
    fn test_update_state_one_stroop_accrual() {
        let e = Env::default();
        let pool_id = generate_contract_id(&e);

        let mut reserve = create_reserve(&e);
        reserve.data.b_supply = 100_0000000;
        reserve.data.d_supply = 5_0000000;
        reserve.data.ir_mod = 0_100_000_000;
        reserve.data.last_time = 99;

        e.ledger().set(LedgerInfo {
            timestamp: 100,
            protocol_version: 1,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let to_mint = reserve.update_rates(&e, 0_200_000_000); // (accrual: 1_000_000_008, util: 0_6565656)

            // assert_eq!(reserve.data.b_rate, 1_000_000_000);
            assert_eq!(reserve.data.d_rate, 1_000_000_001);
            assert_eq!(reserve.data.ir_mod, 0_100_000_000);
            assert_eq!(reserve.data.last_time, 100);
            assert_eq!(to_mint, 0);
        });
    }

    #[test]
    fn test_update_state_small_block_dif() {
        let e = Env::default();
        let pool_id = generate_contract_id(&e);

        let mut reserve = create_reserve(&e);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;
        reserve.data.last_time = 0;

        e.ledger().set(LedgerInfo {
            timestamp: 100 * 5,
            protocol_version: 1,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
        });
        e.as_contract(&pool_id, || {
            let to_mint = reserve.update_rates(&e, 0_200_000_000); // (accrual: 1_000_000_852, util: 0_6565656)

            // assert_eq!(reserve.data.b_rate, 1_000_000_448);
            assert_eq!(reserve.data.d_rate, 1_000_000_853);
            assert_eq!(reserve.data.ir_mod, 0_999_906_566);
            assert_eq!(reserve.data.last_time, 100 * 5);
            assert_eq!(to_mint, 0_0000110);
        });
    }

    #[test]
    fn test_update_state_large_block_dif() {
        let e = Env::default();
        let pool_id = generate_contract_id(&e);

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_123_456_789);
        reserve.data.d_rate = 1_345_678_123;
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;
        reserve.data.last_time = 0;

        e.ledger().set(LedgerInfo {
            timestamp: 123456 * 5,
            protocol_version: 1,
            sequence_number: 123456,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&pool_id, || {
            let to_mint = reserve.update_rates(&e, 0_200_000_000); // (accrual: 1_002_957_369, util: .7864352)

            // assert_eq!(reserve.data.b_rate, 1_125_547_118);
            assert_eq!(reserve.data.d_rate, 1_349_657_792);
            assert_eq!(reserve.data.ir_mod, 1_044_981_440);
            assert_eq!(reserve.data.last_time, 123456 * 5);
            assert_eq!(to_mint, 0_051_735_6);
        });
    }

    /***** Total Supply / Liability Management *****/

    #[test]
    fn test_add_supply() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        reserve.add_supply(&1_1234567);

        assert_eq!(reserve.data.b_supply, 99_0000000 + 1_1234567);
    }

    #[test]
    fn test_remove_supply() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        reserve.remove_supply(&1_1234567);

        assert_eq!(reserve.data.b_supply, 99_0000000 - 1_1234567);
    }

    #[test]
    fn test_add_liability() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        reserve.add_liability(&1_1234567);

        assert_eq!(reserve.data.d_supply, 65_0000000 + 1_1234567);
    }

    #[test]
    fn test_remove_liability() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        reserve.remove_liability(&1_1234567);

        assert_eq!(reserve.data.d_supply, 65_0000000 - 1_1234567);
    }

    /***** Token Transfer Math *****/

    #[test]
    fn test_to_asset_from_d_token() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.d_rate = 1_321_834_961;
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        let result = reserve.to_asset_from_d_token(1_1234567);

        assert_eq!(result, 1_4850244);
    }

    #[test]
    fn test_to_asset_from_b_token() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_321_834_961);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        let result = reserve.to_asset_from_b_token(&e, 1_1234567);

        assert_eq!(result, 1_4850243);
    }

    #[test]
    fn test_to_effective_asset_from_d_token() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.d_rate = 1_321_834_961;
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;
        reserve.config.l_factor = 1_1000000;

        let result = reserve.to_effective_asset_from_d_token(1_1234567);

        assert_eq!(result, 1_3500222);
    }

    #[test]
    fn test_to_effective_asset_from_b_token() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_321_834_961);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;
        reserve.config.c_factor = 0_8500000;

        let result = reserve.to_effective_asset_from_b_token(&e, 1_1234567);

        assert_eq!(result, 1_2622706);
    }

    #[test]
    fn test_total_liabilities() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.d_rate = 1_823_912_692;
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        let result = reserve.total_liabilities();

        assert_eq!(result, 118_5543250);
    }

    #[test]
    fn test_total_supply() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_823_912_692);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        let result = reserve.total_supply(&e);

        assert_eq!(result, 180_5673565);
    }

    #[test]
    fn test_to_d_token_up() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.d_rate = 1_321_834_961;
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        let result = reserve.to_d_token_up(1_4850243);

        assert_eq!(result, 1_1234567);
    }

    #[test]
    fn test_to_d_token_down() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.data.d_rate = 1_321_834_961;
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        let result = reserve.to_d_token_down(1_4850243);

        assert_eq!(result, 1_1234566);
    }

    #[test]
    fn test_to_b_token_up() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_321_834_961);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        let result = reserve.to_b_token_up(&e, 1_4850243);

        assert_eq!(result, 1_1234567);
    }

    #[test]
    fn test_to_b_token_down() {
        let e = Env::default();

        let mut reserve = create_reserve(&e);
        reserve.b_rate = Some(1_321_834_961);
        reserve.data.b_supply = 99_0000000;
        reserve.data.d_supply = 65_0000000;

        let result = reserve.to_b_token_down(&e, 1_4850243);

        assert_eq!(result, 1_1234566);
    }
}
