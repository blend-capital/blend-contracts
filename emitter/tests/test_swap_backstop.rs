#![cfg(test)]

use common::create_backstop;
use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    Address, Env,
};

mod common;
use crate::common::{create_token, create_wasm_emitter, EmitterError};

#[test]
fn test_swap_backstop() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().set(LedgerInfo {
        timestamp: 10000000,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let bombadil = Address::random(&e);

    let (emitter_address, emitter_client) = create_wasm_emitter(&e);
    let (blnd_address, blnd_client) = create_token(&e, &emitter_address);
    let (backstop_token_address, backstop_token_client) = create_token(&e, &bombadil);

    let (backstop_address, backstop_client) = create_backstop(&e);
    backstop_client.initialize(&backstop_token_address, &blnd_address, &Address::random(&e));

    let (new_backstop_address, new_backstop_client) = create_backstop(&e);
    new_backstop_client.initialize(&backstop_token_address, &blnd_address, &Address::random(&e));

    emitter_client.initialize(&backstop_address, &blnd_address);

    blnd_client.mint(&backstop_address, &500_000_0000000);
    backstop_token_client.mint(&backstop_address, &123_1234567);

    backstop_token_client.mint(&new_backstop_address, &123_1234567);

    // verify swaps fail if balance is at most equal
    let result = emitter_client.try_swap_backstop(&new_backstop_address);
    match result {
        Ok(_) => assert!(false),
        Err(err) => assert_eq!(err, Ok(EmitterError::InsufficientBackstopSize)),
    }

    // mint an additional stroop and verify swap succeeds
    backstop_token_client.mint(&new_backstop_address, &1);
    emitter_client.swap_backstop(&new_backstop_address);
    assert_eq!(e.auths(), []);

    assert_eq!(emitter_client.get_backstop(), new_backstop_address);
}
