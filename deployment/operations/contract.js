import { randomBytes } from "crypto";
import { Asset, Operation, hash, xdr } from "soroban-client";
import { Config } from "../config.js";
import { readFileSync } from "fs";
import path from "path";
import { fileURLToPath } from "url";

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

/**
 *
 * @param {string} contractKey
 * @param {string} wasmKey
 * @param {Config} config
 * @param {Keypair} source
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createDeployOperation(contractKey, wasmKey, config, source) {
  let contractIdSalt = randomBytes(32);
  let networkId = hash(Buffer.from(config.network.passphrase));
  let preimage = xdr.HashIdPreimage.envelopeTypeContractIdFromSourceAccount(
    new xdr.HashIdPreimageSourceAccountContractId({
      networkId: networkId,
      sourceAccount: xdr.PublicKey.publicKeyTypeEd25519(source.rawPublicKey()),
      salt: contractIdSalt,
    })
  );
  let contractId = hash(preimage.toXDR());

  config.setContractId(contractKey, contractId.toString("hex"));
  let wasmHash = Buffer.from(config.getWasmHash(wasmKey), "hex");

  let deployFunction = xdr.HostFunction.hostFunctionTypeCreateContract(
    new xdr.CreateContractArgs({
      contractId: xdr.ContractId.contractIdFromSourceAccount(contractIdSalt),
      source: xdr.ScContractExecutable.sccontractExecutableWasmRef(wasmHash),
    })
  );
  let deployFootprint = new xdr.LedgerFootprint({
    readOnly: [
      xdr.LedgerKey.contractCode(
        new xdr.LedgerKeyContractCode({ hash: wasmHash })
      ),
    ],
    readWrite: [
      xdr.LedgerKey.contractData(
        new xdr.LedgerKeyContractData({
          contractId: contractId,
          key: xdr.ScVal.scvLedgerKeyContractExecutable(),
        })
      ),
    ],
  });

  return Operation.invokeHostFunction({
    function: deployFunction,
    parameters: [],
    footprint: deployFootprint,
    auth: [],
  });
}

/**
 * @param {Asset} asset
 * @param {Config} config
 * @returns {xdr.Operation<Operation.InvokeHostFunction>}
 */
export function createDeployStellarAssetOperation(asset, config) {
  let xdrAsset = asset.toXDRObject();
  let networkId = hash(Buffer.from(config.network.passphrase));
  let preimage = xdr.HashIdPreimage.envelopeTypeContractIdFromAsset(
    new xdr.HashIdPreimageFromAsset({
      networkId: networkId,
      asset: xdrAsset,
    })
  );
  let contractId = hash(preimage.toXDR());

  config.setContractId(asset.code, contractId.toString("hex"));

  let deployFunction = xdr.HostFunction.hostFunctionTypeCreateContract(
    new xdr.CreateContractArgs({
      contractId: xdr.ContractId.contractIdFromAsset(xdrAsset),
      source: xdr.ScContractExecutable.sccontractExecutableToken(),
    })
  );

  let readWrite = [
    xdr.LedgerKey.contractData(
      new xdr.LedgerKeyContractData({
        contractId: contractId,
        key: xdr.ScVal.scvLedgerKeyContractExecutable(),
      })
    ),
    xdr.LedgerKey.contractData(
      new xdr.LedgerKeyContractData({
        contractId: contractId,
        key: xdr.ScVal.scvVec([xdr.ScVal.scvSymbol("Metadata")]),
      })
    ),
  ];
  if (!asset.isNative()) {
    readWrite.push(
      xdr.LedgerKey.contractData(
        new xdr.LedgerKeyContractData({
          contractId: contractId,
          key: xdr.ScVal.scvVec([xdr.ScVal.scvSymbol("Admin")]),
        })
      )
    );
  }

  let deployFootprint = new xdr.LedgerFootprint({
    readOnly: [],
    readWrite: readWrite,
  });

  return Operation.invokeHostFunction({
    function: deployFunction,
    parameters: [],
    footprint: deployFootprint,
    auth: [],
  });
}
