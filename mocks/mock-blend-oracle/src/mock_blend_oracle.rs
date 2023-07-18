use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

pub(crate) const INSTANCE_BUMP_AMOUNT: u32 = 34560; // 2 days

#[derive(Clone)]
#[contracttype]
pub enum MockBlendOracleDataKey {
    // The address that can manage the oracle
    Admin,
    // The number of decimals reported
    Decimals,
    // The map of asset price sources (asset contractId -> price source contractId)
    Sources(Address),
    // MOCK: Map of prices to return
    Prices(Address),
    // MOCK: If the oracle should fail
    ToError,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OracleError {
    StaleOracle = 1,
}

/// ### Mock Blend Oracle
///
/// Contract to fetch mocked asset prices.
///
/// ### Dev
/// For testing purposes only!
#[contract]
pub struct MockBlendOracle;

trait MockOracle {
    // NOTE: Copy and pasted from `Oracle` trait
    fn decimals(e: Env) -> u32;

    fn get_price(e: Env, asset: Address) -> Result<u64, OracleError>;

    fn source(e: Env, asset: Address) -> Address;

    fn set_source(e: Env, asset: Address, source: Address);

    fn admin(e: Env) -> Address;

    fn set_admin(e: Env, admin: Address);

    fn is_admin(e: Env) -> bool;

    /// Sets the mocked price for an asset
    fn set_price(e: Env, asset: Address, price: u64);

    /// Sets the oracle error status
    fn set_error(e: Env, to_error: bool);
}

#[contractimpl]
impl MockOracle for MockBlendOracle {
    fn decimals(_e: Env) -> u32 {
        7 as u32
    }

    fn get_price(e: Env, asset: Address) -> Result<u64, OracleError> {
        e.storage().instance().bump(INSTANCE_BUMP_AMOUNT);
        let to_error = e
            .storage()
            .temporary()
            .get::<MockBlendOracleDataKey, bool>(&MockBlendOracleDataKey::ToError)
            .unwrap_or(false);
        if to_error {
            return Err(OracleError::StaleOracle);
        }

        let key = MockBlendOracleDataKey::Prices(asset);
        let price = e
            .storage()
            .temporary()
            .get::<MockBlendOracleDataKey, u64>(&key)
            .unwrap_or(0);
        e.storage().temporary().bump(&key, INSTANCE_BUMP_AMOUNT);
        Ok(price)
    }

    // NOTE: Management functions omitted - not necessary for mock
    fn source(_e: Env, _asset: Address) -> Address {
        panic!("not implemented for mock")
    }

    fn set_source(_e: Env, _asset: Address, _source: Address) {
        panic!("not implemented for mock")
    }

    fn admin(_e: Env) -> Address {
        panic!("not implemented for mock")
    }

    fn set_admin(_e: Env, _admin: Address) {
        panic!("not implemented for mock")
    }

    fn is_admin(_e: Env) -> bool {
        panic!("not implemented for mock")
    }

    fn set_price(e: Env, asset: Address, price: u64) {
        let key = MockBlendOracleDataKey::Prices(asset);
        e.storage()
            .temporary()
            .set::<MockBlendOracleDataKey, u64>(&key, &price);
    }

    fn set_error(e: Env, to_error: bool) {
        let key = MockBlendOracleDataKey::ToError;
        e.storage()
            .temporary()
            .set::<MockBlendOracleDataKey, bool>(&key, &to_error);
        e.storage().temporary().bump(&key, INSTANCE_BUMP_AMOUNT);
    }
}
