use soroban_sdk::{contracttype, Env, Map};

use crate::validator::require_nonnegative;

/// A user / contracts position's with the pool, stored in the Reserve's decimals
#[derive(Clone)]
#[contracttype]
pub struct Positions {
    pub liabilities: Map<u32, i128>, // Map of Reserve Index to liability share balance
    pub collateral: Map<u32, i128>,  // Map of Reserve Index to collateral supply share balance
    pub supply: Map<u32, i128>,      // Map of Reserve Index to non-collateral supply share balance
}

impl Positions {
    /// Create an empty Positions object in the environment
    pub fn env_default(e: &Env) -> Self {
        Positions {
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
    pub fn add_liabilities(&mut self, reserve_index: u32, amount: i128) {
        let new_amount = self.liabilities.get(reserve_index).unwrap_or(0) + amount;
        self.liabilities.set(reserve_index, new_amount);
    }

    /// Remove liabilities from the position expressed in debtTokens
    pub fn remove_liabilities(&mut self, e: &Env, reserve_index: u32, amount: i128) {
        let new_amount = self.liabilities.get(reserve_index).unwrap_or(0) - amount;
        require_nonnegative(e, &new_amount);
        if new_amount == 0 {
            self.liabilities.remove(reserve_index);
        } else {
            self.liabilities.set(reserve_index, new_amount);
        }
    }

    /// Get the collateralized blendToken position for the reserve at the given index
    pub fn get_collateral(&self, reserve_index: u32) -> i128 {
        self.collateral.get(reserve_index).unwrap_or(0)
    }

    /// Add collateral to the position expressed in blendTokens
    pub fn add_collateral(&mut self, reserve_index: u32, amount: i128) {
        let new_amount = self.collateral.get(reserve_index).unwrap_or(0) + amount;
        self.collateral.set(reserve_index, new_amount);
    }

    /// Remove collateral from the position expressed in blendTokens
    pub fn remove_collateral(&mut self, e: &Env, reserve_index: u32, amount: i128) {
        let new_amount = self.collateral.get(reserve_index).unwrap_or(0) - amount;
        require_nonnegative(e, &new_amount);
        if new_amount == 0 {
            self.collateral.remove(reserve_index);
        } else {
            self.collateral.set(reserve_index, new_amount);
        }
    }

    /// Get the uncollateralized blendToken position for the reserve at the given index
    pub fn get_supply(&self, reserve_index: u32) -> i128 {
        self.supply.get(reserve_index).unwrap_or(0)
    }

    /// Add supply to the position expressed in blendTokens
    pub fn add_supply(&mut self, reserve_index: u32, amount: i128) {
        let new_amount = self.supply.get(reserve_index).unwrap_or(0) + amount;
        self.supply.set(reserve_index, new_amount);
    }

    /// Remove supply from the position expressed in blendTokens
    pub fn remove_supply(&mut self, e: &Env, reserve_index: u32, amount: i128) {
        let new_amount = self.supply.get(reserve_index).unwrap_or(0) - amount;
        require_nonnegative(e, &new_amount);
        if new_amount == 0 {
            self.supply.remove(reserve_index);
        } else {
            self.supply.set(reserve_index, new_amount);
        }
    }

    /// Get the total supply and collateral of blendTokens for the user at the given index
    pub fn get_total_supply(&self, reserve_index: u32) -> i128 {
        self.get_collateral(reserve_index) + self.get_supply(reserve_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_liabilities() {
        let e = Env::default();

        let mut positions = Positions::env_default(&e);

        assert_eq!(positions.get_liabilities(0), 0);

        positions.add_liabilities(0, 123);
        assert_eq!(positions.get_liabilities(0), 123);

        positions.add_liabilities(2, 456);
        assert_eq!(positions.get_liabilities(0), 123);
        assert_eq!(positions.get_liabilities(2), 456);

        positions.remove_liabilities(&e, 2, 100);
        assert_eq!(positions.get_liabilities(2), 356);

        positions.remove_liabilities(&e, 2, 356);
        assert_eq!(positions.get_liabilities(2), 0);
        assert_eq!(positions.liabilities.len(), 1);
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(4))")]
    fn test_remove_liabilities_over_balance_panics() {
        let e = Env::default();

        let mut positions = Positions::env_default(&e);

        positions.add_liabilities(1, 123);
        assert_eq!(positions.get_liabilities(1), 123);

        positions.remove_liabilities(&e, 2, 124);
    }

    #[test]
    fn test_collateral() {
        let e = Env::default();

        let mut positions = Positions::env_default(&e);

        assert_eq!(positions.get_collateral(0), 0);

        positions.add_collateral(0, 123);
        assert_eq!(positions.get_collateral(0), 123);

        positions.add_collateral(2, 456);
        assert_eq!(positions.get_collateral(0), 123);
        assert_eq!(positions.get_collateral(2), 456);

        positions.remove_collateral(&e, 2, 100);
        assert_eq!(positions.get_collateral(2), 356);

        positions.remove_collateral(&e, 2, 356);
        assert_eq!(positions.get_collateral(2), 0);
        assert_eq!(positions.collateral.len(), 1);
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(4))")]
    fn test_remove_collateral_over_balance_panics() {
        let e = Env::default();

        let mut positions = Positions::env_default(&e);

        positions.add_collateral(1, 123);
        assert_eq!(positions.get_collateral(1), 123);

        positions.remove_collateral(&e, 2, 124);
    }

    #[test]
    fn test_supply() {
        let e = Env::default();

        let mut positions = Positions::env_default(&e);

        assert_eq!(positions.get_supply(0), 0);

        positions.add_supply(0, 123);
        assert_eq!(positions.get_supply(0), 123);

        positions.add_supply(2, 456);
        assert_eq!(positions.get_supply(0), 123);
        assert_eq!(positions.get_supply(2), 456);

        positions.remove_supply(&e, 2, 100);
        assert_eq!(positions.get_supply(2), 356);

        positions.remove_supply(&e, 2, 356);
        assert_eq!(positions.get_supply(2), 0);
        assert_eq!(positions.supply.len(), 1);
    }

    #[test]
    #[should_panic]
    //#[should_panic(expected = "Status(ContractError(4))")]
    fn test_remove_supply_over_balance_panics() {
        let e = Env::default();

        let mut positions = Positions::env_default(&e);

        positions.add_supply(1, 123);
        assert_eq!(positions.get_supply(1), 123);

        positions.remove_supply(&e, 2, 124);
    }

    #[test]
    fn test_total_supply() {
        let e = Env::default();

        let mut positions = Positions::env_default(&e);

        positions.add_supply(0, 123);
        positions.add_supply(1, 456);
        positions.add_collateral(1, 789);
        assert_eq!(positions.get_total_supply(0), 123);
        assert_eq!(positions.get_total_supply(1), 456 + 789);
    }
}
