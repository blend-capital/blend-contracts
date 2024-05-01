# Blend Contract SDK

This repository contains interfaces, clients, and WASM blobs for the Blend Protocol as implemented in the [Blend Contracts](https://github.com/blend-capital/blend-contracts) repository.

## Documentation

To learn more about the Blend Protocol, visit the the docs:
* [Blend Docs](https://docs.blend.capital/)

## Modules

The Blend Contract SDK generates modules from the `contractimport` [Soroban SDK macro](). Each module exposes a Client, WASM, and the respective types needed to interact with the Blend Protocol. The following Blend contracts are exposed as a module:

* `backstop` - Contract import for the backstop contract
* `emitter`- Contract import for the emitter contract
* `pool` - Contract import for the pool contract
* `pool_factory` - Contract import for the pool factory contract

## Testing (testutils)

### External Dependencies

The Blend Contract SDK includes `contractimport`'s of the [Comet Contracts](https://github.com/CometDEX/comet-contracts) when compiled for test purposes via the `testutils` feature.

This includes:
* `comet` - Contract import for the comet pool contract
* `comet_factory` - Contract import for the comet pool factory contract

NOTE: These contracts were used for testing the Blend Protocol and should not be considered to be the latest version of the Comet Protocol. Please verify any non-test usage of the Comet contracts against the [Comet GitHub](https://github.com/CometDEX/comet-contracts).

### Setup

The `testutils` module allows for easy deployment of Blend Contracts to be used in a unit test. The following example shows how to use the `testutils` to deploy a set of Blend Contracts and set up a pool.

If you require using the pool, please look at the following [sep-41-oracle]() crate to deploy a mock oracle contract: 

```rust
use soroban_sdk::{symbol_short, testutils::{Address as _, BytesN as _}, Address, BytesN, Env};

use blend_contract_sdk::{pool, testutils::{default_reserve_config, BlendFixture}};

let env = Env::default();
let deployer = Address::generate(&env);
let blnd = env.register_stellar_asset_contract(deployer.clone());
let usdc = env.register_stellar_asset_contract(deployer.clone());
let blend = BlendFixture::deploy(&env, &deployer, &blnd, &usdc);

let token = env.register_stellar_asset_contract(deployer.clone());
let pool = blend.pool_factory.mock_all_auths().deploy(
    &deployer,
    &symbol_short!("test"),
    &BytesN::<32>::random(&env),
    &Address::generate(&env),
    &0_1000000, // 10%
    &4, // 4 max positions
);
let pool_client = pool::Client::new(&env, &pool);
let reserve_config = default_reserve_config();
pool_client.mock_all_auths().queue_set_reserve(&token, &reserve_config);
pool_client.mock_all_auths().set_reserve(&token);

blend.backstop.mock_all_auths().deposit(&deployer, &pool, &50_000_0000000);
pool_client.mock_all_auths().set_status(&3); // remove pool from setup status
pool_client.mock_all_auths().update_status(); // update status based on backstop
```

## WASM Verification

The WASM files included will align with the GitHub release the SDK was published with (the version numbers will match).

The WASMs are generated with the [Stellar Expert WASM Release Action](https://github.com/stellar-expert/soroban-build-workflow)

The SHA256 Checksums:
* backstop - `62f61b32fff99f7eec052a8e573c367759f161c481a5caf0e76a10ae4617c3b4`
* emitter - `438a5528cff17ede6fe515f095c43c5f15727af17d006971485e52462e7e7b89`
* pool_factory - `0287f4ad7350935b83d94e046c0bcabc960b233dbce1531008c021b71d406a1d`
* pool - `baf978f10efdbcd85747868bef8832845ea6809f7643b67a4ac0cd669327fc2c`