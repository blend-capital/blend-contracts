use soroban_sdk::{contracttype, Address, Env, Map};

use crate::{emissions, validator::require_nonnegative};

use super::{Pool, Reserve};

/// A user / contracts position's with the pool, stored in the Reserve's decimals
#[derive(Clone)]
#[contracttype]
pub struct Positions {
    pub user: Address,
    pub liabilities: Map<u32, i128>, // Map of Reserve Index to liability share balance
    pub collateral: Map<u32, i128>,  // Map of Reserve Index to collateral supply share balance
    pub supply: Map<u32, i128>,      // Map of Reserve Index to non-collateral supply share balance
}

impl Positions {
    /// Create an empty Positions object in the environment
    pub fn env_default(e: &Env, from: &Address) -> Self {
        Positions {
            user: from.clone(),
            liabilities: Map::new(&e),
            collateral: Map::new(&e),
            supply: Map::new(&e),
        }
    }

    /// Get the debtToken position for the reserve at the given index
    pub fn get_liabilities(&self, reserve_index: u32) -> i128 {
        self.liabilities.get(reserve_index).unwrap_or(0)
    }

    /// Add liabilities to the position expressed in debtTokens
    pub fn add_liabilities(&mut self, e: &Env, reserve: &Reserve, amount: i128) {
        let old_amount = self.liabilities.get(reserve.index).unwrap_or(0);
        self.update_d_emissions(e, reserve, old_amount);
        self.liabilities.set(reserve.index, old_amount + amount);
    }

    /// Remove liabilities from the position expressed in debtTokens
    pub fn remove_liabilities(&mut self, e: &Env, reserve: &Reserve, amount: i128) {
        let old_amount = self.liabilities.get(reserve.index).unwrap_or(0);
        self.update_d_emissions(e, reserve, old_amount);
        let new_amount = self.liabilities.get(reserve.index).unwrap_or(0) - amount;
        require_nonnegative(e, &new_amount);
        if new_amount == 0 {
            self.liabilities.remove(reserve.index);
        } else {
            self.liabilities.set(reserve.index, new_amount);
        }
    }

    /// Get the collateralized blendToken position for the reserve at the given index
    pub fn get_collateral(&self, reserve_index: u32) -> i128 {
        self.collateral.get(reserve_index).unwrap_or(0)
    }

    /// Add collateral to the position expressed in blendTokens
    pub fn add_collateral(&mut self, e: &Env, reserve: &Reserve, amount: i128) {
        let old_amount = self.collateral.get(reserve.index).unwrap_or(0);
        self.update_b_emissions(e, reserve, old_amount);
        self.collateral.set(reserve.index, old_amount + amount);
    }

    /// Remove collateral from the position expressed in blendTokens
    pub fn remove_collateral(&mut self, e: &Env, reserve: &Reserve, amount: i128) {
        let old_amount = self.collateral.get(reserve.index).unwrap_or(0);
        self.update_b_emissions(e, reserve, old_amount);
        let new_amount = self.collateral.get(reserve.index).unwrap_or(0) - amount;
        require_nonnegative(e, &new_amount);
        if new_amount == 0 {
            self.collateral.remove(reserve.index);
        } else {
            self.collateral.set(reserve.index, new_amount);
        }
    }

    /// Get the uncollateralized blendToken position for the reserve at the given index
    pub fn get_supply(&self, reserve_index: u32) -> i128 {
        self.supply.get(reserve_index).unwrap_or(0)
    }

    /// Add supply to the position expressed in blendTokens
    pub fn add_supply(&mut self, e: &Env, reserve: &Reserve, amount: i128) {
        let old_amount = self.supply.get(reserve.index).unwrap_or(0);
        self.update_b_emissions(e, reserve, old_amount);
        self.supply.set(reserve.index, old_amount + amount);
    }

    /// Remove supply from the position expressed in blendTokens
    pub fn remove_supply(&mut self, e: &Env, reserve: &Reserve, amount: i128) {
        let old_amount = self.supply.get(reserve.index).unwrap_or(0);
        self.update_b_emissions(e, reserve, old_amount);
        let new_amount = old_amount - amount;
        require_nonnegative(e, &new_amount);
        if new_amount == 0 {
            self.supply.remove(reserve.index);
        } else {
            self.supply.set(reserve.index, new_amount);
        }
    }

    /// Get the total supply and collateral of blendTokens for the user at the given index
    pub fn get_total_supply(&self, reserve_index: u32) -> i128 {
        self.get_collateral(reserve_index) + self.get_supply(reserve_index)
    }

    // Removes positions from a user - does not consider supply
    pub fn rm_positions(
        &mut self,
        e: &Env,
        pool: &Pool,
        collateral_amounts: Map<Address, i128>,
        liability_amounts: Map<Address, i128>,
    ) {
        for (asset, amount) in collateral_amounts.iter() {
            let reserve = &pool.load_reserve(e, &asset);
            self.remove_collateral(e, reserve, amount);
        }
        for (asset, amount) in liability_amounts.iter() {
            let reserve = &pool.load_reserve(e, &asset);
            self.remove_liabilities(e, reserve, amount);
        }
    }
    // Adds positions to a user - does not consider supply
    pub fn add_positions(
        &mut self,
        e: &Env,
        pool: &Pool,
        collateral_amounts: Map<Address, i128>,
        liability_amounts: Map<Address, i128>,
    ) {
        for (asset, amount) in collateral_amounts.iter() {
            let reserve = &pool.load_reserve(e, &asset);
            self.add_collateral(e, &reserve, amount);
        }
        for (asset, amount) in liability_amounts.iter() {
            let reserve = &pool.load_reserve(e, &asset);
            self.add_liabilities(e, &reserve, amount);
        }
    }

