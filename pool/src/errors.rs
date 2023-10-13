use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
//auction errors are begin at 100
pub enum PoolError {
    // Request Errors (0-9)
    NotAuthorized = 1,
    BadRequest = 2,
    AlreadyInitialized = 3,
    NegativeAmount = 4,
    InvalidPoolInitArgs = 5,
    InvalidReserveMetadata = 6,
    // Pool State Errors (10-19)
    InvalidHf = 10,
    InvalidPoolStatus = 11,
    InvalidUtilRate = 12,
    // Emission Errors (20-29)
    EmissionFailure = 20,
    // Oracle Errors (30-39)
    StalePrice = 30,
    // Auction Errors (100-199)
    InvalidLiquidation = 100,
    InvalidLot = 101,
    InvalidBids = 102,
    AuctionInProgress = 103,
    InvalidAuctionType = 104,
    InvalidLiqTooLarge = 105,
    InvalidLiqTooSmall = 106,
    InterestTooSmall = 107,
}
