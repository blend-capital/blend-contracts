#![cfg(test)]

use emitter::Swap;
use pool::{Request, RequestType, ReserveEmissionMetadata};
use soroban_sdk::{
    testutils::{Address as _, Events},
    vec as svec, Address, IntoVal, String, Symbol, Vec as SVec,
};
use test_suites::{
    create_fixture_with_data,
    pool::default_reserve_metadata,
    test_fixture::{TestFixture, TokenIndex, SCALAR_7},
};

/// Test user exposed functions on the emitter for basic functionality, auth, and events.
/// Does not test internal state management of the emitter, only external effects.
#[test]
fn test_emitter_no_reward_zone() {
    let mut fixture = TestFixture::create(false);
    // mint whale tokens
    let frodo = fixture.users[0].clone();
    fixture.tokens[TokenIndex::STABLE].mint(&frodo, &(100_000 * 10i128.pow(6)));
    fixture.tokens[TokenIndex::XLM].mint(&frodo, &(1_000_000 * SCALAR_7));
    fixture.tokens[TokenIndex::WETH].mint(&frodo, &(100 * 10i128.pow(9)));

    // mint LP tokens with whale
    // frodo has 40m BLND from drop
    fixture.tokens[TokenIndex::BLND].mint(&frodo, &(70_000_000 * SCALAR_7));
    fixture.tokens[TokenIndex::USDC].mint(&frodo, &(2_600_000 * SCALAR_7));
    fixture.lp.join_pool(
        &(10_000_000 * SCALAR_7),
        &svec![&fixture.env, 110_000_000 * SCALAR_7, 2_600_000 * SCALAR_7,],
        &frodo,
    );

    // create pool
    fixture.create_pool(String::from_str(&fixture.env, "Teapot"), 0_1000000, 6);

    let mut stable_config = default_reserve_metadata();
    stable_config.decimals = 6;
    stable_config.c_factor = 0_900_0000;
    stable_config.l_factor = 0_950_0000;
    stable_config.util = 0_850_0000;
    fixture.create_pool_reserve(0, TokenIndex::STABLE, &stable_config);

    let mut xlm_config = default_reserve_metadata();
    xlm_config.c_factor = 0_750_0000;
    xlm_config.l_factor = 0_750_0000;
    xlm_config.util = 0_500_0000;
    fixture.create_pool_reserve(0, TokenIndex::XLM, &xlm_config);

    let mut weth_config = default_reserve_metadata();
    weth_config.decimals = 9;
    weth_config.c_factor = 0_800_0000;
    weth_config.l_factor = 0_800_0000;
    weth_config.util = 0_700_0000;
    fixture.create_pool_reserve(0, TokenIndex::WETH, &weth_config);

    // enable emissions for pool
    let pool_fixture = &fixture.pools[0];

    let reserve_emissions: soroban_sdk::Vec<ReserveEmissionMetadata> = soroban_sdk::vec![
        &fixture.env,
        ReserveEmissionMetadata {
            res_index: 1, // XLM
            res_type: 1,  // b_token
            share: 1_000_0000
        },
    ];
    pool_fixture.pool.set_emissions_config(&reserve_emissions);
    assert_eq!(
        0,
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address)
    );

    fixture.env.budget().reset_unlimited();
    let frodo = &fixture.users[0];
    let pool_fixture = &fixture.pools[0];
    let blnd_token = &fixture.tokens[TokenIndex::BLND];
    blnd_token.balance(&fixture.emitter.address);
    blnd_token.balance(&fixture.backstop.address);

    // Allow 6 days to pass and call distribute
    assert_eq!(blnd_token.balance(&fixture.backstop.address), 0);
    fixture.jump(6 * 24 * 60 * 60);
    let result = fixture.emitter.distribute();
    assert_eq!(result, (13 * 24 * 60 * 60) * SCALAR_7); // 1 token per second are emitted
    let result = fixture.backstop.try_gulp_emissions();
    assert!(result.is_err());

    assert_eq!(fixture.env.auths().len(), 0);
    assert_eq!(
        blnd_token.balance(&fixture.backstop.address),
        (13 * 24 * 60 * 60) * SCALAR_7
    ); // 1 token per second are emitted
    assert_eq!(fixture.env.auths().len(), 0);
    // Validate Emissions can't be claimed
    let result = pool_fixture.pool.claim(
        &fixture.users[0],
        &svec![&fixture.env, 0, 1, 2, 3],
        &fixture.users[0],
    );
    assert!(result == 0);
    let result = fixture.backstop.claim(
        &fixture.users[0],
        &svec![&fixture.env, pool_fixture.pool.address.clone()],
        &fixture.users[0],
    );
    assert!(result == 0);

    fixture.backstop.deposit(
        &fixture.users[0],
        &pool_fixture.pool.address,
        &(50_000 * SCALAR_7),
    );
    fixture.backstop.update_tkn_val();
    pool_fixture.pool.set_status(&3);
    pool_fixture.pool.update_status();

    let requests: SVec<Request> = svec![
        &fixture.env,
        Request {
            request_type: RequestType::SupplyCollateral as u32,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: 100_000 * SCALAR_7,
        },
    ];
    pool_fixture.pool.submit(&frodo, &frodo, &frodo, &requests);

    fixture
        .backstop
        .add_reward(&pool_fixture.pool.address, &Address::generate(&fixture.env));
    fixture.backstop.gulp_emissions();

    let result = pool_fixture.pool.gulp_emissions();
    assert_eq!(result, (13 * 24 * 60 * 60) * 300_0000);
    // Let some time go by
    fixture.jump(7 * 24 * 60 * 60);
    let pre_claim_balance = blnd_token.balance(&fixture.users[0]);
    let result = pool_fixture.pool.claim(
        &fixture.users[0],
        &svec![&fixture.env, 0, 1, 2, 3,],
        &fixture.users[0],
    );
    let post_claim_1_balance = blnd_token.balance(&fixture.users[0]);
    assert_eq!(post_claim_1_balance - pre_claim_balance, result);
    assert_eq!(result, (13 * 24 * 60 * 60) * 300_0000 - 400000); //pool claim is only 30% of the total emissions - subtracting 400000 for rounding
    let result_1 = fixture.backstop.claim(
        &fixture.users[0],
        &svec![&fixture.env, pool_fixture.pool.address.clone()],
        &fixture.users[0],
    );
    assert_eq!(result_1, (13 * 24 * 60 * 60) * 700_0000);
    assert_eq!(result_1 + result, (13 * 24 * 60 * 60) * SCALAR_7 - 400000);
}

