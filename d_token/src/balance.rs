use soroban_auth::Identifier;
use soroban_sdk::{ConversionError, Env};

use crate::{errors::DTokenError, storage_types::DataKey};

pub fn read_balance(e: &Env, id: Identifier) -> Result<u64, ConversionError> {
    let key = DataKey::Balance(id.clone());
    e.data().get(key).unwrap_or(Ok(0))
}

fn write_balance(e: &Env, id: Identifier, amount: u64) {
    let key = DataKey::Balance(id);
    e.data().set(key, amount);
}

pub fn receive_balance(e: &Env, id: Identifier, amount: u64) -> Result<(), DTokenError> {
    let balance = read_balance(e, id.clone()).unwrap();

    let new_balance = balance
        .checked_add(amount)
        .ok_or_else(|| DTokenError::OverflowError);
    write_balance(e, id, new_balance.unwrap());
    Ok(())
}

pub fn spend_balance(e: &Env, id: Identifier, amount: u64) -> Result<(), DTokenError> {
    let balance = read_balance(e, id.clone()).unwrap();
    if balance < amount {
        // TODO: couldn't figure out how to return an error with a message here
        Err(DTokenError::BalanceError)
    } else {
        let new_balance = balance
            .checked_sub(amount)
            .ok_or_else(|| DTokenError::OverflowError);
        write_balance(e, id, new_balance.unwrap());
        Ok(())
    }
}

//Unit tests
#[cfg(test)]
mod tests {
    use std::println;

    use super::*;
    use crate::testutils::generate_contract_id;
    use soroban_auth::Identifier;
    use soroban_sdk::testutils::Accounts;
    use soroban_sdk::Env;

    #[test]
    fn test_receive_balance() {
        let e = Env::default();
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let amount = 100;
        let pool_id = generate_contract_id(&e);
        e.as_contract(&pool_id, || {
            receive_balance(&e, bombadil_id.clone(), amount).unwrap();
            assert_eq!(read_balance(&e, bombadil_id).unwrap(), amount);
        });
    }

    #[test]
    fn test_spend_balance() {
        let e = Env::default();
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let amount = 100;
        let pool_id = generate_contract_id(&e);
        e.as_contract(&pool_id, || {
            receive_balance(&e, bombadil_id.clone(), amount).unwrap();
            assert_eq!(read_balance(&e, bombadil_id.clone()).unwrap(), amount);
            spend_balance(&e, bombadil_id.clone(), amount / 2).unwrap();
            assert_eq!(read_balance(&e, bombadil_id.clone()).unwrap(), amount / 2);
        });
    }

    #[test]
    #[should_panic]
    fn test_overflow_panics() {
        let e = Env::default();
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let amount = i128::MAX as u64;
        let pool_id = generate_contract_id(&e);
        e.as_contract(&pool_id, || {
            receive_balance(&e, bombadil_id.clone(), amount).unwrap();
            assert_eq!(read_balance(&e, bombadil_id.clone()).unwrap(), amount);
            receive_balance(&e, bombadil_id.clone(), amount).unwrap();
        });
    }
}
