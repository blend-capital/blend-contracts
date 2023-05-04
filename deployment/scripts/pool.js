import { Server, Address } from "soroban-client";
import { randomBytes } from "crypto";
import { Config } from "../config.js";
import { createTxBuilder, signPrepareAndSubmitTransaction } from "../utils.js";
import * as backstop from "../operations/backstop.js";
import * as token from "../operations/token.js";
import * as pool from "../operations/pool.js";
import * as poolFactory from "../operations/poolFactory.js";

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 * @param {string} poolName
 * @param {string[]} assets
 * @param {pool.ReserveMetadata[]} metadata
 * @param {pool.ReserveEmissionMetadata[]} emissionMetadata
 */
export async function deployAndSetupPool(
  stellarRpc,
  config,
  poolName,
  assets,
  metadata,
  emissionMetadata
) {
  if (assets.length !== metadata.length) {
    console.log("Unable to deploy");
    return;
  }

  let bombadil = config.getAddress("bombadil");
  let network = config.network.passphrase;
  let backstopTakeRate = "10000000"; // 10% - 9 decimals

  console.log("START Create Pool: ", poolName);
  let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    poolFactory.createDeployPool(
      config,
      bombadil.publicKey(),
      randomBytes(32),
      backstopTakeRate,
      poolName
    )
  );
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  config.writeToFile();
  console.log("deployed ", poolName, "\n");

  for (let i = 0; i < assets.length; i++) {
    let assetKey = assets[i];
    let reserveMeta = metadata[i];

    console.log("START Initialize Reserve: ", assetKey);
    txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
    txBuilder.addOperation(
      pool.createInitReserve(
        poolName,
        config,
        bombadil.publicKey(),
        assetKey,
        reserveMeta
      )
    );
    await signPrepareAndSubmitTransaction(
      stellarRpc,
      network,
      txBuilder.build(),
      bombadil
    );
    config.writeToFile();
    console.log("created reserve for: ", assetKey, "\n");
  }
  console.log("DONE: deployed pool ", poolName, "\n");

  console.log("START: Enable emissions");
  txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    pool.createSetEmissions(
      config,
      poolName,
      bombadil.publicKey(),
      emissionMetadata
    )
  );
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  console.log("DONE: Setup pool emissions");
}

/**
 * Deposit funds into the pools backstop, activate the pool,
 * and add it to the reward zone for the backstop
 * @param {Server} stellarRpc
 * @param {Config} config
 * @param {string} poolName
 */
export async function setupPoolBackstop(stellarRpc, config, poolName) {
  let network = config.network.passphrase;
  let bombadil = config.getAddress("bombadil");
  let frodo = config.getAddress("frodo");
  let backstopToken = config.getContractId("BLNDUSDC");

  console.log("Starting pool backstop setup\n");
  console.log("START: Mint frodo required tokens...");
  let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(
    token.createMint(
      backstopToken,
      bombadil.publicKey(),
      frodo.publicKey(),
      BigInt(1_000_000e7)
    )
  );
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  console.log("minted backstop tokens...");
  console.log("DONE: minted frodo required tokens\n");

  console.log("START: Deposit into backstop");
  txBuilder = await createTxBuilder(stellarRpc, network, frodo);
  txBuilder.addOperation(
    backstop.createDeposit(
      config,
      poolName,
      frodo.publicKey(),
      BigInt(1_000_000e7)
    )
  );
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    frodo
  );
  console.log("DONE: Deposited into backstop\n");

  console.log("START: Active pool");
  txBuilder = await createTxBuilder(stellarRpc, network, frodo);
  txBuilder.addOperation(pool.createUpdateState(config, poolName));
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    frodo
  );
  console.log("DONE: Activated Pool\n");

  console.log("START: Move pool into reward zone");
  txBuilder = await createTxBuilder(stellarRpc, network, frodo);
  txBuilder.addOperation(
    backstop.createAddToRewardZone(
      config,
      config.getContractId(poolName),
      config.getContractId(poolName)
    )
  );
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    frodo
  );
  console.log("DONE: Moved pool into reward zone\n");
}

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 * @param {string[]} poolNames
 */
export async function distribute(stellarRpc, config, poolNames) {
  let network = config.network.passphrase;
  let bombadil = config.getAddress("bombadil");
  let blndToken = config.getContractId("BLND");
  let backstopId = config.getContractId("backstop");

  console.log("START: Start distribution for backstop and pool\n");
  let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  txBuilder.addOperation(backstop.createDistribute(config));
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  console.log("backstop distributed...");

  for (const poolName of poolNames) {
    txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
    txBuilder.addOperation(pool.createUpdateEmissions(config, poolName));
    await signPrepareAndSubmitTransaction(
      stellarRpc,
      network,
      txBuilder.build(),
      bombadil
    );
    console.log("pool distributed...", poolName);
  }

  console.log("DONE: backstop and pool emissions started\n");
}

/**
 * @typedef Amount
 * @property {string} key
 * @property {bigint} amount
 *
 * @param {Server} stellarRpc
 * @param {Config} config
 * @param {Amount[]} amounts
 */
export async function mintWhale(stellarRpc, config, amounts) {
  let network = config.network.passphrase;
  let bombadil = config.getAddress("bombadil");
  let frodo = config.getAddress("frodo");

  console.log("START: Minting tokens");

  for (const toMint of amounts) {
    let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
    txBuilder.addOperation(
      token.createMint(
        config.getContractId(toMint.key),
        bombadil.publicKey(),
        frodo.publicKey(),
        toMint.amount
      )
    );
    await signPrepareAndSubmitTransaction(
      stellarRpc,
      network,
      txBuilder.build(),
      bombadil
    );
    console.log(`minted ${toMint.amount} of ${toMint.key}...\n`);
  }

  console.log("DONE: Minted positions\n");
}

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 * @param {string} poolName
 * @param {Amount[]} supplies
 * @param {Amount[]} borrows
 */
export async function addWhale(
  stellarRpc,
  config,
  poolName,
  supplies,
  borrows
) {
  let network = config.network.passphrase;
  let frodo = config.getAddress("frodo");

  console.log("START: Supplying tokens");
  for (const supply of supplies) {
    let txBuilder = await createTxBuilder(stellarRpc, network, frodo);
    txBuilder.addOperation(
      pool.createSupply(
        config,
        poolName,
        frodo.publicKey(),
        supply.key,
        supply.amount
      )
    );
    await signPrepareAndSubmitTransaction(
      stellarRpc,
      network,
      txBuilder.build(),
      frodo
    );
    console.log(`supplied ${supply.amount} of ${supply.key}...\n`);
  }
  console.log("DONE: Supplying tokens");

  console.log("START: Borrowing tokens");
  for (const borrow of borrows) {
    let txBuilder = await createTxBuilder(stellarRpc, network, frodo);
    txBuilder.addOperation(
      pool.createBorrow(
        config,
        poolName,
        frodo.publicKey(),
        borrow.key,
        borrow.amount,
        frodo.publicKey()
      )
    );
    await signPrepareAndSubmitTransaction(
      stellarRpc,
      network,
      txBuilder.build(),
      frodo
    );
    console.log(`borrowed ${borrow.amount} of ${borrow.key}...\n`);
  }
  console.log("DONE: Borrowing tokens");

  console.log("DONE: Added whale to pool: ", frodo.publicKey(), "\n");
}
