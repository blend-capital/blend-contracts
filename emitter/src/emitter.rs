use crate::{constants::SCALAR_7, errors::EmitterError, storage};
use sep_41_token::StellarAssetClient;
use soroban_sdk::{panic_with_error, Address, Env, Map};

/// Perform a distribution
pub fn execute_distribute(e: &Env, backstop: &Address) -> i128 {
    let timestamp = e.ledger().timestamp();
    let seconds_since_last_distro = timestamp - storage::get_last_distro_time(e, backstop);
    // Blend tokens are distributed at a rate of 1 token per second
    let distribution_amount = (seconds_since_last_distro as i128) * SCALAR_7;
    storage::set_last_distro_time(e, backstop, timestamp);

    let blnd_id = storage::get_blnd_token(e);
    let blnd_client = StellarAssetClient::new(e, &blnd_id);
    blnd_client.mint(backstop, &distribution_amount);

    distribution_amount
}

/// Perform drop BLND distribution
pub fn execute_drop(e: &Env, list: &Map<Address, i128>) {
    let backstop = storage::get_backstop(e);
    backstop.require_auth();

    if storage::get_drop_status(e, &backstop) {
        panic_with_error!(e, EmitterError::BadDrop);
    }
    if storage::get_last_fork(e) + 777600 > e.ledger().sequence() {
        // Check that the last fork was at least 45 days ago
        panic_with_error!(e, EmitterError::BadDrop);
    }

    let mut drop_amount = 0;
    for (_, amt) in list.iter() {
        drop_amount += amt;
    }
    // drop cannot be more than 50 million tokens
    if drop_amount > 50_000_000 * SCALAR_7 {
        panic_with_error!(e, EmitterError::BadDrop);
    }

    let blnd_id = storage::get_blnd_token(e);
    let blnd_client = StellarAssetClient::new(e, &blnd_id);
    for (addr, amt) in list.iter() {
        blnd_client.mint(&addr, &amt);
    }
    storage::set_drop_status(e, &backstop);
}

#[cfg(test)]
mod tests {

    use crate::{storage, testutils::create_emitter};

    use super::*;
    use sep_41_token::testutils::MockTokenClient;
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
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);

        let blnd_id = e.register_stellar_asset_contract(emitter.clone());
        let blnd_client = MockTokenClient::new(&e, &blnd_id);

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_blnd_token(&e, &blnd_id);

            let result = execute_distribute(&e, &backstop);
            assert_eq!(result, 11345_0000000);
            assert_eq!(blnd_client.balance(&backstop), 11345_0000000);
            assert_eq!(storage::get_last_distro_time(&e, &backstop), 12345);
        });
    }

    #[test]
    fn test_drop() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 5000000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let frodo = Address::generate(&e);
        let samwise = Address::generate(&e);
        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);

        let blnd_id = e.register_stellar_asset_contract(emitter.clone());
        let blnd_client = MockTokenClient::new(&e, &blnd_id);
        let drop_list = map![
            &e,
            (frodo.clone(), 20_000_000 * SCALAR_7),
            (samwise.clone(), 30_000_000 * SCALAR_7)
        ];

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_blnd_token(&e, &blnd_id);
            storage::set_last_fork(&e, 4000000);

            execute_drop(&e, &drop_list);
            assert_eq!(storage::get_drop_status(&e, &backstop), true);
            assert_eq!(blnd_client.balance(&frodo), 20_000_000 * SCALAR_7);
            assert_eq!(blnd_client.balance(&samwise), 30_000_000 * SCALAR_7);
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
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let frodo = Address::generate(&e);
        let samwise = Address::generate(&e);
        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);

        let blnd_id = e.register_stellar_asset_contract(emitter.clone());
        let drop_list = map![
            &e,
            (frodo.clone(), 20_000_000 * SCALAR_7),
            (samwise.clone(), 30_000_000 * SCALAR_7)
        ];

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_blnd_token(&e, &blnd_id);
            storage::set_drop_status(&e, &backstop);
            storage::set_last_fork(&e, 4000000);

            execute_drop(&e, &drop_list);
            assert_eq!(storage::get_drop_status(&e, &backstop), true);
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
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let frodo = Address::generate(&e);
        let samwise = Address::generate(&e);
        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);

        let blnd_id = e.register_stellar_asset_contract(emitter.clone());
        let drop_list = map![
            &e,
            (frodo.clone(), 20_000_000 * SCALAR_7),
            (samwise.clone(), 30_000_001 * SCALAR_7)
        ];

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_blnd_token(&e, &blnd_id);
            storage::set_last_fork(&e, 4000000);

            execute_drop(&e, &drop_list);
            assert_eq!(storage::get_drop_status(&e, &backstop), false);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #40)")]
    fn test_drop_bad_block() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 20,
            sequence_number: 5000000,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 10,
            min_persistent_entry_ttl: 10,
            max_entry_ttl: 2000000,
        });

        let bombadil = Address::generate(&e);
        let frodo = Address::generate(&e);
        let samwise = Address::generate(&e);
        let emitter = create_emitter(&e);
        let backstop = Address::generate(&e);

        let blnd_id = e.register_stellar_asset_contract(bombadil.clone());
        let drop_list = map![
            &e,
            (frodo.clone(), 20_000_000 * SCALAR_7),
            (samwise.clone(), 30_000_000 * SCALAR_7)
        ];

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &backstop, 1000);
            storage::set_backstop(&e, &backstop);
            storage::set_blnd_token(&e, &blnd_id);
            storage::set_last_fork(&e, 5000000);

            execute_drop(&e, &drop_list);
        });
    }
}
