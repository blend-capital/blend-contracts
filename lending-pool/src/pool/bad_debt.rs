use soroban_sdk::{map, panic_with_error, Address, Env, Symbol, unwrap::UnwrapOptimized};

use crate::{
    dependencies::TokenClient,
    emissions,
    errors::PoolError,
    storage::{self, has_auction},
};

use super::Reserve;

/// Manage bad debt for a user. If the user is the backstop, burn the bad debt. Otherwise, transfer
/// the bad debt to the backstop.
///
/// ### Panics
/// If the user does not have bad debt or the backstop's bad debt is not eligible to be burnt
pub fn manage_bad_debt(e: &Env, user: &Address) {
    let backstop = storage::get_backstop(e);
    if user.clone() == backstop {
        burn_backstop_bad_debt(e, &backstop);
    } else {
        transfer_bad_debt_to_backstop(e, user, &backstop);
    }
}

/// Transfer bad debt from a user to the backstop. Validates that the user does hold bad debt
/// and transfers all held d_tokens to the backstop.
///
/// ### Arguments
/// * `user` - The user who has bad debt
///
/// ### Panics
/// If the user does not have bad debt
fn transfer_bad_debt_to_backstop(e: &Env, user: &Address, backstop: &Address) {
    let positions = storage::get_user_positions(e, user);
    if positions.collateral.len() != 0 || positions.liabilities.len() == 0 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    // the user does not have collateral and currently holds a liability meaning they hold bad debt
    // transfer all of the user's debt to the backstop

    let reserve_list = storage::get_res_list(e);
    let backstop_positions = storage::get_user_positions(e, &backstop);
    let mut new_user_positions = positions.clone();
    let mut new_backstop_positions = backstop_positions.clone();
    for (reserve_index, liability_balance) in positions.liabilities.iter_unchecked() {
        // no direct action is taken against the reserve, so the reserve's data does not
        // need to be updated. However, emissions need to be accrued for the user up to
        // this point.
        let asset = reserve_list.get_unchecked(reserve_index).unwrap_optimized();
        let reserve_config = storage::get_res_config(e, &asset);
        let reserve_data = storage::get_res_data(e, &asset);
        emissions::update_emissions(
            e,
            reserve_index,
            reserve_data.d_supply,
            10i128.pow(reserve_config.decimals),
            user,
            liability_balance,
            false,
        );
        new_backstop_positions.add_liabilities(reserve_index, liability_balance);

        e.events().publish(
            (Symbol::new(&e, "bad_debt"), user),
            (asset, liability_balance),
        );
    }
    new_user_positions.liabilities = map![e];

    storage::set_user_positions(e, user, &new_user_positions);
    storage::set_user_positions(e, &backstop, &new_backstop_positions);
}

/// Burn bad debt from the backstop. This can only occur if the backstop module has reached a critical balance and
/// a bad debt auction is not currently occurring.
///
/// ### Panics
/// If the backstop's bad debt is not eligible to be burnt
fn burn_backstop_bad_debt(e: &Env, backstop: &Address) {
    if has_auction(e, &1, backstop) {
        panic_with_error!(e, PoolError::AuctionInProgress);
    }

    // TODO: Determine a healthy number for this
    let backstop_balance = TokenClient::new(e, &storage::get_blnd_token(e)).balance(backstop);
    if backstop_balance > 10_000_0000000 {
        panic_with_error!(e, PoolError::BadRequest);
    }

    let backstop_positions = storage::get_user_positions(e, backstop);
    let mut new_backstop_positions = backstop_positions.clone();

    let pool_config = storage::get_pool_config(e);
    let reserve_list = storage::get_res_list(e);
    for (reserve_index, liability_balance) in backstop_positions.liabilities.iter_unchecked() {
        if liability_balance > 0 {
            // remove liability debtTokens from backstop resulting in a shared loss for
            // token suppliers
            let res_asset_address = reserve_list.get_unchecked(reserve_index).unwrap_optimized();
            let mut reserve = Reserve::load(e, &pool_config, &res_asset_address);
            reserve.d_supply -= liability_balance;
            reserve.store(e);

            e.events().publish(
                (Symbol::new(&e, "bad_debt"), backstop),
                (res_asset_address, liability_balance),
            );
        }
    }
    new_backstop_positions.liabilities = map![e];

    storage::set_user_positions(e, &backstop, &new_backstop_positions);
}

