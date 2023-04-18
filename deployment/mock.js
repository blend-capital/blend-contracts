// Create a Blend Lending Pool for USDC and XLM that can be interacted with
import { Server } from "soroban-client";
import { Config } from "./config.js";
import {
  addWhale,
  deployAndSetupPool,
  distribute,
  setupPoolBackstop,
} from "./scripts/pool.js";
import { transferBLNDToEmitter } from "./scripts/deploy.js";
import { setAssetPrices } from "./scripts/oracle.js";
import BigNumber from "bignumber.js";

console.log("starting mock data creation script...");

let config = Config.loadFromFile();
let stellarRpc = new Server(config.network.rpc, {
  allowHttp: true,
});
let poolName = "mockPool";

await transferBLNDToEmitter(stellarRpc, config);

await deployAndSetupPool(stellarRpc, config, poolName);

await setupPoolBackstop(stellarRpc, config, poolName);

await distribute(stellarRpc, config, poolName);

await setAssetPrices(stellarRpc, config, [
  { price: new BigNumber(1e7), assetKey: "USDC" },
  { price: new BigNumber(0.09e7), assetKey: "XLM" },
]);

await addWhale(stellarRpc, config, poolName);

console.log("Completed mock data creation script!");
