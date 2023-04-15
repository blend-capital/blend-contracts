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
import * as emitter from "./operations/emitter.js";
import * as backstop from "./operations/backstop.js";
import * as poolFactory from "./operations/poolFactory.js";




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

try {
  await stellarRpc.requestAirdrop(bombadil.publicKey(), "http://localhost:8000/friendbot")
}
catch (e) {
  console.log(e)
  console.log("Account already funded")
}

/*****Install Contracts*****/
for (let contract in WasmKeys) {
  txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  console.log(`START: installing ${WasmKeys[contract]} contract`);
  txBuilder.addOperation(createInstallOperation(WasmKeys[contract], config));
  await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
  config.writeToFile();
  console.log(`DONE: installing ${WasmKeys[contract]} contract`);
}


//***** tokens *****
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
// USDC

// BLNDUSDC
console.log("START: BLNDUSDC");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  createDeployOperation("BLNDUSDC", WasmKeys.token, config, bombadil)
);
await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
config.writeToFile();
console.log("DONE: deploy BLNDUSDC");

txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  token.createInitialize(
    config.getContractId("BLNDUSDC"),
    bombadil.publicKey(),
    "BLNDUSDC"
  )
);
await signPrepareAndSubmitTransaction(
  stellarRpc,
  network,
  txBuilder.build(),
  bombadil
);
console.log("DONE: BLNDUSDC\n");
// BLNDUSDC

// BLND
console.log("START: BLND");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  createDeployOperation("BLND", WasmKeys.token, config, bombadil)
);
await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
config.writeToFile();
console.log("DONE: deploy BLND");

txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  token.createInitialize(
    config.getContractId("BLND"),
    bombadil.publicKey(),
    "BLND"
  )
);
await signPrepareAndSubmitTransaction(
  stellarRpc,
  network,
  txBuilder.build(),
  bombadil
);
console.log("DONE: BLND\n");
// BLND

// XLM
console.log("START: XLM");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  createDeployStellarAssetOperation(Asset.native(), config)
);
await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
console.log("DONE: XLM\n");

console.log("ending deployment script");

/***** Mock Oracle *****/
console.log("START: deploy mock oracle contract");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  createDeployOperation("oracle", WasmKeys.oracle, config, bombadil)
);
await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
config.writeToFile();
console.log("DONE: deploy mock oracle contract");

/***** Deploy Blend Contracts *****/
console.log("START: deploy backstop contract");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  createDeployOperation("backstop", WasmKeys.backstop, config, bombadil)
);
await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
config.writeToFile();
console.log("DONE: deploy backstop contract");

console.log("START: deploy emitter contract");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  createDeployOperation("emitter", WasmKeys.emitter, config, bombadil)
);
await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
config.writeToFile();
console.log("DONE: deploy emitter contract");

console.log("START: deploy pool factory contract");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  createDeployOperation("poolFactory", WasmKeys.poolFactory, config, bombadil)
);
await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
config.writeToFile();
console.log("DONE: deploy pool factory contract");

/***** Emitter *****/
console.log("START: initalize emitter contract");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  emitter.createInitialize(
    config.getContractId("emitter"),
    config.getContractId("backstop"),
    config.getContractId("BLND")
  )
);
await signPrepareAndSubmitTransaction(
  stellarRpc,
  network,
  txBuilder.build(),
  bombadil
);
console.log("Done: initialize emitter contract");

/***** Backstop *****/
console.log("START: initalize backstop contract");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  backstop.createInitialize(
    config.getContractId("backstop"),
    config.getContractId("BLNDUSDC"),
    config.getContractId("BLND"),
    config.getContractId("poolFactory")
  )
);
await signPrepareAndSubmitTransaction(
  stellarRpc,
  network,
  txBuilder.build(),
  bombadil
);
console.log("DONE: initalize backstop contract");

/***** Pool Factory *****/
console.log("START: initalize pool factory contract");
txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
txBuilder.addOperation(
  poolFactory.createInitialize(
    config.getContractId("poolFactory"),
    {
      "pool_hash": config.getWasmHash("lendingPool"),
      "b_token_hash": config.getWasmHash("bToken"),
      "d_token_hash": config.getWasmHash("dToken"),
      "backstop": config.getContractId("backstop"),
      "blnd_id": config.getContractId("BLND"),
      "usdc_id": config.getContractId("USDC")
    }
  )
);
await signPrepareAndSubmitTransaction(
  stellarRpc,
  network,
  txBuilder.build(),
  bombadil
);
console.log("DONE: initalize pool factory contract")
