use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
/// Error codes for the pool contract. Common errors are codes that match up with the built-in
/// contracts error reporting. Pool specific errors start at 1200.
pub enum PoolError {
    // Common Errors
    InternalError = 1,
    AlreadyInitializedError = 3,

    UnauthorizedError = 4,

    NegativeAmountError = 8,
    BalanceError = 10,
    OverflowError = 12,

    // Pool Request Errors (start at 1200)
    BadRequest = 1200,
    InvalidPoolInitArgs = 1201,
    InvalidReserveMetadata = 1202,
    InitNotUnlocked = 1203,
    StatusNotAllowed = 1204,

    // Pool State Errors
    InvalidHf = 1205,
    InvalidPoolStatus = 1206,
    InvalidUtilRate = 1207,
    MaxPositionsExceeded = 1208,
    InternalReserveNotFound = 1209,

    // Oracle Errors
    StalePrice = 1210,

    // Auction Errors
    InvalidLiquidation = 1211,
    AuctionInProgress = 1212,
    InvalidLiqTooLarge = 1213,
    InvalidLiqTooSmall = 1214,
    InterestTooSmall = 1215,

    // Share Token Errors
    InvalidBTokenMintAmount = 1216,
    InvalidBTokenBurnAmount = 1217,
    InvalidDTokenMintAmount = 1218,
    InvalidDTokenBurnAmount = 1219,
}
