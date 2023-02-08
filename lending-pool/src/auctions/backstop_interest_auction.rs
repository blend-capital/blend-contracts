use crate::{
    constants::{BLND_TOKEN, SCALAR_7},
    dependencies::{BackstopClient, OracleClient, TokenClient},
    errors::PoolError,
    reserve::Reserve,
    storage::{PoolDataStore, StorageManager},
};
use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{vec, BytesN, Env};

use super::auction_v2::{AuctionDataV2, AuctionQuote, AuctionType, AuctionV2};

pub fn create_interest_auction(e: &Env) -> Result<AuctionV2, PoolError> {
    let storage = StorageManager::new(e);
    let bkstp_id = Identifier::Contract(storage.get_backstop());

    if storage.has_auction(AuctionType::InterestAuction as u32, bkstp_id.clone()) {
        return Err(PoolError::AlreadyInProgress);
    }

    // TODO: Determine if any threshold should be required to create interest auction
    //       It is currently guaranteed that if no auction is active, some interest
    //       will be generated.

    let pool_config = storage.get_pool_config();
    let oracle_client = OracleClient::new(e, pool_config.oracle.clone());

    let mut auction_data = AuctionDataV2 {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence() + 1,
    };

    let reserve_count = storage.get_res_list();
    let mut interest_value = 0;
    for i in 0..reserve_count.len() {
        let res_asset_address = reserve_count.get_unchecked(i).unwrap();

        let mut reserve = Reserve::load(&e, res_asset_address.clone());
        let to_mint_bkstp = reserve.update_rates(e, pool_config.bstop_rate);

        let b_token_client = TokenClient::new(e, reserve.config.b_token.clone());
        let b_token_balance = b_token_client.balance(&bkstp_id) + to_mint_bkstp;
        if b_token_balance > 0 {
            let asset_to_base = oracle_client.get_price(&res_asset_address);
            let asset_balance = reserve.to_asset_from_b_token(b_token_balance);
            interest_value += asset_balance
                .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                .unwrap();
            auction_data
                .lot
                .push_back((reserve.config.index, b_token_balance));
        }
    }

    if auction_data.lot.len() == 0 || interest_value == 0 {
        return Err(PoolError::BadRequest);
    }

    let blnd_token = BytesN::from_array(e, &BLND_TOKEN);
    let blnd_to_base = oracle_client.get_price(&blnd_token);
    let bid_amount = interest_value
        .fixed_mul_floor(1_4000000, SCALAR_7)
        .unwrap()
        .fixed_div_floor(i128(blnd_to_base), SCALAR_7)
        .unwrap();
    // u32::MAX is the key for the backstop token
    auction_data.bid.push_back((u32::MAX, bid_amount));

    Ok(AuctionV2 {
        auction_type: AuctionType::InterestAuction,
        user: bkstp_id,
        data: auction_data,
    })
}

/// NOTE: This function is for viewing purposes only and should not be called by functions
///       that modify state
pub fn calc_fill_interest_auction(e: &Env, auction: &AuctionV2) -> AuctionQuote {
    let storage = StorageManager::new(e);

    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };

    let (bid_modifier, lot_modifier) = auction.get_fill_modifiers(e);

    // bid only contains the backstop token
    let (_, bid_amount) = auction.data.bid.first().unwrap().unwrap();
    let bid_amount_modified = bid_amount.fixed_mul_floor(bid_modifier, SCALAR_7).unwrap();
    auction_quote
        .bid
        .push_back((BytesN::from_array(e, &BLND_TOKEN), bid_amount_modified));

    // lot only contains b_token reserves
    let reserve_list = storage.get_res_list();
    for (res_id, amount) in auction.data.lot.iter_unchecked() {
        let res_asset_address = reserve_list.get_unchecked(res_id).unwrap();
        let reserve_config = storage.get_res_config(res_asset_address);
        let amount_modified = amount.fixed_mul_floor(lot_modifier, SCALAR_7).unwrap();
        auction_quote
            .lot
            .push_back((reserve_config.b_token, amount_modified));
    }

    auction_quote
}

