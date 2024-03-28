use soroban_sdk::Map;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Symbol, Vec};

use crate::{auctions, errors::PoolError, validator::require_nonnegative};

use super::pool::Pool;
use super::User;

/// A request a user makes against the pool
#[derive(Clone)]
#[contracttype]
pub struct Request {
    pub request_type: u32,
    pub address: Address, // asset address or liquidatee
    pub amount: i128,
}

/// The type of request to be made against the pool
#[derive(Clone, PartialEq)]
#[repr(u32)]
pub enum RequestType {
    Supply = 0,
    Withdraw = 1,
    SupplyCollateral = 2,
    WithdrawCollateral = 3,
    Borrow = 4,
    Repay = 5,
    FillUserLiquidationAuction = 6,
    FillBadDebtAuction = 7,
    FillInterestAuction = 8,
    DeleteLiquidationAuction = 9,
}

impl RequestType {
    /// Convert a u32 to a RequestType
    ///
    /// ### Panics
    /// If the value is not a valid RequestType
    pub fn from_u32(e: &Env, value: u32) -> Self {
        match value {
            0 => RequestType::Supply,
            1 => RequestType::Withdraw,
            2 => RequestType::SupplyCollateral,
            3 => RequestType::WithdrawCollateral,
            4 => RequestType::Borrow,
            5 => RequestType::Repay,
            6 => RequestType::FillUserLiquidationAuction,
            7 => RequestType::FillBadDebtAuction,
            8 => RequestType::FillInterestAuction,
            9 => RequestType::DeleteLiquidationAuction,
            _ => panic_with_error!(e, PoolError::BadRequest),
        }
    }
}

/// Transfer actions to be taken by the sender and pool
pub struct Actions {
    pub spender_transfer: Map<Address, i128>,
    pub pool_transfer: Map<Address, i128>,
}

impl Actions {
    /// Create an empty set of actions
    pub fn new(e: &Env) -> Self {
        Actions {
            spender_transfer: Map::new(e),
            pool_transfer: Map::new(e),
        }
    }

    /// Add tokens the sender needs to transfer to the pool
    pub fn add_for_spender_transfer(&mut self, asset: &Address, amount: i128) {
        self.spender_transfer.set(
            asset.clone(),
            amount + self.spender_transfer.get(asset.clone()).unwrap_or(0),
        );
    }

    // Add tokens the pool needs to transfer to "to"
    pub fn add_for_pool_transfer(&mut self, asset: &Address, amount: i128) {
        self.pool_transfer.set(
            asset.clone(),
            amount + self.pool_transfer.get(asset.clone()).unwrap_or(0),
        );
    }
}

