// Create a Blend Lending Pool for USDC and XLM that can be interacted with
import { Server } from "soroban-client";
import { Config } from "./config.js";
import {
  deployAndSetupPool,
  distribute,
  setupPoolBackstop,
} from "./scripts/pool.js";
import { transferBLNDToEmitter } from "./scripts/deploy.js";

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

console.log("Completed mock data creation script!");
