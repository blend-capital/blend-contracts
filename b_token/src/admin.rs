use soroban_auth::{Identifier, Signature};
use soroban_sdk::Env;

use crate::{errors::TokenError, storage_types::DataKey};

pub fn read_administrator(e: &Env) -> Identifier {
    let key = DataKey::Admin;
    e.data().get_unchecked(key).unwrap()
}

pub fn write_administrator(e: &Env, id: Identifier) {
    let key = DataKey::Admin;
    e.data().set(key, id)
}

pub fn check_administrator(e: &Env, auth: &Signature) -> Result<(), TokenError> {
    let auth_id = auth.identifier(&e);
    if auth_id != read_administrator(&e) {
        Err(TokenError::NotAuthorized)
    } else {
        Ok(())
    }
}
