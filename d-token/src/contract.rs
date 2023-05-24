use crate::{
    admin, balance,
    errors::TokenError,
    events,
    interface::{BlendPoolToken, CAP4606},
    storage::{self, Asset},
};
use soroban_sdk::{contractimpl, panic_with_error, Address, Bytes, Env};

pub struct Token;

#[contractimpl]
impl CAP4606 for Token {
    fn initialize(e: Env, admin: Address, decimal: u32, name: Bytes, symbol: Bytes) {
        if storage::has_pool(&e) {
            panic_with_error!(&e, TokenError::AlreadyInitializedError)
        }
        storage::write_pool(&e, &admin);

        storage::write_decimals(&e, &decimal);
        storage::write_name(&e, &name);
        storage::write_symbol(&e, &symbol);
    }

    // --------------------------------------------------------------------------------
    // Admin interface – privileged functions.
    // --------------------------------------------------------------------------------

    fn clawback(e: Env, from: Address, amount: i128) {
        let admin = storage::read_pool(&e);
        admin.require_auth();

        require_nonnegative(&e, amount);
        balance::spend_balance(&e, &from, &amount).unwrap();

        events::clawback(&e, admin, from, amount);
    }

    fn mint(e: Env, to: Address, amount: i128) {
        let admin = storage::read_pool(&e);
        admin.require_auth();

        require_nonnegative(&e, amount);
        balance::receive_balance(&e, &to, &amount).unwrap();

        events::mint(&e, admin, to, amount);
    }

    fn set_admin(e: Env, _new_admin: Address) {
        panic_with_error!(&e, TokenError::NotImplemented)
    }

    fn set_authorized(e: Env, _id: Address, _authorize: bool) {
        panic_with_error!(&e, TokenError::NotImplemented)
    }

    // --------------------------------------------------------------------------------
    // Token interface
    // --------------------------------------------------------------------------------

    fn increase_allowance(e: Env, _from: Address, _spender: Address, _amount: i128) {
        panic_with_error!(&e, TokenError::NotImplemented)
    }

    fn decrease_allowance(e: Env, _from: Address, _spender: Address, _amount: i128) {
        panic_with_error!(&e, TokenError::NotImplemented)
    }

    fn transfer(e: Env, _from: Address, _to: Address, _amount: i128) {
        panic_with_error!(&e, TokenError::NotImplemented)
    }

    // @dev: Does not implement standard `xfer_from` functionality. Only allows the pool
    //       to transfer tokens without an allowance from one holder to another. This prevents
    //       calls to both clawback and mint to move tokens and keeps events more consistent.
    fn transfer_from(e: Env, spender: Address, from: Address, to: Address, amount: i128) {
        admin::require_is_pool(&e, &spender);
        spender.require_auth();

        require_nonnegative(&e, amount);
        balance::spend_balance(&e, &from, &amount).unwrap();
        balance::receive_balance(&e, &to, &amount).unwrap();

        events::transfer(&e, from, to, amount);
    }

    fn burn(e: Env, _from: Address, _amount: i128) {
        panic_with_error!(&e, TokenError::NotImplemented)
    }

    fn burn_from(e: Env, _spender: Address, _from: Address, _amount: i128) {
        panic_with_error!(&e, TokenError::NotImplemented)
    }

    // --------------------------------------------------------------------------------
    // Read-only Token interface
    // --------------------------------------------------------------------------------

    fn balance(e: Env, id: Address) -> i128 {
        storage::read_balance(&e, &id)
    }

    fn spendable(e: Env, id: Address) -> i128 {
        storage::read_balance(&e, &id)
    }

    fn authorized(e: Env, _id: Address) -> bool {
        panic_with_error!(&e, TokenError::NotImplemented)
    }

    fn allowance(e: Env, _from: Address, _spender: Address) -> i128 {
        panic_with_error!(&e, TokenError::NotImplemented)
    }

    // --------------------------------------------------------------------------------
    // Descriptive Interface
    // --------------------------------------------------------------------------------

    fn decimals(e: Env) -> u32 {
        storage::read_decimals(&e)
    }

    fn name(e: Env) -> Bytes {
        storage::read_name(&e)
    }

    fn symbol(e: Env) -> Bytes {
        storage::read_symbol(&e)
    }
}

pub struct BToken;

#[contractimpl]
impl BlendPoolToken for BToken {
    fn pool(e: Env) -> Address {
        storage::read_pool(&e)
    }

    fn asset(e: Env) -> Asset {
        storage::read_asset(&e)
    }

    fn initialize_asset(e: Env, admin: Address, asset: Address, index: u32) {
        admin::require_is_pool(&e, &admin);
        admin.require_auth();

        if storage::has_asset(&e) {
            panic_with_error!(&e, TokenError::AlreadyInitializedError)
        }
        storage::write_asset(
            &e,
            &Asset {
                id: asset,
                res_index: index,
            },
        )
    }
}

fn require_nonnegative(e: &Env, amount: i128) {
    if amount.is_negative() {
        panic_with_error!(&e, TokenError::NegativeAmountError)
    }
}
