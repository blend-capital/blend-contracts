#![cfg(test)]
use std::i64::MAX;

use soroban_auth::{Identifier, Signature};
use soroban_sdk::{
    testutils::{Accounts, Ledger, LedgerInfo},
    BigInt, Env, Status,
};

mod common;
use crate::common::{create_token, create_wasm_emitter, generate_contract_id, EmitterError};

#[test]
fn test_distribute_from_backstop() {
    let e = Env::default();
    e.ledger().set(LedgerInfo {
        timestamp: 100,
        protocol_version: 1,
        sequence_number: 10,
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    let bilbo = e.accounts().generate_and_create();
    let bilbo_id = Identifier::Account(bilbo.clone());

    //Note: this is the backstop module contract, it should be a contract, but atm it isn't easy to
    //test calling a function as a contract, so we just pretend the backstop module is an account for now
    // TODO: make this a contract when possible
    let ring = e.accounts().generate_and_create();
    let ring_id = Identifier::Account(ring.clone());

    let (blend_id, blend_client) = create_token(&e, &bilbo_id);

    let blend_lp = generate_contract_id(&e);
    let blend_lp_id = Identifier::Contract(blend_lp.clone());

    let (emitter, emitter_client) = create_wasm_emitter(&e);
    let emitter_id = Identifier::Contract(emitter.clone());
    emitter_client.initialize(&ring_id, &blend_id, &blend_lp_id);

    //Mint Bilbo some Blend
    blend_client.with_source_account(&bilbo).mint(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &bilbo_id,
        &BigInt::from_i64(&e, MAX),
    );
    //Transfer Blend to Emitter
    blend_client.with_source_account(&bilbo).xfer(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &emitter_id,
        &BigInt::from_i64(&e, MAX),
    );
    //pass some time
    let seconds_passed = 10000;
    e.ledger().set(LedgerInfo {
        timestamp: 100 + seconds_passed,
        protocol_version: 1,
        sequence_number: 10,
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    let result = emitter_client.with_source_account(&ring).distribute();

    let expected_emissions = BigInt::from_u64(&e, seconds_passed * 1_0000000);

    assert_eq!(result, expected_emissions);
    assert_eq!(blend_client.balance(&ring_id), expected_emissions);
}

#[test]
fn test_distribute_from_non_backstop_panics() {
    let e = Env::default();
    e.ledger().set(LedgerInfo {
        timestamp: 100,
        protocol_version: 1,
        sequence_number: 10,
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    let bilbo = e.accounts().generate_and_create();
    let bilbo_id = Identifier::Account(bilbo.clone());

    //Note: this is the backstop module contract, it should be a contract, but atm it isn't easy to
    //test calling a function as a contract, so we just pretend the backstop module is an account for now
    // TODO: make this a contract when possible
    let ring = e.accounts().generate_and_create();
    let ring_id = Identifier::Account(ring.clone());

    let (blend_id, blend_client) = create_token(&e, &bilbo_id);

    let blend_lp = generate_contract_id(&e);
    let blend_lp_id = Identifier::Contract(blend_lp.clone());

    let (emitter, emitter_client) = create_wasm_emitter(&e);
    let emitter_id = Identifier::Contract(emitter.clone());
    emitter_client.initialize(&ring_id, &blend_id, &blend_lp_id);

    //Mint Bilbo some Blend
    blend_client.with_source_account(&bilbo).mint(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &bilbo_id,
        &BigInt::from_i64(&e, MAX),
    );
    //Transfer Blend to Emitter
    blend_client.with_source_account(&bilbo).xfer(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &emitter_id,
        &BigInt::from_i64(&e, MAX),
    );
    //pass some time
    let seconds_passed = 10000;
    e.ledger().set(LedgerInfo {
        timestamp: 100 + seconds_passed,
        protocol_version: 1,
        sequence_number: 10,
        network_passphrase: Default::default(),
        base_reserve: 10,
    });

    let result = emitter_client.with_source_account(&bilbo).try_distribute();

    match result {
        Ok(_) => {
            assert!(false);
        }
        Err(error) => match error {
            Ok(p_error) => assert_eq!(p_error, EmitterError::NotAuthorized),
            Err(s_error) => assert_eq!(s_error, Status::from_contract_error(1)),
        },
    }
}
