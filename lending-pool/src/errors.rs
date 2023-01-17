use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
//auction errors are begin at 100
pub enum PoolError {
    NotAuthorized = 1,
    BadRequest = 2,
    InvalidHf = 3,
    InvalidPoolStatus = 4,
    InvalidAsks = 101,
    InvalidBids = 102,
    AlreadyInProgress = 103,
    InvalidAuctionType = 104,
}
