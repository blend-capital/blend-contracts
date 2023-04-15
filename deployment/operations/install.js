import { readFileSync } from "fs";
import path from "path";
import { fileURLToPath } from "url";
import { Operation, hash, xdr } from "soroban-client";
import { Config } from "../config.js";

// Relative paths from __dirname
const CONTRACT_REL_PATH = {
  token: "/../../soroban_token_contract.wasm",
  bToken: "/../../target/wasm32-unknown-unknown/optimized/b_token.wasm",
  dToken: "/../../target/wasm32-unknown-unknown/optimized/d_token.wasm",
  oracle: "/../../target/wasm32-unknown-unknown/release/mock_blend_oracle.wasm",
  emitter: "/../../target/wasm32-unknown-unknown/release/emitter.wasm",
  poolFactory:
    "/../../target/wasm32-unknown-unknown/optimized/pool_factory.wasm",
  backstop:
    "/../../target/wasm32-unknown-unknown/optimized/backstop_module.wasm",
  lendingPool:
    "/../../target/wasm32-unknown-unknown/optimized/lending_pool.wasm",
};

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * @param {string} wasmKey
 * @param {Config} config
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createInstallOperation(wasmKey, config) {
  let contractWasm = readFileSync(
    path.join(__dirname, CONTRACT_REL_PATH[wasmKey])
  );

  let installContractArgs = new xdr.InstallContractCodeArgs({
    code: contractWasm,
  });
  let wasmHash = hash(installContractArgs.toXDR());

  config.setWasmHash(wasmKey, wasmHash.toString("hex"));

  return Operation.invokeHostFunction({
    function:
      xdr.HostFunction.hostFunctionTypeInstallContractCode(installContractArgs),
    parameters: [],
    footprint: new xdr.LedgerFootprint({
      readOnly: [],
      readWrite: [
        xdr.LedgerKey.contractCode(
          new xdr.LedgerKeyContractCode({ hash: wasmHash })
        ),
      ],
    }),
    auth: [],
  });
}
