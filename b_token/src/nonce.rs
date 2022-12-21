use soroban_auth::{Identifier, Signature};
use soroban_sdk::Env;

use crate::{errors::TokenError, storage_types::DataKey};

pub fn read_nonce(e: &Env, id: Identifier) -> u64 {
    e.data().get(DataKey::Nonce(id)).unwrap_or(Ok(0)).unwrap()
}

pub fn read_and_increment_nonce(e: &Env, id: Identifier) -> Result<u64, TokenError> {
    let key = DataKey::Nonce(id.clone());
    let old_nonce: u64 = read_nonce(e, id);
    let new_nonce = old_nonce
        .checked_add(1)
        .ok_or_else(|| TokenError::OverflowError);
    e.data().set::<DataKey, u64>(key, new_nonce.unwrap());
    Ok(old_nonce)
}

pub fn verify_and_consume_nonce(env: &Env, sig: &Signature, nonce: i128) -> Result<(), TokenError> {
    let nonce_u64: u64 = nonce as u64;
    match sig {
        Signature::Invoker => {
            if nonce_u64 != 0 {
                return Err(TokenError::InvalidNonce);
            }
        }
        Signature::Ed25519(_) | Signature::Account(_) => {
            let id = sig.identifier(env);
            if nonce_u64 != read_and_increment_nonce(&env, id).unwrap() {
                return Err(TokenError::InvalidNonce);
            }
        }
    }
    Ok(())
}
