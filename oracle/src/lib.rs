//! Interface for SEP-40 Oracle Price Feed
//! https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0040.md

#![no_std]

use soroban_sdk::{contractclient, contracttype, Address, Env, Vec};

/// Price data for an asset at a specific timestamp
#[contracttype]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

/// Oracle feed interface description
#[contractclient(name = "OracleClient")]
pub trait PriceFeedTrait {
    /// Return the base asset the price is reported in
    fn base(env: Env) -> Address;

    /// Return all assets quoted by the price feed
    fn assets(env: Env) -> Vec<Address>;

    /// Return the number of decimals for all assets quoted by the oracle
    fn decimals(env: Env) -> u32;

    /// Return default tick period timeframe (in seconds)
    fn resolution(env: Env) -> u32;

    /// Get price in base asset at specific timestamp
    fn price(env: Env, asset: Address, timestamp: u64) -> Option<PriceData>;

    /// Get last N price records
    fn prices(env: Env, asset: Address, records: u32) -> Option<Vec<PriceData>>;

    /// Get the most recent price for an asset
    fn lastprice(env: Env, asset: Address) -> Option<PriceData>;
}
