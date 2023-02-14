use crate::{
    constants::{BLND_TOKEN, SCALAR_7},
    dependencies::{BackstopClient, OracleClient, TokenClient},
    errors::PoolError,
    pool::execute_repay,
    reserve::Reserve,
    storage::{PoolDataStore, StorageManager},
};
use cast::i128;
use fixed_point_math::FixedPoint;
use soroban_auth::Identifier;
use soroban_sdk::{vec, BytesN, Env};

use super::auction_v2::{AuctionDataV2, AuctionQuote, AuctionType, AuctionV2};

pub fn create_bad_debt_auction(e: &Env) -> Result<AuctionV2, PoolError> {
    let storage = StorageManager::new(e);
    let bkstp_id = Identifier::Contract(storage.get_backstop());

    if storage.has_auction(AuctionType::BadDebtAuction as u32, bkstp_id.clone()) {
        return Err(PoolError::AlreadyInProgress);
    }

    let pool_config = storage.get_pool_config();
    let oracle_client = OracleClient::new(e, pool_config.oracle.clone());

    let mut auction_data = AuctionDataV2 {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence() + 1,
    };

    let reserve_count = storage.get_res_list();
    let mut debt_value = 0;
    for i in 0..reserve_count.len() {
        let res_asset_address = reserve_count.get_unchecked(i).unwrap();

        let mut reserve = Reserve::load(&e, res_asset_address.clone());
        reserve.update_rates(e, pool_config.bstop_rate);

        let d_token_client = TokenClient::new(e, reserve.config.d_token.clone());
        let d_token_balance = d_token_client.balance(&bkstp_id);
        if d_token_balance > 0 {
            let asset_to_base = oracle_client.get_price(&res_asset_address);
            let asset_balance = reserve.to_asset_from_d_token(d_token_balance);
            debt_value += asset_balance
                .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                .unwrap();
            auction_data
                .bid
                .push_back((reserve.config.index, asset_balance));
        }
    }
    if auction_data.bid.len() == 0 || debt_value == 0 {
        return Err(PoolError::BadRequest);
    }

    let blnd_token = BytesN::from_array(e, &BLND_TOKEN);
    let blnd_to_base = oracle_client.get_price(&blnd_token);
    let lot_amount = debt_value
        .fixed_mul_floor(1_4000000, SCALAR_7)
        .unwrap()
        .fixed_div_floor(i128(blnd_to_base), SCALAR_7)
        .unwrap();
    // u32::MAX is the key for the backstop token
    auction_data.lot.push_back((u32::MAX, lot_amount));

    Ok(AuctionV2 {
        auction_type: AuctionType::BadDebtAuction,
        user: bkstp_id,
        data: auction_data,
    })
}

pub fn calc_fill_bad_debt_auction(e: &Env, auction: &AuctionV2) -> AuctionQuote {
    let storage = StorageManager::new(e);

    let mut auction_quote = AuctionQuote {
        bid: vec![e],
        lot: vec![e],
        block: e.ledger().sequence(),
    };

    let (bid_modifier, lot_modifier) = auction.get_fill_modifiers(e);

    // bid only contains underlying asset amounts
    let reserve_list = storage.get_res_list();
    for (res_id, amount) in auction.data.bid.iter_unchecked() {
        let res_asset_address = reserve_list.get_unchecked(res_id).unwrap();
        let amount_modified = amount.fixed_mul_floor(bid_modifier, SCALAR_7).unwrap();
        auction_quote
            .bid
            .push_back((res_asset_address, amount_modified));
    }

    // lot only contains the backstop token
    let (_, lot_amount) = auction.data.lot.first().unwrap().unwrap();
    let lot_amount_modified = lot_amount.fixed_mul_floor(lot_modifier, SCALAR_7).unwrap();
    auction_quote
        .lot
        .push_back((BytesN::from_array(e, &BLND_TOKEN), lot_amount_modified));

    auction_quote
}

pub fn fill_bad_debt_auction(e: &Env, auction: &AuctionV2, filler: Identifier) -> AuctionQuote {
    let auction_quote = calc_fill_bad_debt_auction(e, auction);

    // TODO: Determine if there is a way to reuse calc code. Currently, this would result in reloads of all
    //       reserves and the minting of tokens to the backstop during previews.
    let storage = StorageManager::new(e);
    let pool_config = storage.get_pool_config();
    let bkstp = storage.get_backstop();
    let bkstp_id = Identifier::Contract(bkstp.clone());

    // bid only contains underlying assets
    for (res_asset_address, bid_amount) in auction_quote.bid.iter_unchecked() {
        let mut reserve = Reserve::load(&e, res_asset_address.clone());
        reserve
            .pre_action(e, &pool_config, 1, bkstp_id.clone())
            .unwrap();

        execute_repay(e, reserve, bid_amount, filler.clone(), &bkstp_id);
    }

    // lot only contains the backstop token
    let (_, lot_amount) = auction_quote.lot.first().unwrap().unwrap();

    // TODO: Make more seamless with "auth-next" by pre-authorizing the transfer taking place
    //       in the backstop client to avoid a double transfer.
    let backstop_client = BackstopClient::new(&e, &bkstp);
    backstop_client.draw(&(lot_amount as u64), &filler);

    auction_quote
}

#[cfg(test)]
mod tests {

