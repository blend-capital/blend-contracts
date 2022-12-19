use soroban_auth::Identifier;
use soroban_sdk::{ConversionError, Env};

use crate::{errors::TokenError, storage_types::DataKey};

pub fn read_balance(e: &Env, id: Identifier) -> Result<u64, ConversionError> {
    let key = DataKey::Balance(id.clone());
    e.data().get(key).unwrap_or(Ok(0))
}

fn write_balance(e: &Env, id: Identifier, amount: u64) {
    let key = DataKey::Balance(id);
    e.data().set(key, amount);
}

pub fn receive_balance(e: &Env, id: Identifier, amount: i128) -> Result<(), TokenError> {
    let balance = read_balance(e, id.clone()).unwrap();
    let amount_u64 = amount as u64;
    let new_balance = balance
        .checked_add(amount_u64)
        .ok_or_else(|| TokenError::OverflowError)
        .unwrap();
    write_balance(e, id, new_balance);
    Ok(())
}

pub fn spend_balance(e: &Env, id: Identifier, amount: i128) -> Result<(), TokenError> {
    let balance = read_balance(e, id.clone()).unwrap();
    let amount_u64 = amount as u64;
    if balance < amount_u64 {
        // TODO: couldn't figure out how to return an error with a message here
        Err(TokenError::BalanceError)
    } else {
        let new_balance = balance
            .checked_sub(amount_u64)
            .ok_or_else(|| TokenError::OverflowError);
        write_balance(e, id, new_balance.unwrap());
        Ok(())
    }
}

//Unit tests
#[cfg(test)]
mod tests {
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
        let amount_u64 = amount as u64;
        let pool_id = generate_contract_id(&e);
        e.as_contract(&pool_id, || {
            receive_balance(&e, bombadil_id.clone(), amount).unwrap();
            assert_eq!(read_balance(&e, bombadil_id).unwrap(), amount_u64);
        });
    }

    #[test]
    fn test_spend_balance() {
        let e = Env::default();
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let amount = 100;
        let amount_u64 = amount as u64;
        let pool_id = generate_contract_id(&e);
        e.as_contract(&pool_id, || {
            receive_balance(&e, bombadil_id.clone(), amount).unwrap();
            assert_eq!(read_balance(&e, bombadil_id.clone()).unwrap(), amount_u64);
            spend_balance(&e, bombadil_id.clone(), amount / 2).unwrap();
            assert_eq!(
                read_balance(&e, bombadil_id.clone()).unwrap(),
                amount_u64 / 2
            );
        });
    }
    #[test]
    #[should_panic]
    fn test_overflow_panics() {
        let e = Env::default();
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let amount = u64::MAX as i128;
        let amount_u64 = amount as u64;
        let pool_id = generate_contract_id(&e);
        e.as_contract(&pool_id, || {
            assert_eq!(amount, amount_u64 as i128);
            receive_balance(&e, bombadil_id.clone(), amount).unwrap();
            assert_eq!(read_balance(&e, bombadil_id.clone()).unwrap(), amount_u64);
            receive_balance(&e, bombadil_id.clone(), 100).unwrap();
        });
    }
}
