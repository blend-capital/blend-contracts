use crate::storage;
use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct MockLendingPool;

pub trait MockLendingPoolTrait {
    /// Fetch the reserve usage configuration for a user
    ///
    /// ### Arguments
    /// * `user` - The Address to fetch the reserve usage for
    fn config(e: Env, user: Address) -> i128;

    /// Mock Only: Set the collateral usage for a user
    ///
    /// ### Arguments
    /// * `user` - The Address to set the reserve usage configuration for
    /// * `index` - The index of the reserve to update
    /// * `is_collat` - True if its used as collateral, false if not (default is true)
    fn set_collat(e: Env, user: Address, index: u32, is_collat: bool);
}

#[contractimpl]
impl MockLendingPoolTrait for MockLendingPool {
    fn config(e: Env, user: Address) -> i128 {
        storage::read_config(&e, &user)
    }

    fn set_collat(e: Env, user: Address, index: u32, is_collat: bool) {
        let mut config = storage::read_config(&e, &user);
        let res_collateral_bit = 1 << (index * 3 + 2);
        if !is_collat {
            // set bit to 1
            config = config | res_collateral_bit;
        } else {
            // set bit to zero
            config = config & !res_collateral_bit;
        }
        storage::write_config(&e, &user, &config);
    }
}
