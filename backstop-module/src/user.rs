use soroban_sdk::{panic_with_error, unwrap::UnwrapOptimized, Address, Env, Vec};

use crate::{
    errors::BackstopError,
    storage::{self, Q4W},
};

/// A user of the backstop module with respect to a given pool
/// Data is lazy loaded as not all struct information is required for each action
pub struct User {
    pool: Address,
    pub id: Address,
    shares: Option<i128>,
    q4w: Option<Vec<Q4W>>,
}

impl User {
    pub fn new(pool: Address, id: Address) -> User {
        User {
            pool,
            id,
            shares: None,
            q4w: None,
        }
    }

    /********** Setters / Lazy Getters / Storage **********/

    /// Fetch the user's shares from either the cache or the ledger
    pub fn get_shares(&mut self, e: &Env) -> i128 {
        match self.shares {
            Some(bal) => bal,
            None => {
                let bal = storage::get_shares(&e, &self.pool, &self.id);
                self.shares = Some(bal);
                bal
            }
        }
    }

    /// Set the user's shares locally
    ///
    /// ### Arguments
    /// * `shares` - The user's shares
    pub fn set_shares(&mut self, shares: i128) {
        self.shares = Some(shares)
    }

    /// Write the currently cached user's shares to the ledger
    pub fn write_shares(&self, e: &Env) {
        match self.shares {
            Some(bal) => storage::set_shares(&e, &self.pool, &self.id, &bal),
            None => panic!("nothing to write"),
        }
    }

    /// Fetch the user's queued for withdraw from either the cache or the ledger
    pub fn get_q4w(&mut self, e: &Env) -> Vec<Q4W> {
        match self.q4w.clone() {
            Some(q4w) => q4w,
            None => {
                let q4w = storage::get_q4w(&e, &self.pool, &self.id);
                self.q4w = Some(q4w.clone());
                q4w
            }
        }
    }

    /// Set the user's queued for withdraw locally
    ///
    /// ### Arguments
    /// * `q4w` - The user's queued for withdraw
    pub fn set_q4w(&mut self, q4w: Vec<Q4W>) {
        self.q4w = Some(q4w)
    }

    /// Write the currently cached user's queued for withdraw to the ledger
    pub fn write_q4w(&self, e: &Env) {
        match self.q4w.clone() {
            Some(q4w) => storage::set_q4w(&e, &self.pool, &self.id, &q4w),
            None => panic!("nothing to write"),
        }
    }

    /********** Logic **********/

    /***** Deposit *****/

    /// Add shares to the user
    ///
    /// Updates but does not write:
    /// * shares
    ///
    /// ### Arguments
    /// * `to_add` - The amount of new shares the user has
    pub fn add_shares(&mut self, e: &Env, to_add: i128) {
        let cur_bal = self.get_shares(e);
        self.set_shares(cur_bal + to_add);
    }

    /***** Withdrawal Queue Management *****/

    /// Queue new shares for withdraw for the user
    ///
    /// Updates but does not write:
    /// * q4w
    ///
    /// ### Arguments
    /// * `to_q` - The amount of new shares to queue for withdraw
    ///
    /// ### Errors
    /// If the amount to queue is greater than the available shares
    pub fn try_queue_shares_for_withdrawal(&mut self, e: &Env, to_q: i128) -> Q4W {
        let mut user_q4w = self.get_q4w(e);
        let mut q4w_amt: i128 = 0;
        for q4w in user_q4w.iter() {
            q4w_amt += q4w.unwrap_optimized().amount
        }

        let shares = self.get_shares(e);
        if shares - q4w_amt < to_q {
            panic_with_error!(e, BackstopError::InvalidBalance);
        }

        // user has enough tokens to withdrawal, add Q4W
        let thirty_days_in_sec = 30 * 24 * 60 * 60;
        let new_q4w = Q4W {
            amount: to_q,
            exp: e.ledger().timestamp() + thirty_days_in_sec,
        };
        user_q4w.push_back(new_q4w.clone());
        self.set_q4w(user_q4w);

        new_q4w
    }

