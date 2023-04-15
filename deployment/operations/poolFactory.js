import { Contract, xdr, Address } from "soroban-client";
import { Config } from "../config.js";

/********** Operation Builders **********/

/**
 * @param {Config} config
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInitialize(config) {
  let poolInitMetaObj = {
    pool_hash: config.getWasmHash("lendingPool"),
    b_token_hash: config.getWasmHash("bToken"),
    d_token_hash: config.getWasmHash("dToken"),
    backstop: config.getContractId("backstop"),
    blnd_id: config.getContractId("BLND"),
    usdc_id: config.getContractId("USDC"),
  };
  let poolInitMeta = [];
  for (const key of Object.keys(poolInitMetaObj).sort()) {
    poolInitMeta.push(
      new xdr.ScMapEntry({
        key: xdr.ScVal.scvSymbol(key),
        val: xdr.ScVal.scvBytes(Buffer.from(poolInitMetaObj[key], "hex")),
      })
    );
  }
  let poolFactoryContract = new Contract(config.getContractId("poolFactory"));
  return poolFactoryContract.call("initialize", xdr.ScVal.scvMap(poolInitMeta));
}
