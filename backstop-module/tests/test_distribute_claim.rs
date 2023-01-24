#![cfg(test)]
use cast::i128;
use common::generate_contract_id;
use soroban_auth::{Identifier, Signature};
use soroban_sdk::{
    testutils::{Accounts, Ledger, LedgerInfo},
    BytesN, Env,
};

mod common;
use crate::common::{
    create_backstop_module, create_mock_pool_factory, create_token_from_id, BackstopError,
};

#[test]
fn test_backstop_distribution_and_claim_happy_path() {
    let e = Env::default();

    let (backstop_addr, backstop_client) = create_backstop_module(&e);
    let backstop_id = Identifier::Contract(backstop_addr.clone());

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let token_id = BytesN::from_array(&e, &[222; 32]);
    let token_client = create_token_from_id(&e, &token_id, &bombadil_id);

    let pool_1 = generate_contract_id(&e); // in reward zone
    let pool_2 = generate_contract_id(&e); // in reward zone
    let pool_3 = generate_contract_id(&e); // out of reward zone

    let mock_pool_factory = create_mock_pool_factory(&e);
    mock_pool_factory.set_pool(&pool_1);
    mock_pool_factory.set_pool(&pool_2);
    mock_pool_factory.set_pool(&pool_3);

    let samwise = e.accounts().generate_and_create();
    let samwise_id = Identifier::Account(samwise.clone());

    token_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &samwise_id,
        &600_000_0000000,
    );
    token_client.with_source_account(&samwise).incr_allow(
        &Signature::Invoker,
        &0,
        &backstop_id,
        &i128(u64::MAX),
    );

    e.ledger().set(LedgerInfo {
        timestamp: 1441065600,
        protocol_version: 1,
        sequence_number: 0,
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_1, &300_000_0000000);
    backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_2, &200_000_0000000);
    backstop_client
        .with_source_account(&samwise)
        .deposit(&pool_3, &100_000_0000000);

    let (pre_tokens_1, pre_shares_1, _pre_q4w_1) = backstop_client.p_balance(&pool_1);
    let (pre_tokens_2, pre_shares_2, _pre_q4w_2) = backstop_client.p_balance(&pool_2);
    let (pre_tokens_3, pre_shares_3, _pre_q4w_3) = backstop_client.p_balance(&pool_3);

    assert_eq!(pre_tokens_1, 300_000_0000000);
    assert_eq!(pre_shares_1, 300_000_0000000);
    assert_eq!(pre_tokens_2, 200_000_0000000);
    assert_eq!(pre_shares_2, 200_000_0000000);
    assert_eq!(pre_tokens_3, 100_000_0000000);
    assert_eq!(pre_shares_3, 100_000_0000000);

    backstop_client.add_reward(&pool_1, &BytesN::from_array(&e, &[0u8; 32]));
    backstop_client.add_reward(&pool_2, &BytesN::from_array(&e, &[0u8; 32]));

    // distribute
    backstop_client.dist();

    let (post_tokens_1, post_shares_1, _post_q4w_1) = backstop_client.p_balance(&pool_1);
    let (post_tokens_2, post_shares_2, _post_q4w_2) = backstop_client.p_balance(&pool_2);
    let (post_tokens_3, post_shares_3, _post_q4w_3) = backstop_client.p_balance(&pool_3);

    assert_eq!(post_tokens_1, 300_000_0000000 + 210_000_0000000);
    assert_eq!(post_shares_1, 300_000_0000000);
    assert_eq!(post_tokens_2, 200_000_0000000 + 140_000_0000000);
    assert_eq!(post_shares_2, 200_000_0000000);
    assert_eq!(post_tokens_3, 100_000_0000000);
    assert_eq!(post_shares_3, 100_000_0000000);

    assert_eq!(backstop_client.pool_eps(&pool_1), 0_1800000);
    assert_eq!(backstop_client.pool_eps(&pool_2), 0_1200000);
    assert_eq!(backstop_client.pool_eps(&pool_3), 0);

    // claim
    e.as_contract(&pool_1, || {
        backstop_client.claim(&samwise_id, &50_000_0000000);
    });

    assert_eq!(token_client.balance(&samwise_id), 50_000_0000000);

    // verify claim doesn't break with an invalid contract
    e.as_contract(&generate_contract_id(&e), || {
        let result = backstop_client.try_claim(&samwise_id, &50_000_0000000);

        match result {
            Ok(_) => assert!(false),
            Err(error) => match error {
                Ok(p_error) => assert_eq!(p_error, BackstopError::NotPool),
                Err(_) => assert!(false),
            },
        }
    });
}
