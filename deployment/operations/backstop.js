import { Contract, xdr, Address } from "soroban-client";
import * as convert from "@soroban-react/utils";
import { Config } from "../config.js";
import BigNumber from "bignumber.js";

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

/**
 * @param {Config} config
 * @param {string} poolName
 * @param {string} from
 * @param {BigNumber} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createDeposit(config, poolName, from, amount) {
  let backstopContract = new Contract(config.getContractId("backstop"));
  return backstopContract.call(
    "deposit",
    new Address(from).toScVal(),
    xdr.ScVal.scvBytes(Buffer.from(config.getContractId(poolName), "hex")),
    convert.bigNumberToI128(amount)
  );
}

/**
 * @param {Config} config
 * @param {string} poolName
 * @param {string} from
 * @param {BigNumber} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createQueueWithdraw(config, poolName, from, amount) {
  let backstopContract = new Contract(config.getContractId("backstop"));
  return backstopContract.call(
    "q_withdraw",
    new Address(from).toScVal(),
    xdr.ScVal.scvBytes(Buffer.from(config.getContractId(poolName), "hex")),
    convert.bigNumberToI128(amount)
  );
}

/**
 * @param {Config} config
 * @param {string} poolName
 * @param {string} from
 * @param {BigNumber} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createDequeueWithdraw(config, poolName, from, amount) {
  let backstopContract = new Contract(config.getContractId("backstop"));
  return backstopContract.call(
    "dequeue_wd",
    new Address(from).toScVal(),
    xdr.ScVal.scvBytes(Buffer.from(config.getContractId(poolName), "hex")),
    convert.bigNumberToI128(amount)
  );
}

/**
 * @param {Config} config
 * @param {string} poolName
 * @param {string} from
 * @param {BigNumber} amount
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createWithdraw(config, poolName, from, amount) {
  let backstopContract = new Contract(config.getContractId("backstop"));
  return backstopContract.call(
    "withdraw",
    new Address(from).toScVal(),
    xdr.ScVal.scvBytes(Buffer.from(config.getContractId(poolName), "hex")),
    convert.bigNumberToI128(amount)
  );
}

/**
 * @param {Config} config
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createDistribute(config) {
  let backstopContract = new Contract(config.getContractId("backstop"));
  return backstopContract.call("dist");
}

/**
 * @param {Config} config
 * @param {string} hexIdToAdd
 * @param {string} hexIdToRemove
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createAddToRewardZone(config, hexIdToAdd, hexIdToRemove) {
  let backstopContract = new Contract(config.getContractId("backstop"));
  return backstopContract.call(
    "add_reward",
    xdr.ScVal.scvBytes(Buffer.from(hexIdToAdd, "hex")),
    xdr.ScVal.scvBytes(Buffer.from(hexIdToRemove, "hex"))
  );
}
