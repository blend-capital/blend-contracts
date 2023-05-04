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
let bombadil = config.getAddress("bombadil");

//***** Env Setup *****//
await airdropAccounts(stellarRpc, config);

//***** Install WASM *****//
await installWasm(stellarRpc, config);

//***** Tokens *****//
await deployAndInitToken(stellarRpc, config, "WBTC");
await deployAndInitToken(stellarRpc, config, "BLND");
await deployAndInitToken(stellarRpc, config, "WETH");
await deployAndInitToken(stellarRpc, config, "BLNDUSDC");
// NOTE: Must deploy Stellar Assets manually via Stellar Lab -> USDC:BOMBADIL
await deployAndInitStellarToken(
  stellarRpc,
  config,
  new Asset("USDC", bombadil.publicKey())
);
await deployAndInitStellarToken(stellarRpc, config, Asset.native());

//***** External Contracts *****//
await deployAndInitExternalContracts(stellarRpc, config);

//***** Blend Contracts *****//
await deployBlendContracts(stellarRpc, config);
await initBlendContracts(stellarRpc, config);

console.log("environment setup script complete!");
