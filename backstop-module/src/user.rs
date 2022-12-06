use soroban_auth::Identifier;
use soroban_sdk::{BytesN, Env, Vec};

use crate::{
    errors::BackstopError,
    storage::{BackstopDataStore, StorageManager, Q4W},
};

/// A user of the backstop module with respect to a given pool
/// Data is lazy loaded as not all struct information is required for each action
pub struct User {
    pool: BytesN<32>,
    pub id: Identifier,
    shares: Option<u64>,
    q4w: Option<Vec<Q4W>>,
}

impl User {
    pub fn new(pool: BytesN<32>, id: Identifier) -> User {
        User {
            pool,
            id,
            shares: None,
            q4w: None,
        }
    }

    /********** Setters / Lazy Getters / Storage **********/

    /// Fetch the user's shares from either the cache or the ledger
    pub fn get_shares(&mut self, e: &Env) -> u64 {
        match self.shares {
            Some(bal) => bal,
            None => {
                let bal = StorageManager::new(e).get_shares(self.pool.clone(), self.id.clone());
                self.shares = Some(bal);
                bal
            }
        }
    }

    /// Set the user's shares locally
    ///
    /// ### Arguments
    /// * `shares` - The user's shares
    pub fn set_shares(&mut self, shares: u64) {
        self.shares = Some(shares)
    }

    /// Write the currently cached user's shares to the ledger
    pub fn write_shares(&self, e: &Env) {
        match self.shares {
            Some(bal) => StorageManager::new(e).set_shares(self.pool.clone(), self.id.clone(), bal),
            None => panic!("nothing to write"),
        }
    }