    /// Dequeue shares from the withdrawal queue
    ///
    /// Updates but does not write:
    /// * q4w
    ///
    /// ### Arguments
    /// * `to_dequeue` - The amount of shares to dequeue from the withdrawal queue
    /// * `require_expired` - If only expired Q4W can be dequeued. This
    ///                       MUST be true if the user is withdrawing.
    ///
    /// ### Errors
    /// If the user does not have enough shares currently queued to dequeue,
    /// or if they don't have enough queued shares to dequeue
    pub fn try_dequeue_shares_for_withdrawal(
        &mut self,
        e: &Env,
        to_dequeue: i128,
        require_expired: bool,
    ) {
        // validate the invoke has enough unlocked Q4W to claim
        // manage the q4w list while verifying
        let mut user_q4w = self.get_q4w(e);
        let mut left_to_dequeue: i128 = to_dequeue;
        for _index in 0..user_q4w.len() {
            let mut cur_q4w = user_q4w.pop_front_unchecked().unwrap_optimized();
            if !require_expired || cur_q4w.exp <= e.ledger().timestamp() {
                if cur_q4w.amount > left_to_dequeue {
                    // last record we need to update, but the q4w should remain
                    cur_q4w.amount -= left_to_dequeue;
                    left_to_dequeue = 0;
                    user_q4w.push_front(cur_q4w);
                    break;
                } else if cur_q4w.amount == left_to_dequeue {
                    // last record we need to update, q4w fully consumed
                    left_to_dequeue = 0;
                    break;
                } else {
                    // allow the pop to consume the record
                    left_to_dequeue -= cur_q4w.amount;
                }
            } else {
                panic_with_error!(e, BackstopError::NotExpired);
            }
        }

        if left_to_dequeue > 0 {
            panic_with_error!(e, BackstopError::InvalidBalance);
        }

        self.set_q4w(user_q4w);
    }

    /// Withdraw shares from the user
    ///
    /// Updates but does not write:
    /// * q4w
    /// * shares
    ///
    /// ### Arguments
    /// * `to_q` - The amount of new shares to queue for withdraw
    ///
    /// ### Errors
    /// If the amount to queue is greater than the available shares
    pub fn try_withdraw_shares(&mut self, e: &Env, to_withdraw: i128) {
        self.try_dequeue_shares_for_withdrawal(e, to_withdraw, true);

        let shares_left = self.get_shares(e) - to_withdraw;
        self.set_shares(shares_left);
    }
}

#[cfg(test)]
mod tests {
    use std::panic;

    use crate::testutils::assert_eq_vec_q4w;

    use super::*;
    use soroban_sdk::{
        testutils::{Address as AddressTestTrait, Ledger, LedgerInfo},
        vec, Address,
    };

    /********** Cache / Getters / Setters **********/

