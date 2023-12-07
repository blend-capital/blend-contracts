use soroban_sdk::{testutils::Address as _, vec, Address, Env};

mod lp_contract {
    soroban_sdk::contractimport!(file = "../comet.wasm");
}

pub use lp_contract::{Client as LPClient, WASM as LP_WASM};

use sep_41_token::testutils::MockTokenClient;

/// Deploy a test Comet LP pool of 80% token_1 / 20% token_2. The admin must be the
/// admin of both of the token contracts used.
///
/// Initializes the pool with the following settings:
/// - Swap fee: 0.3%
/// - Token 1: 1,000
/// - Token 2: 25
/// - Shares: 100
pub(crate) fn create_lp_pool<'a>(
    e: &Env,
    admin: &Address,
    token_1: &Address,
    token_2: &Address,
) -> (Address, LPClient<'a>) {
    let contract_address = Address::generate(e);
    e.register_contract_wasm(&contract_address, LP_WASM);
    let client = LPClient::new(e, &contract_address);

    let token_1_client = MockTokenClient::new(e, token_1);
    let token_2_client = MockTokenClient::new(e, token_2);
    token_1_client.mint(&admin, &1_000_0000000);
    token_2_client.mint(&admin, &25_0000000);
    token_1_client.approve(&admin, &contract_address, &1_000_0000000, &5356700);
    token_2_client.approve(&admin, &contract_address, &1_000_0000000, &5356700);

    client.init(&Address::generate(e), &admin);
    client.bundle_bind(
        &vec![e, token_1.clone(), token_2.clone()],
        &vec![e, 1_000_0000000, 25_0000000],
        &vec![e, 8_0000000, 2_0000000],
    );

    client.set_swap_fee(&0_0030000, &admin);
    client.finalize();
    client.set_public_swap(&admin, &true);

    (contract_address, client)
}
