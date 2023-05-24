#![cfg(test)]
use soroban_sdk::{testutils::Address as AddressTestTrait, vec, Address};
use test_suites::{
    create_fixture_with_data,
    test_fixture::{TokenIndex, SCALAR_7},
};

#[test]
fn test_classic_asset_pool_happy_path() {
    let (fixture, frodo) = create_fixture_with_data();
    let pool_fixture = &fixture.pools[0];

    // Create two new users
    let samwise = Address::random(&fixture.env); //sam will be supplying XLM and borrowing USDC
    let merry = Address::random(&fixture.env); //merry will be supplying USDC and borrowing XLM

    // Mint users tokens
    fixture.tokens[TokenIndex::USDC as usize].mint(&merry, &(250_000 * SCALAR_7));
    fixture.tokens[TokenIndex::XLM as usize].mint(&samwise, &(2_500_000 * SCALAR_7));

    // Supply tokens
    let merry_b_tokens = pool_fixture.pool.supply(
        &merry,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &(190_000 * SCALAR_7),
    );
    assert_eq!(
        fixture.tokens[TokenIndex::USDC as usize].balance(&merry),
        60_000 * SCALAR_7
    );
    assert!((merry_b_tokens < (190_000 * SCALAR_7)) & (merry_b_tokens > (189_999 * SCALAR_7)));
    assert_eq!(
        pool_fixture.reserves[0].b_token.balance(&merry),
        merry_b_tokens
    );
    let sam_b_tokens = pool_fixture.pool.supply(
        &samwise,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &(1_900_000 * SCALAR_7),
    );
    assert_eq!(
        fixture.tokens[TokenIndex::XLM as usize].balance(&samwise),
        600_000 * SCALAR_7
    );
    assert!((sam_b_tokens < (1_900_000 * SCALAR_7)) & (sam_b_tokens > (1_899_899 * SCALAR_7)));
    assert_eq!(
        pool_fixture.reserves[1].b_token.balance(&samwise),
        sam_b_tokens
    );
    // Borrow tokens
    let sam_d_tokens = pool_fixture.pool.borrow(
        &samwise,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &(112_000 * SCALAR_7),
        &samwise,
    ); //sams max borrow is .75*.95*.1*1_900_000 = 135_375 USDC
    assert_eq!(
        fixture.tokens[TokenIndex::USDC as usize].balance(&samwise),
        112_000 * SCALAR_7
    );
    assert!((sam_d_tokens < (112_000 * SCALAR_7)) & (sam_d_tokens > (111_999 * SCALAR_7)));
    assert_eq!(
        pool_fixture.reserves[0].d_token.balance(&samwise),
        sam_d_tokens
    );
    let merry_d_tokens = pool_fixture.pool.borrow(
        &merry,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &(1_135_000 * SCALAR_7),
        &merry,
    ); //merrys max borrow is .75*.9*190_000/.1 = 1_282_5000 XLM
    assert_eq!(
        fixture.tokens[TokenIndex::XLM as usize].balance(&merry),
        1_135_000 * SCALAR_7
    );
    assert!((merry_d_tokens < (1_135_000 * SCALAR_7)) & (merry_d_tokens > (1_134_899 * SCALAR_7)));
    assert_eq!(
        pool_fixture.reserves[1].d_token.balance(&merry),
        merry_d_tokens
    );
    //Utilization is now:
    // * 120_000 / 200_000 = .625 for USDC
    // * 1_200_000 / 2_000_000 = .625 for XLM
    // This equates to the following rough annual interest rates
    //  * 19.9% for XLM borrowing
    //  * 11.1% for XLM lending
    //  * rate will be dragged up due to rate modifier
    //  * 4.7% for USDC borrowing
    //  * 2.6% for USDC lending
    //  * rate will be dragged down due to rate modifier

    // claim frodo's setup emissions
    // - Frodo should receive 60 * 61 * .3 = 1098 BLND from the pool claim
    // - Frodo should receive 60 * 61 * .7 = 2562 BLND from the backstop claim
    let frodo_pool_claim = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 4], &frodo);
    assert_eq!(frodo_pool_claim, 1098_0000000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&frodo),
        1098_0000000
    );
    fixture.backstop.claim(
        &frodo,
        &vec![&fixture.env, pool_fixture.pool.address.clone()],
        &frodo,
    );
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&frodo),
        1098_0000000 + 2562_0000000
    );

    // Let three days pass
    fixture.jump(60 * 60 * 24 * 3);

    // Claim 3 day emissions

    // Claim frodo's three day pool emissions
    let frodo_pool_claim_2 = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 4], &frodo);
    assert_eq!(frodo_pool_claim_2, 4665_6384000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&frodo) - 1098_0000000 - 2562_0000000,
        46656384000
    );

    // Claim sam's three day pool emissions
    let sam_pool_claim = pool_fixture
        .pool
        .claim(&samwise, &vec![&fixture.env, 0, 4], &samwise);
    assert_eq!(sam_pool_claim, 730943066649);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&samwise),
        730943066649
    );

    // Sam repays some of his loan
    let sam_burned_d_tokens = pool_fixture.pool.repay(
        &samwise,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &(55_000 * SCALAR_7),
        &samwise,
    );
    assert_eq!(
        fixture.tokens[TokenIndex::USDC as usize].balance(&samwise),
        57_000 * SCALAR_7
    );
    assert!(
        (sam_burned_d_tokens < (55_000 * SCALAR_7)) & (sam_burned_d_tokens > (54_899 * SCALAR_7))
    );
    assert_eq!(
        pool_fixture.reserves[0].d_token.balance(&samwise),
        sam_d_tokens - sam_burned_d_tokens
    );
    // Merry repays some of his loan
    let merry_burned_d_tokens = pool_fixture.pool.repay(
        &merry,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &(575_000 * SCALAR_7),
        &merry,
    );
    assert_eq!(
        fixture.tokens[TokenIndex::XLM as usize].balance(&merry),
        560_000 * SCALAR_7
    );
    assert!(
        (merry_burned_d_tokens < (575_000 * SCALAR_7))
            & (merry_burned_d_tokens > (574_000 * SCALAR_7))
    );
    assert_eq!(
        pool_fixture.reserves[1].d_token.balance(&merry),
        merry_d_tokens - merry_burned_d_tokens
    );
    // Sam withdraws some of his XLM
    let sam_burned_b_tokens = pool_fixture.pool.withdraw(
        &samwise,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &(1_000_000 * SCALAR_7),
        &samwise,
    );
    assert_eq!(
        fixture.tokens[TokenIndex::XLM as usize].balance(&samwise),
        1_600_000 * SCALAR_7
    );
    assert!(
        (sam_burned_b_tokens < (1_000_000 * SCALAR_7))
            & (sam_burned_b_tokens > (999_000 * SCALAR_7))
    );
    assert_eq!(
        pool_fixture.reserves[1].b_token.balance(&samwise),
        sam_b_tokens - sam_burned_b_tokens
    );
    // Merry withdraws some of his USDC
    let merry_burned_b_tokens = pool_fixture.pool.withdraw(
        &merry,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &(100_000 * SCALAR_7),
        &merry,
    );
    assert_eq!(
        fixture.tokens[TokenIndex::USDC as usize].balance(&merry),
        160_000 * SCALAR_7
    );
    assert!(
        (merry_burned_b_tokens < (100_000 * SCALAR_7))
            & (merry_burned_b_tokens > (99_599 * SCALAR_7))
    );
    assert_eq!(
        pool_fixture.reserves[0].b_token.balance(&merry),
        merry_b_tokens - merry_burned_b_tokens
    );
    // Let one month pass
    fixture.jump(60 * 60 * 24 * 30);

    // Distribute emissions
    fixture.emitter.distribute();
    fixture.backstop.dist();
    pool_fixture.pool.updt_emis();

    // Claim emissions
    let frodo_pool_claim_3 = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 4], &frodo);
    assert_eq!(frodo_pool_claim_3, 116724748000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&frodo,) - 2562_0000000,
        17436_1132000
    );
    fixture.backstop.claim(
        &frodo,
        &vec![&fixture.env, pool_fixture.pool.address.clone()],
        &frodo,
    );

    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&frodo,) - 17436_1132000,
        423360_0000000
    );
    let sam_pool_claim_2 = pool_fixture
        .pool
        .claim(&samwise, &vec![&fixture.env, 0, 4], &samwise);
    assert_eq!(sam_pool_claim_2 - 73094_3066649, 17808_2688770);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&samwise),
        163996_8822068
    );

    // Let a year go by and call update every week
    for _ in 0..52 {
        // Let one week pass
        fixture.jump(60 * 60 * 24 * 7);
        // Update emissions
        fixture.emitter.distribute();
        fixture.backstop.dist();
        pool_fixture.pool.updt_emis();
    }

    // Frodo claims a year worth of backstop emissions
    fixture.backstop.claim(
        &frodo,
        &vec![&fixture.env, pool_fixture.pool.address.clone()],
        &frodo,
    );
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&frodo,) - 423360_0000000 - 17436_1132000,
        22014720_0000000
    );

    // Frodo claims a year worth of pool emissions
    let frodo_pool_claim_4 = pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 4], &frodo);
    assert_eq!(frodo_pool_claim_4, 1073564_6896000);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&frodo,)
            - 17436_1132000
            - 423360_0000000
            - 22014720_0000000,
        1073564_6896000
    );

    // Sam claims a year worth of pool emissions
    let sam_pool_claim_3 = pool_fixture
        .pool
        .claim(&samwise, &vec![&fixture.env, 0, 4], &samwise);
    assert_eq!(sam_pool_claim_3, 8360677_4955065);
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&samwise,) - 163996_8822068,
        8360677_4955065
    );

    // Sam and merry trade some tokens
    fixture.tokens[TokenIndex::USDC as usize].transfer(&merry, &samwise, &(60_000 * SCALAR_7));
    assert_eq!(
        fixture.tokens[TokenIndex::USDC as usize].balance(&samwise),
        117_000 * SCALAR_7
    );
    fixture.tokens[TokenIndex::XLM as usize].transfer(&samwise, &merry, &(600_000 * SCALAR_7));
    assert_eq!(
        fixture.tokens[TokenIndex::XLM as usize].balance(&merry),
        1_160_000 * SCALAR_7
    );

    // Sam repays his loan
    let sam_burned_d_tokens_2 = pool_fixture.pool.repay(
        &samwise,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &i128::MAX,
        &samwise,
    );
    assert_eq!(pool_fixture.reserves[0].d_token.balance(&samwise), 0);
    let sam_usdc_balance = fixture.tokens[TokenIndex::USDC as usize].balance(&samwise);
    assert!((sam_usdc_balance > 56_800 * SCALAR_7) & (sam_usdc_balance < 57_100 * SCALAR_7));
    assert_eq!(sam_burned_d_tokens_2, sam_d_tokens - sam_burned_d_tokens);

    // Merry repays his loan
    let merry_burned_d_tokens_2 = pool_fixture.pool.repay(
        &merry,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &i128::MAX,
        &merry,
    );
    assert_eq!(pool_fixture.reserves[1].d_token.balance(&merry), 0);
    let merry_xlm_balance = fixture.tokens[TokenIndex::XLM as usize].balance(&merry);
    assert!((merry_xlm_balance > 477_000 * SCALAR_7) & (merry_xlm_balance < 478_000 * SCALAR_7));
    assert_eq!(
        merry_burned_d_tokens_2,
        merry_d_tokens - merry_burned_d_tokens
    );

    // Sam withdraws all of his XLM
    let sam_burned_b_tokens_2 = pool_fixture.pool.withdraw(
        &samwise,
        &fixture.tokens[TokenIndex::XLM as usize].address,
        &i128::MAX,
        &samwise,
    );
    assert_eq!(pool_fixture.reserves[1].b_token.balance(&samwise), 0);
    assert_eq!(sam_burned_b_tokens_2, sam_b_tokens - sam_burned_b_tokens);
    let sam_xlm_balance = fixture.tokens[TokenIndex::XLM as usize].balance(&samwise);
    assert!((sam_xlm_balance < (2012000 * SCALAR_7)) & (sam_xlm_balance > (2009000 * SCALAR_7)));

    // Merry withdraws all of his USDC
    let merry_burned_b_tokens_2 = pool_fixture.pool.withdraw(
        &merry,
        &fixture.tokens[TokenIndex::USDC as usize].address,
        &i128::MAX,
        &merry,
    );
    assert_eq!(pool_fixture.reserves[0].b_token.balance(&merry), 0);
    assert_eq!(
        merry_burned_b_tokens_2,
        merry_b_tokens - merry_burned_b_tokens
    );
    let merry_usdc_balance = fixture.tokens[TokenIndex::USDC as usize].balance(&merry);
    assert!(
        (merry_usdc_balance < (193_000 * SCALAR_7)) & (merry_usdc_balance > (192_000 * SCALAR_7))
    );
}
