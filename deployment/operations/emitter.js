import { Contract, xdr, Address } from "soroban-client";

/********** Operation Builders **********/

/**
 * @param {string} address
 * @param {string} backstopId
 * @param {string} blndTokenId
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInitialize(address, backstopId, blndTokenId) {
  let emitterContract = new Contract(address);
  return emitterContract.call(
    "initialize",
    xdr.ScVal.scvBytes(Buffer.from(backstopId, "hex")),
    xdr.ScVal.scvBytes(Buffer.from(blndTokenId, "hex")),
  );
}
