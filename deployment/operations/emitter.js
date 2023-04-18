import { Contract, xdr } from "soroban-client";
import { Config } from "../config.js";

/********** Operation Builders **********/

/**
 * @param {Config} config
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInitialize(config) {
  let emitterContract = new Contract(config.getContractId("emitter"));
  return emitterContract.call(
    "initialize",
    xdr.ScVal.scvBytes(Buffer.from(config.getContractId("backstop"), "hex")),
    xdr.ScVal.scvBytes(Buffer.from(config.getContractId("BLND"), "hex"))
  );
}
