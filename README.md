# Blend Protocol

This repository contains the smart contacts for an implementation of the Blend Protocol. Blend is a universal liquidity protocol primitive that enables the permissionless creation of lending pools.

## Documentation

To learn more about the Blend Protocol, visit the whitepaper or the docs:
* [Whitepaper](https://github.com/blend-capital/blend-whitepaper)
* [Blend Docs](https://docs.blend.capital/)

## Audits

No audits have been conducted for the protocol at this time. Results will be included here at the conclusion of an audit.

## Getting Started

Build the contracts with:

```
make
```

Run all unit tests and the integration test suite with:

```
make test
```

## Deployment

The `make` command creates an optimized and un-optimized set of WASM contracts. It's recommended to use the optimized version if deploying to a network.

These can be found at the path:
```
target/wasm32-unknown-unknown/optimized
```
For help with deployment to a network, please visit the [Blend Utils](https://github.com/blend-capital/blend-utils) repo.

## Community Links

A set of links for various things in the community. Please submit a pull request if you would like a link included.

* [Blend Discord](https://discord.com/invite/a6CDBQQcjW)
