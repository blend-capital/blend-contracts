use sep_41_token::TokenClient;
use soroban_sdk::{contracttype, panic_with_error, Address, Env};

use crate::{emitter, storage, EmitterError};

#[derive(Clone)]
#[contracttype]
pub struct Swap {
    pub new_backstop: Address,
    pub new_backstop_token: Address,
    pub unlock_time: u64,
}

/// Require that the new backstop is larger than the backstop
///
/// Panics otherwise
fn is_new_backstop_is_larger(e: &Env, new_backstop: &Address, backstop: &Address) -> bool {
    let backstop_token = storage::get_backstop_token(e);
    let backstop_token_client = TokenClient::new(e, &backstop_token);

    let backstop_balance = backstop_token_client.balance(&backstop);
    let new_backstop_balance = backstop_token_client.balance(&new_backstop);
    return new_backstop_balance > backstop_balance;
}

/// Perform a backstop swap
pub fn execute_queue_swap_backstop(
    e: &Env,
    new_backstop: &Address,
    new_backstop_token: &Address,
) -> Swap {
    // verify no swap is already queued
    if storage::get_queued_swap(e).is_some() {
        panic_with_error!(e, EmitterError::SwapAlreadyExists);
    }

    let backstop = storage::get_backstop(e);
    if !is_new_backstop_is_larger(e, new_backstop, &backstop) {
        panic_with_error!(e, EmitterError::InsufficientBackstopSize);
    }

    let swap = Swap {
        new_backstop: new_backstop.clone(),
        new_backstop_token: new_backstop_token.clone(),
        unlock_time: e.ledger().timestamp() + 31 * 24 * 60 * 60,
    };
    storage::set_queued_swap(e, &swap);
    swap
}

/// Cancel a backstop swap if it has not maintained a higher balance than the current backstop
pub fn execute_cancel_swap_backstop(e: &Env) -> Swap {
    let swap = storage::get_queued_swap(e)
        .unwrap_or_else(|| panic_with_error!(e, EmitterError::SwapNotQueued));

    let backstop = storage::get_backstop(e);
    if is_new_backstop_is_larger(e, &swap.new_backstop, &backstop) {
        panic_with_error!(e, EmitterError::SwapCannotBeCanceled);
    }

    storage::del_queued_swap(e);
    swap
}

/// Perform a swap from the queue if it has been unlocked and the new backstop has maintained a higher balance than the current backstop
pub fn execute_swap_backstop(e: &Env) -> Swap {
    let swap = storage::get_queued_swap(e)
        .unwrap_or_else(|| panic_with_error!(e, EmitterError::SwapNotQueued));

    if swap.unlock_time > e.ledger().timestamp() {
        panic_with_error!(e, EmitterError::SwapNotUnlocked);
    }

    let backstop = storage::get_backstop(e);
    if !is_new_backstop_is_larger(e, &swap.new_backstop, &backstop) {
        panic_with_error!(e, EmitterError::InsufficientBackstopSize);
    }

    // distribute before swapping to ensure the old backstop gets their tokens
    emitter::execute_distribute(e, &backstop);

    // swap backstop and token
    storage::set_last_fork(e, e.ledger().sequence());
    storage::del_queued_swap(e);
    storage::set_backstop(e, &swap.new_backstop);
    storage::set_backstop_token(e, &swap.new_backstop_token);

    // start distribution for new backstop
    storage::set_last_distro_time(e, &swap.new_backstop, e.ledger().timestamp());

    swap
}

#[cfg(test)]
mod tests {

    use crate::{constants::SCALAR_7, storage, testutils::create_emitter};

