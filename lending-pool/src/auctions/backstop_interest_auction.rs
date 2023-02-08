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

use super::auction_v2::{AuctionQuote, AuctionType, AuctionV2};

pub fn verify_create_interest_auction(e: &Env, auction: &AuctionV2) -> Result<(), PoolError> {
    let storage = StorageManager::new(e);

    let backstop = storage.get_backstop();
    if auction.user != Identifier::Contract(backstop)
        || auction.auction_type != AuctionType::InterestAuction
    {
        return Err(PoolError::BadRequest);
    }

    if storage.has_auction(auction.auction_type.clone() as u32, auction.user.clone()) {
        return Err(PoolError::AlreadyInProgress);
    }

    // TODO: Determine if any threshold should be required to create interest auction
    //       It is currently guaranteed that if no auction is active, some interest
    //       will be generated.

    Ok(())
}

pub fn calc_fill_interest_auction(e: &Env, auction: &AuctionV2) -> AuctionQuote {
    let storage = StorageManager::new(e);
    let pool_config = storage.get_pool_config();
    let bkstp_id = Identifier::Contract(storage.get_backstop());
    let oracle_client = OracleClient::new(e, pool_config.oracle.clone());

    let mut auction_quote = AuctionQuote {
        send: vec![e],
        receive: vec![e],
    };

    let (send_to_mod, receive_from_mod) = auction.get_fill_modifiers(e);
    let reserve_count = storage.get_res_list();
    let mut interest_value = 0;
    for i in 0..reserve_count.len() {
        let res_asset_address = reserve_count.get_unchecked(i).unwrap();

        let mut reserve = Reserve::load(&e, res_asset_address.clone());
        let to_mint_bkstp = reserve.update_rates(e, pool_config.bstop_rate);
        let asset_to_base = oracle_client.get_price(&res_asset_address);

        let b_token_client = TokenClient::new(e, reserve.config.b_token.clone());
        let b_token_balance = b_token_client.balance(&bkstp_id) + to_mint_bkstp;
        if b_token_balance > 0 {
            let asset_balance = reserve.to_asset_from_b_token(b_token_balance);
            interest_value += asset_balance
                .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                .unwrap();
            let receive_from_amount = receive_from_mod
                .fixed_mul_floor(b_token_balance, SCALAR_7)
                .unwrap();
            auction_quote
                .receive
                .push_back((reserve.config.b_token.clone(), receive_from_amount));
        }
    }

    if interest_value > 0 {
        let blnd_token = BytesN::from_array(e, &BLND_TOKEN);
        let blnd_to_base = oracle_client.get_price(&blnd_token);
        let send_to_amount = interest_value
            .fixed_mul_floor(1_4000000, SCALAR_7)
            .unwrap()
            .fixed_div_floor(i128(blnd_to_base), SCALAR_7)
            .unwrap()
            .fixed_mul_floor(send_to_mod, SCALAR_7)
            .unwrap();
        if send_to_amount > 0 {
            auction_quote
                .send
                .push_back((blnd_token.clone(), send_to_amount));
        }
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
    let oracle_client = OracleClient::new(e, pool_config.oracle.clone());

    let mut auction_quote = AuctionQuote {
        send: vec![e],
        receive: vec![e],
    };

    let (send_to_mod, receive_from_mod) = auction.get_fill_modifiers(e);
    let reserve_count = storage.get_res_list();
    let mut interest_value = 0;
    for i in 0..reserve_count.len() {
        let res_asset_address = reserve_count.get_unchecked(i).unwrap();

        let mut reserve = Reserve::load(&e, res_asset_address.clone());
        reserve
            .pre_action(e, &pool_config, 1, bkstp_id.clone())
            .unwrap();
        let asset_to_base = oracle_client.get_price(&res_asset_address);

        let b_token_client = TokenClient::new(e, reserve.config.b_token.clone());
        let b_token_balance = b_token_client.balance(&bkstp_id);
        if b_token_balance > 0 {
            let asset_balance = reserve.to_asset_from_b_token(b_token_balance);
            interest_value += asset_balance
                .fixed_mul_floor(i128(asset_to_base), SCALAR_7)
                .unwrap();
            let receive_from_amount = receive_from_mod
                .fixed_mul_floor(b_token_balance, SCALAR_7)
                .unwrap();
            auction_quote
                .receive
                .push_back((reserve.config.b_token.clone(), receive_from_amount));
            // TODO: Privileged xfer
            b_token_client.clawback(&Signature::Invoker, &0, &bkstp_id, &receive_from_amount);
            b_token_client.mint(&Signature::Invoker, &0, &filler, &receive_from_amount);
        }

        reserve.set_data(e);
    }

    if interest_value > 0 {
        let blnd_token = BytesN::from_array(e, &BLND_TOKEN);
        let blnd_to_base = oracle_client.get_price(&blnd_token);
        let send_to_amount = interest_value
            .fixed_mul_floor(1_4000000, SCALAR_7)
            .unwrap()
            .fixed_div_floor(i128(blnd_to_base), SCALAR_7)
            .unwrap()
            .fixed_mul_floor(send_to_mod, SCALAR_7)
            .unwrap();

        if send_to_amount > 0 {
            auction_quote
                .send
                .push_back((blnd_token.clone(), send_to_amount));

            // TODO: Make more seamless with "auth-next" by pre-authorizing the transfer taking place
            //       in the backstop client to avoid a double transfer.

            let backstop_client = BackstopClient::new(&e, &bkstp);
            TokenClient::new(e, &blnd_token).xfer_from(
                &Signature::Invoker,
                &0,
                &filler,
                &Identifier::Contract(e.current_contract()),
                &send_to_amount,
            );
            backstop_client.donate(&e.current_contract(), &(send_to_amount as u64));
        }
    }
    auction_quote
}

// TODO: Better testing once calc-on-fill is proven functional
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
    fn test_create_interest_auction_happy_path() {
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

        let auction = AuctionV2 {
            auction_type: AuctionType::InterestAuction,
            user: backstop_id,
            block: 101,
        };
        e.as_contract(&pool_id, || {
            storage.set_backstop(backstop.clone());

            let result = verify_create_interest_auction(&e, &auction);

            match result {
                Ok(_) => assert!(true),
                Err(_) => assert!(false),
            };
        });
    }

    #[test]
    fn test_create_interest_auction_not_backstop() {
        let e = Env::default();
        let storage = StorageManager::new(&e);
        let pool_id = generate_contract_id(&e);
        let backstop = generate_contract_id(&e);

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 100,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        let auction = AuctionV2 {
            auction_type: AuctionType::InterestAuction,
            user: Identifier::Contract(generate_contract_id(&e)),
            block: 101,
        };
        e.as_contract(&pool_id, || {
            storage.set_backstop(backstop.clone());

            let result = verify_create_interest_auction(&e, &auction);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::BadRequest),
            };
        });
    }

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

        let auction = AuctionV2 {
            auction_type: AuctionType::InterestAuction,
            user: backstop_id.clone(),
            block: 101,
        };
        e.as_contract(&pool_id, || {
            storage.set_backstop(backstop.clone());
            storage.set_auction(AuctionType::InterestAuction as u32, backstop_id.clone(), 50);

            let result = verify_create_interest_auction(&e, &auction);

            match result {
                Ok(_) => assert!(false),
                Err(err) => assert_eq!(err, PoolError::AlreadyInProgress),
            };
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
            sequence_number: 250,
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
        let (oracle, oracle_client) = create_mock_oracle(&e);
        let blnd_client = create_token_from_id(&e, &blnd_id, &bombadil_id);

        // creating reserves for a pool exhausts the budget
        e.budget().reset();
        let mut reserve_0 = create_reserve(&e);
        reserve_0.data.b_rate = 1_100_000_000;
        reserve_0.data.last_block = 250;
        reserve_0.config.index = 0;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_0);
        let b_token_0 = TokenClient::new(&e, &reserve_0.config.b_token);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.data.b_rate = 1_200_000_000;
        reserve_1.data.last_block = 250;
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool, &bombadil_id, &reserve_1);
        let b_token_1 = TokenClient::new(&e, &reserve_1.config.b_token);

        let mut reserve_2 = create_reserve(&e);
        reserve_2.data.last_block = 250;
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
        let auction = AuctionV2 {
            auction_type: AuctionType::InterestAuction,
            user: backstop_id.clone(),
            block: 50,
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
            storage.set_auction(AuctionType::InterestAuction as u32, backstop_id.clone(), 50);
            storage.set_pool_config(pool_config);
            storage.set_backstop(backstop.clone());

            blnd_client.incr_allow(&Signature::Invoker, &0, &backstop_id, &(u64::MAX as i128));

            b_token_0.mint(&Signature::Invoker, &0, &backstop_id, &10_0000000);
            b_token_1.mint(&Signature::Invoker, &0, &backstop_id, &2_5000000);

            e.budget().reset();
            let result = fill_interest_auction(&e, &auction, user_id.clone());

            assert_eq!(
                result.receive.get_unchecked(0).unwrap(),
                (reserve_0.config.b_token, 10_0000000)
            );
            assert_eq!(
                result.receive.get_unchecked(1).unwrap(),
                (reserve_1.config.b_token, 2_5000000)
            );
            assert_eq!(result.receive.len(), 2);
            assert_eq!(result.send.get_unchecked(0).unwrap(), (blnd_id, 95_2000000));
            assert_eq!(result.send.len(), 1);
            assert_eq!(b_token_0.balance(&backstop_id), 0);
            assert_eq!(b_token_1.balance(&backstop_id), 0);
            assert_eq!(b_token_0.balance(&user_id), 10_0000000);
            assert_eq!(b_token_1.balance(&user_id), 2_5000000);
            assert_eq!(blnd_client.balance(&user_id), 0);
            assert_eq!(blnd_client.balance(&backstop_id), 95_2000000);
        });
    }
}
