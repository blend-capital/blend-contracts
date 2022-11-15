use soroban_sdk::{contracttype, BytesN};

/// The configuration information about a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveConfig {
    pub b_token: BytesN<32>, // the address of the bToken contract
    pub d_token: BytesN<32>, // the address of the dToken contract
    pub decimals: u32, // the decimals used in both the bToken and underlying contract
    pub c_factor: u32, // the collateral factor for the reserve
    pub l_factor: u32, // the liability factor for the reserve
    pub util: u32, // the target utilization rate
    pub r_one: u32, // the R1 value in the interest rate formula
    pub r_two: u32, // the R2 value in the interest rate formula
    pub r_three: u32, // the R3 value in the interest rate formula
    pub index: u32, // the index of the reserve in the list (TODO: Make u8)
}

/// The data for a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveData {
    // TODO: These rates are correlated and can be simplified if both the b/dTokens have a totalSupply
    pub rate: i64, // the conversion rate from bToken to underlying 
    pub d_rate: i64, // the conversion rate from dToken to underlying
    pub ir_mod: i64 // the interest rate curve modifier
}

