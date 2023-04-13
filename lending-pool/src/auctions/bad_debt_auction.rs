use crate::{
    constants::SCALAR_7,
    dependencies::{BackstopClient, OracleClient, TokenClient},
    errors::PoolError,
    pool,
    reserve::Reserve,
    storage,
};
use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_sdk::{map, vec, Address, Env};

use super::{get_fill_modifiers, AuctionData, AuctionQuote, AuctionType};

pub fn create_bad_debt_auction_data(e: &Env, backstop: &Address) -> Result<AuctionData, PoolError> {
    if storage::has_auction(&e, &(AuctionType::BadDebtAuction as u32), backstop) {
        return Err(PoolError::AuctionInProgress);
    }

    let pool_config = storage::get_pool_config(e);
    let oracle_client = OracleClient::new(e, &pool_config.oracle);

    let mut auction_data = AuctionData {
        bid: map![e],
        lot: map![e],
        block: e.ledger().sequence() + 1,
    };

    let reserve_count = storage::get_res_list(e);
    let mut debt_value = 0;
    for i in 0..reserve_count.len() {
        let res_asset_address = reserve_count.get_unchecked(i).unwrap();

        let mut reserve = Reserve::load(&e, res_asset_address.clone());

        let d_token_client = TokenClient::new(e, &reserve.config.d_token);
        let d_token_balance = d_token_client.balance(&backstop);
        if d_token_balance > 0 {
            reserve.update_rates(e, pool_config.bstop_rate);
            let asset_to_base = oracle_client.get_price(&res_asset_address);
            let asset_balance = reserve.to_asset_from_d_token(d_token_balance);
            debt_value += asset_balance
                .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                .unwrap();
            auction_data.bid.set(reserve.config.index, asset_balance);
        }
    }
    if auction_data.bid.len() == 0 || debt_value == 0 {
        return Err(PoolError::BadRequest);
    }

    let backstop_client = BackstopClient::new(e, &storage::get_backstop(e));
    let backstop_token = backstop_client.bstp_token();
    // TODO: This won't have an oracle entry. Once an LP implementation exists, unwrap base from LP
    let backstop_token_to_base = oracle_client.get_price(&backstop_token);
    let mut lot_amount = debt_value
        .fixed_mul_floor(1_4000000, SCALAR_7)
        .unwrap()
        .fixed_div_floor(i128(backstop_token_to_base), SCALAR_7)
        .unwrap();
    let (pool_backstop_balance, _, _) = backstop_client.p_balance(&e.current_contract_id());
    lot_amount = pool_backstop_balance.min(lot_amount);
    // u32::MAX is the key for the backstop token
    auction_data.lot.set(u32::MAX, lot_amount);

    Ok(auction_data)
}

pub fn calc_fill_bad_debt_auction(e: &Env, auction_data: &AuctionData) -> AuctionQuote {
    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };
    let backstop_id = storage::get_backstop(e);

    let (bid_modifier, lot_modifier) = get_fill_modifiers(e, auction_data);

    // bid only contains underlying asset amounts
    let reserve_list = storage::get_res_list(e);
    for (res_id, amount) in auction_data.bid.iter_unchecked() {
        let res_asset_address = reserve_list.get_unchecked(res_id).unwrap();
        let amount_modified = amount.fixed_mul_floor(bid_modifier, SCALAR_7).unwrap();
        auction_quote
            .bid
            .push_back((res_asset_address, amount_modified));
    }

    // lot only contains the backstop token
    let backstop_client = BackstopClient::new(&e, &backstop_id);
    let backstop_token_id = backstop_client.bstp_token();
    let lot_amount = auction_data.lot.get_unchecked(u32::MAX).unwrap();
    let lot_amount_modified = lot_amount.fixed_mul_floor(lot_modifier, SCALAR_7).unwrap();
    auction_quote
        .lot
        .push_back((backstop_token_id, lot_amount_modified));

    auction_quote
}

