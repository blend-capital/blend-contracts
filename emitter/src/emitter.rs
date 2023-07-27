use crate::{
    constants::SCALAR_7,
    dependencies::{BackstopClient, TokenClient},
    errors::EmitterError,
    storage,
};
use soroban_sdk::{panic_with_error, Address, Env};

/// Perform a distribution
pub fn execute_distribute(e: &Env, backstop: &Address) -> i128 {
    let timestamp = e.ledger().timestamp();
    let seconds_since_last_distro = timestamp - storage::get_last_distro_time(e);
    // Blend tokens are distributed at a rate of 1 token per second
    let distribution_amount = (seconds_since_last_distro as i128) * SCALAR_7;
    storage::set_last_distro_time(e, &timestamp);

    let blend_id = storage::get_blend_id(e);
    let blend_client = TokenClient::new(e, &blend_id);
    blend_client.mint(&backstop, &distribution_amount);

    distribution_amount
}

/// Perform a backstop swap
pub fn execute_swap_backstop(e: &Env, new_backstop_id: Address) {
    let backstop = storage::get_backstop(e);
    let backstop_token = BackstopClient::new(&e, &backstop).backstop_token();
    let backstop_token_client = TokenClient::new(&e, &backstop_token);

    let backstop_balance = backstop_token_client.balance(&backstop);
    let new_backstop_balance = backstop_token_client.balance(&new_backstop_id);
    if new_backstop_balance > backstop_balance {
        storage::set_backstop(e, &new_backstop_id);
    } else {
        panic_with_error!(e, EmitterError::InsufficientBackstopSize);
    }
}

#[cfg(test)]
mod tests {
    use crate::{storage, testutils::create_backstop};

    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};

    #[test]
    fn test_distribute() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let emitter = Address::random(&e);
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
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let emitter = Address::random(&e);
        let (backstop, backstop_client) = create_backstop(&e);
        let new_backstop = Address::random(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = TokenClient::new(&e, &backstop_token);

        backstop_client.initialize(&backstop_token, &Address::random(&e), &Address::random(&e));

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_001 * SCALAR_7));

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &1000);
            storage::set_backstop(&e, &backstop);

            execute_swap_backstop(&e, new_backstop.clone());
            assert_eq!(storage::get_backstop(&e), new_backstop);
        });
    }

    #[test]
    #[should_panic(expected = "HostError")]
    // #[should_panic(expected = "ContractError(30)")]
    fn test_swap_backstop_not_enough() {
        let e = Env::default();
        e.mock_all_auths();

        e.ledger().set(LedgerInfo {
            timestamp: 12345,
            protocol_version: 1,
            sequence_number: 50,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_expiration: 10,
            min_persistent_entry_expiration: 10,
            max_entry_expiration: 2000000,
        });

        let bombadil = Address::random(&e);
        let emitter = Address::random(&e);
        let (backstop, backstop_client) = create_backstop(&e);
        let new_backstop = Address::random(&e);

        let backstop_token = e.register_stellar_asset_contract(bombadil.clone());
        let backstop_token_client = TokenClient::new(&e, &backstop_token);

        backstop_client.initialize(&backstop_token, &Address::random(&e), &Address::random(&e));

        backstop_token_client.mint(&backstop, &(1_000_000 * SCALAR_7));
        backstop_token_client.mint(&new_backstop, &(1_000_000 * SCALAR_7));

        e.as_contract(&emitter, || {
            storage::set_last_distro_time(&e, &1000);
            storage::set_backstop(&e, &backstop);

            execute_swap_backstop(&e, new_backstop.clone());
            assert!(false, "Should have panicked");
        });
    }
}
