#![cfg(test)]
use pool::{Request, RequestType};
use soroban_sdk::{testutils::Address as AddressTestTrait, vec, Address, Vec};
use test_suites::{
    create_fixture_with_data,
    test_fixture::{TokenIndex, SCALAR_7},
};

#[test]
#[should_panic(expected = "Error(WasmVm, InvalidAction)")]
fn test_pool_deposit_overflow_panics() {
    let fixture = create_fixture_with_data(true);
    let pool_fixture = &fixture.pools[0];
    let pool_balance = fixture.tokens[TokenIndex::STABLE].balance(&pool_fixture.pool.address);
    fixture.tokens[TokenIndex::STABLE].burn(&pool_fixture.pool.address, &pool_balance);

    // Create a user
    let samwise = Address::generate(&fixture.env);
    fixture.tokens[TokenIndex::STABLE].mint(&samwise, &(i128::MAX));
    let request = Request {
        request_type: RequestType::Supply as u32,
        address: fixture.tokens[TokenIndex::STABLE].address.clone(),
        amount: i128::MAX - 10,
    };

    pool_fixture
        .pool
        .submit(&samwise, &samwise, &samwise, &vec![&fixture.env, request]);
}

// This test ensures that an accessible underflow in the auction flow cannot be hit due to the overflow-checks flag being set
// Without this flag set, filling an auction on the same block it's started would cause an underflow
#[test]
#[should_panic(expected = "Error(WasmVm, InvalidAction)")]
fn test_auction_underflow_panics() {
    let fixture = create_fixture_with_data(true);
    let frodo = fixture.users.get(0).unwrap();
    let pool_fixture = &fixture.pools[0];

    // Create a user
    let samwise = Address::generate(&fixture.env); //sam will be supplying XLM and borrowing STABLE

    // Mint users tokens
    fixture.tokens[TokenIndex::XLM].mint(&samwise, &(500_000 * SCALAR_7));

    // Supply and borrow sam tokens
    let sam_requests: Vec<Request> = vec![
        &fixture.env,
        Request {
            request_type: RequestType::SupplyCollateral as u32,
            address: fixture.tokens[TokenIndex::XLM].address.clone(),
            amount: 6_000 * SCALAR_7,
        },
        Request {
            request_type: RequestType::Borrow as u32,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
            amount: 200 * 10i128.pow(6),
        },
    ];
    pool_fixture
        .pool
        .submit(&samwise, &samwise, &samwise, &sam_requests);

    //tank xlm price
    fixture.oracle.set_price_stable(&vec![
        &fixture.env,
        1000_0000000, // eth
        1_0000000,    // usdc
        0_0000100,    // xlm
        1_0000000,    // stable
    ]);

    // liquidate user
    let liq_pct = 100;
    let auction_data_2 = pool_fixture
        .pool
        .new_liquidation_auction(&samwise, &liq_pct);

    let usdc_bid_amount = auction_data_2
        .bid
        .get_unchecked(fixture.tokens[TokenIndex::STABLE].address.clone());

    //fill user liquidation
    let fill_requests = vec![
        &fixture.env,
        Request {
            request_type: RequestType::FillUserLiquidationAuction as u32,
            address: samwise.clone(),
            amount: 1,
        },
        Request {
            request_type: RequestType::Repay as u32,
            address: fixture.tokens[TokenIndex::STABLE].address.clone(),
            amount: usdc_bid_amount,
        },
    ];
    pool_fixture
        .pool
        .submit(&frodo, &frodo, &frodo, &fill_requests);
}
