use soroban_sdk::{Bytes, Env, IntoVal, RawVal};

use crate::{errors::DTokenError, public_types::Metadata, storage_types::DataKey};

pub fn write_metadata(e: &Env, metadata: Metadata) {
    let key = DataKey::Metadata;
    e.data().set::<DataKey, RawVal>(key, metadata.into_val(&e));
}

pub fn read_metadata(e: &Env) -> Metadata {
    let key = DataKey::Metadata;
    e.data().get_unchecked(key).unwrap()
}

pub fn has_metadata(e: &Env) -> bool {
    let key = DataKey::Metadata;
    e.data().has(key)
}

pub fn read_name(e: &Env) -> Result<Bytes, DTokenError> {
    match read_metadata(e) {
        Metadata::Token(token) => Ok(token.name),
    }
}

pub fn read_symbol(e: &Env) -> Result<Bytes, DTokenError> {
    match read_metadata(e) {
        Metadata::Token(token) => Ok(token.symbol),
    }
}

pub fn read_decimal(e: &Env) -> Result<u32, DTokenError> {
    match read_metadata(e) {
        Metadata::Token(token) => Ok(token.decimals),
    }
}
