use crate::{
    constants::SCALAR_7,
    dependencies::{BackstopClient, TokenClient},
    errors::EmitterError,
    storage,
};
use soroban_sdk::{panic_with_error, Address, Env, Map};

/// Perform a distribution
pub fn execute_distribute(e: &Env, backstop: &Address) -> i128 {
    let timestamp = e.ledger().timestamp();
    let seconds_since_last_distro = timestamp - storage::get_last_distro_time(e);
    // Blend tokens are distributed at a rate of 1 token per second
    let distribution_amount = (seconds_since_last_distro as i128) * SCALAR_7;
    storage::set_last_distro_time(e, &timestamp);

    let blend_id = storage::get_blend_id(e);
    let blend_client = TokenClient::new(e, &blend_id);
    blend_client.mint(backstop, &distribution_amount);

    distribution_amount
}

/// Perform a backstop swap
pub fn execute_swap_backstop(e: &Env, new_backstop_id: Address) {
    let backstop = storage::get_backstop(e);
    let backstop_token = BackstopClient::new(e, &backstop).backstop_token();
    let backstop_token_client = TokenClient::new(e, &backstop_token);

    let backstop_balance = backstop_token_client.balance(&backstop);
    let new_backstop_balance = backstop_token_client.balance(&new_backstop_id);
    if new_backstop_balance > backstop_balance {
        storage::set_backstop(e, &new_backstop_id);
        storage::set_drop_status(e, false);
        storage::set_last_fork(e, e.ledger().sequence());
    } else {
        panic_with_error!(e, EmitterError::InsufficientBackstopSize);
    }
}

/// Perform drop BLND distribution
pub fn execute_drop(e: &Env) -> Map<Address, i128> {
    if storage::get_drop_status(e) {
        panic_with_error!(e, EmitterError::BadDrop);
    }
    if storage::get_last_fork(e) + 777600 > e.ledger().sequence() {
        // Check that the last fork was at least 45 days ago
        panic_with_error!(e, EmitterError::BadDrop);
    }
    let backstop = storage::get_backstop(e);
    let backstop_client = BackstopClient::new(e, &backstop);
    let backstop_token = backstop_client.backstop_token();
    let backstop_token_client = TokenClient::new(e, &backstop_token);

    let drop_list: Map<Address, i128> = backstop_client.drop_list();
    let mut drop_amount = 0;
    for (_, amt) in drop_list.iter() {
        drop_amount += amt;
    }
    // drop cannot be more than 50 million tokens
    if drop_amount > 50_000_000 * SCALAR_7 {
        panic_with_error!(e, EmitterError::BadDrop);
    }
    for (addr, amt) in drop_list.iter() {
        backstop_token_client.mint(&addr, &amt);
    }
    storage::set_drop_status(e, true);
    drop_list
}

#[cfg(test)]
mod tests {

    use crate::{
        storage,
        testutils::{create_backstop, create_emitter},
    };

    use super::*;
    use soroban_sdk::{
        map,
        testutils::{Address as _, Ledger, LedgerInfo},
    };

    #[test]
    fn test_distribute() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let emitter = create_emitter(&e);
        let backstop = Address::random(&e);