    fn update_d_emissions(&self, e: &Env, reserve: &Reserve, amount: i128) {
        emissions::update_emissions(
            e,
            reserve.index * 2,
            reserve.d_supply,
            reserve.scalar,
            &self.user,
            amount,
            false,
        );
    }
    fn update_b_emissions(&self, e: &Env, reserve: &Reserve, amount: i128) {
        emissions::update_emissions(
            e,
            reserve.index * 2 + 1,
            reserve.b_supply,
            reserve.scalar,
            &self.user,
            amount,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutils;
    use soroban_sdk::{testutils::Address as _, Address};

    #[test]
    fn test_liabilities() {
        let e = Env::default();
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let mut positions = Positions::env_default(&e, &samwise);
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve_0 =
            testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_config.index = 2;
        let reserve_1 =
            testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        assert_eq!(positions.get_liabilities(0), 0);

        positions.add_liabilities(&e, &reserve_0, 123);
        assert_eq!(positions.get_liabilities(0), 123);

        positions.add_liabilities(&e, &reserve_1, 456);
        assert_eq!(positions.get_liabilities(0), 123);
        assert_eq!(positions.get_liabilities(2), 456);

        positions.remove_liabilities(&e, &reserve_1, 100);
        assert_eq!(positions.get_liabilities(2), 356);

        positions.remove_liabilities(&e, &reserve_1, 356);
        assert_eq!(positions.get_liabilities(2), 0);
        assert_eq!(positions.liabilities.len(), 1);
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(4))")]
    fn test_remove_liabilities_over_balance_panics() {
        let e = Env::default();
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let mut positions = Positions::env_default(&e, &samwise);
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve_0 =
            testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let mut positions = Positions::env_default(&e, &samwise);

        positions.add_liabilities(&e, &reserve_0, 123);
        assert_eq!(positions.get_liabilities(0), 123);

        positions.remove_liabilities(&e, &reserve_0, 124);
    }

    #[test]
    fn test_collateral() {
        let e = Env::default();
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let mut positions = Positions::env_default(&e, &samwise);
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve_0 =
            testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_config.index = 2;
        let reserve_1 =
            testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let mut positions = Positions::env_default(&e, &samwise);

        assert_eq!(positions.get_collateral(0), 0);

        positions.add_collateral(&e, &reserve_0, 123);
        assert_eq!(positions.get_collateral(0), 123);

        positions.add_collateral(&e, &reserve_1, 456);
        assert_eq!(positions.get_collateral(0), 123);
        assert_eq!(positions.get_collateral(2), 456);

        positions.remove_collateral(&e, &reserve_1, 100);
        assert_eq!(positions.get_collateral(2), 356);

        positions.remove_collateral(&e, &reserve_1, 356);
        assert_eq!(positions.get_collateral(2), 0);
        assert_eq!(positions.collateral.len(), 1);
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(4))")]
    fn test_remove_collateral_over_balance_panics() {
        let e = Env::default();
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let mut positions = Positions::env_default(&e, &samwise);
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve_0 =
            testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let mut positions = Positions::env_default(&e, &samwise);

        positions.add_collateral(&e, &reserve_0, 123);
        assert_eq!(positions.get_collateral(1), 123);

        positions.remove_collateral(&e, &reserve_0, 124);
    }

    #[test]
    fn test_supply() {
        let e = Env::default();
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let mut positions = Positions::env_default(&e, &samwise);
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve_0 =
            testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_config.index = 2;
        let reserve_1 =
            testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let mut positions = Positions::env_default(&e, &samwise);

        assert_eq!(positions.get_supply(0), 0);

        positions.add_supply(&e, &reserve_0, 123);
        assert_eq!(positions.get_supply(0), 123);

        positions.add_supply(&e, &reserve_1, 456);
        assert_eq!(positions.get_supply(0), 123);
        assert_eq!(positions.get_supply(2), 456);

        positions.remove_supply(&e, &reserve_1, 100);
        assert_eq!(positions.get_supply(2), 356);

        positions.remove_supply(&e, &reserve_1, 356);
        assert_eq!(positions.get_supply(2), 0);
        assert_eq!(positions.supply.len(), 1);
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(4))")]
    fn test_remove_supply_over_balance_panics() {
        let e = Env::default();
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let mut positions = Positions::env_default(&e, &samwise);
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve_0 =
            testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let mut positions = Positions::env_default(&e, &samwise);

        positions.add_supply(&e, &reserve_0, 123);
        assert_eq!(positions.get_supply(0), 123);

        positions.remove_supply(&e, &reserve_0, 124);
    }

    #[test]
    fn test_total_supply() {
        let e = Env::default();
        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);
        let pool = Address::random(&e);

        let mut positions = Positions::env_default(&e, &samwise);
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta(&e);
        let reserve_0 =
            testutils::create_reserve(&e, &pool, &underlying_0, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config, mut reserve_data) = testutils::default_reserve_meta(&e);
        reserve_config.index = 1;
        let reserve_1 =
            testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        let mut positions = Positions::env_default(&e, &samwise);

        positions.add_supply(&e, &reserve_0, 123);
        positions.add_supply(&e, &reserve_1, 456);
        positions.add_collateral(&e, &reserve_1, 789);
        assert_eq!(positions.get_total_supply(0), 123);
        assert_eq!(positions.get_total_supply(1), 456 + 789);
    }
}
