import { Contract, xdr, Address, hash } from "soroban-client";
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

/**
 * @param {Config} config
 * @param {string} poolAdmin
 * @param {Buffer} salt
 * @param {string} backstopRate
 * @param {string} name
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createDeployPool(config, poolAdmin, salt, backstopRate, name) {
  // determine deployed pool contractId
  let poolFactoryContractId = config.getContractId("poolFactory");
  let networkId = hash(Buffer.from(config.network.passphrase));
  let preimage = xdr.HashIdPreimage.envelopeTypeContractIdFromContract(
    new xdr.HashIdPreimageContractId({
      networkId: networkId,
      contractId: Buffer.from(poolFactoryContractId, "hex"),
      salt: salt,
    })
  );
  let contractId = hash(preimage.toXDR());
  config.setContractId(name, contractId.toString("hex"));

  let contract = new Contract(poolFactoryContractId);
  return contract.call(
    "deploy",
    new Address(poolAdmin).toScVal(),
    xdr.ScVal.scvBytes(salt),
    xdr.ScVal.scvBytes(Buffer.from(config.getContractId("oracle"), "hex")),
    xdr.ScVal.scvU64(xdr.Uint64.fromString(backstopRate))
  );
}
