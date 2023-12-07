#![cfg(test)]

use pool::{Request, ReserveEmissionMetadata};
use soroban_fixed_point_math::FixedPoint;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation, Events},
    vec, Address, IntoVal, Symbol, Val,
};
use test_suites::{
    assertions::assert_approx_eq_abs,
    create_fixture_with_data,
    pool::default_reserve_metadata,
    test_fixture::{TokenIndex, SCALAR_7, SCALAR_9},
};

/// Test user exposed functions on the lending pool for basic user functionality, auth, and events.
/// Does not test internal state management of the lending pool, only external effects.
#[test]
fn test_pool_user() {
    let fixture = create_fixture_with_data(false);
    let pool_fixture = &fixture.pools[0];
    let xlm_pool_index = pool_fixture.reserves[&TokenIndex::XLM];
    let weth_pool_index = pool_fixture.reserves[&TokenIndex::WETH];
    let xlm = &fixture.tokens[TokenIndex::XLM];
    let weth = &fixture.tokens[TokenIndex::WETH];
    let weth_scalar: i128 = 10i128.pow(weth.decimals());

    let sam = Address::generate(&fixture.env);

    // Mint sam tokens
    let mut sam_xlm_balance = 10_000 * SCALAR_7;
    let mut sam_weth_balance = 1 * weth_scalar;
    xlm.mint(&sam, &sam_xlm_balance);
    weth.mint(&sam, &sam_weth_balance);

    let mut pool_xlm_balance = xlm.balance(&pool_fixture.pool.address);
    let mut pool_weth_balance = weth.balance(&pool_fixture.pool.address);

    let mut sam_xlm_btoken_balance = 0;
    let mut sam_weth_btoken_balance = 0;
    let mut sam_weth_dtoken_balance = 0;

    // Sam supply WETH
    let amount = 5 * (weth_scalar / 10); // 0.5
    let requests = vec![
        &fixture.env,
        Request {
            request_type: 0,
            address: weth.address.clone(),
            amount,
        },
    ];
    let result = pool_fixture.pool.submit(&sam, &sam, &sam, &requests);
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "submit"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        sam.to_val(),
                        sam.to_val(),
                        requests.to_val()
                    ]
                )),
                sub_invocations: std::vec![AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        weth.address.clone(),
                        Symbol::new(&fixture.env, "transfer"),
                        vec![
                            &fixture.env,
                            sam.to_val(),
                            pool_fixture.pool.address.to_val(),
                            amount.into_val(&fixture.env)
                        ]
                    )),
                    sub_invocations: std::vec![]
                }]
            }
        )
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::WETH);
    pool_weth_balance += amount;
    sam_weth_balance -= amount;
    assert_eq!(weth.balance(&sam), sam_weth_balance);
    assert_eq!(weth.balance(&pool_fixture.pool.address), pool_weth_balance);
    sam_weth_btoken_balance += amount
        .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.supply.get_unchecked(weth_pool_index),
        sam_weth_btoken_balance,
        10,
    );
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 2)];
    let event_data: soroban_sdk::Vec<Val> = vec![
        &fixture.env,
        amount.into_val(&fixture.env),
        result
            .supply
            .get_unchecked(weth_pool_index)
            .into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "supply"),
                    weth.address.clone(),
                    sam.clone()
                )
                    .into_val(&fixture.env),
                event_data.into_val(&fixture.env)
            )
        ]
    );

    // Skip 1 day
    fixture.jump(24 * 60 * 60);

    // Sam withdraw WETH
    let amount = 5 * (weth_scalar / 10); // 0.5
    let requests = vec![
        &fixture.env,
        Request {
            request_type: 1,
            address: weth.address.clone(),
            amount,
        },
    ];
    let result = pool_fixture.pool.submit(&sam, &sam, &sam, &requests);
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "submit"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        sam.to_val(),
                        sam.to_val(),
                        requests.to_val()
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::WETH);
    pool_weth_balance -= amount;
    sam_weth_balance += amount;
    assert_eq!(weth.balance(&sam), sam_weth_balance);
    assert_eq!(weth.balance(&pool_fixture.pool.address), pool_weth_balance);
    let pool_tokens = amount
        .fixed_div_ceil(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    sam_weth_btoken_balance -= pool_tokens;
    assert_approx_eq_abs(
        result.supply.get_unchecked(weth_pool_index),
        sam_weth_btoken_balance,
        10,
    );
    assert_ne!(sam_weth_btoken_balance, 0); // some interest was earned
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 2)];
    let event_data: soroban_sdk::Vec<Val> = vec![
        &fixture.env,
        amount.into_val(&fixture.env),
        pool_tokens.into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "withdraw"),
                    weth.address.clone(),
                    sam.clone(),
                )
                    .into_val(&fixture.env),
                event_data.into_val(&fixture.env),
            )
        ]
    );

    // Sam supply collateral XLM
    let amount = 5_000 * SCALAR_7;
    let requests = vec![
        &fixture.env,
        Request {
            request_type: 2,
            address: xlm.address.clone(),
            amount,
        },
    ];
    let result = pool_fixture.pool.submit(&sam, &sam, &sam, &requests);
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "submit"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        sam.to_val(),
                        sam.to_val(),
                        requests.to_val()
                    ]
                )),
                sub_invocations: std::vec![AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        xlm.address.clone(),
                        Symbol::new(&fixture.env, "transfer"),
                        vec![
                            &fixture.env,
                            sam.to_val(),
                            pool_fixture.pool.address.to_val(),
                            amount.into_val(&fixture.env)
                        ]
                    )),
                    sub_invocations: std::vec![]
                }]
            }
        )
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::XLM);
    pool_xlm_balance += amount;
    sam_xlm_balance -= amount;
    assert_eq!(xlm.balance(&sam), sam_xlm_balance);
    assert_eq!(xlm.balance(&pool_fixture.pool.address), pool_xlm_balance);
    sam_xlm_btoken_balance += amount
        .fixed_div_floor(reserve_data.b_rate, SCALAR_9)
        .unwrap();
    assert_approx_eq_abs(
        result.collateral.get_unchecked(xlm_pool_index),
        sam_xlm_btoken_balance,
        10,
    );
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 2)];
    let event_data: soroban_sdk::Vec<Val> = vec![
        &fixture.env,
        amount.into_val(&fixture.env),
        result
            .collateral
            .get_unchecked(xlm_pool_index)
            .into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "supply_collateral"),
                    xlm.address.clone(),
                    sam.clone()
                )
                    .into_val(&fixture.env),
                event_data.into_val(&fixture.env)
            )
        ]
    );

    // Sam borrows WETH
    let amount = 1 * (weth_scalar / 10); // 0.1
    let requests = vec![
        &fixture.env,
        Request {
            request_type: 4,
            address: weth.address.clone(),
            amount,
        },
    ];
    let result = pool_fixture.pool.submit(&sam, &sam, &sam, &requests);
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "submit"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        sam.to_val(),
                        sam.to_val(),
                        requests.to_val()
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    let reserve_data = fixture.read_reserve_data(0, TokenIndex::WETH);
    pool_weth_balance -= amount;
    sam_weth_balance += amount;
    assert_eq!(weth.balance(&sam), sam_weth_balance);
    assert_eq!(weth.balance(&pool_fixture.pool.address), pool_weth_balance);
    sam_weth_dtoken_balance += amount
        .fixed_div_ceil(reserve_data.d_rate, SCALAR_9)
        .unwrap();
    assert_eq!(
        result.liabilities.get_unchecked(weth_pool_index),
        sam_weth_dtoken_balance
    );
    assert_approx_eq_abs(
        result.liabilities.get_unchecked(weth_pool_index),
        sam_weth_dtoken_balance,
        10,
    );
    let events = fixture.env.events().all();
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 2)];
    let event_data: soroban_sdk::Vec<Val> = vec![
        &fixture.env,
        amount.into_val(&fixture.env),
        result
            .liabilities
            .get_unchecked(weth_pool_index)
            .into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "borrow"),
                    weth.address.clone(),
                    sam.clone()
                )
                    .into_val(&fixture.env),
                event_data.into_val(&fixture.env)
            )
        ]
    );

    // allow the rest of the emissions period to pass (6 days - 5d23h59m emitted for XLM supply)
    fixture.jump(6 * 24 * 60 * 60);
    fixture.emitter.distribute();
    fixture.backstop.gulp_emissions();
    pool_fixture.pool.gulp_emissions();
    assert_eq!(fixture.env.auths().len(), 0); // no auth required to update emissions

    // Sam repay and withdrawal positions
    let amount_withdrawal = 5_010 * SCALAR_7;
    let amount_repay = 11 * (weth_scalar / 100); // 0.11
    let requests = vec![
        &fixture.env,
        Request {
            request_type: 3,
            address: xlm.address.clone(),
            amount: amount_withdrawal,
        },
        Request {
            request_type: 5,
            address: weth.address.clone(),
            amount: amount_repay,
        },
    ];
    let result = pool_fixture.pool.submit(&sam, &sam, &sam, &requests);
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "submit"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        sam.to_val(),
                        sam.to_val(),
                        requests.to_val()
                    ]
                )),
                sub_invocations: std::vec![AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        weth.address.clone(),
                        Symbol::new(&fixture.env, "transfer"),
                        vec![
                            &fixture.env,
                            sam.to_val(),
                            pool_fixture.pool.address.to_val(),
                            amount_repay.into_val(&fixture.env)
                        ]
                    )),
                    sub_invocations: std::vec![]
                }]
            }
        )
    );
    let xlm_reserve_data = fixture.read_reserve_data(0, TokenIndex::XLM);
    let est_xlm = sam_xlm_btoken_balance
        .fixed_mul_floor(xlm_reserve_data.b_rate, SCALAR_9)
        .unwrap();
    pool_xlm_balance -= est_xlm;
    sam_xlm_balance += est_xlm;
    assert_approx_eq_abs(xlm.balance(&sam), sam_xlm_balance, 10);
    assert_approx_eq_abs(
        xlm.balance(&pool_fixture.pool.address),
        pool_xlm_balance,
        10,
    );
    assert_eq!(result.collateral.len(), 0);
    let events = fixture.env.events().all();
    // @dev: three transfer events follow the pool events, 1 pool event follows
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 5)];
    let event_data: soroban_sdk::Vec<Val> = vec![
        &fixture.env,
        est_xlm.into_val(&fixture.env),
        sam_xlm_btoken_balance.into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "withdraw_collateral"),
                    xlm.address.clone(),
                    sam.clone()
                )
                    .into_val(&fixture.env),
                event_data.into_val(&fixture.env)
            )
        ]
    );
    let weth_reserve_data = fixture.read_reserve_data(0, TokenIndex::WETH);
    let est_weth = sam_weth_dtoken_balance
        .fixed_mul_ceil(weth_reserve_data.d_rate, SCALAR_9)
        .unwrap();
    pool_weth_balance += est_weth;
    sam_weth_balance -= est_weth;
    assert_eq!(weth.balance(&sam), sam_weth_balance);
    assert_approx_eq_abs(weth.balance(&sam), sam_weth_balance, 10);
    assert_approx_eq_abs(
        weth.balance(&pool_fixture.pool.address),
        pool_weth_balance,
        10,
    );
    assert_eq!(result.liabilities.len(), 0);
    // @dev: three transfer events follow the pool events
    let event = vec![&fixture.env, events.get_unchecked(events.len() - 4)];
    let event_data: soroban_sdk::Vec<Val> = vec![
        &fixture.env,
        est_weth.into_val(&fixture.env),
        sam_weth_dtoken_balance.into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "repay"),
                    weth.address.clone(),
                    sam.clone()
                )
                    .into_val(&fixture.env),
                event_data.into_val(&fixture.env)
            )
        ]
    );

    // Sam claims emissions on XLM supply (5d23h59m)
    let blnd = &fixture.tokens[TokenIndex::BLND];
    let sam_blnd_balance = blnd.balance(&sam);
    let result = pool_fixture
        .pool
        .claim(&sam, &vec![&fixture.env, xlm_pool_index * 2 + 1], &sam);
    assert_eq!(
        fixture.env.auths()[0],
        (
            sam.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "claim"),
                    vec![
                        &fixture.env,
                        sam.to_val(),
                        vec![&fixture.env, xlm_pool_index * 2 + 1].to_val(),
                        sam.to_val(),
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    assert_eq!(result, 2940_3113155); // ~ 4.99k / (100k + 4.99k) * 0.12 (xlm eps) * 5d23hr59m in seconds
    assert_eq!(blnd.balance(&sam), sam_blnd_balance + result);
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (Symbol::new(&fixture.env, "claim"), sam.clone()).into_val(&fixture.env),
                vec![
                    &fixture.env,
                    vec![&fixture.env, xlm_pool_index * 2 + 1].to_val(),
                    result.into_val(&fixture.env),
                ]
                .into_val(&fixture.env)
            )
        ]
    );
}

