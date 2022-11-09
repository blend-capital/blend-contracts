use soroban_sdk::{contracttype, BytesN};

/// The configuration information about a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveConfig {
    pub b_token: BytesN<32>, // the address of the bToken contract
    pub decimals: u32, // the decimals used in both the bToken and underlying contract
    pub c_factor: u32, // the collateral factor for the reserve
    pub l_factor: u32, // the liability factor for the reserve
    pub r_one: u32, // the R1 value in the interest rate formula
    pub r_two: u32, // the R2 value in the interest rate formula
    pub r_three: u32, // the R3 value in the interest rate formula
}

/// The data for a reserve asset
#[derive(Clone)]
#[contracttype]
pub struct ReserveData {
    pub rate: i64, // the conversion rate from bToken to underlying 
    pub ir_mod: i64 // the interest rate curve modifier
}