    use super::*;
    use sep_41_token::testutils::MockTokenClient;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};

    /********** execute_queue_swap_backstop **********/

    #[test]
    fn test_execute_queue_swap_backstop() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);
        let new_backstop = Address::generate(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = MockTokenClient::new(&e, &backstop_token);
        let new_backstop_token = Address::generate(&e);

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_001 * SCALAR_7));

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_backstop_token(&e, &backstop_token);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 123);

            execute_queue_swap_backstop(&e, &new_backstop, &new_backstop_token);

            // verify no swap occurred
            assert_eq!(storage::get_backstop(&e), backstop);
            assert_eq!(storage::get_backstop_token(&e), backstop_token);
            assert_eq!(storage::get_last_fork(&e), 123);

            // verify swap is queued
            let swap = storage::get_queued_swap(&e);
            assert!(swap.is_some());
            let swap = swap.unwrap();
            assert_eq!(swap.new_backstop, new_backstop);
            assert_eq!(swap.new_backstop_token, new_backstop_token);
            assert_eq!(swap.unlock_time, 2678400 + 12345); // 31 days
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #30)")]
    fn test_execute_queue_swap_backstop_insufficient_funds() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);
        let new_backstop = Address::generate(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = MockTokenClient::new(&e, &backstop_token);
        let new_backstop_token = Address::generate(&e);

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_000 * SCALAR_7));

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_backstop_token(&e, &backstop_token);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 123);

            execute_queue_swap_backstop(&e, &new_backstop, &new_backstop_token);
            assert!(false); // should panic
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #60)")]
    fn test_execute_queue_swap_backstop_already_exists() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);
        let new_backstop = Address::generate(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = MockTokenClient::new(&e, &backstop_token);
        let new_backstop_token = Address::generate(&e);

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_001 * SCALAR_7));

        let swap = Swap {
            new_backstop: Address::generate(&e),
            new_backstop_token: Address::generate(&e),
            unlock_time: 0,
        };

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_backstop_token(&e, &backstop_token);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 123);
            storage::set_queued_swap(&e, &swap);

            execute_queue_swap_backstop(&e, &new_backstop, &new_backstop_token);
            assert!(false); // should panic
        });
    }

    /********** execute_cancel_swap_backstop **********/

    #[test]
    fn test_execute_cancel_swap_backstop() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);
        let new_backstop = Address::generate(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = MockTokenClient::new(&e, &backstop_token);
        let new_backstop_token = Address::generate(&e);

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_000 * SCALAR_7));

        let swap = Swap {
            new_backstop: new_backstop.clone(),
            new_backstop_token: new_backstop_token.clone(),
            unlock_time: 12345 + 1000,
        };

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_backstop_token(&e, &backstop_token);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 123);
            storage::set_queued_swap(&e, &swap);

            execute_cancel_swap_backstop(&e);

            // verify no swap occurred
            assert_eq!(storage::get_backstop(&e), backstop);
            assert_eq!(storage::get_backstop_token(&e), backstop_token);
            assert_eq!(storage::get_last_fork(&e), 123);

            // verify swap is removed
            let swap = storage::get_queued_swap(&e);
            assert!(swap.is_none());
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #80)")]
    fn test_execute_cancel_swap_backstop_valid_swap() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);
        let new_backstop = Address::generate(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = MockTokenClient::new(&e, &backstop_token);
        let new_backstop_token = Address::generate(&e);

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_001 * SCALAR_7));

        let swap = Swap {
            new_backstop: new_backstop.clone(),
            new_backstop_token: new_backstop_token.clone(),
            unlock_time: 12345 + 1000,
        };

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_backstop_token(&e, &backstop_token);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 123);
            storage::set_queued_swap(&e, &swap);

            execute_cancel_swap_backstop(&e);
            assert!(false);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #50)")]
    fn test_execute_cancel_swap_backstop_none_queued() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);
        let new_backstop = Address::generate(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = MockTokenClient::new(&e, &backstop_token);

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_000 * SCALAR_7));

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_backstop_token(&e, &backstop_token);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 123);

            execute_cancel_swap_backstop(&e);
            assert!(false);
        });
    }

    /********** execute_swap_backstop **********/

    #[test]
    fn test_execute_swap_backstop() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 500,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let emitter = create_emitter(&e);

        let blnd_token = e.register_stellar_asset_contract(emitter.clone());
        let blnd_token_client = MockTokenClient::new(&e, &blnd_token);

        let backstop = Address::generate(&e);
        let new_backstop = Address::generate(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = MockTokenClient::new(&e, &backstop_token);
        let new_backstop_token = Address::generate(&e);

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_001 * SCALAR_7));

        let swap = Swap {
            new_backstop: new_backstop.clone(),
            new_backstop_token: new_backstop_token.clone(),
            unlock_time: 12345,
        };

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 10000);
            storage::set_backstop(&e, &backstop);
            storage::set_backstop_token(&e, &backstop_token);
            storage::set_blnd_token(&e, &blnd_token);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 123);
            storage::set_queued_swap(&e, &swap);

            execute_swap_backstop(&e);

            // verify swap occurred
            assert_eq!(storage::get_backstop(&e), new_backstop);
            assert_eq!(storage::get_backstop_token(&e), new_backstop_token);
            assert_eq!(storage::get_last_fork(&e), 500);

            // verify swap is removed
            let swap = storage::get_queued_swap(&e);
            assert!(swap.is_none());

            // verify old backstop was distributed and new backstop distribution begins
            assert_eq!(storage::get_last_distro_time(&e, &backstop), 12345);
            assert_eq!(storage::get_last_distro_time(&e, &new_backstop), 12345);
            assert_eq!(blnd_token_client.balance(&backstop), 2345 * SCALAR_7);
            assert_eq!(blnd_token_client.balance(&new_backstop), 0);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #70)")]
    fn test_execute_swap_backstop_not_unlocked() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 500,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let emitter = create_emitter(&e);

        let blnd_token = e.register_stellar_asset_contract(emitter.clone());
        let backstop = Address::generate(&e);
        let new_backstop = Address::generate(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = MockTokenClient::new(&e, &backstop_token);
        let new_backstop_token = Address::generate(&e);

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_001 * SCALAR_7));

        let swap = Swap {
            new_backstop: new_backstop.clone(),
            new_backstop_token: new_backstop_token.clone(),
            unlock_time: 12345 + 1,
        };

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 10000);
            storage::set_backstop(&e, &backstop);
            storage::set_backstop_token(&e, &backstop_token);
            storage::set_blnd_token(&e, &blnd_token);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 123);
            storage::set_queued_swap(&e, &swap);

            execute_swap_backstop(&e);
            assert!(false);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #50)")]
    fn test_execute_swap_backstop_no_queue() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 500,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let emitter = create_emitter(&e);

        let blnd_token = e.register_stellar_asset_contract(emitter.clone());
        let backstop = Address::generate(&e);
        let new_backstop = Address::generate(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = MockTokenClient::new(&e, &backstop_token);

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_001 * SCALAR_7));

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 10000);
            storage::set_backstop(&e, &backstop);
            storage::set_backstop_token(&e, &backstop_token);
            storage::set_blnd_token(&e, &blnd_token);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 123);

            execute_swap_backstop(&e);
            assert!(false);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #30)")]
    fn test_execute_swap_backstop_insufficient_funds() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 500,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let emitter = create_emitter(&e);

        let blnd_token = e.register_stellar_asset_contract(emitter.clone());
        let backstop = Address::generate(&e);
        let new_backstop = Address::generate(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = MockTokenClient::new(&e, &backstop_token);
        let new_backstop_token = Address::generate(&e);

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_000 * SCALAR_7));

        let swap = Swap {
            new_backstop: new_backstop.clone(),
            new_backstop_token: new_backstop_token.clone(),
            unlock_time: 12345,
        };

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 10000);
            storage::set_backstop(&e, &backstop);
            storage::set_backstop_token(&e, &backstop_token);
            storage::set_blnd_token(&e, &blnd_token);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 123);
            storage::set_queued_swap(&e, &swap);

            execute_swap_backstop(&e);
            assert!(false);
        });
    }
}
