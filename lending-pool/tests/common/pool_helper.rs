use soroban_auth::Identifier;
use soroban_sdk::{Env, BytesN, AccountId};
use crate::common::{create_token, generate_contract_id};

use super::lending_pool_wasm::{Client as PoolClient, ReserveConfig};

/// Uses default configuration
pub fn setup_reserve(
    e: &Env, 
    pool: &Identifier,
    pool_client: &PoolClient,
    admin: &AccountId,
) -> (BytesN<32>, BytesN<32>, BytesN<32>) {
    let underlying_id = generate_contract_id(e);
    let b_token_id = generate_contract_id(e);
    let d_token_id = generate_contract_id(e);

    let admin_id = Identifier::Account(admin.to_owned());
    create_token(e, &underlying_id, &admin_id);
    create_token(e, &b_token_id, &pool);
    create_token(e, &d_token_id, &pool);

    let config = ReserveConfig {
        b_token: b_token_id.clone(),
        d_token: d_token_id.clone(),
        decimals: 7,
        c_factor: 0_7500000,
        l_factor: 0_7500000,
        util: 0_8000000,
        r_one: 0_0500000,
        r_two: 0_5000000,
        r_three: 1_5000000, 
    };

    pool_client.with_source_account(&admin).init_res(&underlying_id, &config);

    return (underlying_id, b_token_id, d_token_id);
}