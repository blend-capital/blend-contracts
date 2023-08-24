#![cfg(test)]

use fixed_point_math::FixedPoint;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation, Events},
    vec, Address, IntoVal, Map, Symbol, Val, Vec,
};
use test_suites::{
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    test_fixture::{TokenIndex, SCALAR_7},
};

/// Test user exposed functions on the backstop for basic functionality, auth, and events.
/// Does not test internal state management of the backstop, only external effects.
#[test]
fn test_backstop() {
    let (fixture, frodo) = create_fixture_with_data(false);

    let pool = &fixture.pools[0].pool;
    let bstop_token = &fixture.tokens[TokenIndex::BSTOP];
    let sam = Address::random(&fixture.env);

    // Verify initialization can't be re-run
    let result = fixture.backstop.try_initialize(
        &Address::random(&fixture.env),
        &Address::random(&fixture.env),
        &Address::random(&fixture.env),
        &Address::random(&fixture.env),
        &Map::new(&fixture.env),
    );
    assert!(result.is_err());
    assert_eq!(
        fixture.backstop.backstop_token(),
        bstop_token.address.clone()
    );

    // Mint Sam some backstop tokens
    // assumes Sam makes up 20% of the backstop after depositing
    let mut frodo_bstop_token_balance = bstop_token.balance(&frodo);
    let mut bstop_bstop_token_balance = bstop_token.balance(&fixture.backstop.address);
    let mut sam_bstop_token_balance = 500_000 * SCALAR_7;
    bstop_token.mint(&sam, &sam_bstop_token_balance);

    // Sam deposits 500k backstop tokens
    let amount = 500_000 * SCALAR_7;
    let result = fixture.backstop.deposit(&sam, &pool.address, &amount);
    sam_bstop_token_balance -= amount;
    bstop_bstop_token_balance += amount;
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.backstop.address.clone(),
                    Symbol::new(&fixture.env, "deposit"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        pool.address.to_val(),
                        amount.into_val(&fixture.env)
                    ]
                )),
                sub_invocations: std::vec![AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        bstop_token.address.clone(),
                        Symbol::new(&fixture.env, "transfer"),
                        vec![
                            &fixture.env,
                            sam.to_val(),
                            fixture.backstop.address.to_val(),
                            amount.into_val(&fixture.env)
                        ]
                    )),
                    sub_invocations: std::vec![]
                }]
            }
        )
    );
    assert_eq!(result, amount);
    assert_eq!(bstop_token.balance(&sam), sam_bstop_token_balance);
    assert_eq!(
        bstop_token.balance(&fixture.backstop.address),
        bstop_bstop_token_balance
    );
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    let event_body: Vec<Val> = vec![
        &fixture.env,
        amount.into_val(&fixture.env),
        result.into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.backstop.address.clone(),
                (
                    Symbol::new(&fixture.env, "deposit"),
                    pool.address.clone(),
                    sam.clone()
                )
                    .into_val(&fixture.env),
                event_body.into_val(&fixture.env)
            )
        ]
    );

    // Simulate the pool backstop making money and progress 6d23h (6d23hr total emissions for sam)
    // @dev: setup jumps 1 hour and 1 minute
    fixture.jump(60 * 60 * 24 * 7 - 60 * 60);
    let amount = 2_000 * SCALAR_7;
    fixture.backstop.donate(&frodo, &pool.address, &amount);
    frodo_bstop_token_balance -= amount;
    bstop_bstop_token_balance += amount;
    assert_eq!(
        fixture.env.auths()[0],
        (
            frodo.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.backstop.address.clone(),
                    Symbol::new(&fixture.env, "donate"),
                    vec![
                        &fixture.env,
                        frodo.to_val(),
                        pool.address.to_val(),
                        amount.into_val(&fixture.env)
                    ]
                )),
                sub_invocations: std::vec![AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        bstop_token.address.clone(),
                        Symbol::new(&fixture.env, "transfer"),
                        vec![
                            &fixture.env,
                            frodo.to_val(),
                            fixture.backstop.address.to_val(),
                            amount.into_val(&fixture.env)
                        ]
                    )),
                    sub_invocations: std::vec![]
                }]
            }
        )
    );
    assert_eq!(bstop_token.balance(&frodo), frodo_bstop_token_balance);
    assert_eq!(
        bstop_token.balance(&fixture.backstop.address),
        bstop_bstop_token_balance
    );
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.backstop.address.clone(),
                (
                    Symbol::new(&fixture.env, "donate"),
                    pool.address.clone(),
                    frodo.clone()
                )
                    .into_val(&fixture.env),
                amount.into_val(&fixture.env)
            )
        ]
    );

    // Start the next emission cycle
    fixture.emitter.distribute();
    fixture.backstop.update_emission_cycle();
    assert_eq!(fixture.env.auths().len(), 0);

    // Sam queue for withdrawal
    let amount = 500_000 * SCALAR_7; // shares
    let result = fixture
        .backstop
        .queue_withdrawal(&sam, &pool.address, &amount);
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.backstop.address.clone(),
                    Symbol::new(&fixture.env, "queue_withdrawal"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        pool.address.to_val(),
                        amount.into_val(&fixture.env)
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    assert_eq!(result.amount, amount);
    assert_eq!(
        result.exp,
        fixture.env.ledger().timestamp() + 30 * 24 * 60 * 60
    );
    assert_eq!(bstop_token.balance(&sam), sam_bstop_token_balance);
    assert_eq!(
        bstop_token.balance(&fixture.backstop.address),
        bstop_bstop_token_balance
    );
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    let event_body: Vec<Val> = vec![
        &fixture.env,
        amount.into_val(&fixture.env),
        result.exp.into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.backstop.address.clone(),
                (
                    Symbol::new(&fixture.env, "queue_withdrawal"),
                    pool.address.clone(),
                    sam.clone()
                )
                    .into_val(&fixture.env),
                event_body.into_val(&fixture.env)
            )
        ]
    );

    // Start the next emission cycle and jump 7 days (13d23hr total emissions for sam)
    fixture.jump(60 * 60 * 24 * 7);
    fixture.emitter.distribute();
    fixture.backstop.update_emission_cycle();

    // Sam dequeues some of the withdrawal
    let amount = 250_000 * SCALAR_7; // shares
    fixture
        .backstop
        .dequeue_withdrawal(&sam, &pool.address, &amount);
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.backstop.address.clone(),
                    Symbol::new(&fixture.env, "dequeue_withdrawal"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        pool.address.to_val(),
                        amount.into_val(&fixture.env)
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    assert_eq!(bstop_token.balance(&sam), sam_bstop_token_balance);
    assert_eq!(
        bstop_token.balance(&fixture.backstop.address),
        bstop_bstop_token_balance
    );
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.backstop.address.clone(),
                (
                    Symbol::new(&fixture.env, "dequeue_withdrawal"),
                    pool.address.clone(),
                    sam.clone()
                )
                    .into_val(&fixture.env),
                amount.into_val(&fixture.env)
            )
        ]
    );

    // Start the next emission cycle and jump 7 days (20d23hr total emissions for sam)
    fixture.jump(60 * 60 * 24 * 7);
    fixture.emitter.distribute();
    fixture.backstop.update_emission_cycle();

    // Backstop loses money
    let amount = 1_000 * SCALAR_7;
    fixture.backstop.draw(&pool.address, &amount, &frodo);
    frodo_bstop_token_balance += amount;
    bstop_bstop_token_balance -= amount;
    assert_eq!(
        fixture.env.auths()[0],
        (
            pool.address.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.backstop.address.clone(),
                    Symbol::new(&fixture.env, "draw"),
                    vec![
                        &fixture.env,
                        pool.address.to_val(),
                        amount.into_val(&fixture.env),
                        frodo.to_val()
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    assert_eq!(bstop_token.balance(&frodo), frodo_bstop_token_balance);
    assert_eq!(
        bstop_token.balance(&fixture.backstop.address),
        bstop_bstop_token_balance
    );
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.backstop.address.clone(),
                (Symbol::new(&fixture.env, "draw"), pool.address.clone()).into_val(&fixture.env),
                vec![&fixture.env, frodo.to_val(), amount.into_val(&fixture.env),]
                    .into_val(&fixture.env)
            )
        ]
    );

    // Jump to the end of the withdrawal period (27d23hr total emissions for sam)
    fixture.jump(60 * 60 * 24 * 16 + 1);

    // Sam withdraws the queue position
    let amount = 250_000 * SCALAR_7; // shares
    let result = fixture.backstop.withdraw(&sam, &pool.address, &amount);
    sam_bstop_token_balance += result; // sam caught 20% of 1k profit and is withdrawing half his position
    bstop_bstop_token_balance -= result;
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.backstop.address.clone(),
                    Symbol::new(&fixture.env, "withdraw"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        pool.address.to_val(),
                        amount.into_val(&fixture.env),
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    assert_eq!(result, amount + 100 * SCALAR_7); // sam due 20% of 1k profit. Captures half (100) since withdrawing half his position.
    assert_eq!(bstop_token.balance(&sam), sam_bstop_token_balance);
    assert_eq!(
        bstop_token.balance(&fixture.backstop.address),
        bstop_bstop_token_balance
    );
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    let event_body: Vec<Val> = vec![
        &fixture.env,
        amount.into_val(&fixture.env),
        result.into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.backstop.address.clone(),
                (
                    Symbol::new(&fixture.env, "withdraw"),
                    pool.address.clone(),
                    sam.clone()
                )
                    .into_val(&fixture.env),
                event_body.into_val(&fixture.env)
            )
        ]
    );

    // Sam claims emissions earned on the backstop deposit
    let sam_blnd_balance = &fixture.tokens[TokenIndex::BLND].balance(&sam);
    let bstop_blend_balance = &fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address);
    fixture
        .backstop
        .claim(&sam, &vec![&fixture.env, pool.address.clone()], &sam);
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.backstop.address.clone(),
                    Symbol::new(&fixture.env, "claim"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        vec![&fixture.env, pool.address.clone()].to_val(),
                        sam.to_val(),
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    let emitted_tokens = (28 * 24 * 60 * 60 - 61 * 60) * SCALAR_7; // 27d22hr59m on emissions to claim
    let emission_share = 0_7000000.fixed_mul_floor(0_2000000, SCALAR_7).unwrap();
    let emitted_blnd = emission_share
        .fixed_mul_floor(emitted_tokens, SCALAR_7)
        .unwrap();
    assert_approx_eq_abs(
        fixture.tokens[TokenIndex::BLND].balance(&sam),
        sam_blnd_balance + emitted_blnd,
        100,
    );
    assert_approx_eq_abs(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        bstop_blend_balance - emitted_blnd,
        100,
    );
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.backstop.address.clone(),
                (Symbol::new(&fixture.env, "claim"), sam.clone()).into_val(&fixture.env),
                emitted_blnd.into_val(&fixture.env),
            )
        ]
    );
}