pub fn fill_interest_auction(e: &Env, auction: &AuctionV2, filler: Identifier) -> AuctionQuote {
    // TODO: Determine if there is a way to reuse calc code. Currently, this would result in reloads of all
    //       reserves and the minting of tokens to the backstop during previews.
    let storage = StorageManager::new(e);
    let pool_config = storage.get_pool_config();
    let bkstp = storage.get_backstop();
    let bkstp_id = Identifier::Contract(bkstp.clone());

    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };

    let (bid_modifier, lot_modifier) = auction.get_fill_modifiers(e);

    // bid only contains the backstop token
    let blnd_token = BytesN::from_array(e, &BLND_TOKEN);
    let (_, bid_amount) = auction.data.bid.first().unwrap().unwrap();
    let bid_amount_modified = bid_amount.fixed_mul_floor(bid_modifier, SCALAR_7).unwrap();
    auction_quote
        .bid
        .push_back((blnd_token.clone(), bid_amount_modified));

    // TODO: Make more seamless with "auth-next" by pre-authorizing the transfer taking place
    //       in the backstop client to avoid a double transfer.
    let backstop_client = BackstopClient::new(&e, &bkstp);
    TokenClient::new(e, &blnd_token).xfer_from(
        &Signature::Invoker,
        &0,
        &filler,
        &Identifier::Contract(e.current_contract()),
        &bid_amount_modified,
    );
    backstop_client.donate(&e.current_contract(), &(bid_amount_modified as u64));

    // lot only contains b_token reserves
    let reserve_list = storage.get_res_list();
    for (res_id, lot_amount) in auction.data.lot.iter_unchecked() {
        let res_asset_address = reserve_list.get_unchecked(res_id).unwrap();
        let mut reserve = Reserve::load(&e, res_asset_address.clone());
        reserve
            .pre_action(e, &pool_config, 1, bkstp_id.clone())
            .unwrap();
        let lot_amount_modified = lot_amount.fixed_mul_floor(lot_modifier, SCALAR_7).unwrap();
        auction_quote
            .lot
            .push_back((reserve.config.b_token.clone(), lot_amount_modified));

        // TODO: Privileged xfer
        let b_token_client = TokenClient::new(e, reserve.config.b_token.clone());
        b_token_client.clawback(&Signature::Invoker, &0, &bkstp_id, &lot_amount_modified);
        b_token_client.mint(&Signature::Invoker, &0, &filler, &lot_amount_modified);
    }
    auction_quote
}

#[cfg(test)]
mod tests {
    use crate::{
        auctions::auction_v2::AuctionType,
        storage::{PoolConfig, PoolDataStore, StorageManager},
        testutils::{
            create_backstop, create_mock_oracle, create_mock_pool_factory, create_reserve,
            create_token_from_id, generate_contract_id, setup_reserve,
        },
    };

    use super::*;
    use soroban_auth::Signature;
    use soroban_sdk::{
        testutils::{Accounts, Ledger, LedgerInfo},
        BytesN,
    };

