import { Contract, xdr, Address, hash } from "soroban-client";
import { Config } from "../config.js";

/********** Object Builders **********/

/**
 * @typedef ReserveMetadata
 * @property {number} c_factor - 7 decimals
 * @property {number} decimals - 0 decimals
 * @property {number} l_factor - 7 decimals
 * @property {number} max_util - 7 decimals
 * @property {number} r_one - 7 decimals
 * @property {number} r_three - 7 decimals
 * @property {number} r_two - 7 decimals
 * @property {number} reactivity - 9 decimals
 * @property {number} util - 7 decimals
 *
 * @returns {ReserveMetadata}
 */
export function createDefaultReserveMetadata() {
  return {
    c_factor: 7500000,
    decimals: 7,
    l_factor: 7500000,
    max_util: 9500000,
    r_one: 500000,
    r_three: 15000000,
    r_two: 5000000,
    reactivity: 10000,
    util: 7500000,
  };
}

/********** Operation Builders **********/

/**
 * @param {string} poolKey
 * @param {Config} config
 * @param {string} poolAdmin
 * @param {string} assetKey
 * @param {ReserveMetadata} reserveMetadata
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInitReserve(
  poolKey,
  config,
  poolAdmin,
  assetKey,
  reserveMetadata
) {
  // determine deployed b and d token contractId
  let networkId = hash(Buffer.from(config.network.passphrase));
  let poolContractId = config.getContractId(poolKey);
  let assetContractId = config.getContractId(assetKey);
  let bTokenSalt = Buffer.from("0" + assetContractId.slice(1), "hex");
  let bTokenPreimage = xdr.HashIdPreimage.envelopeTypeContractIdFromContract(
    new xdr.HashIdPreimageContractId({
      networkId: networkId,
      contractId: Buffer.from(poolContractId, "hex"),
      salt: bTokenSalt,
    })
  );
  let bTokenId = hash(bTokenPreimage.toXDR());
  config.setContractId("bToken" + poolKey, bTokenId.toString("hex"));
  let dTokenSalt = Buffer.from("1" + assetContractId.slice(1), "hex");
  let dTokenPreimage = xdr.HashIdPreimage.envelopeTypeContractIdFromContract(
    new xdr.HashIdPreimageContractId({
      networkId: networkId,
      contractId: Buffer.from(poolContractId, "hex"),
      salt: dTokenSalt,
    })
  );
  let dTokenId = hash(dTokenPreimage.toXDR());
  config.setContractId("dToken" + poolKey, dTokenId.toString("hex"));

  // build reserveMetadata ScVal
  let reserveMetadataMap = [];
  for (const key of Object.keys(reserveMetadata).sort()) {
    reserveMetadataMap.push(
      new xdr.ScMapEntry({
        key: xdr.ScVal.scvSymbol(key),
        val: xdr.ScVal.scvU32(reserveMetadata[key]),
      })
    );
  }

  let contract = new Contract(poolContractId);
  return contract.call(
    "init_res",
    new Address(poolAdmin).toScVal(),
    xdr.ScVal.scvBytes(Buffer.from(assetContractId, "hex")),
    xdr.ScVal.scvMap(reserveMetadataMap)
  );
}
