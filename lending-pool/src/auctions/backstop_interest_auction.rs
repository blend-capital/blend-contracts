use crate::{
    constants::SCALAR_7,
    dependencies::{OracleClient, TokenClient},
    errors::PoolError,
    reserve::Reserve,
    storage,
};
use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{map, vec, Address, Env};

use super::{get_fill_modifiers, AuctionData, AuctionQuote, AuctionType};

pub fn create_interest_auction_data(e: &Env, backstop: &Address) -> Result<AuctionData, PoolError> {
    if storage::has_auction(e, &(AuctionType::InterestAuction as u32), backstop) {
        return Err(PoolError::AuctionInProgress);
    }

    // TODO: Determine if any threshold should be required to create interest auction
    //       It is currently guaranteed that if no auction is active, some interest
    //       will be generated.

    let pool_config = storage::get_pool_config(e);
    let oracle_client = OracleClient::new(e, &pool_config.oracle);

    let mut auction_data = AuctionData {
        bid: map![e],
        lot: map![e],
        block: e.ledger().sequence() + 1,
    };

    let reserve_count = storage::get_res_list(e);
    let mut interest_value = 0;
    for i in 0..reserve_count.len() {
        let res_asset_address = reserve_count.get_unchecked(i).unwrap();

        let mut reserve = Reserve::load(&e, res_asset_address.clone());
        let to_mint_bkstp = reserve.update_rates(e, pool_config.bstop_rate);

        let b_token_client = TokenClient::new(e, &reserve.config.b_token);
        let b_token_balance = b_token_client.balance(&backstop) + to_mint_bkstp;
        if b_token_balance > 0 {
            let asset_to_base = oracle_client.get_price(&res_asset_address);
            let asset_balance = reserve.to_asset_from_b_token(e, b_token_balance);
            interest_value += asset_balance
                .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                .unwrap();
            auction_data.lot.set(reserve.config.index, b_token_balance);
        }
    }

    if auction_data.lot.len() == 0 || interest_value == 0 {
        return Err(PoolError::BadRequest);
    }

    let usdc_token = storage::get_usdc_token(e);
    let usdc_to_base = oracle_client.get_price(&usdc_token);
    let bid_amount = interest_value
        .fixed_mul_floor(1_4000000, SCALAR_7)
        .unwrap()
        .fixed_div_floor(i128(usdc_to_base), SCALAR_7)
        .unwrap();
    // u32::MAX is the key for the backstop token
    auction_data.bid.set(u32::MAX, bid_amount);

    Ok(auction_data)
}

/// NOTE: This function is for viewing purposes only and should not be called by functions
///       that modify state
pub fn calc_fill_interest_auction(e: &Env, auction_data: &AuctionData) -> AuctionQuote {
    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };

    let (bid_modifier, lot_modifier) = get_fill_modifiers(e, auction_data);

    // bid only contains the backstop token
    let bid_amount = auction_data.bid.get_unchecked(u32::MAX).unwrap();
    let bid_amount_modified = bid_amount.fixed_mul_floor(bid_modifier, SCALAR_7).unwrap();
    auction_quote
        .bid
        .push_back((storage::get_usdc_token(e), bid_amount_modified));

    // lot only contains b_token reserves
    let reserve_list = storage::get_res_list(e);
    for (res_id, amount) in auction_data.lot.iter_unchecked() {
        let res_asset_address = reserve_list.get_unchecked(res_id).unwrap();
        let reserve_config = storage::get_res_config(e, &res_asset_address);
        let amount_modified = amount.fixed_mul_floor(lot_modifier, SCALAR_7).unwrap();
        auction_quote
            .lot
            .push_back((reserve_config.b_token, amount_modified));
    }

    auction_quote
}

pub fn fill_interest_auction(
    e: &Env,
    auction_data: &AuctionData,
    filler: &Address,
) -> AuctionQuote {
    // TODO: Determine if there is a way to reuse calc code. Currently, this would result in reloads of all
    //       reserves and the minting of tokens to the backstop during previews.
    let pool_config = storage::get_pool_config(e);
    let backstop = storage::get_backstop_address(e);

    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };

    let (bid_modifier, lot_modifier) = get_fill_modifiers(e, auction_data);

    // bid only contains the USDC token
    let usdc_token = storage::get_usdc_token(e);
    let bid_amount = auction_data.bid.get_unchecked(u32::MAX).unwrap();
    let bid_amount_modified = bid_amount.fixed_mul_floor(bid_modifier, SCALAR_7).unwrap();
    auction_quote
        .bid
        .push_back((usdc_token.clone(), bid_amount_modified));

    // TODO: add donate_usdc function to backstop
    // let backstop_client = BackstopClient::new(&e, &backstop_id);
    // backstop_client.donate(&filler, &e.current_contract_id(), &bid_amount_modified);

    // lot only contains b_token reserves
    let reserve_list = storage::get_res_list(e);
    for (res_id, lot_amount) in auction_data.lot.iter_unchecked() {
        let res_asset_address = reserve_list.get_unchecked(res_id).unwrap();
        let mut reserve = Reserve::load(&e, res_asset_address.clone());
        reserve
            .pre_action(e, &pool_config, 1, backstop.clone())
            .unwrap();
        let lot_amount_modified = lot_amount.fixed_mul_floor(lot_modifier, SCALAR_7).unwrap();
        auction_quote
            .lot
            .push_back((reserve.config.b_token.clone(), lot_amount_modified));

        // TODO: Privileged xfer
        let b_token_client = TokenClient::new(e, &reserve.config.b_token);
        b_token_client.clawback(
            &e.current_contract_address(),
            &backstop,
            &lot_amount_modified,
        );
        b_token_client.mint(&e.current_contract_address(), &filler, &lot_amount_modified);
        reserve.set_data(e);
    }
    auction_quote
}

