// Create a Blend Lending Pool for USDC and XLM that can be interacted with
import { Server } from "soroban-client";
import { Config } from "./config.js";
import {
  addWhale,
  deployAndSetupPool,
  distribute,
  mintWhale,
  setupPoolBackstop,
} from "./scripts/pool.js";
import { transferBLNDToEmitter } from "./scripts/deploy.js";
import { setAssetPrices } from "./scripts/oracle.js";
import * as pool from "./operations/pool.js";

console.log("starting mock data creation script...");

let config = Config.loadFromFile();
let stellarRpc = new Server(config.network.rpc, {
  allowHttp: true,
});

await transferBLNDToEmitter(stellarRpc, config);

// Deploy Starbridge pool
let poolName = "Starbridge";
let reserveMetaXLM = pool.createDefaultReserveMetadata();
reserveMetaXLM.c_factor = 900_0000;
reserveMetaXLM.l_factor = 850_0000;
reserveMetaXLM.util = 600_0000;
reserveMetaXLM.r_one = 30_0000;
reserveMetaXLM.r_two = 200_0000;
reserveMetaXLM.r_three = 1_000_0000;

let reserveMetaWETH = pool.createDefaultReserveMetadata();
reserveMetaWETH.util = 650_0000;

let reserveMetaWBTC = pool.createDefaultReserveMetadata();
reserveMetaWBTC.c_factor = 900_0000;
reserveMetaWBTC.l_factor = 900_0000;
let assets = ["XLM", "WETH", "WBTC"];
let metadata = [reserveMetaXLM, reserveMetaWETH, reserveMetaWBTC];
let emissionsMetadata = [
  {
    res_index: 1, // WETH
    res_type: 0, // d_token
    share: 0.5e7, // 50%
  },
  {
    res_index: 2, // WBTC
    res_type: 0, // d_token
    share: 0.5e7, // 50%
  },
];
await deployAndSetupPool(
  stellarRpc,
  config,
  poolName,
  assets,
  metadata,
  emissionsMetadata
);
await setupPoolBackstop(stellarRpc, config, poolName);

// Deploy Stellar pool
let poolName2 = "Stellar";
let reserveMetaXLM2 = pool.createDefaultReserveMetadata();
reserveMetaXLM2.c_factor = 800_0000;
reserveMetaXLM2.l_factor = 850_0000;
reserveMetaXLM2.util = 700_0000;

let reserveMetaUSDC2 = pool.createDefaultReserveMetadata();
reserveMetaUSDC2.c_factor = 975_0000;
reserveMetaUSDC2.l_factor = 975_0000;
reserveMetaUSDC2.util = 850_0000;
reserveMetaUSDC2.r_one = 30_0000;
reserveMetaUSDC2.r_two = 200_0000;
reserveMetaUSDC2.r_three = 1_000_0000;

let assets2 = ["XLM", "USDC"];
let metadata2 = [reserveMetaXLM2, reserveMetaUSDC2];
let emissionsMetadata2 = [
  {
    res_index: 0, // XLM
    res_type: 0, // d_token
    share: 0.7e7, // 70%
  },
  {
    res_index: 1, // USDC
    res_type: 1, // b_token
    share: 0.3e7, // 30%
  },
];
await deployAndSetupPool(
  stellarRpc,
  config,
  poolName2,
  assets2,
  metadata2,
  emissionsMetadata2
);

await setupPoolBackstop(stellarRpc, config, poolName2);

await distribute(stellarRpc, config, [poolName, poolName2]);

await setAssetPrices(stellarRpc, config, [
  { price: BigInt(1e7), assetKey: "USDC" },
  { price: BigInt(30_000e7), assetKey: "WBTC" },
  { price: BigInt(0.1e7), assetKey: "XLM" },
  { price: BigInt(2000e7), assetKey: "WETH" },
  { price: BigInt(0.5e7), assetKey: "BLNDUSDC" },
]);

let mintAmounts = [
  { key: "WBTC", amount: BigInt(10e7) },
  { key: "WETH", amount: BigInt(50e7) },
  { key: "USDC", amount: BigInt(200_000e7) },
];
await mintWhale(stellarRpc, config, mintAmounts);

// Add whale to Starbridge pool
let supplyAmounts = [
  { key: "WBTC", amount: BigInt(1e7) },
  { key: "WETH", amount: BigInt(10e7) },
  { key: "XLM", amount: BigInt(4_000e7) },
];
let borrowAmounts = [
  { key: "WBTC", amount: BigInt(0.6e7) },
  { key: "WETH", amount: BigInt(6.5e7) },
  { key: "XLM", amount: BigInt(3_000e7) },
];
await addWhale(stellarRpc, config, poolName, supplyAmounts, borrowAmounts);

// Add whale to Stellar pool
let supplyAmounts2 = [
  { key: "USDC", amount: BigInt(10_000e7) },
  { key: "XLM", amount: BigInt(4_000e7) },
];
let borrowAmounts2 = [
  { key: "USDC", amount: BigInt(9_000e7) },
  { key: "XLM", amount: BigInt(2_000e7) },
];
await addWhale(stellarRpc, config, poolName2, supplyAmounts2, borrowAmounts2);

console.log("Completed mock data creation script!");
