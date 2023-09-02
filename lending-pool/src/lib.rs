#![no_std]
#[cfg(any(test, feature = "testutils"))]
extern crate std;

#[cfg(any(test, feature = "testutils"))]
pub use pool::{Pool as PoolState, PositionData, Reserve};

mod auctions;
mod constants;
mod contract;
mod dependencies;
mod emissions;
mod errors;
mod pool;
mod storage;
mod testutils;
mod validator;

pub use auctions::{AuctionData, AuctionType};
pub use contract::*;
pub use emissions::ReserveEmissionMetadata;
pub use errors::PoolError;
pub use pool::{Positions, Request};
pub use storage::{
    AuctionKey, PoolConfig, PoolDataKey, PoolEmissionConfig, ReserveConfig, ReserveData,
    ReserveEmissionsConfig, ReserveEmissionsData, UserEmissionData, UserReserveKey,
};

trait UnwrapOrOverflow {
    type Output;

    fn unwrap_or_overflow(self, env: &soroban_sdk::Env) -> Self::Output;
}

impl<T> UnwrapOrOverflow for Option<T> {
    type Output = T;

    #[inline(always)]
    fn unwrap_or_overflow(self, env: &soroban_sdk::Env) -> Self::Output {
        #[soroban_sdk::contracterror]
        #[derive(Copy, Clone)]
        #[repr(u32)]
        enum Error {
            Overflow = 0xFE,
        }

        match self {
            Some(v) => v,
            None => {
                soroban_sdk::panic_with_error!(env, Error::Overflow);
            }
        }
    }
}
