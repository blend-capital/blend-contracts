use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env};

use crate::{admin::read_administrator, dependencies::PoolClient, errors::TokenError};

pub fn read_collateral(e: &Env) -> bool {
    // let admin_id = read_administrator(e);

    // let pool_id: BytesN<32> = match admin_id {
    //     // Functions requiring signatures should only be called by other contracts.
    //     Identifier::Account(admin_id) => Err(TokenError::InvalidAdmin).unwrap(),
    //     // Functions requiring signatures should only be called by other contracts.
    //     Identifier::Ed25519(admin_id) => Err(TokenError::InvalidAdmin).unwrap(),
    //     Identifier::Contract(admin_id) => admin_id,
    // };
    // let pool_client = PoolClient::new(e, pool_id);
    //TODO: we should make user configs public on the pool client
    false
}
