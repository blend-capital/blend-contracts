use soroban_sdk::{contracterror, contractimpl, contracttype, Address, BytesN, Env};

#[derive(Clone)]
#[contracttype]
pub enum MockBlendOracleDataKey {
    // The address that can manage the oracle
    Admin,
    // The number of decimals reported
    Decimals,
    // The map of asset price sources (asset contractId -> price source contractId)
    Sources(BytesN<32>),
    // MOCK: Map of prices to return
    Prices(BytesN<32>),
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
pub struct MockBlendOracle;

trait MockOracle {
    // NOTE: Copy and pasted from `Oracle` trait
    fn decimals(e: Env) -> u32;

    fn get_price(e: Env, asset: BytesN<32>) -> Result<u64, OracleError>;

    fn source(e: Env, asset: BytesN<32>) -> BytesN<32>;

    fn set_source(e: Env, asset: BytesN<32>, source: BytesN<32>);

    fn admin(e: Env) -> Address;

    fn set_admin(e: Env, admin: Address);

    fn is_admin(e: Env) -> bool;

    /// Sets the mocked price for an asset
    fn set_price(e: Env, asset: BytesN<32>, price: u64);

    /// Sets the oracle error status
    fn set_error(e: Env, to_error: bool);
}

#[contractimpl]
impl MockOracle for MockBlendOracle {
    fn decimals(_e: Env) -> u32 {
        7 as u32
    }

    fn get_price(e: Env, asset: BytesN<32>) -> Result<u64, OracleError> {
        let to_error = e
            .data()
            .get::<MockBlendOracleDataKey, bool>(MockBlendOracleDataKey::ToError)
            .unwrap_or_else(|| Ok(false))
            .unwrap();
        if to_error {
            return Err(OracleError::StaleOracle);
        }

        let key = MockBlendOracleDataKey::Prices(asset);
        Ok(e.data()
            .get::<MockBlendOracleDataKey, u64>(key)
            .unwrap_or_else(|| Ok(0))
            .unwrap())
    }

    // NOTE: Management functions omitted - not necessary for mock
    fn source(_e: Env, _asset: BytesN<32>) -> BytesN<32> {
        panic!("not implemented for mock")
    }

    fn set_source(_e: Env, _asset: BytesN<32>, _source: BytesN<32>) {
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

    fn set_price(e: Env, asset: BytesN<32>, price: u64) {
        let key = MockBlendOracleDataKey::Prices(asset);
        e.data().set::<MockBlendOracleDataKey, u64>(key, price);
    }

    fn set_error(e: Env, to_error: bool) {
        let key = MockBlendOracleDataKey::ToError;
        e.data().set::<MockBlendOracleDataKey, bool>(key, to_error);
    }
}
