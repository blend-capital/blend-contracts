#![cfg(test)]

use soroban_sdk::{testutils::Address as AddressTestTrait, Address, Env, Status};

mod common;
use crate::common::{create_token, create_wasm_emitter, generate_contract_id, EmitterError};

#[test]
fn test_swap_backstop() {
    let e = Env::default();

    let backstop = Address::random(&e);
    let new_backstop = Address::random(&e);

    let blend_lp = generate_contract_id(&e);

    let (emitter_bytes, emitter_client) = create_wasm_emitter(&e);
    let emitter = Address::from_contract_id(&e, &emitter_bytes);
    let (blend_id, blend_client) = create_token(&e, &emitter);
    emitter_client.initialize(&backstop, &blend_id, &blend_lp);

    // mint backstop blend
    blend_client.mint(&emitter, &backstop, &100);

    // mint new backstop blend - NOTE: we mint 104 here just to check we're dividing raw Blend balance by 4
    blend_client.mint(&emitter, &new_backstop, &104);

    let result = emitter_client.try_swap_bstop(&new_backstop);

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

    let backstop = Address::random(&e);
    let new_backstop = Address::random(&e);

    let blend_lp = generate_contract_id(&e);

    let (emitter_bytes, emitter_client) = create_wasm_emitter(&e);
    let emitter = Address::from_contract_id(&e, &emitter_bytes);
    let (blend_id, blend_client) = create_token(&e, &emitter);
    emitter_client.initialize(&backstop, &blend_id, &blend_lp);

    // mint backstop blend
    blend_client.mint(&emitter, &backstop, &100);

    // mint new backstop blend - NOTE: we mint 103 here just to check we're dividing raw Blend balance by 4
    blend_client.mint(&emitter, &new_backstop, &103);

    let result = emitter_client.try_swap_bstop(&new_backstop);

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
