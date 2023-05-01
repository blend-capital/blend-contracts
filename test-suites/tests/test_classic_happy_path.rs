#![cfg(test)]
use soroban_sdk::{
    testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
    vec, Address, Env, Symbol,
};
use test_suites::{
    backstop::BackstopDataKey,
    create_fixture_with_data,
    pool::{PoolDataKey, UserEmissionData, UserReserveKey},
    test_fixture::{TokenIndex, SCALAR_7},
};

#[test]
fn test_classic_asset_pool_happy_path() {
    let (fixture, frodo) = create_fixture_with_data();
    let pool_fixture = &fixture.pools[0];

    // create two new users
    let samwise = Address::random(&fixture.env); //sam will be supplying XLM and borrowing USDC
    let merry = Address::random(&fixture.env); //merry will be supplying USDC and borrowing XLM

    //mint them tokens
    fixture.tokens[TokenIndex::USDC as usize].mint(
        &fixture.bombadil,
        &merry,
        &(200_000 * SCALAR_7),
    );
    fixture.tokens[TokenIndex::XLM as usize].mint(
        &fixture.bombadil,
        &samwise,
        &(2_000_000 * SCALAR_7),
    );

    // supply tokens
    pool_fixture.pool.supply(
        &merry,
        &fixture.tokens[TokenIndex::USDC as usize].contract_id,
        &(190_000 * SCALAR_7),
    );
    pool_fixture.pool.supply(
        &samwise,
        &fixture.tokens[TokenIndex::XLM as usize].contract_id,
        &(1_900_000 * SCALAR_7),
    );

    //borrow tokens
    pool_fixture.pool.borrow(
        &samwise,
        &fixture.tokens[TokenIndex::USDC as usize].contract_id,
        &(112_000 * SCALAR_7),
        &samwise,
    ); //sams max borrow is .75*.95*.1*1_900_000 = 135_375 USDC
    pool_fixture.pool.borrow(
        &merry,
        &fixture.tokens[TokenIndex::XLM as usize].contract_id,
        &(1_135_000 * SCALAR_7),
        &merry,
    ); //merrys max borrow is .75*.9*190_000/.1 = 1_282_5000 XLM

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

    pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 4], &frodo);

    assert_eq!(
        pool_fixture.reserves[1].b_token.balance(&frodo),
        100_000_0000000
    );
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&frodo),
        1097_9900000
    );
    // Let three days pass
    fixture.jump(60 * 60 * 24 * 3);

    // //supply token
    // pool_fixture.pool.supply(
    //     &frodo,
    //     &fixture.tokens[TokenIndex::XLM as usize].contract_id,
    //     &(1 * SCALAR_7),
    // );
    // // borrow token
    // pool_fixture.pool.borrow(
    //     &frodo,
    //     &fixture.tokens[TokenIndex::USDC as usize].contract_id,
    //     &(1 * SCALAR_7),
    //     &frodo,
    // );
    // println!(
    //     "XLM allocated emissions post third supply &jump: {:?}",
    //     &pool_fixture
    //         .pool
    //         .res_emis(&fixture.tokens[TokenIndex::XLM as usize].contract_id, &1)
    // );
    // println!(
    //     "USDC allocated emissions post third supply &jump: {:?}",
    //     &pool_fixture
    //         .pool
    //         .res_emis(&fixture.tokens[TokenIndex::USDC as usize].contract_id, &0)
    // );
    // fixture.env.as_contract(&pool_fixture.pool.contract_id, || {
    //     println!(
    //         "frodo emissions post third supply & jump,XLM:{:?}",
    //         fixture
    //             .env
    //             .storage()
    //             .get::<_, UserEmissionData>(&PoolDataKey::UserEmis(UserReserveKey {
    //                 user: frodo.clone(),
    //                 reserve_id: 4,
    //             }))
    //             .unwrap()
    //             .unwrap()
    //     );
    // });
    // fixture.env.as_contract(&pool_fixture.pool.contract_id, || {
    //     println!(
    //         "frodo emissions post third supply & jump,USDC:{:?}",
    //         fixture
    //             .env
    //             .storage()
    //             .get::<_, UserEmissionData>(&PoolDataKey::UserEmis(UserReserveKey {
    //                 user: frodo.clone(),
    //                 reserve_id: 0,
    //             }))
    //             .unwrap()
    //             .unwrap()
    //     );
    // });
    // println!("timestamp:{}", fixture.env.ledger().timestamp());
    // println!("");

    //claim emissions
    // * Frodo should receive (60 * 60 * 24 * 365 + 60*60 + 60)*.7 = 22077762 blnd from the backstop
    // * Frodo should receive 60*60*.3 = 1080 (first hour) + 60*60*24*365*(100_000/2_000_000)*.4*.3 + 60*60*24*365*(8_000/120_000)*.6*.3 = 568728 blnd from the pool
    // * Sam should roughly receive 60*60*24*365*(1_900_000/2_000_000)*.4*.3 + 60*60*24*365*(112_000/120_000)*.6*.3 = 8893152 blnd from the pool
    println!(
        "backstop BLND balance: {}",
        fixture.tokens[TokenIndex::BLND as usize].balance(&fixture.backstop.address(),)
    );
    println!(
        "XLM allocated emissions post emission update: {:?}",
        &pool_fixture
            .pool
            .res_emis(&fixture.tokens[TokenIndex::XLM as usize].contract_id, &1)
    );
    fixture.env.as_contract(&fixture.backstop.contract_id, || {
        println!(
            "pool emissions,{}",
            fixture
                .env
                .storage()
                .get::<_, i128>(&BackstopDataKey::PoolEmis(
                    pool_fixture.pool.contract_id.clone(),
                ))
                .unwrap()
                .unwrap()
        );
    });

    // fixture.backstop.claim  (
    //     &frodo,
    //     &vec![&fixture.env, pool_fixture.pool.contract_id.clone()],
    //     &frodo,
    // );
    // assert_eq!(
    //     fixture.tokens[TokenIndex::BLND as usize].balance(&frodo,),
    //     22077762 * SCALAR_7
    // );
    // pool_fixture.pool.supply(
    //     &frodo,
    //     &fixture.tokens[TokenIndex::XLM as usize].contract_id,
    //     &(1 * SCALAR_7),
    // );
    // fixture.env.as_contract(&pool_fixture.pool.contract_id, || {
    //     println!(
    //         "frodo emissions post4th supply & update,{:?}",
    //         fixture
    //             .env
    //             .storage()
    //             .get::<_, UserEmissionData>(&PoolDataKey::UserEmis(UserReserveKey {
    //                 user: frodo.clone(),
    //                 reserve_id: 4,
    //             }))
    //             .unwrap()
    //             .unwrap()
    //     );
    // });
    println!("");
    pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 4], &frodo);
    fixture.env.as_contract(&pool_fixture.pool.contract_id, || {
        println!(
            "frodo emissions post claim,{:?}",
            fixture
                .env
                .storage()
                .get::<_, UserEmissionData>(&PoolDataKey::UserEmis(UserReserveKey {
                    user: frodo.clone(),
                    reserve_id: 4,
                }))
                .unwrap()
                .unwrap()
        );
    });
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&frodo) - 1097_9900000,
        4665_6184000
    );
    // pool_fixture.pool.claim(
    //     &samwise,
    //     &vec![
    //         &fixture.env,
    //         TokenIndex::USDC as u32,
    //         TokenIndex::XLM as u32,
    //     ],
    //     &samwise,
    // );
    // assert_eq!(
    //     fixture.tokens[TokenIndex::BLND as usize].balance(&samwise),
    //     8893152 * SCALAR_7
    // );
    // Let one month pass
    fixture.jump(60 * 60 * 24 * 30);

    //supply token
    pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].contract_id,
        &(1 * SCALAR_7),
    );
    println!(
        "XLM allocated emissions post third supply &jump: {:?}",
        &pool_fixture
            .pool
            .res_emis(&fixture.tokens[TokenIndex::XLM as usize].contract_id, &1)
    );
    fixture.env.as_contract(&pool_fixture.pool.contract_id, || {
        println!(
            "frodo emissions post third supply & jump,{:?}",
            fixture
                .env
                .storage()
                .get::<_, UserEmissionData>(&PoolDataKey::UserEmis(UserReserveKey {
                    user: frodo.clone(),
                    reserve_id: 4,
                }))
                .unwrap()
                .unwrap()
        );
    });
    println!("");
    //distribute emissions
    fixture.emitter.distribute();
    fixture.backstop.dist();
    pool_fixture.pool.updt_emis();

    //claim emissions
    // * Frodo should receive (60 * 60 * 24 * 365 + 60*60 + 60)*.7 = 22077762 blnd from the backstop
    // * Frodo should receive 60*60*.3 = 1080 (first hour) + 60*60*24*365*(100_000/2_000_000)*.4*.3 + 60*60*24*365*(8_000/120_000)*.6*.3 = 568728 blnd from the pool
    // * Sam should roughly receive 60*60*24*365*(1_900_000/2_000_000)*.4*.3 + 60*60*24*365*(112_000/120_000)*.6*.3 = 8893152 blnd from the pool
    println!(
        "backstop BLND balance: {}",
        fixture.tokens[TokenIndex::BLND as usize].balance(&fixture.backstop.address(),)
    );
    println!(
        "XLM allocated emissions post emission update: {:?}",
        &pool_fixture
            .pool
            .res_emis(&fixture.tokens[TokenIndex::XLM as usize].contract_id, &1)
    );
    fixture.env.as_contract(&fixture.backstop.contract_id, || {
        println!(
            "pool emissions,{}",
            fixture
                .env
                .storage()
                .get::<_, i128>(&BackstopDataKey::PoolEmis(
                    pool_fixture.pool.contract_id.clone(),
                ))
                .unwrap()
                .unwrap()
        );
    });

    // fixture.backstop.claim  (
    //     &frodo,
    //     &vec![&fixture.env, pool_fixture.pool.contract_id.clone()],
    //     &frodo,
    // );
    // assert_eq!(
    //     fixture.tokens[TokenIndex::BLND as usize].balance(&frodo,),
    //     22077762 * SCALAR_7
    // );
    pool_fixture.pool.supply(
        &frodo,
        &fixture.tokens[TokenIndex::XLM as usize].contract_id,
        &(1 * SCALAR_7),
    );
    fixture.env.as_contract(&pool_fixture.pool.contract_id, || {
        println!(
            "frodo emissions post4th supply & update,{:?}",
            fixture
                .env
                .storage()
                .get::<_, UserEmissionData>(&PoolDataKey::UserEmis(UserReserveKey {
                    user: frodo.clone(),
                    reserve_id: 4,
                }))
                .unwrap()
                .unwrap()
        );
    });
    println!("");
    pool_fixture
        .pool
        .claim(&frodo, &vec![&fixture.env, 0, 4], &frodo);
    fixture.env.as_contract(&pool_fixture.pool.contract_id, || {
        println!(
            "frodo emissions post claim - XLM,{:?}",
            fixture
                .env
                .storage()
                .get::<_, UserEmissionData>(&PoolDataKey::UserEmis(UserReserveKey {
                    user: frodo.clone(),
                    reserve_id: 4,
                }))
                .unwrap()
                .unwrap()
        );
    });
    fixture.env.as_contract(&pool_fixture.pool.contract_id, || {
        println!(
            "frodo emissions post claim - USDC,{:?}",
            fixture
                .env
                .storage()
                .get::<_, UserEmissionData>(&PoolDataKey::UserEmis(UserReserveKey {
                    user: frodo.clone(),
                    reserve_id: 0,
                }))
                .unwrap()
                .unwrap()
        );
    });
    assert_eq!(
        fixture.tokens[TokenIndex::BLND as usize].balance(&frodo,),
        11912_9532000
    );
    // pool_fixture.pool.claim(
    //     &samwise,
    //     &vec![
    //         &fixture.env,
    //         TokenIndex::USDC as u32,
    //         TokenIndex::XLM as u32,
    //     ],
    //     &samwise,
    // );
    // assert_eq!(
    //     fixture.tokens[TokenIndex::BLND as usize].balance(&samwise),
    //     8893152 * SCALAR_7
    // );
}
