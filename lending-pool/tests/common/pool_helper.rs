use crate::common::create_token;
use soroban_sdk::{Address, BytesN, Env};

use super::{PoolClient, ReserveConfig};

/// Uses default configuration
pub fn setup_reserve(
    e: &Env,
    pool: &Address,
    pool_client: &PoolClient,
    admin: &Address,
) -> (BytesN<32>, BytesN<32>, BytesN<32>) {
    let (underlying_id, _) = create_token(e, &admin);
    let (b_token_id, _) = create_token(e, &pool);
    let (d_token_id, _) = create_token(e, &pool);

    let config = ReserveConfig {
        b_token: b_token_id.clone(),
        d_token: d_token_id.clone(),
        decimals: 7,
        c_factor: 0_7500000,
        l_factor: 0_7500000,
        util: 0_5000000,
        r_one: 0_0500000,
        r_two: 0_5000000,
        r_three: 1_5000000,
        reactivity: 100, // 10e-5
        index: 0,
    };

    pool_client.init_res(&admin, &underlying_id, &config);

    return (underlying_id, b_token_id, d_token_id);
}
