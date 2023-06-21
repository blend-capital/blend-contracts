use crate::{
    constants::SCALAR_7,
    dependencies::{OracleClient, TokenClient},
    errors::PoolError,
    pool::Pool,
    storage,
    validator::require_nonnegative,
};
use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{map, panic_with_error, vec, Address, Env, unwrap::UnwrapOptimized};

use super::{get_fill_modifiers, AuctionData, AuctionQuote, AuctionType};

pub fn create_interest_auction_data(e: &Env, backstop: &Address) -> AuctionData {
    if storage::has_auction(e, &(AuctionType::InterestAuction as u32), backstop) {
        panic_with_error!(e, PoolError::AuctionInProgress);
    }

    // TODO: Determine if any threshold should be required to create interest auction
    //       It is currently guaranteed that if no auction is active, some interest
    //       will be generated.

    let mut pool = Pool::load(e);
    let oracle_client = OracleClient::new(e, &pool.config.oracle);

    let mut auction_data = AuctionData {
        bid: map![e],
        lot: map![e],
        block: e.ledger().sequence() + 1,
    };

    let reserve_list = storage::get_res_list(e);
    let mut interest_value = 0; // expressed in the oracle's decimals
    for i in 0..reserve_list.len() {
        let res_asset_address = reserve_list.get_unchecked(i).unwrap_optimized();
        // don't store updated reserve data back to ledger. This will occur on the the auction's fill.
        let reserve = pool.load_reserve(e, &res_asset_address);
        if reserve.backstop_credit > 0 {
            let asset_to_base = oracle_client.get_price(&res_asset_address);
            interest_value += i128(asset_to_base)
                .fixed_mul_floor(reserve.backstop_credit, 10i128.pow(reserve.decimals))
                .unwrap_optimized();
            auction_data.lot.set(i, reserve.backstop_credit);
        }
    }

    if auction_data.lot.len() == 0 || interest_value == 0 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let usdc_token = storage::get_usdc_token(e);
    let usdc_to_base = oracle_client.get_price(&usdc_token);
    let bid_amount = interest_value
        .fixed_mul_floor(1_4000000, SCALAR_7)
        .unwrap_optimized()
        .fixed_div_floor(i128(usdc_to_base), SCALAR_7)
        .unwrap_optimized();
    // u32::MAX is the key for the USDC lot
    auction_data.bid.set(u32::MAX, bid_amount);

    auction_data
}

