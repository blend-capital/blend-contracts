import { Contract, xdr, Address, hash } from "soroban-client";
import { Config } from "../config.js";
import { createTxBuilder, signPrepareAndSubmitTransaction } from "../utils.js";

/**
 * @typedef AssetPrice
 * @property {BigInt} price - 7 decimals
 * @property {string} assetKey
 *
 * @param {Server} stellarRpc
 * @param {Config} config
 * @param {AssetPrice[]} assetPrices
 */
export async function setAssetPrices(stellarRpc, config, assetPrices) {
  let network = config.network.passphrase;
  let bombadil = config.getAddress("bombadil");
  let contract = new Contract(config.getContractId("oracle"));

  console.log("START: setting asset prices for oracle");
  for (const asset of assetPrices) {
    let txBuilder = await createTxBuilder(stellarRpc, network, bombadil);
    txBuilder.addOperation(
      contract.call(
        "set_price",
        xdr.ScVal.scvBytes(
          Buffer.from(config.getContractId(asset.assetKey), "hex")
        ),
        xdr.ScVal.scvU64(xdr.Uint64.fromString(asset.price.toString()))
      )
    );
    await signPrepareAndSubmitTransaction(
      stellarRpc,
      network,
      txBuilder.build(),
      bombadil
    );
    console.log(
      "set price for " + asset.assetKey + " to " + asset.price.toString()
    );
  }
  console.log("DONE: asset prices set\n");
}