        let blnd_id = e.register_stellar_asset_contract(emitter.clone());
        let blnd_client = TokenClient::new(&e, &blnd_id);

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &1000);
            storage::set_backstop(&e, &backstop);
            storage::set_blend_id(&e, &blnd_id);

            let result = execute_distribute(&e, &backstop);
            assert_eq!(result, 11345_0000000);
            assert_eq!(blnd_client.balance(&backstop), 11345_0000000);
            assert_eq!(storage::get_last_distro_time(&e), 12345);
        });
    }

    #[test]
    fn test_swap_backstop() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let emitter = create_emitter(&e);
        let (backstop, backstop_client) = create_backstop(&e);
        let new_backstop = Address::random(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = TokenClient::new(&e, &backstop_token);

        backstop_client.initialize(
            &backstop_token,
            &Address::random(&e),
            &Address::random(&e),
            &Address::random(&e),
            &Map::new(&e),
        );

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_001 * SCALAR_7));

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &1000);
            storage::set_backstop(&e, &backstop);
            storage::set_drop_status(&e, true);

            execute_swap_backstop(&e, new_backstop.clone());
            assert_eq!(storage::get_backstop(&e), new_backstop);
            assert_eq!(storage::get_drop_status(&e), false);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #30)")]
    fn test_swap_backstop_not_enough() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let emitter = create_emitter(&e);
        let (backstop, backstop_client) = create_backstop(&e);
        let new_backstop = Address::random(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = TokenClient::new(&e, &backstop_token);

        backstop_client.initialize(
            &backstop_token,
            &Address::random(&e),
            &Address::random(&e),
            &Address::random(&e),
            &Map::new(&e),
        );

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_000 * SCALAR_7));

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &1000);
            storage::set_backstop(&e, &backstop);

            execute_swap_backstop(&e, new_backstop.clone());
            assert!(false, "Should have panicked");
        });
    }
    #[test]
    fn test_drop() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 5000000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let frodo = Address::random(&e);
        let samwise = Address::random(&e);
        let emitter = create_emitter(&e);
        let (backstop, backstop_client) = create_backstop(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = TokenClient::new(&e, &backstop_token);
        let drop_list = map![
            &e,
            (frodo.clone(), 20_000_000 * SCALAR_7),
            (samwise.clone(), 30_000_000 * SCALAR_7)
        ];

        backstop_client.initialize(
            &backstop_token,
            &Address::random(&e),
            &Address::random(&e),
            &Address::random(&e),
            &drop_list,
        );

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &1000);
            storage::set_backstop(&e, &backstop);
            storage::set_drop_status(&e, false);
            storage::set_last_fork(&e, 4000000);

            let list = execute_drop(&e);
            assert_eq!(storage::get_drop_status(&e), true);
            assert_eq!(list.len(), 2);
            assert_eq!(backstop_token_client.balance(&frodo), 20_000_000 * SCALAR_7);
            assert_eq!(
                backstop_token_client.balance(&samwise),
                30_000_000 * SCALAR_7
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #40)")]
    fn test_drop_already_dropped() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 5000000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let frodo = Address::random(&e);
        let samwise = Address::random(&e);
        let emitter = create_emitter(&e);
        let (backstop, backstop_client) = create_backstop(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let drop_list = map![
            &e,
            (frodo.clone(), 20_000_000 * SCALAR_7),
            (samwise.clone(), 30_000_000 * SCALAR_7)
        ];

        backstop_client.initialize(
            &backstop_token,
            &Address::random(&e),
            &Address::random(&e),
            &Address::random(&e),
            &drop_list,
        );

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &1000);
            storage::set_backstop(&e, &backstop);
            storage::set_drop_status(&e, true);
            storage::set_last_fork(&e, 4000000);

            execute_drop(&e);
            assert_eq!(storage::get_drop_status(&e), true);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #40)")]
    fn test_drop_too_large() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 5000000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let frodo = Address::random(&e);
        let samwise = Address::random(&e);
        let emitter = create_emitter(&e);
        let (backstop, backstop_client) = create_backstop(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let drop_list = map![
            &e,
            (frodo.clone(), 20_000_000 * SCALAR_7),
            (samwise.clone(), 30_000_001 * SCALAR_7)
        ];

        backstop_client.initialize(
            &backstop_token,
            &Address::random(&e),
            &Address::random(&e),
            &Address::random(&e),
            &drop_list,
        );

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &1000);
            storage::set_backstop(&e, &backstop);
            storage::set_drop_status(&e, false);
            storage::set_last_fork(&e, 4000000);

            execute_drop(&e);
            assert_eq!(storage::get_drop_status(&e), false);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Storage, MissingValue)")]
    fn test_drop_no_status() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let frodo = Address::random(&e);
        let samwise = Address::random(&e);
        let emitter = create_emitter(&e);
        let (backstop, backstop_client) = create_backstop(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let drop_list = map![
            &e,
            (frodo.clone(), 20_000_000 * SCALAR_7),
            (samwise.clone(), 30_000_000 * SCALAR_7)
        ];

        backstop_client.initialize(
            &backstop_token,
            &Address::random(&e),
            &Address::random(&e),
            &Address::random(&e),
            &drop_list,
        );

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &1000);
            storage::set_backstop(&e, &backstop);

            execute_drop(&e);
        });
    }
    #[test]
    #[should_panic(expected = "Error(Contract, #40)")]
    fn test_drop_bad_block() {
        let e = Env::default();
        e.mock_all_auths_allowing_non_root_auth();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 5000000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let frodo = Address::random(&e);
        let samwise = Address::random(&e);
        let emitter = create_emitter(&e);
        let (backstop, backstop_client) = create_backstop(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let drop_list = map![
            &e,
            (frodo.clone(), 20_000_000 * SCALAR_7),
            (samwise.clone(), 30_000_000 * SCALAR_7)
        ];

        backstop_client.initialize(
            &backstop_token,
            &Address::random(&e),
            &Address::random(&e),
            &Address::random(&e),
            &drop_list,
        );

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &1000);
            storage::set_backstop(&e, &backstop);
            storage::set_last_fork(&e, 5000000);
            storage::set_drop_status(&e, false);

            execute_drop(&e);
        });
    }
}
