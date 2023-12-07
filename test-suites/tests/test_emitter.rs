#![cfg(test)]

use emitter::Swap;
use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, IntoVal, Symbol,
};
use test_suites::{
    create_fixture_with_data,
    test_fixture::{TokenIndex, SCALAR_7},
};

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
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.emitter.address.clone(),
                (Symbol::new(&fixture.env, "distribute"),).into_val(&fixture.env),
                vec![
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
        &vec![&fixture.env, 505_001 * SCALAR_7, 13_501 * SCALAR_7],
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
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
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
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
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
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
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
