use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{symbol, BytesN, Env};

use crate::{
    constants::{SCALAR_7, SCALAR_9},
    dependencies::TokenClient,
    emissions_distributor,
    errors::PoolError,
    interest::calc_accrual,
    storage::{PoolConfig, PoolDataStore, ReserveConfig, ReserveData, StorageManager},
};

pub struct Reserve {
    pub asset: BytesN<32>,
    pub config: ReserveConfig,
    pub data: ReserveData,
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
        let storage = StorageManager::new(e);
        let config = storage.get_res_config(asset.clone());
        let data = storage.get_res_data(asset.clone());
        Reserve {
            asset,
            config,
            data,
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
        user: Identifier,
    ) -> Result<(), PoolError> {
        let to_mint = self.update_rates(e, pool_config.bstop_rate);

        emissions_distributor::update(e, &self, res_token_type, user)?;

        if to_mint > 0 {
            let bkstp_addr = StorageManager::new(&e).get_backstop();
            TokenClient::new(&e, self.config.b_token.clone()).mint(
                &Signature::Invoker,
                &0,
                &Identifier::Contract(bkstp_addr),
                &to_mint,
            );
        }

        Ok(())
    }

    /// Update the reserve rates based on the current chain state
    ///
    /// Returns the amount of b_tokens to mint to the backstop module
    ///
    /// Does not store reserve data back to ledger
    pub fn update_rates(&mut self, e: &Env, bstop_rate: u64) -> i128 {
        // if updating has already happened this block, don't repeat
        if e.ledger().sequence() == self.data.last_block {
            return 0;
        }

        // accrue interest to current block
        let cur_util = self
            .total_liabilities()
            .fixed_div_floor(self.total_supply(), SCALAR_7)
            .unwrap();
        let (loan_accrual, new_ir_mod) = calc_accrual(
            e,
            &self.config,
            cur_util,
            self.data.ir_mod,
            self.data.last_block,
        );
        let b_rate_accrual: i128;
        let bstop_amount: i128;
        if bstop_rate > 0 {
            let backstop_rate = i128(bstop_rate);
            // mint the required amount of b_tokens to the backstop addr
            let b_accrual = (loan_accrual - SCALAR_9)
                .fixed_mul_floor(i128(cur_util), SCALAR_7)
                .unwrap();
            bstop_amount = b_accrual
                .fixed_mul_floor(self.total_supply(), SCALAR_7)
                .unwrap()
                .fixed_mul_floor(backstop_rate, SCALAR_9)
                .unwrap();
            self.add_supply(&bstop_amount);

            b_rate_accrual = b_accrual
                .fixed_mul_floor(SCALAR_9 - backstop_rate, SCALAR_9)
                .unwrap()
                + SCALAR_9;
        } else {
            b_rate_accrual = (loan_accrual - SCALAR_9)
                .fixed_mul_floor(i128(cur_util), SCALAR_7)
                .unwrap()
                + SCALAR_9;
            bstop_amount = 0;
        }

        self.data.b_rate = b_rate_accrual
            .fixed_mul_floor(self.data.b_rate, SCALAR_9)
            .unwrap();
        self.data.d_rate = loan_accrual
            .fixed_mul_ceil(self.data.d_rate, SCALAR_9)
            .unwrap();
        self.data.ir_mod = new_ir_mod;

        self.data.last_block = e.ledger().sequence();
        e.events().publish(
            (symbol!("Update"), symbol!("Reserve"), symbol!("Rates")),
            (
                &self.asset,
                self.data.b_rate,
                self.data.d_rate,
                self.data.ir_mod,
            ),
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
        StorageManager::new(e).set_res_data(self.asset.clone(), self.data.clone());
    }

    // ***** Conversion functions *****

    /// Fetch the total liabilities for the reserve
    pub fn total_liabilities(&self) -> i128 {
        self.to_asset_from_d_token(self.data.d_supply)
    }

    /// Fetch the total supply for the reserve
    pub fn total_supply(&self) -> i128 {
        self.to_asset_from_b_token(self.data.b_supply)
    }

    /// Convert d_tokens to the corresponding asset value
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of tokens to convert
    pub fn to_asset_from_d_token(&self, d_tokens: i128) -> i128 {
        self.data
            .d_rate
            .fixed_mul_floor(d_tokens, SCALAR_9)
            .unwrap()
    }

    /// Convert b_tokens to the corresponding asset value
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of tokens to convert
    pub fn to_asset_from_b_token(&self, b_tokens: i128) -> i128 {
        self.data
            .b_rate
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
            .fixed_div_floor(i128(self.config.l_factor), SCALAR_7)
            .unwrap()
    }

    /// Convert b_tokens to the corresponding effective asset value. This
    /// takes into account the collateral factor.
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of tokens to convert
    pub fn to_effective_asset_from_b_token(&self, b_tokens: i128) -> i128 {
        let assets = self.to_asset_from_b_token(b_tokens);
        assets
            .fixed_mul_floor(i128(self.config.c_factor), SCALAR_7)
            .unwrap()
    }

    /// Convert asset tokens to the corresponding d token value
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_d_token(&self, amount: i128) -> i128 {
        amount.fixed_div_floor(self.data.d_rate, SCALAR_9).unwrap()
    }

    /// Convert asset tokens to the corresponding b token value
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_b_token(&self, amount: i128) -> i128 {
        amount.fixed_div_floor(self.data.b_rate, SCALAR_9).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::{ReserveConfig, ReserveData},
        testutils::{create_token_contract, generate_contract_id},
    };

    use super::*;
    use soroban_sdk::testutils::{Accounts, Ledger, LedgerInfo};

    /***** Update State *****/

    #[test]
    fn test_pre_action() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_address = generate_contract_id(&e);
        let backstop_address = generate_contract_id(&e);
        let oracle_address = generate_contract_id(&e);

        let (b_token_id, b_token_client) =
            create_token_contract(&e, &Identifier::Contract(pool_address.clone()));

        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let mut reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: b_token_id.clone(),
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
                b_rate: 1_123_456_789,
                d_rate: 1_345_678_123,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 0,
            },
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 123456,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_200_000_000,
            status: 0,
        };

        e.as_contract(&pool_address, || {
            storage.set_pool_config(pool_config.clone());
            storage.set_backstop(backstop_address.clone());
            storage.set_res_config(reserve.asset.clone(), reserve.config.clone());
            storage.set_res_data(reserve.asset.clone(), reserve.data.clone());

            reserve.pre_action(&e, &pool_config, 0, samwise_id).unwrap(); // (accrual: 1_002_957_369, util: .7864352)

            assert_eq!(reserve.data.b_rate, 1_125_547_118);
            assert_eq!(reserve.data.d_rate, 1_349_657_792);
            assert_eq!(reserve.data.ir_mod, 1_044_981_440);
            assert_eq!(reserve.data.last_block, 123456);
            assert_eq!(
                b_token_client.balance(&Identifier::Contract(backstop_address)),
                0_051_735_661
            );
        });
    }

    /***** Update State *****/

    #[test]
    fn test_update_state_same_block_skips() {
        let e = Env::default();

        let mut reserve = Reserve {
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
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 123,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let to_mint = reserve.update_rates(&e, 0_200_000_000);

        assert_eq!(reserve.data.b_rate, 1_000_000_000);
        assert_eq!(reserve.data.d_rate, 1_000_000_000);
        assert_eq!(reserve.data.ir_mod, 1_000_000_000);
        assert_eq!(reserve.data.last_block, 123);
        assert_eq!(to_mint, 0);
    }

    #[test]
    fn test_update_state_one_stroop_accrual() {
        let e = Env::default();

        let mut reserve = Reserve {
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
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 0_100_000_000,
                b_supply: 100_0000000,
                d_supply: 5_0000000,
                last_block: 99,
            },
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let to_mint = reserve.update_rates(&e, 0_200_000_000); // (accrual: 1_000_000_008, util: 0_6565656)

        assert_eq!(reserve.data.b_rate, 1_000_000_000);
        assert_eq!(reserve.data.d_rate, 1_000_000_001);
        assert_eq!(reserve.data.ir_mod, 0_100_000_000);
        assert_eq!(reserve.data.last_block, 100);
        assert_eq!(to_mint, 0);
    }

    #[test]
    fn test_update_state_small_block_dif() {
        let e = Env::default();

        let mut reserve = Reserve {
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
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
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

        let to_mint = reserve.update_rates(&e, 0_200_000_000); // (accrual: 1_000_000_852, util: 0_6565656)

        assert_eq!(reserve.data.b_rate, 1_000_000_448);
        assert_eq!(reserve.data.d_rate, 1_000_000_853);
        assert_eq!(reserve.data.ir_mod, 0_999_906_566);
        assert_eq!(reserve.data.last_block, 100);
        // TODO: Calculator claims this is 11_068 with expected rounding. Determine why this is different
        assert_eq!(to_mint, 0_000_011_088);
    }

    #[test]
    fn test_update_state_large_block_dif() {
        let e = Env::default();

        let mut reserve = Reserve {
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
                b_rate: 1_123_456_789,
                d_rate: 1_345_678_123,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 0,
            },
        };

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 123456,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let to_mint = reserve.update_rates(&e, 0_200_000_000); // (accrual: 1_002_957_369, util: .7864352)

        assert_eq!(reserve.data.b_rate, 1_125_547_118);
        assert_eq!(reserve.data.d_rate, 1_349_657_792);
        assert_eq!(reserve.data.ir_mod, 1_044_981_440);
        assert_eq!(reserve.data.last_block, 123456);
        assert_eq!(to_mint, 0_051_735_661);
    }

    /***** Total Supply / Liability Management *****/

    #[test]
    fn test_add_supply() {
        let e = Env::default();

        let mut reserve = Reserve {
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
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        reserve.add_supply(&1_1234567);

        assert_eq!(reserve.data.b_supply, 99_0000000 + 1_1234567);
    }

    #[test]
    fn test_remove_supply() {
        let e = Env::default();

        let mut reserve = Reserve {
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
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        reserve.remove_supply(&1_1234567);

        assert_eq!(reserve.data.b_supply, 99_0000000 - 1_1234567);
    }

    #[test]
    fn test_add_liability() {
        let e = Env::default();

        let mut reserve = Reserve {
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
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        reserve.add_liability(&1_1234567);

        assert_eq!(reserve.data.d_supply, 65_0000000 + 1_1234567);
    }

    #[test]
    fn test_remove_liability() {
        let e = Env::default();

        let mut reserve = Reserve {
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
                b_rate: 1_000_000_000,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        reserve.remove_liability(&1_1234567);

        assert_eq!(reserve.data.d_supply, 65_0000000 - 1_1234567);
    }

    /***** Token Transfer Math *****/

    #[test]
    fn test_to_asset_from_d_token() {
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
                b_rate: 1_000_000_000,
                d_rate: 1_321_834_961,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        let result = reserve.to_asset_from_d_token(1_1234567);

        assert_eq!(result, 1_4850243);
    }

    #[test]
    fn test_to_asset_from_b_token() {
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
                b_rate: 1_321_834_961,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        let result = reserve.to_asset_from_b_token(1_1234567);

        assert_eq!(result, 1_4850243);
    }

    #[test]
    fn test_to_effective_asset_from_d_token() {
        let e = Env::default();

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0,
                l_factor: 1_1000000,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000, // 10e-5
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_000_000_000,
                d_rate: 1_321_834_961,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        let result = reserve.to_effective_asset_from_d_token(1_1234567);

        assert_eq!(result, 1_3500220);
    }

    #[test]
    fn test_to_effective_asset_from_b_token() {
        let e = Env::default();

        let reserve = Reserve {
            asset: generate_contract_id(&e),
            config: ReserveConfig {
                b_token: generate_contract_id(&e),
                d_token: generate_contract_id(&e),
                decimals: 7,
                c_factor: 0_8500000,
                l_factor: 0,
                util: 0_7500000,
                r_one: 0_0500000,
                r_two: 0_5000000,
                r_three: 1_5000000,
                reactivity: 0_000_010_000, // 10e-5
                index: 0,
            },
            data: ReserveData {
                b_rate: 1_321_834_961,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        let result = reserve.to_effective_asset_from_b_token(1_1234567);

        assert_eq!(result, 1_2622706);
    }

    #[test]
    fn test_total_liabilities() {
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
                b_rate: 1_000_000_000,
                d_rate: 1_823_912_692,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        let result = reserve.total_liabilities();

        assert_eq!(result, 118_5543249);
    }

    #[test]
    fn test_total_supply() {
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
                b_rate: 1_823_912_692,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        let result = reserve.total_supply();

        assert_eq!(result, 180_5673565);
    }

    #[test]
    fn test_to_d_token() {
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
                b_rate: 1_000_000_000,
                d_rate: 1_321_834_961,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        let result = reserve.to_d_token(1_4850243);

        assert_eq!(result, 1_1234566);
    }

    #[test]
    fn test_to_b_token() {
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
                b_rate: 1_321_834_961,
                d_rate: 1_000_000_000,
                ir_mod: 1_000_000_000,
                b_supply: 99_0000000,
                d_supply: 65_0000000,
                last_block: 123,
            },
        };

        let result = reserve.to_b_token(1_4850243);

        assert_eq!(result, 1_1234566);
    }
}
