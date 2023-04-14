import { Asset, Server, TransactionBuilder } from "soroban-client";
import {
  WasmKeys,
  createTxBuilder,
  signAndSubmitTransaction,
  signPrepareAndSubmitTransaction,
} from "./utils.js";
import {
  createDeployOperation,
  createDeployStellarAssetOperation,
} from "./operations/deploy.js";
import { Config } from "./config.js";
import { createInstallOperation } from "./operations/install.js";
import * as token from "./operations/token.js";

/**
 * outline
 *
 * rpc.requestAirdrop all initial user keys? (shouldn't fail if funded)
 *
 * deploy tokens
 * - deploy USDC (soroban asset)
 * - deploy XLM (native stellar asset)
 * - deploy BLND token
 * - deploy backstop token
 * - initialize tokens
 *
 * deploy external contracts
 * - deploy mock oracle
 *
 * deploy blnd
 * - deploy emitter
 * - deploy backstop
 * - deploy lending pool
 * - deploy pool factory
 * - initialize contracts
 */

console.log("starting deployment script...");

let config = Config.loadFromFile();
let bombadil = config.getAddress("bombadil");
let stellarRpc = new Server(config.network.rpc, {
  allowHttp: true,
});

let network = config.network.passphrase;

let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);

//***** tokens *****

console.log("START: installing token contract");
txBuilder.addOperation(createInstallOperation(WasmKeys.token, config));
await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
config.writeToFile();
console.log("DONE: installing token contract");

// USDC
console.log("START: USDC");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  createDeployOperation("USDC", WasmKeys.token, config, bombadil)
);
await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
config.writeToFile();
console.log("DONE: deploy USDC");

txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  token.createInitialize(
    config.getContractId("USDC"),
    bombadil.publicKey(),
    "USDC"
  )
);
await signPrepareAndSubmitTransaction(
  stellarRpc,
  network,
  txBuilder.build(),
  bombadil
);
console.log("DONE: USDC\n");

// XLM
console.log("START: XLM");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  createDeployStellarAssetOperation(Asset.native(), config)
);
await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
console.log("DONE: XLM\n");

console.log("ending deployment script");
