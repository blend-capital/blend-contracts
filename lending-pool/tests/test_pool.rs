#![cfg(test)]
use soroban_sdk::{BigInt, Env, testutils::Accounts};
use soroban_auth::{Identifier, Signature};

mod common;
use crate::common::{create_wasm_lending_pool, generate_contract_id, pool_helper, token};

// TODO: Investigate if mint / burn semantics will be better (operate in bTokens)
#[test]
fn test_pool_happy_path() {
    let e = Env::default();

    let admin = e.accounts().generate_and_create();
    let admin_id = Identifier::Account(admin.clone());

    let user1 = e.accounts().generate_and_create();
    let user1_id = Identifier::Account(user1.clone());

    let pool = generate_contract_id(&e);
    let pool_id = Identifier::Contract(pool.clone());
    let pool_client = create_wasm_lending_pool(&e, &pool);
    pool_client.initialize(&admin_id, &generate_contract_id(&e));

    let (asset1_id, btoken1_id, dtoken1_id) = pool_helper::setup_reserve(&e, &pool_id, &pool_client, &admin);
    let asset1_client = token::Client::new(&e, &asset1_id);
    let btoken1_id_client = token::Client::new(&e, &btoken1_id);
    let dtoken1_id_client = token::Client::new(&e, &dtoken1_id);

    let supply_amount = BigInt::from_u64(&e, 11000000);
    asset1_client.with_source_account(&admin).mint(
        &Signature::Invoker,
        &BigInt::zero(&e),
        &user1_id,
        &supply_amount,
    );
    asset1_client.with_source_account(&user1).approve(
        &Signature::Invoker, 
        &BigInt::zero(&e), 
        &pool_id, 
        &BigInt::from_u64(&e, u64::MAX)
    );
    assert_eq!(asset1_client.balance(&user1_id), supply_amount);

    // supply
    let minted_btokens = pool_client.with_source_account(&user1).supply(&asset1_id, &supply_amount);

    assert_eq!(asset1_client.balance(&user1_id), BigInt::zero(&e));
    assert_eq!(asset1_client.balance(&pool_id), supply_amount);
    assert_eq!(btoken1_id_client.balance(&user1_id), minted_btokens);
    assert_eq!(minted_btokens, 1_0000000); // TODO: Update once actual rates are a thing
    println!("supply successful");

    // borrow
    let borrow_amount = BigInt::from_u64(&e, 0_6000000);
    let minted_dtokens = pool_client.with_source_account(&user1).borrow(&asset1_id, &borrow_amount, &user1_id);

    assert_eq!(asset1_client.balance(&user1_id), borrow_amount);
    assert_eq!(asset1_client.balance(&pool_id), supply_amount.clone() - borrow_amount.clone());
    assert_eq!(btoken1_id_client.balance(&user1_id), minted_btokens);
    assert_eq!(dtoken1_id_client.balance(&user1_id), minted_dtokens);
    assert_eq!(minted_dtokens, 0_5000000); // TODO: Update once actual rates are a thing
    println!("borrow successful");

    // repay
    let burnt_dtokens = pool_client.with_source_account(&user1).repay(&asset1_id, &borrow_amount, &user1_id);

    assert_eq!(asset1_client.balance(&user1_id), BigInt::zero(&e));
    assert_eq!(asset1_client.balance(&pool_id), supply_amount);
    assert_eq!(btoken1_id_client.balance(&user1_id), minted_btokens);
    assert_eq!(dtoken1_id_client.balance(&user1_id), BigInt::zero(&e));
    assert_eq!(burnt_dtokens, minted_dtokens);
    println!("repay successful");

    // withdraw
    let burnt_btokens = pool_client.with_source_account(&user1).withdraw(&asset1_id, &supply_amount, &user1_id);

    assert_eq!(asset1_client.balance(&user1_id), supply_amount);
    assert_eq!(asset1_client.balance(&pool_id), BigInt::zero(&e));
    assert_eq!(btoken1_id_client.balance(&user1_id), BigInt::zero(&e));
    assert_eq!(burnt_btokens, minted_btokens);
}