#[cfg(test)]
mod tests {
    use crate::{
        auctions::AuctionData,
        storage::PoolConfig,
        testutils::{create_blnd_token, create_reserve, setup_reserve},
    };

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as _, Ledger, LedgerInfo},
    };

    /***** transfer_bad_debt_to_backstop ******/

    #[test]
    fn test_transfer_bad_debt_happy_path() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let pool_address = Address::random(&e);
        let backstop_address = Address::random(&e);

        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        // setup user (collateralize reserve 0 and borrow reserve 1)
        let liability_amount_0 = 24_0000000;
        let liability_amount_1 = 25_0000000;

        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_address);
            let mut user_config = ReserveUsage::new(0);
            user_config.set_liability(0, true);
            user_config.set_liability(1, true);
            storage::set_user_config(&e, &samwise, &user_config.config);

            let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);
            d_token_0.mint(&samwise, &liability_amount_0);
            let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
            d_token_1.mint(&samwise, &liability_amount_1);

            e.budget().reset_unlimited();
            transfer_bad_debt_to_backstop(&e, &samwise, &backstop_address).unwrap_optimized();

            assert_eq!(d_token_0.balance(&samwise), 0);
            assert_eq!(d_token_0.balance(&backstop_address), liability_amount_0);
            assert_eq!(d_token_1.balance(&samwise), 0);
            assert_eq!(d_token_1.balance(&backstop_address), liability_amount_1);

            let reserve_0_data = storage::get_res_data(&e, &reserve_0.asset);
            let reserve_1_data = storage::get_res_data(&e, &reserve_1.asset);
            assert_eq!(reserve_0_data.last_time, 1500000000);
            assert_eq!(reserve_1_data.last_time, 1500000000);
        });
    }

    #[test]
    fn test_transfer_bad_debt_with_collateral_errors() {
        let e = Env::default();
        e.mock_all_auths();

        let pool_address = Address::random(&e);
        let backstop_address = Address::random(&e);

        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_address);
            let mut user_config = ReserveUsage::new(0);
            user_config.set_liability(0, true);
            user_config.set_liability(1, true);
            user_config.set_supply(1, true);
            storage::set_user_config(&e, &samwise, &user_config.config);

            let result = transfer_bad_debt_to_backstop(&e, &samwise, &backstop_address);

            match result {
                Ok(_) => assert!(false),
                Err(error) => assert_eq!(error, PoolError::BadRequest),
            }
        });
    }

    #[test]
    fn test_transfer_bad_debt_without_liability_errors() {
        let e = Env::default();
        e.mock_all_auths();

        let pool_address = Address::random(&e);
        let backstop_address = Address::random(&e);

        let samwise = Address::random(&e);
        let bombadil = Address::random(&e);

        let mut reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_address);
            let mut user_config = ReserveUsage::new(0);
            user_config.set_supply(1, true);
            storage::set_user_config(&e, &samwise, &user_config.config);

            let result = transfer_bad_debt_to_backstop(&e, &samwise, &backstop_address);

            match result {
                Ok(_) => assert!(false),
                Err(error) => assert_eq!(error, PoolError::BadRequest),
            }
        });
    }

    /***** burn_backstop_bad_debt ******/

    #[test]
    fn test_burn_backstop_bad_debt() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);

        let pool_address = Address::random(&e);
        let backstop_address = Address::random(&e);

        let (_, blnd_client) = create_blnd_token(&e, &pool_address, &bombadil);

        let mut reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);

        blnd_client.mint(&backstop_address, &123);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        // setup user (collateralize reserve 0 and borrow reserve 1)
        let liability_amount_0 = 24_0000000;
        let liability_amount_1 = 25_0000000;

        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_address);

            let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);
            d_token_0.mint(&backstop_address, &liability_amount_0);
            reserve_0.add_liability(&liability_amount_0);
            reserve_0.set_data(&e);
            let d_token_supply_0 = reserve_0.data.d_supply;
            let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
            d_token_1.mint(&backstop_address, &liability_amount_1);
            reserve_1.add_liability(&liability_amount_1);
            reserve_1.set_data(&e);
            let d_token_supply_1 = reserve_1.data.d_supply;

            e.budget().reset_unlimited();
            burn_backstop_bad_debt(&e, &backstop_address).unwrap_optimized();

            assert_eq!(d_token_0.balance(&backstop_address), 0);
            assert_eq!(d_token_1.balance(&backstop_address), 0);

            let reserve_0_data = storage::get_res_data(&e, &reserve_0.asset);
            let reserve_1_data = storage::get_res_data(&e, &reserve_1.asset);
            assert_eq!(reserve_0_data.last_time, 1500000000);
            assert_eq!(
                reserve_0_data.d_supply,
                d_token_supply_0 - liability_amount_0
            );
            assert_eq!(reserve_1_data.last_time, 1500000000);
            assert_eq!(
                reserve_1_data.d_supply,
                d_token_supply_1 - liability_amount_1
            );
        });
    }

    #[test]
    fn test_burn_backstop_bad_debt_with_balance_panics() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);

        let pool_address = Address::random(&e);
        let backstop_address = Address::random(&e);

        let (_, blnd_client) = create_blnd_token(&e, &pool_address, &bombadil);

        let mut reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);

        blnd_client.mint(&backstop_address, &10_000_0000001);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        // setup user (collateralize reserve 0 and borrow reserve 1)
        let liability_amount_0 = 24_0000000;
        let liability_amount_1 = 25_0000000;

        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_address);

            let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);
            d_token_0.mint(&backstop_address, &liability_amount_0);
            reserve_0.add_liability(&liability_amount_0);
            reserve_0.set_data(&e);
            let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
            d_token_1.mint(&backstop_address, &liability_amount_1);
            reserve_1.add_liability(&liability_amount_1);
            reserve_1.set_data(&e);

            e.budget().reset_unlimited();
            let result = burn_backstop_bad_debt(&e, &backstop_address);
            assert_eq!(result, Err(PoolError::BadRequest));
        });
    }

    #[test]
    fn test_burn_backstop_bad_debt_with_auction_panics() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 1500000000,
            protocol_version: 1,
            sequence_number: 123,
            network_id: Default::default(),
            base_reserve: 10,
        });

        let bombadil = Address::random(&e);

        let pool_address = Address::random(&e);
        let backstop_address = Address::random(&e);

        create_blnd_token(&e, &pool_address, &bombadil);

        let mut reserve_0 = create_reserve(&e);
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_0);

        let mut reserve_1 = create_reserve(&e);
        reserve_1.config.index = 1;
        setup_reserve(&e, &pool_address, &bombadil, &mut reserve_1);

        let pool_config = PoolConfig {
            oracle: Address::random(&e),
            bstop_rate: 0_100_000_000,
            status: 0,
        };

        // setup user (collateralize reserve 0 and borrow reserve 1)
        let liability_amount_0 = 24_0000000;
        let liability_amount_1 = 25_0000000;

        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_backstop(&e, &backstop_address);

            let d_token_0 = TokenClient::new(&e, &reserve_0.config.d_token);
            d_token_0.mint(&backstop_address, &liability_amount_0);
            reserve_0.add_liability(&liability_amount_0);
            reserve_0.set_data(&e);
            let d_token_1 = TokenClient::new(&e, &reserve_1.config.d_token);
            d_token_1.mint(&backstop_address, &liability_amount_1);
            reserve_1.add_liability(&liability_amount_1);
            reserve_1.set_data(&e);

            // mock a created liquidation auction
            storage::set_auction(
                &e,
                &1,
                &backstop_address,
                &AuctionData {
                    bid: map![&e],
                    lot: map![&e],
                    block: e.ledger().sequence(),
                },
            );

            e.budget().reset_unlimited();
            let result = burn_backstop_bad_debt(&e, &backstop_address);
            assert_eq!(result, Err(PoolError::AuctionInProgress));
        });
    }
}
