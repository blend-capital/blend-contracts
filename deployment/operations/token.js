import { Contract, Server, xdr, Address } from "soroban-client";
import { bigintToI128, scvalToBigInt } from "../utils.js";

/********** Operation Builders **********/

/**
 * @param {string} address
 * @param {string} admin
 * @param {string} symbol
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInitialize(address, admin, symbol) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "initialize",
    new Address(admin).toScVal(),
    xdr.ScVal.scvU32(7),
    xdr.ScVal.scvBytes(Buffer.from(symbol + " Token")),
    xdr.ScVal.scvBytes(Buffer.from(symbol))
  );
}

/**
 * @param {string} address
 * @param {string} oldAdmin
 * @param {string} newAdmin
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createSetAdminToContract(address, oldAdmin, newAdmin) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "set_admin",
    new Address(oldAdmin).toScVal(),
    Address.contract(Buffer.from(newAdmin, "hex")).toScVal()
  );
}

/**
 * @param {string} address
 * @param {string} admin
 * @param {string} to
 * @param {BigInt} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createMint(address, admin, to, amount) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "mint",
    new Address(admin).toScVal(),
    new Address(to).toScVal(),
    bigintToI128(amount)
  );
}

/**
 * @param {string} address
 * @param {string} admin
 * @param {string} from
 * @param {BigInt} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createBurn(address, admin, from, amount) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "clawback",
    new Address(admin.publicKey()).toScVal(),
    new Address(from).toScVal(),
    bigintToI128(amount)
  );
}

/**
 * @param {string} address
 * @param {string} from
 * @param {string} spender
 * @param {BigInt} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createIncrAllow(address, from, spender, amount) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "incr_allow",
    new Address(from).toScVal(),
    new Address(spender).toScVal(),
    bigintToI128(amount)
  );
}

/**
 * @param {string} address
 * @param {string} from
 * @param {string} spender
 * @param {BigInt} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createDecrAllow(address, from, spender, amount) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "decr_allow",
    new Address(from).toScVal(),
    new Address(spender).toScVal(),
    bigintToI128(amount)
  );
}

/**
 * @param {string} address
 * @param {string} from
 * @param {string} to
 * @param {BigInt} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createTransfer(address, from, to, amount) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "xfer",
    new Address(from).toScVal(),
    new Address(to).toScVal(),
    bigintToI128(amount)
  );
}

/********** Data Fetchers **********/

/**
 * @param {Server} stellarRpc
 * @param {string} address
 * @param {xdr.ScVal} from
 * @returns {Promise<BigInt>}
 */
export async function getBalance(stellarRpc, address, from) {
  try {
    let contract_key = xdr.ScVal.scvVec([xdr.ScVal.scvSymbol("Balance"), from]);
    let scValResp = await stellarRpc.getContractData(address, contract_key);
    let entryData = xdr.LedgerEntryData.fromXDR(scValResp.xdr, "base64")
      .contractData()
      .val();
    return scvalToBigInt(entryData);
  } catch (e) {
    if (e.message.includes("not found")) {
      return new BigInt(0);
    }
    console.error(e);
    throw e;
  }
}
