#![cfg(test)]

use pool::{Request, RequestType};
use soroban_sdk::{testutils::Address as _, vec, Address, String};
use test_suites::{
    pool::default_reserve_metadata,
    test_fixture::{TestFixture, TokenIndex, SCALAR_7},
};

#[test]
fn test_pool_inflation_attack() {
    let mut fixture = TestFixture::create(false);

    let whale = Address::generate(&fixture.env);
    let sauron = Address::generate(&fixture.env);
    let pippen = Address::generate(&fixture.env);

    // create pool with 1 new reserve
    fixture.create_pool(String::from_str(&fixture.env, "Teapot"), 0, 6);

    let xlm_config = default_reserve_metadata();
    fixture.create_pool_reserve(0, TokenIndex::XLM, &xlm_config);

    // setup backstop and update pool status
    fixture.tokens[TokenIndex::BLND].mint(&whale, &(500_100 * SCALAR_7));
    fixture.tokens[TokenIndex::USDC].mint(&whale, &(12_600 * SCALAR_7));
    fixture.lp.join_pool(
        &(50_000 * SCALAR_7),
        &vec![&fixture.env, 500_100 * SCALAR_7, 12_600 * SCALAR_7],
        &whale,
    );
    fixture
        .backstop
        .deposit(&whale, &fixture.pools[0].pool.address, &(50_000 * SCALAR_7));
    fixture.backstop.update_tkn_val();
    fixture.pools[0].pool.set_status(&0);
    fixture.jump_with_sequence(60);

    // execute inflation attack against pippen
    let starting_balance = 1_000_000 * SCALAR_7;
    fixture.tokens[TokenIndex::XLM].mint(&sauron, &starting_balance);
    fixture.tokens[TokenIndex::XLM].mint(&pippen, &starting_balance);

    // 1. Attacker deposits a single stroop as the initial depositor
    let requests = vec![
        &fixture.env,
        Request {
            request_type: RequestType::Supply as u32,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: 1,
        },
    ];
    fixture.pools[0]
        .pool
        .submit(&sauron, &sauron, &sauron, &requests);

    // skip a ledger to force pool to refresh reserve data
    fixture.jump_with_sequence(5);

    // 2. Attacker frontruns victim's deposit by depositing a large amount of underlying
    //    to try and force an error in minting B tokens
    let inflation_amount = 100 * SCALAR_7;
    fixture.tokens[TokenIndex::XLM].transfer(
        &sauron,
        &fixture.pools[0].pool.address,
        &inflation_amount,
    );

    let attack_amount = 42 * SCALAR_7;
    let requests = vec![
        &fixture.env,
        Request {
            request_type: RequestType::Supply as u32,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: attack_amount,
        },
    ];
    fixture.pools[0]
        .pool
        .submit(&pippen, &pippen, &pippen, &requests);

    // skip a ledger to force pool to refresh reserve data
    fixture.jump_with_sequence(5);

    // 3. Attacker withdraws all funds and victim withdraws all funds
    let requests = vec![
        &fixture.env,
        Request {
            request_type: RequestType::Withdraw as u32,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: attack_amount + inflation_amount,
        },
    ];
    fixture.pools[0]
        .pool
        .submit(&sauron, &sauron, &sauron, &requests);

    let requests = vec![
        &fixture.env,
        Request {
            request_type: RequestType::Withdraw as u32,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: attack_amount + inflation_amount,
        },
    ];
    fixture.pools[0]
        .pool
        .submit(&pippen, &pippen, &pippen, &requests);

    // Verify the attack was unnsuccessul and victim did not lose their funds
    assert_eq!(
        fixture.tokens[TokenIndex::XLM].balance(&pippen),
        starting_balance
    );
    assert_eq!(
        fixture.tokens[TokenIndex::XLM].balance(&sauron),
        starting_balance - inflation_amount
    );
    assert_eq!(
        fixture.tokens[TokenIndex::XLM].balance(&fixture.pools[0].pool.address),
        inflation_amount
    );
}
