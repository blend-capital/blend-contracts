use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    vec,
    xdr::ScHostAuthErrorCode,
    Address, Env, IntoVal, Status, Symbol,
};

mod common;
use crate::common::{create_b_token, create_lending_pool, BTokenClient, TokenError};

fn create_and_init_b_token<'a>(
    e: &Env,
    pool_address: &Address,
    asset: &Address,
    index: &u32,
) -> (Address, BTokenClient<'a>) {
    let (b_token_id, b_token_client) = create_b_token(e);
    b_token_client.initialize(pool_address, &7, &"name".into_val(e), &"symbol".into_val(e));
    b_token_client.initialize_asset(pool_address, &asset, index);
    (b_token_id, b_token_client)
}

#[test]
fn test_mint() {
    let e = Env::default();
    e.mock_all_auths();
    let bombadil = Address::random(&e);
    let underlying = e.register_stellar_asset_contract(bombadil);

    let pool_address = Address::random(&e);

    let (b_token_id, b_token_client) = create_and_init_b_token(&e, &pool_address, &underlying, &2);

    let samwise = Address::random(&e);
    let sauron = Address::random(&e);

    // verify happy path
    b_token_client.mint(&samwise, &123456789);
    let authorizations = e.auths();
    assert_eq!(
        authorizations[0],
        (
            pool_address.clone(),
            b_token_id.clone(),
            Symbol::new(&e, "mint"),
            vec![&e, samwise.clone().to_raw(), 123456789_i128.into_val(&e)]
        )
    );
    assert_eq!(123456789, b_token_client.balance(&samwise));
    // verify only pool can mint
    let result = b_token_client
        .mock_auths(&[MockAuth {
            address: &sauron,
            nonce: 0,
            invoke: &MockAuthInvoke {
                contract: &b_token_id,
                fn_name: "mint",
                args: vec![&e, samwise.to_raw(), 2_i128.into_val(&e)],
                sub_invokes: &[],
            },
        }])
        .try_mint(&samwise, &2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(ScHostAuthErrorCode::NotAuthorized)
    );
    e.mock_all_auths();
    // verify can't mint a negative number
    let result = b_token_client.try_mint(&samwise, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_clawback() {
    let e = Env::default();
    e.mock_all_auths();

    let bombadil = Address::random(&e);
    let underlying = e.register_stellar_asset_contract(bombadil);

    let pool_address = Address::random(&e);

    let (b_token_id, b_token_client) = create_and_init_b_token(&e, &pool_address, &underlying, &2);

    let samwise = Address::random(&e);
    let sauron = Address::random(&e);

    // verify happy path
    b_token_client.mint(&samwise, &123456789);
    assert_eq!(123456789, b_token_client.balance(&samwise));

    b_token_client.clawback(&samwise, &23456789);
    let authorizations = e.auths();
    assert_eq!(
        authorizations[0],
        (
            pool_address.clone(),
            b_token_id.clone(),
            Symbol::new(&e, "clawback"),
            vec![&e, samwise.clone().to_raw(), 23456789_i128.into_val(&e)]
        )
    );
    assert_eq!(100000000, b_token_client.balance(&samwise));

    // verify only pool can clawback
    let result = b_token_client
        .mock_auths(&[MockAuth {
            address: &sauron,
            nonce: 0,
            invoke: &MockAuthInvoke {
                contract: &b_token_id,
                fn_name: "clawback",
                args: vec![&e, samwise.to_raw(), 2_i128.into_val(&e)],
                sub_invokes: &[],
            },
        }])
        .try_clawback(&samwise, &2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(ScHostAuthErrorCode::NotAuthorized)
    );

    e.mock_all_auths();
    // verify can't clawback a negative number
    let result = b_token_client.try_clawback(&samwise, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_increase_allowance() {
    let e = Env::default();
    e.mock_all_auths();

    let bombadil = Address::random(&e);
    let underlying = e.register_stellar_asset_contract(bombadil);

    let res_index = 7;
    let pool_address = Address::random(&e);

    let (b_token_id, b_token_client) =
        create_and_init_b_token(&e, &pool_address, &underlying, &res_index);

    let samwise = Address::random(&e);
    let spender = Address::random(&e);

    // verify happy path
    b_token_client.increase_allowance(&samwise, &spender, &123456789);
    let authorizations = e.auths();
    assert_eq!(
        authorizations[0],
        (
            samwise.clone(),
            b_token_id.clone(),
            Symbol::new(&e, "increase_allowance"),
            vec![
                &e,
                samwise.clone().to_raw(),
                spender.clone().to_raw(),
                123456789_i128.into_val(&e)
            ]
        )
    );
    assert_eq!(123456789, b_token_client.allowance(&samwise, &spender));

    // verify negative balance cannot be used
    let result = b_token_client.try_increase_allowance(&samwise, &spender, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_decrease_allowance() {
    let e = Env::default();
    e.mock_all_auths();

    let bombadil = Address::random(&e);
    let underlying = e.register_stellar_asset_contract(bombadil);

    let res_index = 7;
    let pool_address = Address::random(&e);

    let (b_token_id, b_token_client) =
        create_and_init_b_token(&e, &pool_address, &underlying, &res_index);

    let samwise = Address::random(&e);
    let spender = Address::random(&e);

    // verify happy path
    b_token_client.increase_allowance(&samwise, &spender, &123456789);
    b_token_client.decrease_allowance(&samwise, &spender, &23456789);
    let authorizations = e.auths();
    assert_eq!(
        authorizations[0],
        (
            samwise.clone(),
            b_token_id.clone(),
            Symbol::new(&e, "decrease_allowance"),
            vec![
                &e,
                samwise.clone().to_raw(),
                spender.clone().to_raw(),
                23456789_i128.into_val(&e)
            ]
        )
    );
    assert_eq!(100000000, b_token_client.allowance(&samwise, &spender));

    // verify negative balance cannot be used
    let result = b_token_client.try_decrease_allowance(&samwise, &spender, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_transfer() {
    let e = Env::default();
    e.mock_all_auths();

    let bombadil = Address::random(&e);
    let underlying = e.register_stellar_asset_contract(bombadil);

    let res_index = 7;
    let (pool_address, pool_client) = create_lending_pool(&e);
    let (b_token_id, b_token_client) =
        create_and_init_b_token(&e, &pool_address, &underlying, &res_index);

    let samwise = Address::random(&e);
    let frodo = Address::random(&e);

    // verify happy path
    b_token_client.mint(&samwise, &123456789);
    assert_eq!(123456789, b_token_client.balance(&samwise));
    pool_client.set_collat(&samwise, &res_index, &false);

    b_token_client.transfer(&samwise, &frodo, &23456789);
    let authorizations = e.auths();
    assert_eq!(
        authorizations[0],
        (
            samwise.clone(),
            b_token_id.clone(),
            Symbol::new(&e, "transfer"),
            vec![
                &e,
                samwise.clone().to_raw(),
                frodo.clone().to_raw(),
                23456789_i128.into_val(&e)
            ]
        )
    );
    assert_eq!(100000000, b_token_client.balance(&samwise));
    assert_eq!(23456789, b_token_client.balance(&frodo));

    // verify collateralized balance cannot transfer
    pool_client.set_collat(&frodo, &res_index, &true);
    let result = b_token_client.try_transfer(&frodo, &samwise, &1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::UnauthorizedError)
    );

    // verify negative balance cannot be used
    let result = b_token_client.try_transfer(&samwise, &frodo, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );
}

#[test]
fn test_transfer_from() {
    let e = Env::default();
    e.mock_all_auths();

    let bombadil = Address::random(&e);
    let underlying = e.register_stellar_asset_contract(bombadil);

    let res_index = 7;
    let (pool_address, pool_client) = create_lending_pool(&e);
    let (b_token_id, b_token_client) =
        create_and_init_b_token(&e, &pool_address, &underlying, &res_index);

    let samwise = Address::random(&e);
    let frodo = Address::random(&e);
    let spender = Address::random(&e);

    // verify happy path
    b_token_client.mint(&samwise, &123456789);
    assert_eq!(123456789, b_token_client.balance(&samwise));
    pool_client.set_collat(&samwise, &res_index, &false);

    b_token_client.increase_allowance(&samwise, &spender, &223456789);
    b_token_client.transfer_from(&spender, &samwise, &frodo, &23456789);
    let authorizations = e.auths();
    assert_eq!(
        authorizations[0],
        (
            spender.clone(),
            b_token_id.clone(),
            Symbol::new(&e, "transfer_from"),
            vec![
                &e,
                spender.clone().to_raw(),
                samwise.clone().to_raw(),
                frodo.clone().to_raw(),
                23456789_i128.into_val(&e)
            ]
        )
    );
    assert_eq!(100000000, b_token_client.balance(&samwise));
    assert_eq!(23456789, b_token_client.balance(&frodo));
    assert_eq!(200000000, b_token_client.allowance(&samwise, &spender));

    // verify negative balance cannot be used
    let result = b_token_client.try_transfer_from(&spender, &samwise, &frodo, &-1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::NegativeAmountError)
    );

    // verify collateralized balance cannot transfer
    pool_client.set_collat(&samwise, &res_index, &true);
    let result = b_token_client.try_transfer_from(&spender, &samwise, &frodo, &1);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Status::from(TokenError::UnauthorizedError)
    );
}
