use soroban_auth::Identifier;
use soroban_sdk::{ConversionError, Env};

use crate::{
    errors::TokenError,
    storage_types::{AllowanceDataKey, DataKey},
};

pub fn read_allowance(
    e: &Env,
    from: Identifier,
    spender: Identifier,
) -> Result<u64, ConversionError> {
    let key = DataKey::Allowance(AllowanceDataKey { from, spender });
    e.data().get(key).unwrap_or(Ok(0))
}

pub fn write_allowance(e: &Env, from: Identifier, spender: Identifier, amount: u64) {
    let key = DataKey::Allowance(AllowanceDataKey { from, spender });
    e.data().set(key, amount);
}

pub fn spend_allowance(
    e: &Env,
    from: Identifier,
    spender: Identifier,
    amount: i128,
) -> Result<(), TokenError> {
    let allowance = read_allowance(e, from.clone(), spender.clone()).unwrap();
    let amount_u64: u64 = amount as u64;
    if allowance < amount_u64 {
        Err(TokenError::AllowanceError)
    } else {
        let new_allowance = allowance
            .checked_sub(amount_u64)
            .ok_or_else(|| TokenError::OverflowError);
        write_allowance(e, from, spender, new_allowance.unwrap());
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
    fn test_write_allowance() {
        let e = Env::default();
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());

        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let amount = 100;
        let pool_id = generate_contract_id(&e);
        e.as_contract(&pool_id, || {
            write_allowance(&e, bombadil_id.clone(), samwise_id.clone(), amount);
            assert_eq!(read_allowance(&e, bombadil_id, samwise_id).unwrap(), amount);
        });
    }

    #[test]
    fn test_spend_allowance() {
        let e = Env::default();
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let amount = 100;
        let spend_amount: i128 = 60;
        let end_amount: u64 = 40;
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());

        let pool_id = generate_contract_id(&e);
        e.as_contract(&pool_id, || {
            write_allowance(&e, bombadil_id.clone(), samwise_id.clone(), amount);
            assert_eq!(
                read_allowance(&e, bombadil_id.clone(), samwise_id.clone()).unwrap(),
                amount
            );
            spend_allowance(&e, bombadil_id.clone(), samwise_id.clone(), spend_amount).unwrap();
            assert_eq!(
                read_allowance(&e, bombadil_id, samwise_id).unwrap(),
                end_amount
            );
        });
    }

    #[test]
    fn test_overspend_panics() {
        let e = Env::default();
        let bombadil = e.accounts().generate_and_create();
        let bombadil_id = Identifier::Account(bombadil.clone());
        let samwise = e.accounts().generate_and_create();
        let samwise_id = Identifier::Account(samwise.clone());
        let allow_amount = 100;
        let spend_amount: i128 = i128::MAX;
        let pool_id = generate_contract_id(&e);
        e.as_contract(&pool_id, || {
            write_allowance(&e, bombadil_id.clone(), samwise_id.clone(), allow_amount);
            assert_eq!(
                read_allowance(&e, bombadil_id.clone(), samwise_id.clone()).unwrap(),
                allow_amount
            );
            let result = spend_allowance(&e, bombadil_id.clone(), samwise_id.clone(), spend_amount);
            match result {
                Ok(_) => {
                    assert!(false);
                }
                Err(error) => match error {
                    error => assert_eq!(error, TokenError::AllowanceError),
                },
            }
            assert_eq!(
                read_allowance(&e, bombadil_id.clone(), samwise_id.clone()).unwrap(),
                allow_amount
            );
        });
    }
}