pub fn fill_interest_auction(
    e: &Env,
    auction_data: &AuctionData,
    filler: &Address,
) -> AuctionQuote {
    let backstop = storage::get_backstop(e);
    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };
    let (bid_modifier, lot_modifier) = get_fill_modifiers(e, auction_data);

    // bid only contains the USDC token
    let usdc_token = storage::get_usdc_token(e);
    let bid_amount = auction_data.bid.get_unchecked(u32::MAX).unwrap_optimized();
    let bid_amount_modified = bid_amount.fixed_mul_floor(bid_modifier, SCALAR_7).unwrap_optimized();
    auction_quote
        .bid
        .push_back((usdc_token.clone(), bid_amount_modified));

    // TODO: add donate_usdc function to backstop
    // let backstop_client = BackstopClient::new(&e, &backstop_address);
    // backstop_client.donate(&filler, &e.current_contract_id(), &bid_amount_modified);

    // lot contains underlying tokens, but the backstop credit must be updated on the reserve
    let mut pool = Pool::load(e);
    let reserve_list = storage::get_res_list(e);
    for (res_id, lot_amount) in auction_data.lot.iter_unchecked() {
        let res_asset_address = reserve_list.get_unchecked(res_id).unwrap_optimized();
        let mut reserve = pool.load_reserve(e, &res_asset_address);
        let lot_amount_modified = lot_amount.fixed_mul_floor(lot_modifier, SCALAR_7).unwrap_optimized();
        auction_quote
            .lot
            .push_back((reserve.asset.clone(), lot_amount_modified));
        reserve.backstop_credit -= lot_amount_modified;
        // TODO: Is this necessary? Might be impossible for backstop credit to become negative
        require_nonnegative(e, &reserve.backstop_credit);
        reserve.store(e);
        TokenClient::new(e, &reserve.asset).transfer(
            &e.current_contract_address(),
            &filler,
            &lot_amount_modified,
        );
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
        testutils::{Address as _, Ledger, LedgerInfo},
        Address,
    };

    #[test]
    fn test_create_interest_auction_already_in_progress() {
        let e = Env::default();

        let pool_address = Address::random(&e);
        let backstop_address = Address::random(&e);

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
        e.as_contract(&pool_address, || {
            storage::set_auction(
                &e,
                &(AuctionType::InterestAuction as u32),
                &backstop_address,
                &auction_data,
            );

            let result = create_interest_auction_data(&e, &backstop_address);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::AuctionInProgress),
            };
        });
    }

    #[test]
    fn test_create_interest_auction() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);

        let pool_address = Address::random(&e);
        let (usdc_id, _) = create_usdc_token(&e, &pool_address, &bombadil);
        let (backstop_address, _backstop_client) = create_backstop(&e);
        setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::random(&e),
            &Address::random(&e),
        );
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 12345;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &100_0000000);
        oracle_client.set_price(&usdc_id, &1_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&backstop_address, &10_0000000);
            b_token_1.mint(&backstop_address, &2_5000000);

            let result = create_interest_auction_data(&e, &backstop_address).unwrap_optimized();

            assert_eq!(result.block, 51);
            assert_eq!(result.bid.get_unchecked(u32::MAX).unwrap_optimized(), 47_6000000);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(reserve_0.config.index).unwrap_optimized(),
                10_0000000
            );
            assert_eq!(
                result.lot.get_unchecked(reserve_1.config.index).unwrap_optimized(),
                2_5000000
            );
            assert_eq!(result.lot.len(), 2);
        });
    }

    #[test]
    fn test_create_interest_auction_applies_interest() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 150,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);

        let pool_address = Address::random(&e);
        let (usdc_id, _) = create_usdc_token(&e, &pool_address, &bombadil);
        let (backstop_address, _backstop_client) = create_backstop(&e);
        setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::random(&e),
            &Address::random(&e),
        );
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 11845;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 11845;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 11845;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &100_0000000);
        oracle_client.set_price(&usdc_id, &1_0000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);

            b_token_0.mint(&backstop_address, &10_0000000);
            b_token_1.mint(&backstop_address, &2_5000000);

            let result = create_interest_auction_data(&e, &backstop_address).unwrap_optimized();

            assert_eq!(result.block, 151);
            assert_eq!(result.bid.get_unchecked(u32::MAX).unwrap_optimized(), 47_6010785);
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(reserve_0.config.index).unwrap_optimized(),
                10_0000065
            );
            assert_eq!(
                result.lot.get_unchecked(reserve_1.config.index).unwrap_optimized(),
                2_5000061
            );
            assert_eq!(
                result.lot.get_unchecked(reserve_2.config.index).unwrap_optimized(),
                71
            );
            assert_eq!(result.lot.len(), 3);
        });
    }

    #[test]
    fn test_fill_interest_auction() {
        let e = Env::default();
        e.mock_all_auths();
        e.budget().reset_unlimited(); // setup exhausts budget

        e.ledger().set(LedgerInfo {
            timestamp: 12350,
            protocol_version: 1,
            sequence_number: 301, // 75% bid, 100% lot
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);
        let samwise = Address::random(&e);

        let pool_address = Address::random(&e);
        let (usdc_id, usdc_client) = create_usdc_token(&e, &pool_address, &bombadil);
        let (backstop_address, _backstop_client) = create_backstop(&e);
        setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &Address::random(&e),
            &Address::random(&e),
        );

        let mut reserve_0 = create_reserve(&e);
        reserve_0.b_rate = Some(1_100_000_000);
        reserve_0.data.last_time = 12345;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.b_rate = Some(1_200_000_000);
        reserve_1.data.last_time = 12345;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_time = 12345;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_2);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let auction_data = AuctionData {
            bid: map![&e, (u32::MAX, 95_2000000)],
            lot: map![&e, (0, 10_0000000), (1, 2_5000000)],
            block: 51,
        };
        usdc_client.mint(&samwise, &95_2000000);
        //samwise increase allowance for pool
        usdc_client.increase_allowance(&samwise, &pool_address, &i128::MAX);
        e.as_contract(&pool_address, || {
            storage::set_auction(
                &e,
                &(AuctionType::InterestAuction as u32),
                &backstop_address,
                &auction_data,
            );
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_address);

            usdc_client.increase_allowance(&pool_address, &backstop_address, &(u64::MAX as i128));

            b_token_0.mint(&backstop_address, &10_0000000);
            b_token_1.mint(&backstop_address, &2_5000000);

            e.budget().reset_unlimited();
            let result = fill_interest_auction(&e, &auction_data, &samwise);
            // let result = calc_fill_interest_auction(&e, &auction);

            assert_eq!(result.bid.get_unchecked(0).unwrap_optimized(), (usdc_id, 71_4000000));
            assert_eq!(result.bid.len(), 1);
            assert_eq!(
                result.lot.get_unchecked(0).unwrap_optimized(),
                (reserve_0.config.b_token, 10_0000000)
            );
            assert_eq!(
                result.lot.get_unchecked(1).unwrap_optimized(),
                (reserve_1.config.b_token, 2_5000000)
            );
            assert_eq!(result.lot.len(), 2);
            // TODO: add donate_usdc function to backstop
            // assert_eq!(usdc_client.balance(&samwise), 23_8000000);
            // assert_eq!(usdc_client.balance(&backstop), 71_4000000);
            assert_eq!(b_token_0.balance(&backstop_address), 0);
            assert_eq!(b_token_1.balance(&backstop_address), 0);
            assert_eq!(b_token_0.balance(&samwise), 10_0000000);
            assert_eq!(b_token_1.balance(&samwise), 2_5000000);
        });
    }
}
