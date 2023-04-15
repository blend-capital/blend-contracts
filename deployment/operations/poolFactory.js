import { Contract, xdr, Address } from "soroban-client";

/********** Operation Builders **********/

/**
 * @param {string} address
 * @param {Config} config
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInitialize(address, config) {
  // TODO: build PoolInitMeta from config
  let poolInitMeta = []
  for (const key of Object.keys(config).sort()) {
    poolInitMeta.push(new xdr.ScMapEntry(
      {
        key: xdr.ScVal.scvSymbol(key), val: xdr.ScVal.scvBytes(Buffer.from(config[key], "hex"))
      }
    ));
  };
  let poolFactoryContract = new Contract(address);
  return poolFactoryContract.call(
    "initialize",
    xdr.ScVal.scvMap(poolInitMeta),
  );

}
