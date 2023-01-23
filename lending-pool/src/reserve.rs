use soroban_auth::Identifier;
use soroban_sdk::{symbol, BytesN, Env};

use crate::{
    emissions_distributor,
    errors::PoolError,
    interest::calc_accrual,
    storage::{PoolDataStore, ReserveConfig, ReserveData, StorageManager},
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
        res_token_type: u32,
        user: Identifier,
    ) -> Result<(), PoolError> {
        self.update_rates(e);

        emissions_distributor::update(e, &self, res_token_type, user)
    }

    /// Update the reserve rates based on the current chain state
    ///
    /// Does not store reserve data back to ledger
    pub fn update_rates(&mut self, e: &Env) {
        // if updating has already happened this block, don't repeat
        if e.ledger().sequence() == self.data.last_block {
            return;
        }

        // accrue interest to current block
        let cur_util = ((self.total_liabilities() as u128 * 1_0000000 as u128)
            / self.total_supply() as u128) as u64;
        let (loan_accrual, new_ir_mod) = calc_accrual(
            e,
            &self.config,
            cur_util,
            self.data.ir_mod,
            self.data.last_block,
        );
        let b_rate_accrual =
            ((loan_accrual - 1_000_000_000) * cur_util) / 1_0000000 + 1_000_000_000;
        self.data.b_rate = (self.data.b_rate * b_rate_accrual) / 1_000_000_000; // TODO: Will overflow with u64 math past 18x
        self.data.d_rate = (self.data.d_rate * loan_accrual) / 1_000_000_000;
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
    }

    /// Adds tokens to the total b token supply
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of b_tokens minted
    pub fn add_supply(&mut self, b_tokens: &u64) {
        self.data.b_supply += b_tokens;
    }

    /// Removes tokens from the total b token supply
    ///
    /// ### Arguments
    /// * `b_tokens` - The amount of b_tokens burnt
    pub fn remove_supply(&mut self, b_tokens: &u64) {
        // rust underflow protection will error if b_tokens is too large
        self.data.b_supply -= b_tokens;
    }

    /// Adds tokens to the total d token supply
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of d_tokens minted
    pub fn add_liability(&mut self, d_tokens: &u64) {
        self.data.d_supply += d_tokens;
    }

    /// Removes tokens from the total d token supply
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of d_tokens burnt
    pub fn remove_liability(&mut self, d_tokens: &u64) {
        self.data.d_supply -= d_tokens;
    }

    /// Persist reserve data to ledger
    pub fn set_data(&self, e: &Env) {
        StorageManager::new(e).set_res_data(self.asset.clone(), self.data.clone());
    }

    // ***** Conversion functions *****

    /// Fetch the total liabilities for the reserve
    pub fn total_liabilities(&self) -> u64 {
        self.to_asset_from_d_token(&self.data.d_supply)
    }

    /// Fetch the total supply for the reserve
    pub fn total_supply(&self) -> u64 {
        self.to_asset_from_b_token(&self.data.b_supply)
    }

    /// Convert d tokens to the corresponding asset value
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of tokens to convert
    pub fn to_asset_from_d_token(&self, d_tokens: &u64) -> u64 {
        (self.data.d_rate * d_tokens) / 1_000_000_000
    }

    /// Convert d tokens to the corresponding asset value
    ///
    /// ### Arguments
    /// * `d_tokens` - The amount of tokens to convert
    pub fn to_asset_from_b_token(&self, b_tokens: &u64) -> u64 {
        (self.data.b_rate * b_tokens) / 1_000_000_000
    }

    /// Convert asset tokens to the corresponding d token value
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_d_token(&self, amount: &u64) -> u64 {
        (amount * 1_000_000_000) / self.data.d_rate
    }

    /// Convert asset tokens to the corresponding b token value
    ///
    /// ### Arguments
    /// * `amount` - The amount of tokens to convert
    pub fn to_b_token(&self, amount: &u64) -> u64 {
        (amount * 1_000_000_000) / self.data.b_rate
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::{ReserveConfig, ReserveData},
        testutils::generate_contract_id,
    };

    use super::*;
    use soroban_sdk::testutils::{Ledger, LedgerInfo};

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

        reserve.update_rates(&e);

        assert_eq!(reserve.data.b_rate, 1_000_000_000);
        assert_eq!(reserve.data.d_rate, 1_000_000_000);
        assert_eq!(reserve.data.ir_mod, 1_000_000_000);
        assert_eq!(reserve.data.last_block, 123);
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

        reserve.update_rates(&e); // (accrual: 1_000_000_852, util: 0_6565656)

        assert_eq!(reserve.data.b_rate, 1_000_000_559);
        assert_eq!(reserve.data.d_rate, 1_000_000_852);
        assert_eq!(reserve.data.ir_mod, 0_999_906_566);
        assert_eq!(reserve.data.last_block, 100);
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

        reserve.update_rates(&e); // (accrual: 1_002_957_369, util: .786435)

        assert_eq!(reserve.data.b_rate, 1_126_069_701);
        assert_eq!(reserve.data.d_rate, 1_349_657_789);
        assert_eq!(reserve.data.ir_mod, 1_044_981_440);
        assert_eq!(reserve.data.last_block, 123456);
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

        let result = reserve.to_asset_from_d_token(&1_1234567);

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

        let result = reserve.to_asset_from_b_token(&1_1234567);

        assert_eq!(result, 1_4850243);
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

        let result = reserve.to_d_token(&1_4850243);

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

        let result = reserve.to_b_token(&1_4850243);

        assert_eq!(result, 1_1234566);
    }
}
