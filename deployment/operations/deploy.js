import { randomBytes } from "crypto";
import { Asset, Operation, hash, xdr } from "soroban-client";
import { Config } from "../config.js";

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
  if (asset.isNative()) {
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