pub fn fill_bad_debt_auction(
    e: &Env,
    auction_data: &AuctionData,
    filler: &Address,
) -> AuctionQuote {
    let auction_quote = calc_fill_bad_debt_auction(e, auction_data);

    let backstop_id = storage::get_backstop(e);
    let backstop = Address::from_contract_id(e, &backstop_id);

    // bid only contains underlying assets
    for (res_asset_address, bid_amount) in auction_quote.bid.iter_unchecked() {
        pool::execute_repay(e, filler, &res_asset_address, bid_amount, &backstop).unwrap();
    }

    // lot only contains the backstop token
    let (_, lot_amount) = auction_quote.lot.first().unwrap().unwrap();

    let backstop_client = BackstopClient::new(&e, &backstop_id);
    backstop_client.draw(&e.current_contract_id(), &lot_amount, &filler);

    auction_quote
}

#[cfg(test)]
mod tests {

    use crate::{
        auctions::auction::AuctionType,
        storage::PoolConfig,
        testutils::{
            create_backstop, create_mock_oracle, create_reserve, create_token_contract,
            setup_backstop, setup_reserve,
        },
    };

    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _, Ledger, LedgerInfo},
        BytesN,
    };

    #[test]
    fn test_create_bad_debt_auction_already_in_progress() {
        let e = Env::default();
        e.budget().reset_unlimited(); // setup exhausts budget

        let pool_id = BytesN::<32>::random(&e);
        let (backstop_id, _backstop_client) = create_backstop(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &BytesN::<32>::random(&e),
            &BytesN::<32>::random(&e),
        );

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
                &(AuctionType::BadDebtAuction as u32),
                &backstop,
                &auction_data,
            );

            let result = create_bad_debt_auction_data(&e, &backstop);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::AuctionInProgress),
            };
        });
    }

    #[test]
    fn test_create_bad_debt_auction() {
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
        let samwise = Address::random(&e);

        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &BytesN::<32>::random(&e),
        );
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        backstop_token_client.mint(&bombadil, &samwise, &200_0000000);
        backstop_token_client.incr_allow(&samwise, &backstop, &i128::MAX);
        backstop_client.deposit(&samwise, &pool_id, &100_0000000);

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &100_0000000);
        oracle_client.set_price(&backstop_token_id, &0_5000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            d_token_0.mint(&pool, &backstop, &10_0000000);
            d_token_1.mint(&pool, &backstop, &2_5000000);

            let result = create_bad_debt_auction_data(&e, &backstop).unwrap();

            assert_eq!(result.block, 51);
            assert_eq!(
                result.bid.get_unchecked(reserve_0.config.index).unwrap(),
                11_0000000
            );
            assert_eq!(
                result.bid.get_unchecked(reserve_1.config.index).unwrap(),
                3_0000000
            );
            assert_eq!(result.bid.len(), 2);
            assert_eq!(result.lot.get_unchecked(u32::MAX).unwrap(), 95_2000000);
            assert_eq!(result.lot.len(), 1);
        });
    }

    #[test]
    fn test_create_bad_debt_auction_max_balance() {
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
        let samwise = Address::random(&e);

        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &BytesN::<32>::random(&e),
        );
        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        backstop_token_client.mint(&bombadil, &samwise, &200_0000000);
        backstop_token_client.incr_allow(&samwise, &backstop, &i128::MAX);
        backstop_client.deposit(&samwise, &pool_id, &95_0000000);

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &100_0000000);
        oracle_client.set_price(&backstop_token_id, &0_5000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);

            d_token_0.mint(&pool, &backstop, &10_0000000);
            d_token_1.mint(&pool, &backstop, &2_5000000);

            let result = create_bad_debt_auction_data(&e, &backstop).unwrap();

            assert_eq!(result.block, 51);
            assert_eq!(
                result.bid.get_unchecked(reserve_0.config.index).unwrap(),
                11_0000000
            );
            assert_eq!(
                result.bid.get_unchecked(reserve_1.config.index).unwrap(),
                3_0000000
            );
            assert_eq!(result.bid.len(), 2);
            assert_eq!(result.lot.get_unchecked(u32::MAX).unwrap(), 95_0000000);
            assert_eq!(result.lot.len(), 1);
        });
    }

    #[test]
    fn test_create_bad_debt_auction_applies_interest() {
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
        let samwise = Address::random(&e);

        let pool_id = BytesN::<32>::random(&e);
        let pool = Address::from_contract_id(&e, &pool_id);
        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &BytesN::<32>::random(&e),
        );

        let (oracle_id, oracle_client) = create_mock_oracle(&e);

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 50;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        backstop_token_client.mint(&bombadil, &samwise, &200_0000000);
        backstop_token_client.incr_allow(&samwise, &backstop, &i128::MAX);
        backstop_client.deposit(&samwise, &pool_id, &100_0000000);

        oracle_client.set_price(&reserve_0.asset, &2_0000000);
        oracle_client.set_price(&reserve_1.asset, &4_0000000);
        oracle_client.set_price(&reserve_2.asset, &100_0000000);
        oracle_client.set_price(&backstop_token_id, &0_5000000);

        let pool_config = PoolConfig {
            oracle: oracle_id,
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        e.as_contract(&pool_id, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_id);

            d_token_0.mint(&pool, &backstop, &10_0000000);
            d_token_1.mint(&pool, &backstop, &2_5000000);

            let result = create_bad_debt_auction_data(&e, &backstop).unwrap();

            assert_eq!(result.block, 151);
            assert_eq!(
                result.bid.get_unchecked(reserve_0.config.index).unwrap(),
                11_0000432
            );
            assert_eq!(
                result.bid.get_unchecked(reserve_1.config.index).unwrap(),
                3_0000207
            );
            assert_eq!(result.bid.len(), 2);
            assert_eq!(result.lot.get_unchecked(u32::MAX).unwrap(), 95_2004736);
            assert_eq!(result.lot.len(), 1);
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
        let (backstop_token_id, backstop_token_client) = create_token_contract(&e, &bombadil);
        let (backstop_id, backstop_client) = create_backstop(&e);
        let backstop = Address::from_contract_id(&e, &backstop_id);
        setup_backstop(
            &e,
            &pool_id,
            &backstop_id,
            &backstop_token_id,
            &BytesN::<32>::random(&e),
        );

        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_rate = 1_100_000_000;
        reserve_0.data.last_block = 301;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_0);
        let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);
        let token_0 = TokenClient::new(&e, &reserve_0.asset);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.data.last_block = 301;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
        let token_1 = TokenClient::new(&e, &reserve_1.asset);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 301;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool_id, &bombadil, &mut reserve_2);

        // set up user reserves
        token_0.mint(&bombadil, &samwise, &12_0000000);
        token_1.mint(&bombadil, &samwise, &3_5000000);
        token_0.incr_allow(&samwise, &pool, &i128::MAX);
        token_1.incr_allow(&samwise, &pool, &i128::MAX);
        let pool_config = PoolConfig {
            oracle: BytesN::<32>::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let auction_data = AuctionData {
            bid: map![&e, (0, 11_0000000), (1, 3_0000000)],
            lot: map![&e, (u32::MAX, 95_2000000)],
            block: 51,
        };
        backstop_token_client.mint(&bombadil, &samwise, &95_2000000);
        backstop_token_client.incr_allow(&samwise, &backstop, &i128::MAX);
        backstop_client.deposit(&samwise, &pool_id, &95_2000000);
        e.as_contract(&pool_id, || {
            storage::set_auction(
                &e,
                &(AuctionType::BadDebtAuction as u32),
                &backstop,
                &auction_data,
            );
            storage::set_pool_config(&e, &pool_config);

            backstop_token_client.incr_allow(&pool, &backstop, &(u64::MAX as i128));

            d_token_0.mint(&pool, &backstop, &10_0000000);
            d_token_1.mint(&pool, &backstop, &2_5000000);

            let result = fill_bad_debt_auction(&e, &auction_data, &samwise);

            assert_eq!(
                result.lot.get_unchecked(0).unwrap(),
                (backstop_token_id, 95_2000000)
            );
            assert_eq!(result.lot.len(), 1);
            assert_eq!(
                result.bid.get_unchecked(0).unwrap(),
                (reserve_0.asset, 8_2500000)
            );
            assert_eq!(
                result.bid.get_unchecked(1).unwrap(),
                (reserve_1.asset, 2_2500000)
            );
            assert_eq!(result.bid.len(), 2);
            assert_eq!(backstop_token_client.balance(&backstop), 0);
            assert_eq!(backstop_token_client.balance(&samwise), 95_2000000);
            assert_eq!(d_token_0.balance(&backstop), 2_5000000);
            assert_eq!(d_token_1.balance(&backstop), 6250000);
            assert_eq!(token_0.balance(&samwise), 3_7500000);
            assert_eq!(token_1.balance(&samwise), 1_2500000);
        });
    }
}