/// Test user exposed functions on the emitter for basic functionality, auth, and events.
/// Does not test internal state management of the emitter, only external effects.
#[test]
fn test_emitter() {
    let fixture = create_fixture_with_data(false);

    let bstop_token = &fixture.lp;
    let blnd_token = &fixture.tokens[TokenIndex::BLND];

    let emitter_blnd_balance = blnd_token.balance(&fixture.emitter.address);
    let mut backstop_blnd_balance = blnd_token.balance(&fixture.backstop.address);

    // Verify initialization can't be re-run
    let result = fixture.emitter.try_initialize(
        &Address::generate(&fixture.env),
        &Address::generate(&fixture.env),
        &Address::generate(&fixture.env),
    );
    assert!(result.is_err());
    assert_eq!(
        fixture.emitter.get_backstop(),
        fixture.backstop.address.clone()
    );

    // Allow 6 days to pass and call distribute
    // @dev: 1h1m have passed since the emitter was deployed during setup
    fixture.jump(6 * 24 * 60 * 60);
    let result = fixture.emitter.distribute();
    backstop_blnd_balance += result;
    assert_eq!(fixture.env.auths().len(), 0);
    assert_eq!(result, (6 * 24 * 60 * 60 + 61 * 60) * SCALAR_7); // 1 token per second are emitted
    assert_eq!(
        blnd_token.balance(&fixture.emitter.address),
        emitter_blnd_balance
    );
    assert_eq!(
        blnd_token.balance(&fixture.backstop.address),
        backstop_blnd_balance
    );
    let event = svec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        svec![
            &fixture.env,
            (
                fixture.emitter.address.clone(),
                (Symbol::new(&fixture.env, "distribute"),).into_val(&fixture.env),
                svec![
                    &fixture.env,
                    fixture.backstop.address.to_val(),
                    result.into_val(&fixture.env)
                ]
                .into_val(&fixture.env)
            )
        ]
    );

    // Mint enough tokens to a new backstop address to perform a swap, then queue the swap
    let old_backstop_balance = bstop_token.balance(&fixture.backstop.address);
    let new_backstop = Address::generate(&fixture.env);
    fixture.tokens[TokenIndex::BLND].mint(&new_backstop, &(600_001 * SCALAR_7));
    fixture.tokens[TokenIndex::USDC].mint(&new_backstop, &(20_501 * SCALAR_7));
    fixture.lp.join_pool(
        &(old_backstop_balance + 1),
        &svec![&fixture.env, 505_001 * SCALAR_7, 13_501 * SCALAR_7],
        &new_backstop,
    );
    fixture
        .emitter
        .queue_swap_backstop(&new_backstop, &fixture.lp.address);
    let swap_unlock_time = fixture.env.ledger().timestamp() + 31 * 24 * 60 * 60;
    assert_eq!(fixture.env.auths().len(), 0);
    assert_eq!(
        fixture.emitter.get_backstop(),
        fixture.backstop.address.clone()
    );
    let event = svec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        svec![
            &fixture.env,
            (
                fixture.emitter.address.clone(),
                (Symbol::new(&fixture.env, "q_swap"),).into_val(&fixture.env),
                Swap {
                    new_backstop: new_backstop.clone(),
                    new_backstop_token: fixture.lp.address.clone(),
                    unlock_time: swap_unlock_time,
                }
                .into_val(&fixture.env)
            )
        ]
    );

    // Let some time go by
    fixture.jump(5 * 24 * 60 * 60);

    // Remove tokens from the new backstop and cancel the swap
    fixture.lp.transfer(&new_backstop, &fixture.bombadil, &5);
    fixture.emitter.cancel_swap_backstop();
    assert_eq!(fixture.env.auths().len(), 0);
    assert_eq!(
        fixture.emitter.get_backstop(),
        fixture.backstop.address.clone()
    );
    let event = svec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        svec![
            &fixture.env,
            (
                fixture.emitter.address.clone(),
                (Symbol::new(&fixture.env, "del_swap"),).into_val(&fixture.env),
                Swap {
                    new_backstop: new_backstop.clone(),
                    new_backstop_token: fixture.lp.address.clone(),
                    unlock_time: swap_unlock_time,
                }
                .into_val(&fixture.env)
            )
        ]
    );

    // Restart the swap, wait for it to unlock, then swap
    fixture.lp.transfer(&fixture.bombadil, &new_backstop, &5);
    fixture
        .emitter
        .queue_swap_backstop(&new_backstop, &fixture.lp.address);
    let swap_unlock_time = fixture.env.ledger().timestamp() + 31 * 24 * 60 * 60;
    fixture.jump(swap_unlock_time + 1);
    fixture.emitter.swap_backstop();
    let event = svec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        svec![
            &fixture.env,
            (
                fixture.emitter.address.clone(),
                (Symbol::new(&fixture.env, "swap"),).into_val(&fixture.env),
                Swap {
                    new_backstop: new_backstop.clone(),
                    new_backstop_token: fixture.lp.address.clone(),
                    unlock_time: swap_unlock_time,
                }
                .into_val(&fixture.env)
            )
        ]
    );
    assert_eq!(fixture.emitter.get_backstop(), new_backstop.clone());
}
