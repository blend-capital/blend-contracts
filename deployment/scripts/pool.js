import { Server } from "soroban-client";
import { randomBytes } from "crypto";
import { Config } from "../config.js";
import { createTxBuilder, signPrepareAndSubmitTransaction } from "../utils.js";
import * as pool from "../operations/pool.js";
import * as poolFactory from "../operations/poolFactory.js";

/**
 * @param {Server} stellarRpc
 * @param {Config} config
 * @param {string} poolName
 */
export async function deployAndSetupPool(stellarRpc, config, poolName) {
  let bombadil = config.getAddress("bombadil");
  let network = config.network.passphrase;
  let backstopTakeRate = "20000000"; // 20% - 9 decimals

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

  console.log("START Initialize Reserves");
  txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  let reserveMetaXLM = pool.createDefaultReserveMetadata();
  reserveMetaXLM.c_factor = 800000;
  txBuilder.addOperation(
    pool.createInitReserve(
      poolName,
      config,
      bombadil.publicKey(),
      "XLM",
      reserveMetaXLM
    )
  );
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  config.writeToFile();
  console.log("created reserve for XLM in ", poolName, "\n");

  txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
  let reserveMetaUSDC = pool.createDefaultReserveMetadata();
  reserveMetaUSDC.c_factor = 900000;
  reserveMetaUSDC.l_factor = 950000;
  reserveMetaUSDC.util = 850000;
  txBuilder.addOperation(
    pool.createInitReserve(
      poolName,
      config,
      bombadil.publicKey(),
      "USDC",
      reserveMetaUSDC
    )
  );
  await signPrepareAndSubmitTransaction(
    stellarRpc,
    network,
    txBuilder.build(),
    bombadil
  );
  config.writeToFile();
  console.log("created reserve for USDC in ", poolName, "\n");

  console.log("DONE: deployed pool ", poolName, "\n");
}
