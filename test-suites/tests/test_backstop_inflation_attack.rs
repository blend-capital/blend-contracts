#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Error, String};
use test_suites::{
    pool::default_reserve_metadata,
    test_fixture::{TestFixture, TokenIndex, SCALAR_7},
};

#[test]
fn test_backstop_inflation_attack() {
    let mut fixture = TestFixture::create(false);

    let whale = Address::generate(&fixture.env);
    let sauron = Address::generate(&fixture.env);
    let pippen = Address::generate(&fixture.env);

    // create pool with 1 new reserve
    fixture.create_pool(String::from_str(&fixture.env, "Teapot"), 0, 6);

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
    let sauron_deposit_amount = 100;
    let sauron_shares = fixture
        .backstop
        .deposit(&sauron, &pool_address, &sauron_deposit_amount);

    // 2. Attacker tries to send a large amount to the backstop before the victim can perform a deposit
    let inflation_amount = 10_000 * SCALAR_7;
    fixture
        .lp
        .transfer(&sauron, &pool_address, &inflation_amount);

    // contract correctly mints share amounts regardless of the token balance
    let deposit_amount = 100;
    let pippen_shares = fixture
        .backstop
        .deposit(&pippen, &pool_address, &deposit_amount);
    assert_eq!(pippen_shares, 100);
    assert_eq!(sauron_shares, pippen_shares);

    // 2b. Attacker tries to donate a large amount to the backstop before the victim can perform a deposit
    //    #! NOTE - Contract will stop a random address from donating. This can ONLY come from the pool.
    //              However, authorizations are mocked during intergation tests, so this will succeed.
    fixture
        .backstop
        .donate(&sauron, &pool_address, &inflation_amount);

    // contracts stop any zero share deposits
    let bad_deposit_result = fixture
        .backstop
        .try_deposit(&pippen, &pool_address, &deposit_amount);
    assert_eq!(
        bad_deposit_result.err(),
        Some(Ok(Error::from_contract_error(1005)))
    );
}
