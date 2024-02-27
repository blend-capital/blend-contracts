#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Error, Symbol};
use test_suites::{
    pool::default_reserve_metadata,
    test_fixture::{TestFixture, TokenIndex, SCALAR_7},
};

// This test showcases an inflation attack during a backstops initialization stage. This is a 
// high risk inflation attack for the attacker, and requires significant capital to execute. 
// If the donation does not happen before the victim deposits, the attacker will lose the majority
// of the capital they donated to the backstop.
//
// However, since the attack is feasible, this test exists to ensure it's effects are minimized
// and common pitfalls are avoided.
#[test]
fn test_backstop_inflation_attack() {
    let mut fixture = TestFixture::create(false);

    let whale = Address::generate(&fixture.env);
    let sauron = Address::generate(&fixture.env);
    let pippen = Address::generate(&fixture.env);

    // create pool with 1 new reserve
    fixture.create_pool(Symbol::new(&fixture.env, "Teapot"), 0, 6);

    let xlm_config = default_reserve_metadata();
    fixture.create_pool_reserve(0, TokenIndex::XLM, &xlm_config);
    let pool_address = fixture.pools[0].pool.address.clone();

    // setup backstop and update pool status
    fixture.tokens[TokenIndex::BLND].mint(&whale, &(5_001_000 * SCALAR_7));
    fixture.tokens[TokenIndex::USDC].mint(&whale, &(121_000 * SCALAR_7));
    fixture.lp.join_pool(
        &(400_000 * SCALAR_7),
        &vec![&fixture.env, 5_001_000 * SCALAR_7, 121_000 * SCALAR_7],
        &whale,
    );

    // execute inflation attack against pippen
    let starting_balance = 200_000 * SCALAR_7;
    fixture.lp.transfer(&whale, &sauron, &starting_balance);
    fixture.lp.transfer(&whale, &pippen, &starting_balance);

    // 1. Attacker deposits a small amount as the initial depositor

    // contracts stop very small initial deposits
    let bad_init_deposit =
        fixture
            .backstop
            .try_deposit(&sauron, &pool_address, &(SCALAR_7 / 10 - 1));
    assert_eq!(
        bad_init_deposit.err(),
        Some(Ok(Error::from_contract_error(1005)))
    );

    let sauron_deposit_amount = SCALAR_7 / 10;
    let sauron_shares = fixture
        .backstop
        .deposit(&sauron, &pool_address, &sauron_deposit_amount);

    // 2. Attacker donates a large amount to the backstop before the victim can perform a deposit
    let inflation_amount = 100_000 * SCALAR_7;
    fixture
        .backstop
        .donate(&sauron, &pool_address, &inflation_amount);

    // contracts stop any zero share deposits
    let mut deposit_amount = 1000;
    let bad_deposit_result = fixture
        .backstop
        .try_deposit(&pippen, &pool_address, &deposit_amount);
    assert_eq!(
        bad_deposit_result.err(),
        Some(Ok(Error::from_contract_error(1005)))
    );

    // user can still be in a situation where they get adversely affected by the inflation attacks
    // but to a small extent
    deposit_amount = SCALAR_7;
    let pippen_shares = fixture
        .backstop
        .deposit(&pippen, &pool_address, &deposit_amount);
    assert_eq!(pippen_shares, 9); // actual is 9.99...

    // 3. Attacker and victim withdraw funds
    fixture
        .backstop
        .queue_withdrawal(&sauron, &pool_address, &sauron_shares);
    fixture
        .backstop
        .queue_withdrawal(&pippen, &pool_address, &pippen_shares);

    // wait enough time so all shares can be withdrawn
    fixture.jump(21 * 24 * 60 * 60 + 1);

    fixture
        .backstop
        .withdraw(&sauron, &pool_address, &sauron_shares);
    fixture
        .backstop
        .withdraw(&pippen, &pool_address, &pippen_shares);

    // pippen loses less than 10% of initial deposit due to rounding
    assert!(fixture.lp.balance(&sauron) < starting_balance + SCALAR_7 / 10);
    assert!(fixture.lp.balance(&pippen) > starting_balance - SCALAR_7 / 10);
}
