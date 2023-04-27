// Invoke a test script to interact with Soroban

import { Address, Server, xdr } from "soroban-client";
import { Config } from "./config.js";
import * as token from "./operations/token.js";

let config = Config.loadFromFile();
let stellarRpc = new Server(config.network.rpc, {
  allowHttp: true,
});
let poolName = "mockPool";

let xlm_id = config.getContractId("XLM");
let usdc_id = config.getContractId("USDC");
let weth_id = config.getContractId("WETH");
let pool_id = config.getContractId(poolName);

// console.log(
//   "pool XLM: ",
//   await token.getBalance(stellarRpc, xlm_id, scval_address)
// );
console.log(
  "pool USDC: ",
  await token.getBalance(stellarRpc, usdc_id, scval_address)
);
console.log(
  "pool WETH: ",
  await token.getBalance(stellarRpc, weth_id, scval_address)
);
