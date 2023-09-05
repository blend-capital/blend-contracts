use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Vec};

use oracle::{PriceData, PriceFeedTrait};

pub(crate) const LEDGER_THRESHOLD_SHARED: u32 = 172800; // ~ 10 days
pub(crate) const LEDGER_BUMP_SHARED: u32 = 241920; // ~ 14 days

#[derive(Clone)]
#[contracttype]
pub enum MockOracleDataKey {
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

/// ### Mock Oracle
///
/// Contract to fetch mocked asset prices.
///
/// ### Dev
/// For testing purposes only!
#[contract]
pub struct MockOracle;

trait MockOraclePrice {
    /// Sets the mocked price for an asset.
    ///
    /// Will always return with the latest ledger as the timestamp.
    fn set_price(e: Env, asset: Address, price: i128);

    /// Sets the mocked price for an asset.
    ///
    /// Will return the given timestamp as the PriceData timestamp.
    fn set_price_timestamp(e: Env, asset: Address, price: i128, timestamp: u64);
}

#[contractimpl]
impl MockOraclePrice for MockOracle {
    fn set_price(e: Env, asset: Address, price: i128) {
        let key = MockOracleDataKey::Prices(asset);
        e.storage().temporary().set::<MockOracleDataKey, PriceData>(
            &key,
            &PriceData {
                price,
                timestamp: 0,
            },
        );
        e.storage()
            .temporary()
            .bump(&key, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
    }

    fn set_price_timestamp(e: Env, asset: Address, price: i128, timestamp: u64) {
        let key = MockOracleDataKey::Prices(asset);
        e.storage()
            .temporary()
            .set::<MockOracleDataKey, PriceData>(&key, &PriceData { price, timestamp });
        e.storage()
            .temporary()
            .bump(&key, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
    }
}

#[contractimpl]
impl PriceFeedTrait for MockOracle {
    fn base(_e: Env) -> Address {
        panic!("not impl")
    }

    fn assets(_e: Env) -> Vec<Address> {
        panic!("not impl")
    }

    fn decimals(_e: Env) -> u32 {
        7_u32
    }

    fn resolution(_e: Env) -> u32 {
        panic!("not impl")
    }

    fn price(_e: Env, _asset: Address, _timestamp: u64) -> Option<PriceData> {
        panic!("not impl")
    }

    fn prices(_e: Env, _asset: Address, _records: u32) -> Option<Vec<PriceData>> {
        panic!("not impl")
    }

    fn lastprice(e: Env, asset: Address) -> Option<PriceData> {
        e.storage()
            .instance()
            .bump(LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
        let key = MockOracleDataKey::Prices(asset);
        let mut price = e
            .storage()
            .temporary()
            .get::<MockOracleDataKey, PriceData>(&key)
            .unwrap_or(PriceData {
                price: 0,
                timestamp: 1,
            });
        if price.timestamp == 0 {
            price.timestamp = e.ledger().timestamp();
        }
        e.storage()
            .temporary()
            .bump(&key, LEDGER_THRESHOLD_SHARED, LEDGER_BUMP_SHARED);
        Some(price)
    }
}
