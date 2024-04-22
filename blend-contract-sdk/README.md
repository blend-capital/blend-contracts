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

The WASM files included will align with the GitHub release the SDK was published with (the version numbers will match). The WASM files were generated with the Makefile.

Since WASM builds can vary based on factors like OS, here are the details of the machine that built the WASMs included in this package:

* Ubuntu 22.04.4 LTS
* stable-x86_64-unknown-linux-gnu (default)
* rustc 1.77.1 (7cf61ebde 2024-03-27)
* soroban 20.3.1 (ae5446f63ca8a275e61912019199254d598f3bd5)
* soroban-env 20.2.1 (18a10592853d9edf4e341b565b0b1638f95f0393)
* soroban-env interface version 85899345920
* stellar-xdr 20.1.0 (8b9d623ef40423a8462442b86997155f2c04d3a1)
* xdr curr (b96148cd4acc372cc9af17b909ffe4b12c43ecb6)

The SHA256 Checksums:
* backstop - `8dfbc6ba300cde6ebe747cf985cf9221bf3594981595b9c26bbd108ff19a5598`
* emitter - `b1555702a4cca7c44e02beb5aa82b0271c0367471c68f2ec9246c31b202e38ee`
* pool_factory - `8bc7894d8a4e46b085d0579e233e3c436bb34e18f9a2a83d4bde8526cde18cb6`
* pool - `76ebcea354d5959c5b0d38818ddf1524c5af16970231b532f5caa63121930861`