#![cfg(test)]

use pool::{Request, RequestType};
use soroban_sdk::{testutils::Address as _, vec, Address, Error, Symbol};
use test_suites::{
    pool::default_reserve_metadata,
    test_fixture::{TestFixture, TokenIndex, SCALAR_7},
};

#[test]
fn test_pool_inflation_attack_0_mint() {
    let mut fixture = TestFixture::create(false);

    let whale = Address::generate(&fixture.env);
    let sauron = Address::generate(&fixture.env);
    let pippen = Address::generate(&fixture.env);

    // create pool with 1 new reserve
    fixture.create_pool(Symbol::new(&fixture.env, "Teapot"), 0, 6);

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
    //    to force the victim's minted shares to be zero.
    // !! Contract blocks a submission that results in zero b_token mint to stop exploit
    fixture.tokens[TokenIndex::XLM].transfer(
        &sauron,
        &fixture.pools[0].pool.address,
        &(100 * SCALAR_7),
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
    let result = fixture.pools[0]
        .pool
        .try_submit(&pippen, &pippen, &pippen, &requests);
    assert_eq!(result.err(), Some(Ok(Error::from_contract_error(1216))));
}
