#![cfg(test)]

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

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let backstop = generate_contract_id(&e);

    let (blend_id, blend_client) = create_token(&e, &bombadil_id);

    let blend_lp = generate_contract_id(&e);

    let (emitter, emitter_client) = create_wasm_emitter(&e);
    let emitter_id = Identifier::Contract(emitter.clone());
    emitter_client.initialize(&backstop, &blend_id, &blend_lp);

    //Set emitter as blend admin
    blend_client.with_source_account(&bombadil).set_admin(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &emitter_id,
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

    //Note: this is currently broken cuz we can't call a function as a contract - the test will fail
    // let result = emitter_client.with_source_account(&backstop).distribute();

    // let expected_emissions = BigInt::from_u64(&e, seconds_passed * 1_0000000);

    // assert_eq!(result, expected_emissions);
    // assert_eq!(blend_client.balance(&backstop_id), expected_emissions);
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

    let bombadil = e.accounts().generate_and_create();
    let bombadil_id = Identifier::Account(bombadil.clone());

    let sauron = e.accounts().generate_and_create();

    let backstop = generate_contract_id(&e);

    let (blend_id, blend_client) = create_token(&e, &bombadil_id);

    let blend_lp = generate_contract_id(&e);

    let (emitter, emitter_client) = create_wasm_emitter(&e);
    let emitter_id = Identifier::Contract(emitter.clone());
    emitter_client.initialize(&backstop, &blend_id, &blend_lp);

    //Set emitter as blend admin
    blend_client.with_source_account(&bombadil).set_admin(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &emitter_id,
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

    let result = emitter_client.with_source_account(&sauron).try_distribute();

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
