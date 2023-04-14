import { Contract, xdr, Address } from "soroban-client";

/********** Operation Builders **********/

/**
 * @param {string} address
 * @param {string} backstopTokenId
 * @param {string} blndTokenId
 * @param {string} poolFactoryId
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInitialize(
  address,
  backstopTokenId,
  blndTokenId,
  poolFactoryId
) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "initialize",
    new Address(admin).toScVal(),
    xdr.ScVal.scvBytes(Buffer.from(backstopTokenId, "hex")),
    xdr.ScVal.scvBytes(Buffer.from(blndTokenId, "hex")),
    xdr.ScVal.scvBytes(Buffer.from(poolFactoryId, "hex"))
  );
}