/// Build a set of pool actions and the new positions from the supplied requests. Validates that the requests
/// are valid based on the status and supported reserves in the pool.
///
/// ### Arguments
/// * pool - The pool
/// * from - The sender of the requests
/// * requests - The requests to be processed
///
/// ### Returns
/// A tuple of (actions, positions, check_health) where:
/// * actions - A actions to be taken by the pool
/// * user - The state of the "from" user after the requests have been processed
/// * check_health - A bool indicating if a health factor check should be performed
///
/// ### Panics
/// If the request is invalid, or if the pool is in an invalid state.
pub fn build_actions_from_request(
    e: &Env,
    pool: &mut Pool,
    from: &Address,
    requests: Vec<Request>,
) -> (Actions, User, bool) {
    let mut actions = Actions::new(e);
    let mut from_state = User::load(e, from);
    let prev_positions_count = from_state.positions.effective_count();
    let mut check_health = false;
    for request in requests.iter() {
        // verify the request is allowed
        require_nonnegative(e, &request.amount);
        pool.require_action_allowed(e, request.request_type);
        match RequestType::from_u32(e, request.request_type) {
            RequestType::Supply => {
                let mut reserve = pool.load_reserve(e, &request.address, true);
                let b_tokens_minted = reserve.to_b_token_down(request.amount);
                from_state.add_supply(e, &mut reserve, b_tokens_minted);
                actions.add_for_spender_transfer(&reserve.asset, request.amount);
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(e, "supply"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (request.amount, b_tokens_minted),
                );
            }
            RequestType::Withdraw => {
                let mut reserve = pool.load_reserve(e, &request.address, true);
                let cur_b_tokens = from_state.get_supply(reserve.index);
                let mut to_burn = reserve.to_b_token_up(request.amount);
                let mut tokens_out = request.amount;
                if to_burn > cur_b_tokens {
                    to_burn = cur_b_tokens;
                    tokens_out = reserve.to_asset_from_b_token(cur_b_tokens);
                }
                from_state.remove_supply(e, &mut reserve, to_burn);
                actions.add_for_pool_transfer(&reserve.asset, tokens_out);
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(e, "withdraw"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (tokens_out, to_burn),
                );
            }
            RequestType::SupplyCollateral => {
                let mut reserve = pool.load_reserve(e, &request.address, true);
                let b_tokens_minted = reserve.to_b_token_down(request.amount);
                from_state.add_collateral(e, &mut reserve, b_tokens_minted);
                actions.add_for_spender_transfer(&reserve.asset, request.amount);
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(e, "supply_collateral"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (request.amount, b_tokens_minted),
                );
            }
            RequestType::WithdrawCollateral => {
                let mut reserve = pool.load_reserve(e, &request.address, true);
                let cur_b_tokens = from_state.get_collateral(reserve.index);
                let mut to_burn = reserve.to_b_token_up(request.amount);
                let mut tokens_out = request.amount;
                if to_burn > cur_b_tokens {
                    to_burn = cur_b_tokens;
                    tokens_out = reserve.to_asset_from_b_token(cur_b_tokens);
                }
                from_state.remove_collateral(e, &mut reserve, to_burn);
                actions.add_for_pool_transfer(&reserve.asset, tokens_out);
                check_health = true;
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(e, "withdraw_collateral"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (tokens_out, to_burn),
                );
            }
            RequestType::Borrow => {
                let mut reserve = pool.load_reserve(e, &request.address, true);
                let d_tokens_minted = reserve.to_d_token_up(request.amount);
                from_state.add_liabilities(e, &mut reserve, d_tokens_minted);
                reserve.require_utilization_below_max(e);
                actions.add_for_pool_transfer(&reserve.asset, request.amount);
                check_health = true;
                pool.cache_reserve(reserve);
                e.events().publish(
                    (
                        Symbol::new(e, "borrow"),
                        request.address.clone(),
                        from.clone(),
                    ),
                    (request.amount, d_tokens_minted),
                );
            }
            RequestType::Repay => {
                let mut reserve = pool.load_reserve(e, &request.address, true);
                let cur_d_tokens = from_state.get_liabilities(reserve.index);
                let d_tokens_burnt = reserve.to_d_token_down(request.amount);
                actions.add_for_spender_transfer(&reserve.asset, request.amount);
                if d_tokens_burnt > cur_d_tokens {
                    let amount_to_refund =
                        request.amount - reserve.to_asset_from_d_token(cur_d_tokens);
                    require_nonnegative(e, &amount_to_refund);
                    from_state.remove_liabilities(e, &mut reserve, cur_d_tokens);
                    actions.add_for_pool_transfer(&reserve.asset, amount_to_refund);
                    e.events().publish(
                        (
                            Symbol::new(e, "repay"),
                            request.address.clone().clone(),
                            from.clone(),
                        ),
                        (request.amount - amount_to_refund, cur_d_tokens),
                    );
                } else {
                    from_state.remove_liabilities(e, &mut reserve, d_tokens_burnt);
                    e.events().publish(
                        (
                            Symbol::new(e, "repay"),
                            request.address.clone().clone(),
                            from.clone(),
                        ),
                        (request.amount, d_tokens_burnt),
                    );
                }
                pool.cache_reserve(reserve);
            }
            RequestType::FillUserLiquidationAuction => {
                auctions::fill(
                    e,
                    pool,
                    0,
                    &request.address,
                    &mut from_state,
                    request.amount as u64,
                );
                check_health = true;

                e.events().publish(
                    (
                        Symbol::new(e, "fill_auction"),
                        request.address.clone().clone(),
                        0_u32,
                    ),
                    (from.clone(), request.amount),
                );
            }
            RequestType::FillBadDebtAuction => {
                // Note: will fail if input address is not the backstop since there cannot be a bad debt auction for a different address in storage
                auctions::fill(
                    e,
                    pool,
                    1,
                    &request.address,
                    &mut from_state,
                    request.amount as u64,
                );
                check_health = true;

                e.events().publish(
                    (
                        Symbol::new(e, "fill_auction"),
                        request.address.clone().clone(),
                        1_u32,
                    ),
                    (from.clone(), request.amount),
                );
            }
            RequestType::FillInterestAuction => {
                // Note: will fail if input address is not the backstop since there cannot be an interest auction for a different address in storage
                auctions::fill(
                    e,
                    pool,
                    2,
                    &request.address,
                    &mut from_state,
                    request.amount as u64,
                );
                e.events().publish(
                    (
                        Symbol::new(e, "fill_auction"),
                        request.address.clone().clone(),
                        2_u32,
                    ),
                    (from.clone(), request.amount),
                );
            }
            RequestType::DeleteLiquidationAuction => {
                // Note: request object is ignored besides type
                auctions::delete_liquidation(e, &from);
                check_health = true;
                e.events().publish(
                    (Symbol::new(&e, "delete_liquidation_auction"), from.clone()),
                    (),
                );
            }
        }
    }

    // Verify max positions haven't been exceeded
    pool.require_under_max(e, &from_state.positions, prev_positions_count);

    (actions, from_state, check_health)
}

