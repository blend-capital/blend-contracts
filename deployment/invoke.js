// Invoke a test script to interact with Soroban

import {
  Address,
  Server,
  xdr,
  Contract,
  TransactionBuilder,
  Account,
} from "soroban-client";
import { Config } from "./config.js";
import { createTxBuilder, signPrepareAndSubmitTransaction } from "./utils.js";
import * as token from "./operations/token.js";
import { setAssetPrices } from "./scripts/oracle.js";
import { airdropAccounts } from "./scripts/deploy.js";

let config = Config.loadFromFile();
let network = config.network.passphrase;
let stellarRpc = new Server(config.network.rpc, {
  allowHttp: true,
});
let poolName = "mockPool";

let xlm_id = config.getContractId("XLM");
let usdc_id = config.getContractId("USDC");
let weth_id = config.getContractId("WETH");
let oracle_id = config.getContractId("oracle");
let pool_id = config.getContractId(poolName);

let frodo_id = config.getAddress("frodo");
let bombadil_id = config.getAddress("bombadil");
let samwise_id = config.getAddress("samwise");
console.log("frodo: ", frodo_id.publicKey());
console.log("bombadil_id: ", bombadil_id.publicKey());
console.log("samwise_id: ", samwise_id.publicKey());

console.log(frodo_id.publicKey());
let address_frodo = new Address(frodo_id.publicKey());
let address_pool = Address.contract(Buffer.from(pool_id, "hex"));
let scval_frodo = address_frodo.toScVal();
let scval_pool = address_pool.toScVal();
let backstopToken = config.getContractId("BLNDUSDC");

console.log("START: Mint frodo required tokens...");
let txBuilder = await createTxBuilder(stellarRpc, network, bombadil_id);
txBuilder.addOperation(
  token.createMint(
    backstopToken,
    bombadil_id.publicKey(),
    samwise_id.publicKey(),
    BigInt(567_000e7)
  )
);
await signPrepareAndSubmitTransaction(
  stellarRpc,
  network,
  txBuilder.build(),
  bombadil_id
);
console.log("minted backstop tokens...");

//***** Env Setup *****//
//await airdropAccounts(stellarRpc, config);

// await setAssetPrices(stellarRpc, config, [
//   { price: BigInt(1e7), assetKey: "USDC" },
//   { price: BigInt(0.1e7), assetKey: "XLM" },
//   { price: BigInt(2000e7), assetKey: "WETH" },
//   { price: BigInt(0.5e7), assetKey: "BLNDUSDC" },
// ]);

// let data_key = xdr.ScVal.scvVec([xdr.ScVal.scvSymbol("Balance"), scval_frodo]);

// let data_key = xdr.ScVal.scvVec([
//   xdr.ScVal.scvSymbol("Prices"),
//   xdr.ScVal.scvBytes(Buffer.from(weth_id, "hex")),
// ]);
// let res_list_entry = await stellarRpc.getContractData(oracle_id, data_key);
// console.log("xdr: ", res_list_entry.xdr);

// let tx_response = await stellarRpc.getTransaction(
//   "0000000000000000000000000000000000000000000000000000000000000000"
// );
// let latest_ledger = tx_response.latestLedger;
// console.log(JSON.stringify(tx_response));
// console.log(latest_ledger);

// let account = new Account(
//   "GANXGJV2RNOFMOSQ2DTI3RKDBAVERXUVFC27KW3RLVQCLB3RYNO3AAI4",
//   "123"
// ); // await stellarRpc.getAccount(frodo_id.publicKey());

// let tx_builder = new TransactionBuilder(account, {
//   fee: "1000",
//   timebounds: { minTime: 0, maxTime: 0 },
//   networkPassphrase: network,
// });
// tx_builder.addOperation(
//   new Contract(xlm_id).call("balance", address_frodo.toScVal())
// );
// let result = await stellarRpc.simulateTransaction(tx_builder.build());
// console.log(result?.results?.at(0)?.xdr);

// console.log(
//   "pool XLM: ",
//   await token.getBalance(stellarRpc, xlm_id, scval_frodo)
// );
// console.log(
//   "pool USDC: ",
//   await token.getBalance(stellarRpc, usdc_id, scval_frodo)
// );
// console.log(
//   "pool WETH: ",
//   await token.getBalance(stellarRpc, weth_id, scval_frodo)
// );
