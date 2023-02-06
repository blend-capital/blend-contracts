use crate::{
    errors::PoolError,
    storage::{PoolDataStore, StorageManager},
};
use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_auth::{Identifier};
use soroban_sdk::{contracttype, BytesN, Env, Vec};

use super::{
    accrued_interest_auction::{
        calc_fill_interest_auction, fill_interest_auction, verify_create_interest_auction,
    },
    backstop_liquidation_auction::{
        calc_fill_backstop_liq_auction, fill_backstop_liq_auction,
        verify_create_backstop_liq_auction,
    },
    user_liquidation_auction::{
        calc_fill_user_liq_auction, fill_user_liq_auction, verify_create_user_liq_auction,
    },
};

#[derive(Clone, PartialEq)]
#[repr(u32)]
pub enum AuctionType {
    UserLiquidation = 0,
    BackstopLiquidation = 1,
    InterestAuction = 2,
}

impl AuctionType {
    fn from_u32(value: u32) -> Self {
        match value {
            0 => AuctionType::UserLiquidation,
            1 => AuctionType::BackstopLiquidation,
            2 => AuctionType::InterestAuction,
            _ => panic!("internal error"),
        }
    }
}

#[derive(Clone)]
#[contracttype]
pub struct AuctionQuote {
    send: Vec<(BytesN<32>, i128)>,
    receive: Vec<(BytesN<32>, i128)>,
}

// TODO: Rename symbol to Auction once auction functionality is fully ported
/// ### Auction
///
/// Conducts an auction of "user's" assets for a set of asset's delivered on fill. The asset's
/// and amounts involved in the auction depend on the user, the auction type, and the number
/// of blocks that have passed since the starting block.
pub struct AuctionV2 {
    pub auction_type: AuctionType, // the type of auction
    pub user: Identifier,          // the user whose assets are involved in the auction
    pub block: u32,                // the start block of the auction
}

impl AuctionV2 {
    /// Create an auction. Stores the resulting auction to the ledger to begin on the next block
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction being created
    /// * `user` - The user whose assets are involved in the auction
    pub fn create(e: &Env, auction_type: u32, user: Identifier) -> Result<Self, PoolError> {
        let storage = StorageManager::new(e);

        let auct_type = AuctionType::from_u32(auction_type);
        let start_block = e.ledger().sequence() + 1;
        let auction = AuctionV2 {
            auction_type: auct_type.clone(),
            user: user.clone(),
            block: e.ledger().sequence() + 1,
        };
        match auct_type {
            AuctionType::UserLiquidation => verify_create_user_liq_auction(e, &auction)?,
            AuctionType::BackstopLiquidation => verify_create_backstop_liq_auction(e, &auction)?,
            AuctionType::InterestAuction => verify_create_interest_auction(e, &auction)?,
        };

        storage.set_auction(auction_type, user, start_block);

        return Ok(auction);
    }

    /// Load an auction from the ledger.
    ///
    /// ### Arguments
    /// * `auction_type` - The type of auction being created
    /// * `user` - The user whose assets are involved in the auction
    ///
    /// ### Errors
    /// If the auction does not exist
    pub fn load(e: &Env, auction_type: u32, user: Identifier) -> AuctionV2 {
        let start_block = StorageManager::new(&e).get_auction(auction_type, user.clone());
        let auct_type = AuctionType::from_u32(auction_type);
        AuctionV2 {
            auction_type: auct_type,
            user,
            block: start_block,
        }
    }

    /// Preview the quote the auction will be filled at
    ///
    /// ### Arguments
    /// * `block` - The block to get a quote at
    ///
    /// ### Errors
    /// If the auction does not exist
    pub fn preview_fill(&self, e: &Env, block: u32) -> AuctionQuote {
        match self.auction_type {
            AuctionType::UserLiquidation => calc_fill_user_liq_auction(e, &self, block),
            AuctionType::BackstopLiquidation => calc_fill_backstop_liq_auction(e, &self, block),
            AuctionType::InterestAuction => calc_fill_interest_auction(e, &self, block),
        }
    }