    use crate::{
        auctions::auction_v2::AuctionType,
        storage::PoolConfig,
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
    fn test_create_bad_debt_auction_already_in_progress() {
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
                AuctionType::BadDebtAuction as u32,
                backstop_id.clone(),
                auction_data,
            );

            let result = create_bad_debt_auction(&e);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::AlreadyInProgress),
            };
        });
    }

    #[test]
    fn test_create_bad_debt_auction() {
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
        reserve_0.data.d_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);

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

            d_token_0.mint(&Signature::Invoker, &0, &backstop_id, &10_0000000);
            d_token_1.mint(&Signature::Invoker, &0, &backstop_id, &2_5000000);

            e.budget().reset();
            let result = create_bad_debt_auction(&e).unwrap();

            assert_eq!(
                result.auction_type as u32,
                AuctionType::BadDebtAuction as u32
            );
            assert_eq!(result.user, backstop_id);
            assert_eq!(result.data.block, 51);
            assert_eq!(
                result.data.bid.get_unchecked(0).unwrap(),
                (reserve_0.config.index, 11_0000000)
            );
            assert_eq!(
                result.data.bid.get_unchecked(1).unwrap(),
                (reserve_1.config.index, 3_0000000)
            );
            assert_eq!(result.data.bid.len(), 2);
            assert_eq!(
                result.data.lot.get_unchecked(0).unwrap(),
                (u32::MAX, 95_2000000)
            );
            assert_eq!(result.data.lot.len(), 1);
        });
    }

    #[test]
    fn test_create_bad_debt_auction_applies_interest() {
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
        reserve_0.data.d_rate = 1_100_000_000;
        reserve_0.data.last_block = 50;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.data.last_block = 50;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);

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
            storage.set_pool_config(pool_config.clone());
            storage.set_backstop(backstop.clone());

            d_token_0.mint(&Signature::Invoker, &0, &backstop_id, &10_0000000);
            d_token_1.mint(&Signature::Invoker, &0, &backstop_id, &2_5000000);

            e.budget().reset();
            let result = create_bad_debt_auction(&e).unwrap();

            assert_eq!(
                result.auction_type as u32,
                AuctionType::BadDebtAuction as u32
            );
            assert_eq!(result.user, backstop_id);
            assert_eq!(result.data.block, 151);
            assert_eq!(
                result.data.bid.get_unchecked(0).unwrap(),
                (reserve_0.config.index, 11_0000431)
            );
            assert_eq!(
                result.data.bid.get_unchecked(1).unwrap(),
                (reserve_1.config.index, 3_0000206)
            );
            assert_eq!(result.data.bid.len(), 2);
            assert_eq!(
                result.data.lot.get_unchecked(0).unwrap(),
                (u32::MAX, 95_2004720)
            );
            assert_eq!(result.data.lot.len(), 1);
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
        let pool_id = Identifier::Contract(pool.clone());
        let (backstop, backstop_client) = create_backstop(&e);
        let backstop_id = Identifier::Contract(backstop.clone());
        let mock_pool_factory = create_mock_pool_factory(&e);
        mock_pool_factory.set_pool(&pool);
        let blnd_client = create_token_from_id(&e, &blnd_id, &bombadil_id);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.d_rate = 1_100_000_000;
        reserve_0.data.last_block = 301;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);
        let token_0 = TokenClient::new(&e, &reserve_0.asset);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.d_rate = 1_200_000_000;
        reserve_1.data.last_block = 301;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
        let token_1 = TokenClient::new(&e, &reserve_1.asset);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 301;
        reserve_2.config.index = 2;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_2);
        e.budget().reset();
        // set up user reserves
        token_0
            .with_source_account(&bombadil)
            .mint(&Signature::Invoker, &0, &user_id, &12_0000000);
        token_1
            .with_source_account(&bombadil)
            .mint(&Signature::Invoker, &0, &user_id, &3_5000000);
        token_0.with_source_account(&user).incr_allow(
            &Signature::Invoker,
            &0,
            &pool_id,
            &i128::MAX,
        );
        token_1.with_source_account(&user).incr_allow(
            &Signature::Invoker,
            &0,
            &pool_id,
            &i128::MAX,
        );
        let pool_config = PoolConfig {
            oracle: generate_contract_id(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };
        let auction_data = AuctionDataV2 {
            bid: vec![&e, (0, 11_0000000), (1, 3_0000000)],
            lot: vec![&e, (u32::MAX, 95_2000000)],
            block: 51,
        };
        let auction = AuctionV2 {
            auction_type: AuctionType::BadDebtAuction,
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
            &backstop_id,
            &i128::MAX,
        );
        backstop_client
            .with_source_account(&user)
            .deposit(&pool, &95_2000000);
        e.as_contract(&pool, || {
            storage.set_auction(
                AuctionType::BadDebtAuction as u32,
                backstop_id.clone(),
                auction_data,
            );
            storage.set_pool_config(pool_config);
            storage.set_backstop(backstop.clone());

            blnd_client.incr_allow(&Signature::Invoker, &0, &backstop_id, &(u64::MAX as i128));

            d_token_0.mint(&Signature::Invoker, &0, &backstop_id, &10_0000000);
            d_token_1.mint(&Signature::Invoker, &0, &backstop_id, &2_5000000);

            e.budget().reset();

            let result = fill_bad_debt_auction(&e, &auction, user_id.clone());
            assert_eq!(result.lot.get_unchecked(0).unwrap(), (blnd_id, 95_2000000));
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
            assert_eq!(blnd_client.balance(&backstop_id), 0);
            assert_eq!(blnd_client.balance(&user_id), 95_2000000);
            assert_eq!(d_token_0.balance(&backstop_id), 2_5000000);
            assert_eq!(d_token_1.balance(&backstop_id), 6250000);
            assert_eq!(token_0.balance(&user_id), 3_7500000);
            assert_eq!(token_1.balance(&user_id), 1_2500000);
        });
    }
}
