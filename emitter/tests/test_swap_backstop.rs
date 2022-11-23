#![cfg(test)]
use std::i64::MAX;

use soroban_auth::{Identifier, Signature};
use soroban_sdk::{testutils::Accounts, BigInt, Env, Status};

mod common;
use crate::common::{create_token, create_wasm_emitter, generate_contract_id, EmitterError};

#[test]
fn test_swap_backstop() {
    let e = Env::default();

    let bilbo = e.accounts().generate_and_create();
    let bilbo_id = Identifier::Account(bilbo.clone());

    let ring = generate_contract_id(&e);
    let ring_id = Identifier::Contract(ring.clone());

    let new_ring = generate_contract_id(&e);
    let new_ring_id = Identifier::Contract(new_ring.clone());

    let (blend_id, blend_client) = create_token(&e, &bilbo_id);

    let blend_lp = generate_contract_id(&e);
    let blend_lp_id = Identifier::Contract(blend_lp.clone());

    let (emitter, emitter_client) = create_wasm_emitter(&e);
    emitter_client.initialize(&ring_id, &blend_id, &blend_lp_id);

    //Mint Bilbo some Blend
    blend_client.with_source_account(&bilbo).mint(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &bilbo_id,
        &BigInt::from_i64(&e, MAX),
    );
    //Transfer Blend to Ring
    blend_client.with_source_account(&bilbo).xfer(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &ring_id,
        &BigInt::from_i64(&e, 100),
    );
    //Transfer Blend to New Ring - Note: we transfer 104 here just to check we're dividing raw Blend balance by 4
    blend_client.with_source_account(&bilbo).xfer(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &new_ring_id,
        &BigInt::from_i64(&e, 104),
    );

    let result = emitter_client.try_swap_bstop(&new_ring_id);

    match result {
        Ok(_) => assert!(true),
        Err(_) => assert!(false),
    }
}

#[test]
fn test_swap_backstop_fails_with_insufficient_blend() {
    let e = Env::default();

    let bilbo = e.accounts().generate_and_create();
    let bilbo_id = Identifier::Account(bilbo.clone());

    let ring = generate_contract_id(&e);
    let ring_id = Identifier::Contract(ring.clone());

    let new_ring = generate_contract_id(&e);
    let new_ring_id = Identifier::Contract(new_ring.clone());

    let (blend_id, blend_client) = create_token(&e, &bilbo_id);

    let blend_lp = generate_contract_id(&e);
    let blend_lp_id = Identifier::Contract(blend_lp.clone());

    let (emitter, emitter_client) = create_wasm_emitter(&e);
    emitter_client.initialize(&ring_id, &blend_id, &blend_lp_id);

    //Mint Bilbo some Blend
    blend_client.with_source_account(&bilbo).mint(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &bilbo_id,
        &BigInt::from_i64(&e, MAX),
    );
    //Transfer Blend to Ring
    blend_client.with_source_account(&bilbo).xfer(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &ring_id,
        &BigInt::from_i64(&e, 100),
    );
    //Transfer Blend to New Ring - Note: we transfer 103 here just to check we're dividing raw Blend balance by 4
    blend_client.with_source_account(&bilbo).xfer(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &new_ring_id,
        &BigInt::from_i64(&e, 103),
    );

    let result = emitter_client.try_swap_bstop(&new_ring_id);

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