    /// Fetch the user's queued for withdraw from either the cache or the ledger
    pub fn get_q4w(&mut self, e: &Env) -> Vec<Q4W> {
        match self.q4w.clone() {
            Some(q4w) => q4w,
            None => {
                let q4w = StorageManager::new(e).get_q4w(self.pool.clone(), self.id.clone());
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
            Some(q4w) => StorageManager::new(e).set_q4w(self.pool.clone(), self.id.clone(), q4w),
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
    pub fn add_shares(&mut self, e: &Env, to_add: u64) {
        let cur_bal = self.get_shares(e);
        self.set_shares(cur_bal + to_add);
    }

    /***** Queue for Withdrawal *****/

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
    pub fn try_queue_shares_for_withdrawal(
        &mut self,
        e: &Env,
        to_q: u64,
    ) -> Result<Q4W, BackstopError> {
        let mut user_q4w = self.get_q4w(e);
        let mut q4w_amt: u64 = 0;
        for q4w in user_q4w.iter() {
            q4w_amt += q4w.unwrap().amount
        }

        let shares = self.get_shares(e);
        if shares - q4w_amt < to_q {
            return Err(BackstopError::InvalidBalance);
        }

        // user has enough tokens to withdrawal, add Q4W
        let thirty_days_in_sec = 30 * 24 * 60 * 60;
        let new_q4w = Q4W {
            amount: to_q,
            exp: e.ledger().timestamp() + thirty_days_in_sec,
        };
        user_q4w.push_back(new_q4w.clone());
        self.set_q4w(user_q4w);

        Ok(new_q4w)
    }

    /***** Queue for Withdrawal *****/

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
    pub fn try_withdraw_shares(&mut self, e: &Env, to_withdraw: u64) -> Result<(), BackstopError> {
        // validate the invoke has enough unlocked Q4W to claim
        // manage the q4w list while verifying
        let mut user_q4w = self.get_q4w(e);
        let mut left_to_withdraw: u64 = to_withdraw;
        for _index in 0..user_q4w.len() {
            let mut cur_q4w = user_q4w.pop_front_unchecked().unwrap();
            if cur_q4w.exp <= e.ledger().timestamp() {
                if cur_q4w.amount > left_to_withdraw {
                    // last record we need to update, but the q4w should remain
                    cur_q4w.amount -= left_to_withdraw;
                    left_to_withdraw = 0;
                    user_q4w.push_front(cur_q4w);
                    break;
                } else if cur_q4w.amount == left_to_withdraw {
                    // last record we need to update, q4w fully consumed
                    left_to_withdraw = 0;
                    break;
                } else {
                    // allow the pop to consume the record
                    left_to_withdraw -= cur_q4w.amount;
                }
            } else {
                return Err(BackstopError::NotExpired);
            }
        }

        if left_to_withdraw > 0 {
            return Err(BackstopError::InvalidBalance);
        }

        self.set_q4w(user_q4w);
        let shares_left = self.get_shares(e) - to_withdraw;
        self.set_shares(shares_left);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::testutils::generate_contract_id;

    use super::*;
    use soroban_sdk::{
        testutils::{Accounts, Ledger, LedgerInfo},
        vec,
    };

    /********** Cache / Getters / Setters **********/

    #[test]
    fn test_share_cache() {
        let e = Env::default();
        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let pool_addr = generate_contract_id(&e);

        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());
        let mut user = User::new(pool_addr.clone(), user_id.clone());

        let first_share_amt = 100;
        e.as_contract(&backstop_addr, || {
            storage.set_shares(pool_addr.clone(), user_id.clone(), first_share_amt.clone());
            let first_result = user.get_shares(&e);
            assert_eq!(first_result, first_share_amt);
        });

        e.as_contract(&backstop_addr, || {
            // cached version returned
            storage.set_shares(pool_addr.clone(), user_id.clone(), 1);
            let cached_result = user.get_shares(&e);
            assert_eq!(cached_result, first_share_amt);

            // new amount gets set and stored
            let second_share_amt = 200;
            user.set_shares(second_share_amt);
            let second_result = user.get_shares(&e);
            assert_eq!(second_result, second_share_amt);

            // write stores to chain
            user.write_shares(&e);
            let chain_result = storage.get_shares(pool_addr, user_id);
            assert_eq!(chain_result, second_share_amt);
        });
    }

    #[test]
    fn test_q4w_cache() {
        let e = Env::default();
        let storage = StorageManager::new(&e);

        let backstop_addr = generate_contract_id(&e);
        let pool_addr = generate_contract_id(&e);

        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());
        let mut user = User::new(pool_addr.clone(), user_id.clone());

        let first_q4w = vec![
            &e,
            Q4W {
                amount: 100,
                exp: 1234567,
            },
        ];
        e.as_contract(&backstop_addr, || {
            storage.set_q4w(pool_addr.clone(), user_id.clone(), first_q4w.clone());
            let first_result = user.get_q4w(&e);
            assert_eq_vec_q4w(&first_q4w, &first_result);
        });

        e.as_contract(&backstop_addr, || {
            // cached version returned
            storage.set_q4w(pool_addr.clone(), user_id.clone(), vec![&e]);
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
            let chain_result = storage.get_q4w(pool_addr.clone(), user_id.clone());
            assert_eq_vec_q4w(&second_q4w, &chain_result);
        });
    }

    /********** Share Management **********/

    #[test]
    fn test_add_shares() {
        let e = Env::default();

        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());

        let mut user = User {
            pool: generate_contract_id(&e),
            id: user_id,
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

        let backstop_addr = generate_contract_id(&e);
        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());

        let mut user = User {
            pool: generate_contract_id(&e),
            id: user_id,
            shares: Some(1000),
            q4w: None,
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 10000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_queue = 500;
            let res_q4w = user.try_queue_shares_for_withdrawal(&e, to_queue);
            match res_q4w {
                Ok(q4w) => {
                    assert_eq!(q4w.amount, to_queue);
                    assert_eq!(q4w.exp, 10000 + 30 * 24 * 60 * 60);

                    // validate method stores q4w in cache
                    let cached_q4w = user.get_q4w(&e);
                    assert_eq_vec_q4w(&cached_q4w, &vec![&e, q4w]);
                }
                Err(_) => assert!(false),
            }
        });
    }

