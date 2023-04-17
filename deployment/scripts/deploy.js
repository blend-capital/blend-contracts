import { Keypair, Server, Asset } from "soroban-client";
import { Config } from "../config.js";
import {
  WasmKeys,
  createTxBuilder,
  signAndSubmitTransaction,
  signPrepareAndSubmitTransaction,
} from "../utils.js";
import {
  createDeployOperation,
  createDeployStellarAssetOperation,
  createInstallOperation,
} from "../operations/contract.js";
import * as token from "../operations/token.js";
import * as emitter from "../operations/emitter.js";
import * as backstop from "../operations/backstop.js";
import * as poolFactory from "../operations/poolFactory.js";
import BigNumber from "bignumber.js";

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 */
export async function airdropAccounts(stellarRpc, config) {
  for (let user in config.users) {
    let pubKey = config.getAddress(user).publicKey();
    try {
      await stellarRpc.requestAirdrop(
        pubKey,
        "http://localhost:8000/friendbot"
      );
      console.log("Funded: ", pubKey);
    } catch (e) {
      console.log(pubKey, " already funded");
    }
  }
  console.log("All users airdropped\n");
}

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 */
export async function installWasm(stellarRpc, config) {
  let bombadil = config.getAddress("bombadil");
  let network = config.network.passphrase;
  for (let contract in WasmKeys) {
    let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
    console.log(`START: installing ${WasmKeys[contract]} contract`);
    txBuilder.addOperation(createInstallOperation(WasmKeys[contract], config));
    await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
    config.writeToFile();
    console.log(`DONE: installing ${WasmKeys[contract]} contract\n`);
  }
  console.log("All WASM installed\n");
}

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 * @param {string} symbol
 */
export async function deployAndInitToken(stellarRpc, config, symbol) {
  let bombadil = config.getAddress("bombadil");
  let network = config.network.passphrase;
  console.log("START Token: ", symbol);
  let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    createDeployOperation(symbol, WasmKeys.token, config, bombadil)
  );
  await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
  config.writeToFile();
  console.log("deployed ", symbol);

  txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    token.createInitialize(
      config.getContractId(symbol),
      bombadil.publicKey(),
      symbol
    )
  );
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  console.log("DONE Token: ", symbol, "\n");
}

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 * @param {Asset} asset
 */
export async function deployAndInitStellarToken(stellarRpc, config, asset) {
  let bombadil = config.getAddress("bombadil");
  let network = config.network.passphrase;
  console.log("START Stellar Token: ", asset.code);
  let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(createDeployStellarAssetOperation(asset, config));
  await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
  config.writeToFile();
  console.log("DONE Stellar Token: ", asset.code, "\n");
}

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 */
export async function deployAndInitExternalContracts(stellarRpc, config) {
  let bombadil = config.getAddress("bombadil");
  let network = config.network.passphrase;
  console.log("START: deploy mock oracle");
  let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    createDeployOperation("oracle", WasmKeys.oracle, config, bombadil)
  );
  await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
  config.writeToFile();
  console.log("DONE: deploy mock oracle\n");
}

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 */
export async function deployBlendContracts(stellarRpc, config) {
  let bombadil = config.getAddress("bombadil");
  let network = config.network.passphrase;
  console.log("START: deploy backstop contract");
  let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    createDeployOperation("backstop", WasmKeys.backstop, config, bombadil)
  );
  await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
  config.writeToFile();
  console.log("DONE: deploy backstop contract\n");

  console.log("START: deploy emitter contract");
  txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    createDeployOperation("emitter", WasmKeys.emitter, config, bombadil)
  );
  await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
  config.writeToFile();
  console.log("DONE: deploy emitter contract\n");

  console.log("START: deploy pool factory contract");
  txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    createDeployOperation("poolFactory", WasmKeys.poolFactory, config, bombadil)
  );
  await signAndSubmitTransaction(stellarRpc, txBuilder.build(), bombadil);
  config.writeToFile();
  console.log("DONE: deploy pool factory contract\n");
}

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 */
export async function initBlendContracts(stellarRpc, config) {
  let bombadil = config.getAddress("bombadil");
  let network = config.network.passphrase;
  console.log("START: initalize emitter contract");
  let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(emitter.createInitialize(config));
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  console.log("DONE: initialize emitter contract\n");

  console.log("START: initialize backstop contract");
  txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(backstop.createInitialize(config));
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  console.log("DONE: initialize backstop contract\n");

  console.log("START: initialize pool factory contract");
  txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(poolFactory.createInitialize(config));
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  console.log("DONE: initialize pool factory contract\n");
}

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 */
export async function transferBLNDToEmitter(stellarRpc, config) {
  let bombadil = config.getAddress("bombadil");
  let network = config.network.passphrase;
  let blndToken = config.getContractId("BLND");

  console.log("START: Mint extra BLND and transfer BLND admin to Emitter");
  let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    token.createMint(
      blndToken,
      bombadil.publicKey(),
      bombadil.publicKey(),
      BigNumber(10_000_000e7)
    )
  );
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  console.log("minted 10m BLND for testing...");

  txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    token.createSetAdminToContract(
      blndToken,
      bombadil.publicKey(),
      config.getContractId("emitter")
    )
  );
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  console.log("DONE: Emitter is now the BLND admin\n");
}