#[cfg(test)]
mod tests {
    use crate::{
        auctions::auction::AuctionType,
        storage::{self, PoolConfig},
        testutils::{
            create_backstop, create_mock_oracle, create_reserve, create_usdc_token, setup_backstop,
            setup_reserve,
        },
    };

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _, Ledger, LedgerInfo},
        Address, BytesN,
    };

    #[test]
    fn test_create_interest_auction_already_in_progress() {
        let e = Env::default();

        let pool_id = BytesN::<32>::random(&e);
        let backstop_id = BytesN::<32>::random(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let auction_data = AuctionData {
            bid: map![&e],
            lot: map![&e],
            block: 50,
        };
        e.as_contract(&pool_id, || {
            storage::set_auction(
                &e,
                &(AuctionType::InterestAuction as u32),
                &backstop,
                &auction_data,
            );

            let result = create_interest_auction_data(&e, &backstop);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::AuctionInProgress),
            };
        });
    }

    #[test]
    fn test_create_interest_auction() {
        let e = Env::default();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);

        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (usdc_id, _) = create_usdc_token(&e, &pool_id, &bombadil);
        let (backstop_id, _backstop_client) = create_backstop(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &BytesN::<32>::random(&e),
            &BytesN::<32>::random(&e),
        );
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_block = 50;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_block = 50;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &100_0000000);
        oracle_client.set_price(&usdc_id, &1_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&pool, &backstop, &10_0000000);
            b_token_1.mint(&pool, &backstop, &2_5000000);

            let result = create_interest_auction_data(&e, &backstop).unwrap();

            assert_eq!(result.block, 51);
            assert_eq!(result.bid.get_unchecked(u32::MAX).unwrap(), 47_6000000);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(reserve_0.config.index).unwrap(),
                10_0000000
            );
            assert_eq!(
                result.lot.get_unchecked(reserve_1.config.index).unwrap(),
                2_5000000
            );
            assert_eq!(result.lot.len(), 2);
        });
    }

    #[test]
    fn test_create_interest_auction_applies_interest() {
        let e = Env::default();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 150,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);

        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (usdc_id, _) = create_usdc_token(&e, &pool_id, &bombadil);
        let (backstop_id, _backstop_client) = create_backstop(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &BytesN::<32>::random(&e),
            &BytesN::<32>::random(&e),
        );
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_block = 50;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_block = 50;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &100_0000000);
        oracle_client.set_price(&usdc_id, &1_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&pool, &backstop, &10_0000000);
            b_token_1.mint(&pool, &backstop, &2_5000000);

            let result = create_interest_auction_data(&e, &backstop).unwrap();

            assert_eq!(result.block, 151);
            assert_eq!(result.bid.get_unchecked(u32::MAX).unwrap(), 47_6010785);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(reserve_0.config.index).unwrap(),
                10_0000065
            );
            assert_eq!(
                result.lot.get_unchecked(reserve_1.config.index).unwrap(),
                2_5000061
            );
            assert_eq!(
                result.lot.get_unchecked(reserve_2.config.index).unwrap(),
                71
            );
            assert_eq!(result.lot.len(), 3);
        });
    }

    #[test]
    fn test_fill_interest_auction() {
        let e = Env::default();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 301, // 75% bid, 100% lot
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (usdc_id, usdc_client) = create_usdc_token(&e, &pool_id, &bombadil);
        let (backstop_id, _backstop_client) = create_backstop(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &BytesN::<32>::random(&e),
            &BytesN::<32>::random(&e),
        );

        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_block = 301;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_block = 301;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 301;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        let pool_config = PoolConfig {
            oracle: BytesN::<32>::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let auction_data = AuctionData {
            bid: map![&e, (u32::MAX, 95_2000000)],
            lot: map![&e, (0, 10_0000000), (1, 2_5000000)],
            block: 51,
        };
        usdc_client.mint(&bombadil, &samwise, &95_2000000);
        usdc_client.incr_allow(&samwise, &pool, &i128::MAX);
        e.as_contract(&pool_id, || {
            storage::set_auction(
                &e,
                &(AuctionType::InterestAuction as u32),
                &backstop,
                &auction_data,
            );
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);
            storage::set_backstop_address(&e, &backstop);

            usdc_client.incr_allow(&pool, &backstop, &(u64::MAX as i128));

            b_token_0.mint(&pool, &backstop, &10_0000000);
            b_token_1.mint(&pool, &backstop, &2_5000000);

            e.budget().reset_unlimited();
            let result = fill_interest_auction(&e, &auction_data, &samwise);
            // let result = calc_fill_interest_auction(&e, &auction);

            assert_eq!(result.bid.get_unchecked(0).unwrap(), (usdc_id, 71_4000000));
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
            // TODO: add donate_usdc function to backstop
            // assert_eq!(usdc_client.balance(&samwise), 23_8000000);
            // assert_eq!(usdc_client.balance(&backstop), 71_4000000);
            assert_eq!(b_token_0.balance(&backstop), 0);
            assert_eq!(b_token_1.balance(&backstop), 0);
            assert_eq!(b_token_0.balance(&samwise), 10_0000000);
            assert_eq!(b_token_1.balance(&samwise), 2_5000000);
        });
    }
}