    #[test]
    fn test_try_q4w_new_placed_last() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());

        let mut cur_q4w = vec![
            &e,
            Q4W {
                amount: 200,
                exp: 12592000,
            },
        ];
        let mut user = User {
            pool: generate_contract_id(&e),
            id: user_id,
            shares: Some(1000),
            q4w: Some(cur_q4w.clone()),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11000000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_queue = 500;
            let res_q4w = user.try_queue_shares_for_withdrawal(&e, to_queue);
            match res_q4w {
                Ok(q4w) => {
                    cur_q4w.push_back(q4w);
                    // validate method stores q4w in cache
                    let cached_q4w = user.get_q4w(&e);
                    assert_eq_vec_q4w(&cached_q4w, &cur_q4w);
                }
                Err(_) => assert!(false),
            }
        });
    }

    #[test]
    fn test_try_q4w_over_shares_panics() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 200,
                exp: 12592000,
            },
        ];
        let mut user = User {
            pool: generate_contract_id(&e),
            id: user_id,
            shares: Some(1000),
            q4w: Some(cur_q4w),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11000000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_queue = 801;
            let res_q4w = user.try_queue_shares_for_withdrawal(&e, to_queue);
            match res_q4w {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::InvalidBalance => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    #[test]
    fn test_try_withdraw_shares_no_q4w_panics() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());

        let mut user = User {
            pool: generate_contract_id(&e),
            id: user_id,
            shares: Some(1000),
            q4w: None,
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11000000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_wd = 1;
            let res = user.try_withdraw_shares(&e, to_wd);
            match res {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::InvalidBalance => assert!(true),
                    _ => assert!(false),
                },
            }
        });
    }

    #[test]
    fn test_try_withdraw_shares_exact_amount() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 200,
                exp: 12592000,
            },
        ];
        let mut user = User {
            pool: generate_contract_id(&e),
            id: user_id,
            shares: Some(1000),
            q4w: Some(cur_q4w),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 12592000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_wd = 200;
            let res = user.try_withdraw_shares(&e, to_wd);
            match res {
                Ok(_) => {
                    let q4w = user.get_q4w(&e);
                    assert_eq_vec_q4w(&q4w, &vec![&e]);
                    assert_eq!(user.get_shares(&e), 800);
                }
                Err(_) => assert!(false),
            }
        });
    }

    #[test]
    fn test_try_withdraw_shares_less_than_entry() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());

        let cur_q4w = vec![
            &e,
            Q4W {
                amount: 200,
                exp: 12592000,
            },
        ];
        let mut user = User {
            pool: generate_contract_id(&e),
            id: user_id,
            shares: Some(1000),
            q4w: Some(cur_q4w),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 12592000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_wd = 150;
            let res = user.try_withdraw_shares(&e, to_wd);
            match res {
                Ok(_) => {
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
                }
                Err(_) => assert!(false),
            }
        });
    }

    #[test]
    fn test_try_withdraw_shares_multiple_entries() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());

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
            pool: generate_contract_id(&e),
            id: user_id,
            shares: Some(1000),
            q4w: Some(cur_q4w),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 22592000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_wd = 300;
            let res = user.try_withdraw_shares(&e, to_wd);
            match res {
                Ok(_) => {
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
                }
                Err(_) => assert!(false),
            }
        });
    }

    #[test]
    fn test_try_withdraw_shares_multiple_entries_not_exp() {
        let e = Env::default();

        let backstop_addr = generate_contract_id(&e);
        let user_acct = e.accounts().generate_and_create();
        let user_id = Identifier::Account(user_acct.clone());

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
            pool: generate_contract_id(&e),
            id: user_id,
            shares: Some(1000),
            q4w: Some(cur_q4w.clone()),
        };

        e.ledger().set(LedgerInfo {
            protocol_version: 1,
            sequence_number: 1,
            timestamp: 11192000,
            network_passphrase: Default::default(),
            base_reserve: 10,
        });

        e.as_contract(&backstop_addr, || {
            let to_wd = 300;
            let res = user.try_withdraw_shares(&e, to_wd);
            match res {
                Ok(_) => assert!(false),
                Err(err) => match err {
                    BackstopError::NotExpired => {
                        // verify q4w vec was not modified
                        let q4w = user.get_q4w(&e);
                        assert_eq_vec_q4w(&q4w, &cur_q4w);
                        assert_eq!(user.get_shares(&e), 1000);
                    }
                    _ => assert!(false),
                },
            }
        });
    }

    /********** Helpers **********/

    fn assert_eq_vec_q4w(actual: &Vec<Q4W>, expected: &Vec<Q4W>) {
        assert_eq!(actual.len(), expected.len());
        for index in 0..actual.len() {
            let actual_q4w = actual.get(index).unwrap().unwrap();
            let expected_q4w = expected.get(index).unwrap().unwrap();
            assert_eq!(actual_q4w.amount, expected_q4w.amount);
            assert_eq!(actual_q4w.exp, expected_q4w.exp);
        }
    }
}
