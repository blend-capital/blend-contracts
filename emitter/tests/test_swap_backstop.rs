#![cfg(test)]

use soroban_auth::{Identifier, Signature};
use soroban_sdk::{testutils::Accounts, Env, Status};

mod common;
use crate::common::{create_token, create_wasm_emitter, generate_contract_id, EmitterError};

#[test]
fn test_swap_backstop() {
    let e = Env::default();

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let backstop = generate_contract_id(&e);
    let backstop_id = Identifier::Contract(backstop.clone());

    let new_backstop = generate_contract_id(&e);
    let new_backstop_id = Identifier::Contract(new_backstop.clone());

    let (blend_id, blend_client) = create_token(&e, &bombadil_id);

    let blend_lp = generate_contract_id(&e);

    let (emitter, emitter_client) = create_wasm_emitter(&e);
    let emitter_id = Identifier::Contract(emitter.clone());
    emitter_client.initialize(&backstop, &blend_id, &blend_lp);

    // Mint Backstop Blend
    blend_client
        .with_source_account(&bombadil)
        .mint(&Signature::Invoker, &0, &backstop_id, &100);
    // Mint new Backstop Blend - Note: we mint 104 here just to check we're dividing raw Blend balance by 4
    blend_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &new_backstop_id,
        &104,
    );

    // Set emitter as blend admin
    blend_client
        .with_source_account(&bombadil)
        .set_admin(&Signature::Invoker, &0, &emitter_id);

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

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let backstop = generate_contract_id(&e);
    let backstop_id = Identifier::Contract(backstop.clone());

    let new_backstop = generate_contract_id(&e);
    let new_backstop_id = Identifier::Contract(new_backstop.clone());

    let (blend_id, blend_client) = create_token(&e, &bombadil_id);

    let blend_lp = generate_contract_id(&e);

    let (emitter, emitter_client) = create_wasm_emitter(&e);
    let emitter_id = Identifier::Contract(emitter.clone());
    emitter_client.initialize(&backstop, &blend_id, &blend_lp);
    //Mint Blend to Backstop
    blend_client
        .with_source_account(&bombadil)
        .mint(&Signature::Invoker, &0, &backstop_id, &100);
    //Mint Blend to new backstop - Note: we mint 103 here just to check we're dividing raw Blend balance by 4
    blend_client.with_source_account(&bombadil).mint(
        &Signature::Invoker,
        &0,
        &new_backstop_id,
        &103,
    );
    //Set emitter as Blend admin
    blend_client
        .with_source_account(&bombadil)
        .set_admin(&Signature::Invoker, &0, &emitter_id);

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
