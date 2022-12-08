use soroban_auth::Identifier;
use soroban_sdk::{panic_with_error, Env};

use crate::{errors::DTokenError, storage_types::DataKey};

pub fn read_balance(e: &Env, id: Identifier) -> u64 {
    let key = DataKey::Balance(id);
    if e.data().has(&key) {
        return e.data().get_unchecked(key).unwrap();
    } else {
        return 0;
    }
}

fn write_balance(e: &Env, id: Identifier, amount: u64) {
    let key = DataKey::Balance(id);
    e.data().set(key, amount);
}

pub fn receive_balance(e: &Env, id: Identifier, amount: u64) -> Result<(), DTokenError> {
    let balance = read_balance(e, id.clone());

    let new_balance = balance
        .checked_add(amount)
        .ok_or_else(|| panic_with_error!(e, DTokenError::OverflowError))?;
    write_balance(e, id, new_balance);
    Ok(())
}

pub fn spend_balance(e: &Env, id: Identifier, amount: u64) -> Result<(), DTokenError> {
    let balance = read_balance(e, id.clone());
    if balance < amount {
        // TODO: couldn't figure out how to return an error with a message here
        panic_with_error!(e, DTokenError::BalanceError)
    } else {
        let new_balance = balance
            .checked_sub(amount)
            .ok_or_else(|| panic_with_error!(e, DTokenError::OverflowError))?;
        write_balance(e, id, new_balance);
        Ok(())
    }
}