    /// Fills the auction from the invoker. The filler is expected to maintain allowances to both
    /// the pool and the backstop module.
    ///
    /// TODO: Use auth-next to avoid required allowances
    ///
    /// ### Arguments
    /// * `filler` - The identifier filling the auction
    ///
    /// ### Errors
    /// If the auction does not exist, or if the pool is unable to fulfill either side
    /// of the auction quote
    pub fn fill(&self, e: &Env, filler: Identifier) -> Result<AuctionQuote, PoolError> {
        let quote = match self.auction_type {
            AuctionType::UserLiquidation => fill_user_liq_auction(e, &self, filler),
            AuctionType::BackstopLiquidation => fill_backstop_liq_auction(e, &self, filler),
            AuctionType::InterestAuction => fill_interest_auction(e, &self, filler),
        };
        Ok(quote)
    }

    /// Get the current fill modifiers for the auction
    /// 
    /// Returns a tuple of i128's => (send to modifier, receive from modifier) scaled
    /// to 7 decimal places
    pub fn get_fill_modifiers(&self, e: &Env) -> (i128, i128) {
        let block_dif = i128(e.ledger().sequence() - self.block) * 1_0000000;
        let send_to_mod: i128;
        let receive_from_mod: i128;
        // increment the modifier 0.5% every block
        let per_block_scalar: i128 = 0_0050000;
        if block_dif > 400_0000000 {
            send_to_mod = 0;
            receive_from_mod = 1_0000000;
        } else if block_dif > 200_0000000 {
            send_to_mod = 2_0000000 - block_dif.fixed_mul_floor(per_block_scalar, 1_0000000).unwrap();
            receive_from_mod = 1_0000000;
        } else {
            send_to_mod = 1_000_0000;
            receive_from_mod = block_dif.fixed_mul_floor(per_block_scalar, 1_0000000).unwrap();
        };
        (send_to_mod, receive_from_mod)
    }
}

#[cfg(test)]
mod tests {
    use crate::testutils::generate_contract_id;

    use super::*;
    use soroban_sdk::testutils::{LedgerInfo, Ledger};

    #[test]
    fn test_get_fill_modifiers() {
        let e = Env::default();

        let auction = AuctionV2 {
            auction_type: AuctionType::UserLiquidation,
            user: Identifier::Contract(generate_contract_id(&e)),
            block: 1000,
        };

        let mut send_to_modifier: i128;
        let mut receive_from_modifier: i128;

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        (send_to_modifier, receive_from_modifier) = auction.get_fill_modifiers(&e);
        assert_eq!(send_to_modifier, 1_0000000);
        assert_eq!(receive_from_modifier, 0);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1100,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        (send_to_modifier, receive_from_modifier) = auction.get_fill_modifiers(&e);
        assert_eq!(send_to_modifier, 1_0000000);
        assert_eq!(receive_from_modifier, 0_5000000);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1200,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        (send_to_modifier, receive_from_modifier) = auction.get_fill_modifiers(&e);
        assert_eq!(send_to_modifier, 1_0000000);
        assert_eq!(receive_from_modifier, 1_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1201,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        (send_to_modifier, receive_from_modifier) = auction.get_fill_modifiers(&e);
        assert_eq!(send_to_modifier, 0_9950000);
        assert_eq!(receive_from_modifier, 1_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1300,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        (send_to_modifier, receive_from_modifier) = auction.get_fill_modifiers(&e);
        assert_eq!(send_to_modifier, 0_5000000);
        assert_eq!(receive_from_modifier, 1_0000000);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 1400,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });
        (send_to_modifier, receive_from_modifier) = auction.get_fill_modifiers(&e);
        assert_eq!(send_to_modifier, 0);
        assert_eq!(receive_from_modifier, 1_0000000);
    }
}