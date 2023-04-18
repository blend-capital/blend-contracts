import { Transaction, Server, TransactionBuilder, xdr } from "soroban-client";

export const WasmKeys = {
  token: "token",
  bToken: "bToken",
  dToken: "dToken",
  oracle: "oracle",
  emitter: "emitter",
  poolFactory: "poolFactory",
  backstop: "backstop",
  lendingPool: "lendingPool",
};

/**
 * @param {Server} stellarRpc
 * @param {string} network
 * @param {Transaction} tx
 * @param {Keypair} source
 */
export async function signPrepareAndSubmitTransaction(
  stellarRpc,
  network,
  tx,
  source
) {
  let prepped_tx = await stellarRpc.prepareTransaction(tx, network);
  await signAndSubmitTransaction(stellarRpc, prepped_tx, source);
}

/**
 * @param {Server} stellarRpc
 * @param {Transaction} tx
 * @param {Keypair} source
 */
export async function signAndSubmitTransaction(stellarRpc, tx, source) {
  try {
    tx.sign(source);
    console.log("submitting tx...");
    let response = await stellarRpc.sendTransaction(tx);
    let status = response.status;
    let tx_hash = response.hash;
    console.log(JSON.stringify(response));
    // Poll this until the status is not "pending"
    while (status === "PENDING" || status == "NOT_FOUND") {
      // See if the transaction is complete
      await new Promise((resolve) => setTimeout(resolve, 2000));
      console.log("checking tx...");
      response = await stellarRpc.getTransaction(tx_hash);
      status = response.status;
    }
    console.log("Transaction status:", response.status);
    console.log("Hash: ", tx_hash);
  } catch (e) {
    console.error(e);
    throw e;
  }
}

/**
 * @param {Server} stellarRpc
 * @param {string} network
 * @param {Keypair} source
 * @returns {Promise<TransactionBuilder>}
 */
export async function createTxBuilder(stellarRpc, network, source) {
  try {
    let account = await stellarRpc.getAccount(source.publicKey());

    return new TransactionBuilder(account, {
      fee: "1000",
      timebounds: { minTime: 0, maxTime: 0 },
      networkPassphrase: network,
    });
  } catch (e) {
    console.error(e);
    throw e;
  }
}
