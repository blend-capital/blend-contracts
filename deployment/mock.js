// Create a Blend Lending Pool for USDC and XLM that can be interacted with
import { Server } from "soroban-client";
import { Config } from "./config.js";
import { deployAndSetupPool } from "./scripts/pool.js";

console.log("starting mock data creation script...");

let config = Config.loadFromFile();
let stellarRpc = new Server(config.network.rpc, {
  allowHttp: true,
});

await deployAndSetupPool(stellarRpc, config, "mockPool");

console.log("Completed mock data creation script!");