    #[test]
    fn test_share_cache() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);
        let pool_addr = Address::random(&e);

        let samwise = Address::random(&e);
        let mut user = User::new(pool_addr.clone(), samwise.clone());

        let first_share_amt = 100;
        e.as_contract(&backstop_addr, || {
            storage::set_shares(&e, &pool_addr, &samwise, &first_share_amt);
            let first_result = user.get_shares(&e);
            assert_eq!(first_result, first_share_amt);
        });

        e.as_contract(&backstop_addr, || {
            // cached version returned
            storage::set_shares(&e, &pool_addr, &samwise, &1);
            let cached_result = user.get_shares(&e);
            assert_eq!(cached_result, first_share_amt);

            // new amount gets set and stored
            let second_share_amt = 200;
            user.set_shares(second_share_amt);
            let second_result = user.get_shares(&e);
            assert_eq!(second_result, second_share_amt);

            // write stores to chain
            user.write_shares(&e);
            let chain_result = storage::get_shares(&e, &pool_addr, &samwise);
            assert_eq!(chain_result, second_share_amt);
        });
    }

    #[test]
    fn test_q4w_cache() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);
        let pool_addr = Address::random(&e);

        let samwise = Address::random(&e);
        let mut user = User::new(pool_addr.clone(), samwise.clone());

        let first_q4w = vec![
            &e,
            Q4W {
                amount: 100,
                exp: 1234567,
            },
        ];
        e.as_contract(&backstop_addr, || {
            storage::set_q4w(&e, &pool_addr, &samwise, &first_q4w);
            let first_result = user.get_q4w(&e);
            assert_eq_vec_q4w(&first_q4w, &first_result);
        });

        e.as_contract(&backstop_addr, || {
            // cached version returned
            storage::set_q4w(&e, &pool_addr, &samwise, &vec![&e]);
            let cached_result = user.get_q4w(&e);
            assert_eq_vec_q4w(&first_q4w, &cached_result);

            // new amount gets set and stored
            let second_q4w = vec![
                &e,
                Q4W {
                    amount: 200,
                    exp: 7654321,
                },
            ];
            user.set_q4w(second_q4w.clone());
            let second_result = user.get_q4w(&e);
            assert_eq_vec_q4w(&second_q4w, &second_result);

            // write stores to chain
            user.write_q4w(&e);
            let chain_result = storage::get_q4w(&e, &pool_addr, &samwise);
            assert_eq_vec_q4w(&second_q4w, &chain_result);
        });
    }

    /********** Share Management **********/

    #[test]
    fn test_add_shares() {
        let e = Env::default();

        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(100),
            q4w: None,
        };

        let to_add = 12318972;
        user.add_shares(&e, to_add);

        assert_eq!(user.get_shares(&e), to_add + 100);
    }

    /********** Q4W Management **********/

    #[test]
    fn test_try_q4w_none_queued() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: None,
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 10000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_queue = 500;
            let res_q4w = user.try_queue_shares_for_withdrawal(&e, to_queue);
            assert_eq!(res_q4w.amount, to_queue);
            assert_eq!(res_q4w.exp, 10000 + 30 * 24 * 60 * 60);

            // validate method stores q4w in cache
            let cached_q4w = user.get_q4w(&e);
            assert_eq_vec_q4w(&cached_q4w, &vec![&e, res_q4w]);
        });
    }

    #[test]
    fn test_try_q4w_new_placed_last() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let mut cur_q4w = vec![
            &e,
            Q4W {
                amount: 200,
                exp: 12592000,
            },
        ];
        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: Some(cur_q4w.clone()),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11000000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_queue = 500;
            let res_q4w = user.try_queue_shares_for_withdrawal(&e, to_queue);
            cur_q4w.push_back(res_q4w);
            // validate method stores q4w in cache
            let cached_q4w = user.get_q4w(&e);
            assert_eq_vec_q4w(&cached_q4w, &cur_q4w);
        });
    }

    #[test]
    #[should_panic(expected = "HostError\nValue: Status(ContractError(2))")]
    fn test_try_q4w_over_shares_panics() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 200,
                exp: 12592000,
            },
        ];
        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: Some(cur_q4w),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11000000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_queue = 801;
            let res_q4w = user.try_queue_shares_for_withdrawal(&e, to_queue);
        });
    }

    #[test]
    #[should_panic(expected = "HostError\nValue: Status(ContractError(2))")]
    fn test_try_withdraw_shares_no_q4w_panics() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: None,
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11000000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_wd = 1;
            let res = user.try_withdraw_shares(&e, to_wd);
        });
    }

    #[test]
    fn test_try_withdraw_shares_exact_amount() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 200,
                exp: 12592000,
            },
        ];
        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: Some(cur_q4w),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 12592000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_wd = 200;
            let res = user.try_withdraw_shares(&e, to_wd);
            let q4w = user.get_q4w(&e);

            assert_eq_vec_q4w(&q4w, &vec![&e]);
            assert_eq!(user.get_shares(&e), 800);
        });
    }

    #[test]
    fn test_try_withdraw_shares_less_than_entry() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 200,
                exp: 12592000,
            },
        ];
        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: Some(cur_q4w),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 12592000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_wd = 150;
            let res = user.try_withdraw_shares(&e, to_wd);
            let expected_q4w = vec![
                &e,
                Q4W {
                    amount: 50,
                    exp: 12592000,
                },
            ];
            let q4w = user.get_q4w(&e);
            assert_eq_vec_q4w(&q4w, &expected_q4w);
            assert_eq!(user.get_shares(&e), 850);
        });
    }

    #[test]
    fn test_try_withdraw_shares_multiple_entries() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 125,
                exp: 10000000,
            },
            Q4W {
                amount: 200,
                exp: 12592000,
            },
            Q4W {
                amount: 50,
                exp: 19592000,
            },
        ];
        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: Some(cur_q4w),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 22592000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_wd = 300;
            let res = user.try_withdraw_shares(&e, to_wd);
            let expected_q4w = vec![
                &e,
                Q4W {
                    amount: 25,
                    exp: 12592000,
                },
                Q4W {
                    amount: 50,
                    exp: 19592000,
                },
            ];
            let q4w = user.get_q4w(&e);
            assert_eq_vec_q4w(&q4w, &expected_q4w);
            assert_eq!(user.get_shares(&e), 700);
        });
    }

    #[test]
    #[should_panic(expected = "HostError\nValue: Status(ContractError(3))")]
    fn test_try_withdraw_shares_multiple_entries_not_exp() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 125,
                exp: 10000000,
            },
            Q4W {
                amount: 200,
                exp: 12592000,
            },
            Q4W {
                amount: 50,
                exp: 19592000,
            },
        ];
        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: Some(cur_q4w.clone()),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11192000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_wd = 300;
            let res = user.try_withdraw_shares(&e, to_wd);
        });
    }

    #[test]
    fn test_try_dequeue_shares() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 125,
                exp: 10000000,
            },
            Q4W {
                amount: 200,
                exp: 12592000,
            },
            Q4W {
                amount: 50,
                exp: 19592000,
            },
        ];
        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: Some(cur_q4w.clone()),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11192000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_dequeue = 300;

            // verify exp is ignored if only dequeueing
            let res = user.try_dequeue_shares_for_withdrawal(&e, to_dequeue, false);
            let expected_q4w = vec![
                &e,
                Q4W {
                    amount: 25,
                    exp: 12592000,
                },
                Q4W {
                    amount: 50,
                    exp: 19592000,
                },
            ];
            let q4w = user.get_q4w(&e);
            assert_eq_vec_q4w(&q4w, &expected_q4w);
            assert_eq!(user.get_shares(&e), 1000);
        });
    }

    #[test]
    #[should_panic(expected = "HostError\nValue: Status(ContractError(3))")]
    fn test_try_dequeue_shares_require_expireed_expect_panic() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 125,
                exp: 10000000,
            },
            Q4W {
                amount: 200,
                exp: 12592000,
            },
            Q4W {
                amount: 50,
                exp: 19592000,
            },
        ];
        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: Some(cur_q4w.clone()),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11192000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_dequeue = 300;
            let res = user.try_dequeue_shares_for_withdrawal(&e, to_dequeue, true);
        });
    }

    #[test]
    #[should_panic(expected = "HostError\nValue: Status(ContractError(2))")]
    fn test_try_withdraw_shares_over_total() {
        let e = Env::default();

        let backstop_addr = Address::random(&e);

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 125,
                exp: 10000000,
            },
            Q4W {
                amount: 200,
                exp: 12592000,
            },
            Q4W {
                amount: 50,
                exp: 19592000,
            },
        ];
        let mut user = User {
            pool: Address::random(&e),
            id: Address::random(&e),
            shares: Some(1000),
            q4w: Some(cur_q4w.clone()),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11192000,
            network_id: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_dequeue = 376;
            let res = user.try_dequeue_shares_for_withdrawal(&e, to_dequeue, false);
        });
    }
}
