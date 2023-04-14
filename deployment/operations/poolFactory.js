import { Contract, xdr, Address } from "soroban-client";

/********** Operation Builders **********/

/**
 * @param {string} address
 * @param {Config} config
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInitialize(address, config) {
  // TODO: build PoolInitMeta from config
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "initialize",
    new Address(admin).toScVal(),
    xdr.ScVal.scvBytes(Buffer.from(backstopTokenId, "hex")),
    xdr.ScVal.scvBytes(Buffer.from(blndTokenId, "hex")),
    xdr.ScVal.scvBytes(Buffer.from(poolFactoryId, "hex"))
  );
}
