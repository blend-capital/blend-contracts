import { Contract, xdr, Address } from "soroban-client";
import { Config } from "../config.js";

/********** Operation Builders **********/

/**
 * @param {Config} config
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInitialize(config) {
  let backstopContract = new Contract(config.getContractId("backstop"));
  return backstopContract.call(
    "initialize",
    xdr.ScVal.scvBytes(Buffer.from(config.getContractId("BLNDUSDC"), "hex")),
    xdr.ScVal.scvBytes(Buffer.from(config.getContractId("BLND"), "hex")),
    xdr.ScVal.scvBytes(Buffer.from(config.getContractId("poolFactory"), "hex"))
  );
}