    #[test]
    fn test_create_interest_auction_already_in_progress() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);
        let backstop = generate_contract_id(&e);
        let backstop_id = Identifier::Contract(backstop.clone());

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let auction_data = AuctionDataV2 {
            bid: vec![&e],
            lot: vec![&e],
            block: 50,
        };
        e.as_contract(&pool_id, || {
            storage.set_backstop(backstop.clone());
            storage.set_auction(
                AuctionType::InterestAuction as u32,
                backstop_id.clone(),
                auction_data,
            );

            let result = create_interest_auction(&e);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::AlreadyInProgress),
            };
        });
    }

    #[test]
    fn test_create_interest_auction() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let blnd_id = BytesN::from_array(&e, &BLND_TOKEN);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let pool = generate_contract_id(&e);
        let (backstop, _backstop_client) = create_backstop(&e);
        let backstop_id = Identifier::Contract(backstop.clone());
        let mock_pool_factory = create_mock_pool_factory(&e);
        mock_pool_factory.set_pool(&pool);
        let (oracle, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.b_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.b_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_2);
        e.budget().reset();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &100_0000000);
        oracle_client.set_price(&blnd_id, &0_5000000);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage.set_pool_config(pool_config);
            storage.set_backstop(backstop.clone());

            b_token_0.mint(&Signature::Invoker, &0, &backstop_id, &10_0000000);
            b_token_1.mint(&Signature::Invoker, &0, &backstop_id, &2_5000000);

            e.budget().reset();
            let result = create_interest_auction(&e).unwrap();

            assert_eq!(
                result.auction_type as u32,
                AuctionType::InterestAuction as u32
            );
            assert_eq!(result.user, backstop_id);
            assert_eq!(result.data.block, 51);
            assert_eq!(
                result.data.bid.get_unchecked(0).unwrap(),
                (u32::MAX, 95_2000000)
            );
            assert_eq!(result.data.bid.len(), 1);
            assert_eq!(
                result.data.lot.get_unchecked(0).unwrap(),
                (reserve_0.config.index, 10_0000000)
            );
            assert_eq!(
                result.data.lot.get_unchecked(1).unwrap(),
                (reserve_1.config.index, 2_5000000)
            );
            assert_eq!(result.data.lot.len(), 2);
        });
    }

    #[test]
    fn test_create_interest_auction_applies_interest() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let blnd_id = BytesN::from_array(&e, &BLND_TOKEN);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 150,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let pool = generate_contract_id(&e);
        let (backstop, _backstop_client) = create_backstop(&e);
        let backstop_id = Identifier::Contract(backstop.clone());
        let mock_pool_factory = create_mock_pool_factory(&e);
        mock_pool_factory.set_pool(&pool);
        let (oracle, oracle_client) = create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.b_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.b_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_2);
        e.budget().reset();

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &100_0000000);
        oracle_client.set_price(&blnd_id, &0_5000000);

        let pool_config = PoolConfig {
            oracle,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool, || {
            storage.set_pool_config(pool_config);
            storage.set_backstop(backstop.clone());

            b_token_0.mint(&Signature::Invoker, &0, &backstop_id, &10_0000000);
            b_token_1.mint(&Signature::Invoker, &0, &backstop_id, &2_5000000);

            e.budget().reset();
            let result = create_interest_auction(&e).unwrap();

            assert_eq!(
                result.auction_type as u32,
                AuctionType::InterestAuction as u32
            );
            assert_eq!(result.user, backstop_id);
            assert_eq!(result.data.block, 151);
            assert_eq!(
                result.data.bid.get_unchecked(0).unwrap(),
                (u32::MAX, 95_4122842)
            );
            assert_eq!(result.data.bid.len(), 1);
            assert_eq!(
                result.data.lot.get_unchecked(0).unwrap(),
                (reserve_0.config.index, 10_0006589)
            );
            assert_eq!(
                result.data.lot.get_unchecked(1).unwrap(),
                (reserve_1.config.index, 2_5006144)
            );
            assert_eq!(result.data.lot.len(), 2);
        });
    }

    #[test]
    fn test_fill_interest_auction() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let blnd_id = BytesN::from_array(&e, &BLND_TOKEN);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 301, // 75% bid, 100% lot
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let user = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user.clone());
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let pool = generate_contract_id(&e);
        let (backstop, _backstop_client) = create_backstop(&e);
        let backstop_id = Identifier::Contract(backstop.clone());
        let mock_pool_factory = create_mock_pool_factory(&e);
        mock_pool_factory.set_pool(&pool);
        let blnd_client = create_token_from_id(&e, &blnd_id, &bombadil_id);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.b_rate = 1_100_000_000;
        reserve_0.data.last_block = 301;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.b_rate = 1_200_000_000;
        reserve_1.data.last_block = 301;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 301;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_2);
        e.budget().reset();

        let pool_config = PoolConfig {
            oracle: generate_contract_id(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let auction_data = AuctionDataV2 {
            bid: vec![&e, (u32::MAX, 95_2000000)],
            lot: vec![&e, (0, 10_0000000), (1, 2_5000000)],
            block: 51,
        };
        let auction = AuctionV2 {
            auction_type: AuctionType::InterestAuction,
            user: backstop_id.clone(),
            data: auction_data.clone(),
        };
        blnd_client.with_source_account(&bombadil).mint(
            &Signature::Invoker,
            &0,
            &user_id,
            &95_2000000,
        );
        blnd_client.with_source_account(&user).incr_allow(
            &Signature::Invoker,
            &0,
            &Identifier::Contract(pool.clone()),
            &i128::MAX,
        );
        e.as_contract(&pool, || {
            storage.set_auction(
                AuctionType::InterestAuction as u32,
                backstop_id.clone(),
                auction_data,
            );
            storage.set_pool_config(pool_config);
            storage.set_backstop(backstop.clone());

            blnd_client.incr_allow(&Signature::Invoker, &0, &backstop_id, &(u64::MAX as i128));

            b_token_0.mint(&Signature::Invoker, &0, &backstop_id, &10_0000000);
            b_token_1.mint(&Signature::Invoker, &0, &backstop_id, &2_5000000);

            e.budget().reset();
            let result = fill_interest_auction(&e, &auction, user_id.clone());
            // let result = calc_fill_interest_auction(&e, &auction);

            assert_eq!(result.bid.get_unchecked(0).unwrap(), (blnd_id, 71_4000000));
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(0).unwrap(),
                (reserve_0.config.b_token, 10_0000000)
            );
            assert_eq!(
                result.lot.get_unchecked(1).unwrap(),
                (reserve_1.config.b_token, 2_5000000)
            );
            assert_eq!(result.lot.len(), 2);
            assert_eq!(blnd_client.balance(&user_id), 23_8000000);
            assert_eq!(blnd_client.balance(&backstop_id), 71_4000000);
            assert_eq!(b_token_0.balance(&backstop_id), 0);
            assert_eq!(b_token_1.balance(&backstop_id), 0);
            assert_eq!(b_token_0.balance(&user_id), 10_0000000);
            assert_eq!(b_token_1.balance(&user_id), 2_5000000);
        });
    }
}
