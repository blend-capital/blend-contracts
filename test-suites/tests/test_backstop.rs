#![cfg(test)]

use soroban_fixed_point_math::FixedPoint;
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
    let fixture = create_fixture_with_data(false);
    let frodo = fixture.users.get(0).unwrap();

    let pool = &fixture.pools[0].pool;
    let bstop_token = &fixture.lp;
    let sam = Address::generate(&fixture.env);

    // Verify initialization can't be re-run
    let result = fixture.backstop.try_initialize(
        &Address::generate(&fixture.env),
        &Address::generate(&fixture.env),
        &Address::generate(&fixture.env),
        &Address::generate(&fixture.env),
        &Address::generate(&fixture.env),
        &Map::new(&fixture.env),
    );
    assert!(result.is_err());
    assert_eq!(
        fixture.backstop.backstop_token(),
        bstop_token.address.clone()
    );

    // Mint some backstop tokens
    // assumes Sam makes up 20% of the backstop after depositing (50k / 0.8 * 0.2 = 12.5k)
    //  -> mint 12.5k LP tokens to sam
    fixture.tokens[TokenIndex::BLND].mint(&sam, &(125_001_000_0000_0000_000_000 * SCALAR_7)); // 10 BLND per LP token
    fixture.tokens[TokenIndex::BLND].approve(&sam, &bstop_token.address, &i128::MAX, &99999);
    fixture.tokens[TokenIndex::USDC].mint(&sam, &(3_126_000_0000_0000_000_000 * SCALAR_7)); // 0.25 USDC per LP token
    fixture.tokens[TokenIndex::USDC].approve(&sam, &bstop_token.address, &i128::MAX, &99999);
    bstop_token.join_pool(
        &(12_500 * SCALAR_7),
        &vec![
            &fixture.env,
            125_001_000_0000_0000_000 * SCALAR_7,
            3_126_000_0000_0000_000 * SCALAR_7,
        ],
        &sam,
    );

    //  -> mint Frodo additional backstop tokens (5k) for donation later
    fixture.tokens[TokenIndex::BLND].mint(&frodo, &(50_001 * SCALAR_7)); // 10 BLND per LP token
    fixture.tokens[TokenIndex::BLND].approve(&frodo, &bstop_token.address, &i128::MAX, &99999);
    fixture.tokens[TokenIndex::USDC].mint(&frodo, &(1_251 * SCALAR_7)); // 0.25 USDC per LP token
    fixture.tokens[TokenIndex::USDC].approve(&frodo, &bstop_token.address, &i128::MAX, &99999);
    bstop_token.join_pool(
        &(5_000 * SCALAR_7),
        &vec![&fixture.env, 50_001 * SCALAR_7, 1_251 * SCALAR_7],
        &frodo,
    );

    let mut frodo_bstop_token_balance = bstop_token.balance(&frodo);
    let mut bstop_bstop_token_balance = bstop_token.balance(&fixture.backstop.address);
    let mut sam_bstop_token_balance = bstop_token.balance(&sam);
    assert_eq!(sam_bstop_token_balance, 12_500 * SCALAR_7);

    // Sam deposits 12.5k backstop tokens
    let amount = 12_500 * SCALAR_7;
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
    fixture.backstop.gulp_emissions();
    assert_eq!(fixture.env.auths().len(), 0);

    // Sam queues 100% of position for withdrawal
    let amount = 12_500 * SCALAR_7; // shares
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
    fixture.backstop.gulp_emissions();

    // Sam dequeues half of the withdrawal
    let amount = 6_250 * SCALAR_7; // shares
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
    fixture.backstop.gulp_emissions();

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
    let amount = 6_250 * SCALAR_7; // shares
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
    let bstop_blend_balance = &fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address);
    let comet_blend_balance = &fixture.tokens[TokenIndex::BLND].balance(&fixture.lp.address);
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
    //6d23hr at full
    //7 days at none
    //7 at 6250 + (60 * 60 * 24 * 16 + 1)
    let emission_share_1 = 0_7000000.fixed_mul_floor(0_2000000, SCALAR_7).unwrap();
    let emission_share_2 = 0_7000000.fixed_mul_floor(0_1111111, SCALAR_7).unwrap();
    let emitted_blnd_1 = ((7 * 24 * 60 * 60 - 61 * 60) * SCALAR_7)
        .fixed_mul_floor(emission_share_1, SCALAR_7)
        .unwrap();
    let emitted_blnd_2 = ((14 * 24 * 60 * 60 + 1) * SCALAR_7 + 2096022)
        .fixed_mul_floor(emission_share_2, SCALAR_7)
        .unwrap();

    assert_approx_eq_abs(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.lp.address),
        comet_blend_balance + emitted_blnd_1 + emitted_blnd_2,
        SCALAR_7,
    );
    assert_approx_eq_abs(
        fixture.tokens[TokenIndex::BLND].balance(&fixture.backstop.address),
        bstop_blend_balance - emitted_blnd_1 - emitted_blnd_2,
        SCALAR_7,
    );
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.backstop.address.clone(),
                (Symbol::new(&fixture.env, "claim"), sam.clone()).into_val(&fixture.env),
                (emitted_blnd_1 + emitted_blnd_2).into_val(&fixture.env),
            )
        ]
    );
}
