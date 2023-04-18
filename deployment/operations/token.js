import { Contract, Server, xdr, Address } from "soroban-client";
import BigNumber from "bignumber.js";
import * as convert from "@soroban-react/utils";
import { Config } from "../config.js";

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
 * @param {BigNumber} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createMint(address, admin, to, amount) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "mint",
    new Address(admin).toScVal(),
    new Address(to).toScVal(),
    convert.bigNumberToI128(amount)
  );
}

/**
 * @param {string} address
 * @param {string} admin
 * @param {string} from
 * @param {BigNumber} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createBurn(address, admin, from, amount) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "clawback",
    new Address(admin.publicKey()).toScVal(),
    new Address(from).toScVal(),
    convert.bigNumberToI128(amount)
  );
}

/**
 * @param {string} address
 * @param {string} from
 * @param {string} spender
 * @param {BigNumber} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createIncrAllow(address, from, spender, amount) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "incr_allow",
    new Address(from).toScVal(),
    new Address(spender).toScVal(),
    convert.bigNumberToI128(amount)
  );
}

/**
 * @param {string} address
 * @param {string} from
 * @param {string} spender
 * @param {BigNumber} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createDecrAllow(address, from, spender, amount) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "decr_allow",
    new Address(from).toScVal(),
    new Address(spender).toScVal(),
    convert.bigNumberToI128(amount)
  );
}

/**
 * @param {string} address
 * @param {string} from
 * @param {string} to
 * @param {BigNumber} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createTransfer(address, from, to, amount) {
  let tokenContract = new Contract(address);
  return tokenContract.call(
    "xfer",
    new Address(from).toScVal(),
    new Address(to).toScVal(),
    convert.bigNumberToI128(amount)
  );
}

/********** Data Fetchers **********/

/**
 * @param {Server} stellarRpc
 * @param {string} address
 * @param {xdr.ScVal} from
 * @returns {Promise<BigNumber>}
 */
export async function getBalance(stellarRpc, address, from) {
  try {
    let contract_key = xdr.ScVal.scvVec([xdr.ScVal.scvSymbol("Balance"), from]);
    let scValResp = await stellarRpc.getContractData(address, contract_key);
    let entryData = xdr.LedgerEntryData.fromXDR(
      scValResp.xdr,
      "base64"
    ).contractData();
    return convert.scvalToBigNumber(entryData.val());
  } catch (e) {
    if (e.message.includes("not found")) {
      return new BigNumber(0);
    }
    console.error(e);
    throw e;
  }
}
