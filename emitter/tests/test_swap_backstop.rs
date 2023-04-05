#![cfg(test)]

use common::create_backstop;
use soroban_sdk::{testutils::Address as AddressTestTrait, Address, Env, Status};

mod common;
use crate::common::{create_token, create_wasm_emitter, generate_contract_id, EmitterError};

#[test]
fn test_swap_backstop() {
    let e = Env::default();

    let (backstop_id, backstop_client) = create_backstop(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    let backstop_token_id = generate_contract_id(&e);
    backstop_client.initialize(&backstop_token_id);

    let (new_backstop_id, new_backstop_client) = create_backstop(&e);
    let new_backstop = Address::from_contract_id(&e, &new_backstop_id);
    let new_backstop_token_id = generate_contract_id(&e);
    new_backstop_client.initialize(&new_backstop_token_id);

    let (emitter_id, emitter_client) = create_wasm_emitter(&e);
    let emitter = Address::from_contract_id(&e, &emitter_id);
    let (blend_id, blend_client) = create_token(&e, &emitter);
    emitter_client.initialize(&backstop, &backstop_id, &blend_id);

    // mint backstop blend
    blend_client.mint(&emitter, &backstop, &100);

    // mint new backstop blend - NOTE: we mint 104 here just to check we're dividing raw Blend balance by 4
    blend_client.mint(&emitter, &new_backstop, &104);

    let result = emitter_client.try_swap_bstop(&new_backstop, &new_backstop_id);

    match result {
        Ok(_) => {
            let emitter_bstop = emitter_client.get_bstop();
            assert_eq!(emitter_bstop, new_backstop);
        }
        Err(_) => assert!(false),
    }
}

#[test]
fn test_swap_backstop_fails_with_insufficient_blend() {
    let e = Env::default();

    let (backstop_id, backstop_client) = create_backstop(&e);
    let backstop = Address::from_contract_id(&e, &backstop_id);
    let backstop_token_id = generate_contract_id(&e);
    backstop_client.initialize(&backstop_token_id);

    let (new_backstop_id, new_backstop_client) = create_backstop(&e);
    let new_backstop = Address::from_contract_id(&e, &new_backstop_id);
    let new_backstop_token_id = generate_contract_id(&e);
    new_backstop_client.initialize(&new_backstop_token_id);

    let (emitter_id, emitter_client) = create_wasm_emitter(&e);
    let emitter = Address::from_contract_id(&e, &emitter_id);
    let (blend_id, blend_client) = create_token(&e, &emitter);
    emitter_client.initialize(&backstop, &backstop_id, &blend_id);

    // mint backstop blend
    blend_client.mint(&emitter, &backstop, &100);

    // mint new backstop blend - NOTE: we mint 103 here just to check we're dividing raw Blend balance by 4
    blend_client.mint(&emitter, &new_backstop, &103);

    let result = emitter_client.try_swap_bstop(&new_backstop, &new_backstop_id);

    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, EmitterError::InsufficientBLND),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(2)),
        },
    }
}
