#![cfg(test)]
use common::generate_contract_id;
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{
    testutils::{Accounts, Ledger, LedgerInfo},
    BytesN, Env, Status,
};

mod common;
use crate::common::{create_backstop_module, create_token_from_id, BackstopError};

#[test]
fn test_donate_happy_path() {
    let e = Env::default();

    // create backstop module
    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop_id = Identifier::Contract(backstop_addr.clone());

    // create token
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let token_id = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_id, &bombadil_id);

    // create pools
    let pool_1 = generate_contract_id(&e);
    let pool_2 = generate_contract_id(&e);
    let pool_2_id = Identifier::Contract(pool_2.clone());

    // create user to deposit
    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    // mint tokens to user and approve backstop
    token_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &samwise_id,
        &700_000_0000000, // total deposit amount
    );
    token_client.with_source_account(&samwise).approve(
        &Signature::Invoker,
        &0,
        &backstop_id,
        &(u64::MAX as i128),
    );

    e.ledger().set(LedgerInfo {
        timestamp: 1441065600,
        protocol_version: 1,
        sequence_number: 0,
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    // deposit into backstop module
    backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_1, &400_000_0000000);
    backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_2, &200_000_0000000);

    let (pre_tokens_1, pre_shares_1, _pre_q4w_1) = backstop_client.p_balance(&pool_1);
    let (pre_tokens_2, pre_shares_2, _pre_q4w_2) = backstop_client.p_balance(&pool_2);

    assert_eq!(pre_tokens_1, 400_000_0000000);
    assert_eq!(pre_shares_1, 400_000_0000000);
    assert_eq!(pre_tokens_2, 200_000_0000000);
    assert_eq!(pre_shares_2, 200_000_0000000);

    backstop_client.add_reward(&pool_1, &BytesN::from_array(&e, &[0u8; 32]));
    backstop_client.add_reward(&pool_2, &BytesN::from_array(&e, &[0u8; 32]));

    // donate
    token_client.with_source_account(&samwise).approve(
        &Signature::Invoker,
        &0,
        &pool_2_id,
        &(u64::MAX as i128),
    );
    e.as_contract(&pool_2, || {
        backstop_client.donate(&pool_2, &100_000_0000000, &samwise_id);
    });

    let (post_tokens_1, post_shares_1, _post_q4w_1) = backstop_client.p_balance(&pool_1);
    let (post_tokens_2, post_shares_2, _post_q4w_2) = backstop_client.p_balance(&pool_2);

    assert_eq!(post_tokens_1, 400_000_0000000);
    assert_eq!(post_shares_1, 400_000_0000000);
    assert_eq!(post_tokens_2, 200_000_0000000 + 100_000_0000000);
    assert_eq!(post_shares_2, 200_000_0000000);
}

#[test]
fn test_donate_fails_not_authorized() {
    let e = Env::default();

    // create backstop module
    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop_id = Identifier::Contract(backstop_addr.clone());

    // create token
    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let token_id = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_id, &bombadil_id);

    // create pools
    let pool_1 = generate_contract_id(&e);
    let pool_2 = generate_contract_id(&e);
    let pool_2_id = Identifier::Contract(pool_2.clone());

    // create user to deposit
    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    //create user to call incorrectly
    let sauron = e.accounts().generate_and_create();

    // mint tokens to user and approve backstop
    token_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &samwise_id,
        &600_000_0000000, // total deposit amount
    );
    token_client.with_source_account(&samwise).approve(
        &Signature::Invoker,
        &0,
        &backstop_id,
        &(u64::MAX as i128),
    );

    e.ledger().set(LedgerInfo {
        timestamp: 1441065600,
        protocol_version: 1,
        sequence_number: 0,
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    // deposit into backstop module
    backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_1, &400_000_0000000);
    backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_2, &200_000_0000000);

    let (pre_tokens_1, pre_shares_1, _pre_q4w_1) = backstop_client.p_balance(&pool_1);
    let (pre_tokens_2, pre_shares_2, _pre_q4w_2) = backstop_client.p_balance(&pool_2);

    assert_eq!(pre_tokens_1, 400_000_0000000);
    assert_eq!(pre_shares_1, 400_000_0000000);
    assert_eq!(pre_tokens_2, 200_000_0000000);
    assert_eq!(pre_shares_2, 200_000_0000000);

    backstop_client.add_reward(&pool_1, &BytesN::from_array(&e, &[0u8; 32]));
    backstop_client.add_reward(&pool_2, &BytesN::from_array(&e, &[0u8; 32]));

    // draw
    let result = backstop_client.with_source_account(&sauron).try_donate(
        &pool_2,
        &100_000_0000000,
        &pool_2_id,
    );

    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, BackstopError::NotAuthorized),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(5)),
        },
    }
}
