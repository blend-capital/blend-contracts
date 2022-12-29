use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PoolError {
    NotAuthorized = 1,
    BadRequest = 2,
    InvalidHf = 3,
    InvalidPoolStatus = 4,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AuctionError {
    InvalidBids = 1,
    InvalidAsks = 2,
    AuctionInProgress = 3,
    InvalidPoolStatus = 4,
    InvalidAuctionType = 5,
}
