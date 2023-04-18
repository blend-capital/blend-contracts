import { readFileSync, writeFileSync } from "fs";
import path from "path";
import { fileURLToPath } from "url";
import { Keypair } from "soroban-client";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export class Config {
  constructor(network, users, addresses, wasmHashes) {
    this.network = network;
    this.users = users;
    this.addresses = addresses;
    this.wasmHashes = wasmHashes;
  }

  /**
   * @returns {Config}
   */
  static loadFromFile() {
    let configFile = readFileSync(path.join(__dirname, "/local.config.json"));
    let configObj = JSON.parse(configFile.toString());
    return new Config(
      configObj.network,
      configObj.users,
      configObj.addresses,
      configObj.wasmHashes
    );
  }

  writeToFile() {
    let newFile = JSON.stringify(this, null, 2);
    writeFileSync(path.join(__dirname, "/local.config.json"), newFile);
  }

  /**
   * @param {string} userKey
   * @returns {Keypair}
   */
  getAddress(userKey) {
    let userSecret = this.users[userKey];

    if (userSecret != undefined) {
      return Keypair.fromSecret(userSecret);
    } else {
      console.error("unable to find user in config: ", userKey);
      throw Error();
    }
  }

  /**
   * @param {string} userKey
   * @param {Keypair} keypair
   */
  setAddress(userKey, keypair) {
    this.users[userKey] = keypair.secret();
  }

  /**
   * @param {string} contractKey
   * @returns {string} - Hex encoded contractId
   */
  getContractId(contractKey) {
    let contractId = this.addresses[contractKey];

    if (contractId != undefined) {
      return contractId;
    } else {
      console.error("unable to find address in config: ", contractKey);
      throw Error();
    }
  }

  /**
   * @param {string} contractKey
   * @param {string} contractId - Hex encoded contractId
   */
  setContractId(contractKey, contractId) {
    this.addresses[contractKey] = contractId;
  }

  /**
   * @param {string} contractKey
   * @returns {string} -
   */
  getWasmHash(contractKey) {
    let washHash = this.wasmHashes[contractKey];

    if (washHash != undefined) {
      return washHash;
    } else {
      console.error("unable to find hash in config: ", contractKey);
      throw Error();
    }
  }

  /**
   * @param {string} contractKey
   * @param {string} wasmHash - Hex encoded wasmHash
   */
  setWasmHash(contractKey, wasmHash) {
    this.wasmHashes[contractKey] = wasmHash;
  }
}