#[cfg(test)]
mod tests {

    use crate::{
        constants::SCALAR_7,
        storage::{self, PoolConfig},
        testutils::{self, create_comet_lp_pool, create_pool},
        AuctionData, AuctionType, Positions,
    };

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as _, Ledger, LedgerInfo},
        vec,
    };

    // d_rate -> 1_000_001_142
    // b_rate -> 1_000_000_686

    /***** supply *****/

    #[test]
    fn test_build_actions_from_request_supply() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::Supply as u32,
                    address: underlying.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, user, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            let spender_transfer = actions.spender_transfer;
            let pool_transfer = actions.pool_transfer;
            assert_eq!(spender_transfer.len(), 1);
            assert_eq!(
                spender_transfer.get_unchecked(underlying.clone()),
                10_1234567
            );
            assert_eq!(pool_transfer.len(), 0);

            let positions = user.positions.clone();
            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 1);
            assert_eq!(user.get_supply(0), 10_1234488);

            let reserve = pool.load_reserve(&e, &underlying, false);
            assert_eq!(reserve.b_supply, reserve_data.b_supply + user.get_supply(0));
        });
    }

    /***** withdraw *****/

    #[test]
    fn test_build_actions_from_request_withdraw() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };

        let user_positions = Positions {
            liabilities: map![&e],
            collateral: map![&e],
            supply: map![&e, (0, 20_0000000)],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::Withdraw as u32,
                    address: underlying.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, user, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            let spender_transfer = actions.spender_transfer;
            let pool_transfer = actions.pool_transfer;
            assert_eq!(spender_transfer.len(), 0);
            assert_eq!(pool_transfer.len(), 1);
            assert_eq!(pool_transfer.get_unchecked(underlying.clone()), 10_1234567);

            let positions = user.positions.clone();
            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 1);
            assert_eq!(user.get_supply(0), 9_8765502);

            let reserve = pool.load_reserve(&e, &underlying, false);
            assert_eq!(
                reserve.b_supply,
                reserve_data.b_supply - (20_0000000 - 9_8765502)
            );
        });
    }

    #[test]
    fn test_build_actions_from_request_withdraw_over_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        let user_positions = Positions {
            liabilities: map![&e],
            collateral: map![&e],
            supply: map![&e, (0, 20_0000000)],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::Withdraw as u32,
                    address: underlying.clone(),
                    amount: 21_0000000,
                },
            ];
            let (actions, user, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            let spender_transfer = actions.spender_transfer;
            let pool_transfer = actions.pool_transfer;
            assert_eq!(spender_transfer.len(), 0);
            assert_eq!(pool_transfer.len(), 1);
            assert_eq!(pool_transfer.get_unchecked(underlying.clone()), 20_0000137);

            let positions = user.positions.clone();
            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);

            let reserve = pool.load_reserve(&e, &underlying.clone(), false);
            assert_eq!(reserve.b_supply, reserve_data.b_supply - 20_0000000);
        });
    }

    /***** supply collateral *****/

    #[test]
    fn test_build_actions_from_request_supply_collateral() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::SupplyCollateral as u32,
                    address: underlying.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, user, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            let spender_transfer = actions.spender_transfer;
            let pool_transfer = actions.pool_transfer;
            assert_eq!(spender_transfer.len(), 1);
            assert_eq!(
                spender_transfer.get_unchecked(underlying.clone()),
                10_1234567
            );
            assert_eq!(pool_transfer.len(), 0);

            let positions = user.positions.clone();
            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 1);
            assert_eq!(positions.supply.len(), 0);
            assert_eq!(user.get_collateral(0), 10_1234488);

            let reserve = pool.load_reserve(&e, &underlying.clone(), false);
            assert_eq!(
                reserve.b_supply,
                reserve_data.b_supply + user.get_collateral(0)
            );
        });
    }

    /***** withdraw collateral *****/

    #[test]
    fn test_build_actions_from_request_withdraw_collateral() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        let user_positions = Positions {
            liabilities: map![&e],
            collateral: map![&e, (0, 20_0000000)],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::WithdrawCollateral as u32,
                    address: underlying.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, user, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);

            let spender_transfer = actions.spender_transfer;
            let pool_transfer = actions.pool_transfer;
            assert_eq!(spender_transfer.len(), 0);
            assert_eq!(pool_transfer.len(), 1);
            assert_eq!(pool_transfer.get_unchecked(underlying.clone()), 10_1234567);

            let positions = user.positions.clone();
            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 1);
            assert_eq!(positions.supply.len(), 0);
            assert_eq!(user.get_collateral(0), 9_8765502);

            let reserve = pool.load_reserve(&e, &underlying, false);
            assert_eq!(
                reserve.b_supply,
                reserve_data.b_supply - (20_0000000 - 9_8765502)
            );
        });
    }

    #[test]
    fn test_build_actions_from_request_withdraw_collateral_over_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        let user_positions = Positions {
            liabilities: map![&e],
            collateral: map![&e, (0, 20_0000000)],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::WithdrawCollateral as u32,
                    address: underlying.clone(),
                    amount: 21_0000000,
                },
            ];
            let (actions, user, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);

            let spender_transfer = actions.spender_transfer;
            let pool_transfer = actions.pool_transfer;
            assert_eq!(spender_transfer.len(), 0);
            assert_eq!(pool_transfer.len(), 1);
            assert_eq!(pool_transfer.get_unchecked(underlying.clone()), 20_0000137);

            let positions = user.positions.clone();
            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);

            let reserve = pool.load_reserve(&e, &underlying, false);
            assert_eq!(reserve.b_supply, reserve_data.b_supply - 20_0000000);
        });
    }

    /***** borrow *****/

    #[test]
    fn test_build_actions_from_request_borrow() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);
        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::Borrow as u32,
                    address: underlying.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, user, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);
            assert_eq!(health_check, true);

            let spender_transfer = actions.spender_transfer;
            let pool_transfer = actions.pool_transfer;
            assert_eq!(spender_transfer.len(), 0);
            assert_eq!(pool_transfer.len(), 1);
            assert_eq!(pool_transfer.get_unchecked(underlying.clone()), 10_1234567);

            let positions = user.positions.clone();
            assert_eq!(positions.liabilities.len(), 1);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);
            assert_eq!(user.get_liabilities(0), 10_1234452);

            let reserve = pool.load_reserve(&e, &underlying, false);
            assert_eq!(reserve.d_supply, reserve_data.d_supply + 10_1234452);
        });
    }

    /***** repay *****/

    #[test]
    fn test_build_actions_from_request_repay() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        let user_positions = Positions {
            liabilities: map![&e, (0, 20_0000000)],
            collateral: map![&e],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::Repay as u32,
                    address: underlying.clone(),
                    amount: 10_1234567,
                },
            ];
            let (actions, user, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            let spender_transfer = actions.spender_transfer;
            let pool_transfer = actions.pool_transfer;
            assert_eq!(spender_transfer.len(), 1);
            assert_eq!(
                spender_transfer.get_unchecked(underlying.clone()),
                10_1234567
            );
            assert_eq!(pool_transfer.len(), 0);

            let positions = user.positions.clone();
            assert_eq!(positions.liabilities.len(), 1);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);
            let d_tokens_repaid = 10_1234451;
            assert_eq!(user.get_liabilities(0), 20_0000000 - d_tokens_repaid);

            let reserve = pool.load_reserve(&e, &underlying, false);
            assert_eq!(reserve.d_supply, reserve_data.d_supply - d_tokens_repaid);
        });
    }

    #[test]
    fn test_build_actions_from_request_repay_over_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        let user_positions = Positions {
            liabilities: map![&e, (0, 20_0000000)],
            collateral: map![&e],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::Repay as u32,
                    address: underlying.clone(),
                    amount: 21_0000000,
                },
            ];
            let (actions, user, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, false);

            let spender_transfer = actions.spender_transfer;
            let pool_transfer = actions.pool_transfer;
            assert_eq!(spender_transfer.len(), 1);
            assert_eq!(
                spender_transfer.get_unchecked(underlying.clone()),
                21_0000000
            );
            assert_eq!(pool_transfer.len(), 1);
            assert_eq!(pool_transfer.get_unchecked(underlying.clone()), 0_9999771);

            let positions = user.positions.clone();
            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 0);
            assert_eq!(positions.supply.len(), 0);

            let reserve = pool.load_reserve(&e, &underlying, false);
            assert_eq!(reserve.d_supply, reserve_data.d_supply - 20_0000000);
        });
    }

    #[test]
    fn test_aggregating_actions() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, mut reserve_data) = testutils::default_reserve_meta();
        reserve_data.last_time = 600;
        testutils::create_reserve(
            &e,
            &pool,
            &underlying.clone(),
            &reserve_config,
            &reserve_data,
        );

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });
        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };
        let user_positions = Positions::env_default(&e);
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::Supply as u32,
                    address: underlying.clone(),
                    amount: 10_0000000,
                },
                Request {
                    request_type: RequestType::Withdraw as u32,
                    address: underlying.clone(),
                    amount: 5_0000000,
                },
                Request {
                    request_type: RequestType::SupplyCollateral as u32,
                    address: underlying.clone(),
                    amount: 10_0000000,
                },
                Request {
                    request_type: RequestType::WithdrawCollateral as u32,
                    address: underlying.clone(),
                    amount: 5_0000000,
                },
                Request {
                    request_type: RequestType::Borrow as u32,
                    address: underlying.clone(),
                    amount: 20_0000000,
                },
                Request {
                    request_type: RequestType::Repay as u32,
                    address: underlying.clone(),
                    amount: 21_0000000,
                },
            ];
            let (actions, user, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);

            let spender_transfer = actions.spender_transfer;
            let pool_transfer = actions.pool_transfer;
            assert_eq!(spender_transfer.len(), 1);
            assert_eq!(
                spender_transfer.get_unchecked(underlying.clone()),
                10_0000000 + 10_0000000 + 21_0000000
            );
            assert_eq!(pool_transfer.len(), 1);
            assert_eq!(
                pool_transfer.get_unchecked(underlying.clone()),
                5_0000000 + 5_0000000 + 20_0000000 + 1_0000000
            );

            let positions = user.positions.clone();
            assert_eq!(positions.liabilities.len(), 0);
            assert_eq!(positions.collateral.len(), 1);
            assert_eq!(positions.supply.len(), 1);
            assert_eq!(positions.collateral.get_unchecked(0), 5_0000000);
            assert_eq!(positions.supply.get_unchecked(0), 5_0000000);
        });
    }

    #[test]
    fn test_fill_user_liquidation() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 176 + 200,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
        reserve_data_0.last_time = 12345;
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_config_0.c_factor = 0_8500000;
        reserve_config_0.l_factor = 0_9000000;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
        reserve_data_1.b_rate = 1_200_000_000;
        reserve_config_1.c_factor = 0_7500000;
        reserve_config_1.l_factor = 0_7500000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );

        let (underlying_2, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, reserve_data_2) = testutils::default_reserve_meta();
        reserve_config_2.c_factor = 0_0000000;
        reserve_config_2.l_factor = 0_7000000;
        reserve_config_2.index = 2;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );

        let auction_data = AuctionData {
            bid: map![&e, (underlying_2.clone(), 1_2375000)],
            lot: map![
                &e,
                (underlying_0.clone(), 30_5595329),
                (underlying_1.clone(), 1_5395739)
            ],
            block: 176,
        };
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 4,
        };
        let positions: Positions = Positions {
            collateral: map![
                &e,
                (reserve_config_0.index, 90_9100000),
                (reserve_config_1.index, 04_5800000),
            ],
            liabilities: map![&e, (reserve_config_2.index, 02_7500000),],
            supply: map![&e],
        };
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &positions);
            storage::set_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise,
                &auction_data,
            );

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::FillUserLiquidationAuction as u32,
                    address: samwise.clone(),
                    amount: 50,
                },
            ];
            let (actions, _, health_check) =
                build_actions_from_request(&e, &mut pool, &frodo, requests);

            assert_eq!(health_check, true);
            let exp_new_auction = AuctionData {
                bid: map![&e, (underlying_2.clone(), 6187500)],
                lot: map![
                    &e,
                    (underlying_0.clone(), 15_2797665),
                    (underlying_1.clone(), 7697870)
                ],
                block: 176,
            };
            let new_auction =
                storage::get_auction(&e, &(AuctionType::UserLiquidation as u32), &samwise);
            assert_eq!(exp_new_auction.bid, new_auction.bid);
            assert_eq!(exp_new_auction.lot, new_auction.lot);
            assert_eq!(exp_new_auction.block, new_auction.block);
            assert_eq!(actions.pool_transfer.len(), 0);
            assert_eq!(actions.spender_transfer.len(), 0);
        });
    }

    #[test]
    fn test_fill_bad_debt_auction() {
        let e = Env::default();

        e.mock_all_auths();
        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 51 + 200,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let frodo = Address::generate(&e);

        let pool_address = create_pool(&e);

        let (oracle_address, _) = testutils::create_mock_oracle(&e);

        // creating reserves for a pool exhausts the budget
        e.budget().reset_unlimited();
        let (backstop_token_id, backstop_token_client) =
            testutils::create_token_contract(&e, &bombadil);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &Address::generate(&e),
            &Address::generate(&e),
        );
        let (underlying_0, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
        reserve_data_0.last_time = 12345;
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_config_0.c_factor = 0_8500000;
        reserve_config_0.l_factor = 0_9000000;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
        reserve_data_1.b_rate = 1_200_000_000;
        reserve_config_1.c_factor = 0_7500000;
        reserve_config_1.l_factor = 0_7500000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );
        let pool_config = PoolConfig {
            oracle: oracle_address,
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        let auction_data = AuctionData {
            bid: map![&e, (underlying_0, 10_0000000), (underlying_1, 2_5000000)],
            lot: map![&e, (backstop_token_id, 95_2000000)],
            block: 51,
        };
        let positions: Positions = Positions {
            collateral: map![&e],
            liabilities: map![
                &e,
                (reserve_config_0.index, 10_0000000),
                (reserve_config_1.index, 2_5000000)
            ],
            supply: map![&e],
        };
        backstop_token_client.mint(&samwise, &95_2000000);
        backstop_token_client.approve(&samwise, &backstop_address, &i128::MAX, &1000000);
        backstop_client.deposit(&samwise, &pool_address, &95_2000000);
        e.as_contract(&pool_address, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &backstop_address, &positions);
            storage::set_auction(
                &e,
                &(AuctionType::BadDebtAuction as u32),
                &backstop_address,
                &auction_data,
            );

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::FillBadDebtAuction as u32,
                    address: backstop_address.clone(),
                    amount: 100,
                },
            ];
            let (actions, _, health_check) =
                build_actions_from_request(&e, &mut pool, &frodo, requests);

            assert_eq!(health_check, true);
            assert_eq!(
                storage::has_auction(&e, &(AuctionType::BadDebtAuction as u32), &backstop_address),
                false
            );
            assert_eq!(actions.pool_transfer.len(), 0);
            assert_eq!(actions.spender_transfer.len(), 0);
        });
    }

    #[test]
    fn test_fill_interest_auction() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 51 + 250,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);

        let pool_address = create_pool(&e);
        let (usdc_id, usdc_client) = testutils::create_token_contract(&e, &bombadil);
        let (blnd_id, blnd_client) = testutils::create_blnd_token(&e, &pool_address, &bombadil);

        let (backstop_token_id, backstop_token_client) =
            create_comet_lp_pool(&e, &bombadil, &blnd_id, &usdc_id);
        let (backstop_address, backstop_client) = testutils::create_backstop(&e);
        blnd_client.mint(&samwise, &10_000_0000000);
        usdc_client.mint(&samwise, &250_0000000);
        let exp_ledger = e.ledger().sequence() + 100;
        blnd_client.approve(&bombadil, &backstop_token_id, &2_000_0000000, &exp_ledger);
        usdc_client.approve(&bombadil, &backstop_token_id, &2_000_0000000, &exp_ledger);
        backstop_token_client.join_pool(
            &(100 * SCALAR_7),
            &vec![&e, 10_000_0000000, 250_0000000],
            &samwise,
        );
        testutils::setup_backstop(
            &e,
            &pool_address,
            &backstop_address,
            &backstop_token_id,
            &usdc_id,
            &blnd_id,
        );
        backstop_client.deposit(&bombadil, &pool_address, &(50 * SCALAR_7));
        backstop_client.update_tkn_val();

        let (underlying_0, underlying_0_client) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_0, mut reserve_data_0) = testutils::default_reserve_meta();
        reserve_data_0.b_rate = 1_100_000_000;
        reserve_data_0.last_time = 12345;
        reserve_config_0.index = 0;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_0,
            &reserve_config_0,
            &reserve_data_0,
        );
        underlying_0_client.mint(&pool_address, &1_000_0000000);

        let (underlying_1, underlying_1_client) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_1, mut reserve_data_1) = testutils::default_reserve_meta();
        reserve_data_1.b_rate = 1_100_000_000;
        reserve_data_1.last_time = 12345;
        reserve_config_1.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_1,
            &reserve_config_1,
            &reserve_data_1,
        );
        underlying_1_client.mint(&pool_address, &1_000_0000000);

        let (underlying_2, underlying_2_client) = testutils::create_token_contract(&e, &bombadil);
        let (mut reserve_config_2, mut reserve_data_2) = testutils::default_reserve_meta();
        reserve_data_2.b_rate = 1_100_000_000;
        reserve_data_2.last_time = 12345;
        reserve_config_2.index = 1;
        testutils::create_reserve(
            &e,
            &pool_address,
            &underlying_2,
            &reserve_config_2,
            &reserve_data_2,
        );
        underlying_2_client.mint(&pool_address, &1_000_0000000);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        let auction_data = AuctionData {
            bid: map![&e, (backstop_token_id.clone(), 100_0000000)],
            lot: map![
                &e,
                (underlying_0.clone(), 100_0000000),
                (underlying_1.clone(), 25_0000000)
            ],
            block: 51,
        };

        e.as_contract(&pool_address, || {
            e.mock_all_auths_allowing_non_root_auth();
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(
                &e,
                &(AuctionType::InterestAuction as u32),
                &backstop_address,
                &auction_data,
            );
            storage::set_backstop(&e, &backstop_address);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::FillInterestAuction as u32,
                    address: backstop_address.clone(),
                    amount: 100,
                },
            ];
            let pre_fill_backstop_token_balance = backstop_token_client.balance(&backstop_address);
            let (actions, _, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(backstop_token_client.balance(&samwise), 25_0000000);
            assert_eq!(
                backstop_token_client.balance(&backstop_address),
                pre_fill_backstop_token_balance + 75_0000000
            );
            assert_eq!(underlying_0_client.balance(&samwise), 100_0000000);
            assert_eq!(underlying_1_client.balance(&samwise), 25_0000000);
            assert_eq!(health_check, false);
            assert_eq!(
                storage::has_auction(
                    &e,
                    &(AuctionType::InterestAuction as u32),
                    &backstop_address
                ),
                false
            );
            assert_eq!(actions.pool_transfer.len(), 0);
            assert_eq!(actions.spender_transfer.len(), 0);
        });
    }

    /***** delete liquidation auction *****/

    #[test]
    fn test_delete_liquidation_auction() {
        let e = Env::default();
        e.budget().reset_unlimited();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 51 + 200,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let samwise = Address::generate(&e);
        let underlying_0 = Address::generate(&e);
        let underlying_1 = Address::generate(&e);

        let pool_address = create_pool(&e);

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_1000000,
            status: 0,
            max_positions: 2,
        };
        let auction_data = AuctionData {
            bid: map![&e, (underlying_0.clone(), 952_0000000)],
            lot: map![
                &e,
                (underlying_0.clone(), 100_0000000),
                (underlying_1.clone(), 25_0000000)
            ],
            block: 51,
        };

        e.as_contract(&pool_address, || {
            e.mock_all_auths_allowing_non_root_auth();
            storage::set_pool_config(&e, &pool_config);
            storage::set_auction(
                &e,
                &(AuctionType::UserLiquidation as u32),
                &samwise,
                &auction_data,
            );

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::DeleteLiquidationAuction as u32,
                    address: Address::generate(&e),
                    amount: 0,
                },
            ];
            let (actions, _, health_check) =
                build_actions_from_request(&e, &mut pool, &samwise, requests);

            assert_eq!(health_check, true);
            assert_eq!(
                storage::has_auction(&e, &(AuctionType::UserLiquidation as u32), &samwise),
                false
            );
            assert_eq!(actions.pool_transfer.len(), 0);
            assert_eq!(actions.spender_transfer.len(), 0);
        });
    }

    /********** positions_under_max **********/

    #[test]
    fn test_actions_requires_positions_under_max_with_decrease() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        let (underlying_1, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying_1, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 2,
        };

        let user_positions = Positions {
            liabilities: map![&e, (0, 5_0000000), (1, 1_0000000)],
            collateral: map![&e, (0, 20_0000000), (1, 10)],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::WithdrawCollateral as u32,
                    address: underlying_1.clone(),
                    amount: 20,
                },
            ];

            let (_, user, _) = build_actions_from_request(&e, &mut pool, &samwise, requests);
            assert_eq!(user.positions.effective_count(), 3)
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1208)")]
    fn test_actions_requires_positions_under_max() {
        let e = Env::default();
        e.mock_all_auths();

        let bombadil = Address::generate(&e);
        let samwise = Address::generate(&e);
        let pool = testutils::create_pool(&e);

        let (underlying, _) = testutils::create_token_contract(&e, &bombadil);
        let (reserve_config, reserve_data) = testutils::default_reserve_meta();
        testutils::create_reserve(&e, &pool, &underlying, &reserve_config, &reserve_data);

        e.ledger().set(LedgerInfo {
            timestamp: 600,
            protocol_version: 20,
            sequence_number: 1234,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 3110400,
        });

        let pool_config = PoolConfig {
            oracle: Address::generate(&e),
            bstop_rate: 0_2000000,
            status: 0,
            max_positions: 1,
        };

        let user_positions = Positions {
            liabilities: map![&e],
            collateral: map![&e, (0, 20_0000000)],
            supply: map![&e],
        };
        e.as_contract(&pool, || {
            storage::set_pool_config(&e, &pool_config);
            storage::set_user_positions(&e, &samwise, &user_positions);

            let mut pool = Pool::load(&e);

            let requests = vec![
                &e,
                Request {
                    request_type: RequestType::Borrow as u32,
                    address: underlying.clone(),
                    amount: 1_0000000,
                },
            ];

            build_actions_from_request(&e, &mut pool, &samwise, requests);
        });
    }
}
