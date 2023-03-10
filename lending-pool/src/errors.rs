use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
//auction errors are begin at 100
pub enum PoolError {
    // Request Errors (0-9)
    NotAuthorized = 1,
    BadRequest = 2,
    // Pool State Errors (10-19)
    InvalidHf = 10,
    InvalidPoolStatus = 11,
    // Emission Errors (20-29)
    EmissionFailure = 20,
    // Auction Errors (100-199)
    InvalidLiquidation = 100,
    InvalidAsks = 106,
    InvalidBids = 102,
    AuctionInProgress = 103,
    InvalidAuctionType = 104,
}
