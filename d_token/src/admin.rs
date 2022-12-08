use soroban_auth::{Identifier, Signature};
use soroban_sdk::{panic_with_error, Env};

use crate::{errors::DTokenError, storage_types::DataKey};

fn read_administrator(e: &Env) -> Identifier {
    let key = DataKey::Admin;
    e.data().get_unchecked(key).unwrap()
}

pub fn write_administrator(e: &Env, id: Identifier) {
    let key = DataKey::Admin;
    e.data().set(key, id)
}

pub fn check_administrator(e: &Env, auth: &Signature) -> Result<(), DTokenError> {
    let auth_id = auth.identifier(&e);
    if auth_id != read_administrator(&e) {
        panic_with_error!(e, DTokenError::NotAuthorized);
    }
    Ok(())
}
