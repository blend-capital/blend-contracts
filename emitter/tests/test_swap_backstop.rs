#![cfg(test)]

use common::create_backstop;
use soroban_sdk::{
    testutils::{Address as _, BytesN as _, Ledger, LedgerInfo},
    Address, BytesN, Env,
};

mod common;
use crate::common::{create_token, create_wasm_emitter, EmitterError};

#[test]
fn test_swap_backstop() {
    let e = Env::default();

    e.ledger().set(LedgerInfo {
        timestamp: 10000000,
        protocol_version: 1,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
    });

    let bombadil = Address::random(&e);

    let (emitter_id, emitter_client) = create_wasm_emitter(&e);
    let emitter = Address::from_contract_id(&e, &emitter_id);
    let (blnd_id, blnd_client) = create_token(&e, &emitter);
    let (backstop_token_id, backstop_token_client) = create_token(&e, &bombadil);

    let (backstop_id, backstop_client) = create_backstop(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    backstop_client.initialize(&backstop_token_id, &blnd_id, &BytesN::<32>::random(&e));

    let (new_backstop_id, new_backstop_client) = create_backstop(&e);
    let new_backstop = Address::from_contract_id(&e, &new_backstop_id);
    new_backstop_client.initialize(&backstop_token_id, &blnd_id, &BytesN::<32>::random(&e));

    emitter_client.initialize(&backstop_id, &blnd_id);

    blnd_client.mint(&emitter, &backstop, &500_000_0000000);
    backstop_token_client.mint(&bombadil, &backstop, &123_1234567);

    backstop_token_client.mint(&bombadil, &new_backstop, &123_1234567);

    // verify swaps fail if balance is at most equal
    let result = emitter_client.try_swap_bstop(&new_backstop_id);
    match result {
        Ok(_) => assert!(false),
        Err(err) => assert_eq!(err, Ok(EmitterError::InsufficientBackstopSize)),
    }

    // mint an additional stroop and verify swap succeeds
    backstop_token_client.mint(&bombadil, &new_backstop, &1);
    emitter_client.swap_bstop(&new_backstop_id);
    assert_eq!(e.recorded_top_authorizations(), []);

    assert_eq!(emitter_client.get_bstop(), new_backstop_id);
}
