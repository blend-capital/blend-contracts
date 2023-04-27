import { Asset, Server } from "soroban-client";
import { Config } from "./config.js";
import {
  airdropAccounts,
  deployAndInitExternalContracts,
  deployAndInitStellarToken,
  deployAndInitToken,
  deployBlendContracts,
  initBlendContracts,
  installWasm,
} from "./scripts/deploy.js";

console.log("starting environment setup script...");

let config = Config.loadFromFile();
let stellarRpc = new Server(config.network.rpc, {
  allowHttp: true,
});

//***** Env Setup *****//
await airdropAccounts(stellarRpc, config);

//***** Install WASM *****//
await installWasm(stellarRpc, config);

//***** Tokens *****//
await deployAndInitToken(stellarRpc, config, "USDC");
await deployAndInitToken(stellarRpc, config, "BLND");
await deployAndInitToken(stellarRpc, config, "WETH");
await deployAndInitToken(stellarRpc, config, "BLNDUSDC");
await deployAndInitStellarToken(stellarRpc, config, Asset.native());

//***** External Contracts *****//
await deployAndInitExternalContracts(stellarRpc, config);

//***** Blend Contracts *****//
await deployBlendContracts(stellarRpc, config);
await initBlendContracts(stellarRpc, config);

console.log("environment setup script complete!");
