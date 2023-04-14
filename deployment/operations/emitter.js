import { Contract, xdr, Address } from "soroban-client";

/********** Operation Builders **********/

/**
 * @param {string} address
 * @param {string} backstopId
 * @param {string} blndTokenId
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInitialize(address, backstopId, blndTokenId) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "initialize",
    new Address(admin).toScVal(),
    xdr.ScVal.scvBytes(Buffer.from(backstopId, "hex")),
    xdr.ScVal.scvBytes(Buffer.from(blndTokenId, "hex"))
  );
}