/// Test user exposed functions on the lending pool for basic configuration functionality, auth, and events.
/// Does not test internal state management of the lending pool, only external effects.
#[test]
fn test_pool_config() {
    let fixture = create_fixture_with_data(false);

    let pool_fixture = &fixture.pools[0];

    // Verify initialize can't be run again
    let result = pool_fixture.pool.try_initialize(
        &Address::generate(&fixture.env),
        &Symbol::new(&fixture.env, "teapot"),
        &Address::generate(&fixture.env),
        &10000,
        &Address::generate(&fixture.env),
        &Address::generate(&fixture.env),
        &Address::generate(&fixture.env),
    );
    assert!(result.is_err());

    // Update pool config (admin only)
    pool_fixture.pool.update_pool(&0_050_000_000);
    assert_eq!(
        fixture.env.auths()[0],
        (
            fixture.bombadil.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "update_pool"),
                    vec![&fixture.env, 0_050_000_000u64.into_val(&fixture.env)]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    let new_pool_config = fixture.read_pool_config(0);
    assert_eq!(new_pool_config.bstop_rate, 0_050_000_000);
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "update_pool"),
                    fixture.bombadil.clone()
                )
                    .into_val(&fixture.env),
                0_050_000_000u64.into_val(&fixture.env)
            )
        ]
    );

    // Initialize a reserve (admin only)
    let blnd = &fixture.tokens[TokenIndex::BLND];
    let mut reserve_config = default_reserve_metadata();
    reserve_config.l_factor = 0_500_0000;
    reserve_config.c_factor = 0_200_0000;
    pool_fixture
        .pool
        .init_reserve(&blnd.address, &reserve_config);
    assert_eq!(
        fixture.env.auths()[0],
        (
            fixture.bombadil.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "init_reserve"),
                    vec![
                        &fixture.env,
                        blnd.address.to_val(),
                        reserve_config.into_val(&fixture.env)
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    let new_reserve_config = fixture.read_reserve_config(0, TokenIndex::BLND);
    assert_eq!(new_reserve_config.l_factor, 0_500_0000);
    assert_eq!(new_reserve_config.c_factor, 0_200_0000);
    assert_eq!(new_reserve_config.index, 3); // setup includes 3 assets (0 indexed)
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    let event_data: soroban_sdk::Vec<Val> = vec![
        &fixture.env,
        blnd.address.into_val(&fixture.env),
        3_u32.into_val(&fixture.env),
    ];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "init_reserve"),
                    fixture.bombadil.clone()
                )
                    .into_val(&fixture.env),
                event_data.into_val(&fixture.env)
            )
        ]
    );

    // Update reserve config (admin only)
    reserve_config.c_factor = 0;
    pool_fixture
        .pool
        .update_reserve(&blnd.address, &reserve_config);
    assert_eq!(
        fixture.env.auths()[0],
        (
            fixture.bombadil.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "update_reserve"),
                    vec![
                        &fixture.env,
                        blnd.address.to_val(),
                        reserve_config.into_val(&fixture.env)
                    ]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    let new_reserve_config = fixture.read_reserve_config(0, TokenIndex::BLND);
    assert_eq!(new_reserve_config.l_factor, 0_500_0000);
    assert_eq!(new_reserve_config.c_factor, 0);
    assert_eq!(new_reserve_config.index, 3);
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "update_reserve"),
                    fixture.bombadil.clone()
                )
                    .into_val(&fixture.env),
                blnd.address.to_val()
            )
        ]
    );

    // Set admin (admin only)
    let new_admin = Address::generate(&fixture.env);
    pool_fixture.pool.set_admin(&new_admin);
    assert_eq!(
        fixture.env.auths()[0],
        (
            fixture.bombadil.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "set_admin"),
                    vec![&fixture.env, new_admin.to_val(),]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (
                    Symbol::new(&fixture.env, "set_admin"),
                    fixture.bombadil.clone()
                )
                    .into_val(&fixture.env),
                new_admin.into_val(&fixture.env)
            )
        ]
    );

    // Set status (admin only)
    pool_fixture.pool.set_status(&1);
    assert_eq!(
        fixture.env.auths()[0],
        (
            new_admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "set_status"),
                    vec![&fixture.env, 1u32.into_val(&fixture.env)]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    let new_pool_config = fixture.read_pool_config(0);
    assert_eq!(new_pool_config.status, 1);
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (Symbol::new(&fixture.env, "set_status"), new_admin.clone()).into_val(&fixture.env),
                1u32.into_val(&fixture.env)
            )
        ]
    );

    // Update status (backstop is healthy, so this should update to active)
    pool_fixture.pool.update_status();
    assert_eq!(fixture.env.auths().len(), 0);
    let new_pool_config = fixture.read_pool_config(0);
    assert_eq!(new_pool_config.status, 0);
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                pool_fixture.pool.address.clone(),
                (Symbol::new(&fixture.env, "set_status"),).into_val(&fixture.env),
                0u32.into_val(&fixture.env)
            )
        ]
    );

    // Set emissions config (admin only)
    let reserve_emissions: soroban_sdk::Vec<ReserveEmissionMetadata> = soroban_sdk::vec![
        &fixture.env,
        ReserveEmissionMetadata {
            res_index: 0, // USDC
            res_type: 0,  // d_token
            share: 0_400_0000
        },
        ReserveEmissionMetadata {
            res_index: 1, // XLM
            res_type: 1,  // b_token
            share: 0_400_0000
        },
        ReserveEmissionMetadata {
            res_index: 3, // BLND
            res_type: 1,  // b_token
            share: 0_200_0000
        },
    ];
    pool_fixture.pool.set_emissions_config(&reserve_emissions);
    assert_eq!(
        fixture.env.auths()[0],
        (
            new_admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    pool_fixture.pool.address.clone(),
                    Symbol::new(&fixture.env, "set_emissions_config"),
                    vec![&fixture.env, reserve_emissions.to_val()]
                )),
                sub_invocations: std::vec![]
            }
        )
    );
    let new_emissions_config = fixture.read_pool_emissions(0);
    assert_eq!(new_emissions_config.len(), 3);
    assert_eq!(new_emissions_config.get_unchecked(0), 0_400_0000);
    assert_eq!(new_emissions_config.get_unchecked(1 * 2 + 1), 0_400_0000);
    assert_eq!(new_emissions_config.get_unchecked(3 * 2 + 1), 0_200_0000);
}
