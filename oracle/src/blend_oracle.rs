use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

#[derive(Clone)]
#[contracttype]
pub enum BlendOracleDataKey {
    // The address that can manage the oracle
    Admin,
    // The number of decimals reported
    Decimals,
    // The map of asset price sources (asset contractId -> price source contractId)
    Sources(Address),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OracleError {
    StaleOracle = 1,
}

/// ### Blend Oracle
///
/// Contract to fetch asset prices, manage price sources, and ensure oracle liveness.
///
/// ### Dev
/// Left private to avoid misuse in tests. Use the MockBlendOracle for testing purposes.
#[contract]
struct BlendOracle;

pub trait BlendOracleTrait {
    /// The number of decimal places used
    fn decimals(e: Env) -> u32;

    /// Fetch the price of an asset with `decimals` precision
    ///
    /// ### Arguments
    /// * `asset` - The address of the asset's contract
    ///
    /// ### Panics
    /// If the oracle is stale
    fn get_price(e: Env, asset: Address) -> u64;

    /// Fetch the price source of an asset or the 0 address if none is listed
    ///
    /// ### Arguments
    /// * `asset` - The address of the asset's contract
    fn source(e: Env, asset: Address) -> Address;

    /// Set the price source for an asset
    ///
    /// ### Arguments
    /// * `asset` - The address of the asset's contract
    /// * `source` - The address of the price source contract
    ///
    /// ### Panics
    /// If the `admin` is not the invoker
    ///
    /// ### Notes
    /// It is expected that `source`
    fn set_source(e: Env, asset: Address, source: Address);

    /// The admin of the contract
    fn admin(e: Env) -> Address;

    /// Set the admin for the contract
    ///
    /// ### Arguments
    /// * `admin` - The address of the admin
    fn set_admin(e: Env, admin: Address);

    /// Determines if the current invoker is the admin
    ///
    /// ### Notes
    /// The caller is expected to handle any negative case
    fn is_admin(e: Env) -> bool;
}

#[contractimpl]
impl BlendOracleTrait for BlendOracle {
    fn decimals(_e: Env) -> u32 {
        7 as u32
    }

    fn get_price(_e: Env, _asset: Address) -> u64 {
        panic!("not implemented")
    }

    fn source(_e: Env, _asset: Address) -> Address {
        panic!("not implemented")
    }

    fn set_source(_e: Env, _asset: Address, _source: Address) {
        panic!("not implemented")
    }

    fn admin(_e: Env) -> Address {
        panic!("not implemented")
    }

    fn set_admin(_e: Env, _admin: Address) {
        panic!("not implemented")
    }

    fn is_admin(_e: Env) -> bool {
        panic!("not implemented")
    }
}
