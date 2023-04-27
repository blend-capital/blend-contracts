import { Transaction, Server, TransactionBuilder, xdr } from "soroban-client";

export const WasmKeys = {
  token: "token",
  bToken: "bToken",
  dToken: "dToken",
  oracle: "oracle",
  emitter: "emitter",
  poolFactory: "poolFactory",
  backstop: "backstop",
  lendingPool: "lendingPool",
};

/**
 * @param {Server} stellarRpc
 * @param {string} network
 * @param {Transaction} tx
 * @param {Keypair} source
 */
export async function signPrepareAndSubmitTransaction(
  stellarRpc,
  network,
  tx,
  source
) {
  let prepped_tx = await stellarRpc.prepareTransaction(tx, network);
  await signAndSubmitTransaction(stellarRpc, prepped_tx, source);
}

/**
 * @param {Server} stellarRpc
 * @param {Transaction} tx
 * @param {Keypair} source
 */
export async function signAndSubmitTransaction(stellarRpc, tx, source) {
  try {
    tx.sign(source);
    console.log("submitting tx...");
    let response = await stellarRpc.sendTransaction(tx);
    let status = response.status;
    let tx_hash = response.hash;
    console.log(JSON.stringify(response));
    // Poll this until the status is not "pending"
    while (status === "PENDING" || status == "NOT_FOUND") {
      // See if the transaction is complete
      await new Promise((resolve) => setTimeout(resolve, 2000));
      console.log("checking tx...");
      response = await stellarRpc.getTransaction(tx_hash);
      status = response.status;
    }
    console.log("Transaction status:", response.status);
    console.log("Hash: ", tx_hash);
  } catch (e) {
    console.error(e);
    throw e;
  }
}

/**
 * @param {Server} stellarRpc
 * @param {string} network
 * @param {Keypair} source
 * @returns {Promise<TransactionBuilder>}
 */
export async function createTxBuilder(stellarRpc, network, source) {
  try {
    let account = await stellarRpc.getAccount(source.publicKey());

    return new TransactionBuilder(account, {
      fee: "1000",
      timebounds: { minTime: 0, maxTime: 0 },
      networkPassphrase: network,
    });
  } catch (e) {
    console.error(e);
    throw e;
  }
}

/**
 * @param {BigInt} value
 * @returns {xdr.ScVal}
 */
export function bigintToI128(value) {
  let hex = value.toString(16).replace(/^-/, "");
  if (hex.length > 32) {
    throw new Error("value overflow i128");
  }

  const buf = Buffer.alloc(16);
  if (hex.length % 2 !== 0) {
    hex = "0" + hex;
  }
  buf.write(hex, 16 - hex.length / 2, "hex"); // BE

  // perform two's compliment if negative and i128:MIN is not passed
  if (value < 0) {
    // throw if MSB bit is 1 and is not i128:MIN
    if ((buf[0] & 0x80) != 0 && hex != "80000000000000000000000000000000") {
      throw new Error("value underflow i128");
    }
    twosComplimentInPlace(buf, 16);
  } else {
    if ((buf[0] & 0x80) != 0) {
      throw new Error("value overflow i128");
    }
  }

  // store binary in xdr i128 parts
  const lo = new xdr.Uint64(
    buf.subarray(12, 16).readUint32BE(),
    buf.subarray(8, 12).readUint32BE()
  );
  const hi = new xdr.Uint64(
    buf.subarray(4, 8).readUint32BE(),
    buf.subarray(0, 4).readUint32BE()
  );

  return xdr.ScVal.scvI128(new xdr.Int128Parts({ lo, hi }));
}

/**
 *
 * @param {xdr.ScVal} scval
 * @returns {bigint}
 */
export function scvalToBigInt(scval) {
  switch (scval.switch()) {
    case xdr.ScValType.scvI128(): {
      const parts = scval.i128();
      const u64_lo = parts.lo();
      const u64_high = parts.hi();

      // build BE buffer
      const buf = Buffer.alloc(16);
      buf.writeInt32BE(u64_lo.low, 12);
      buf.writeInt32BE(u64_lo.high, 8);
      buf.writeInt32BE(u64_high.low, 4);
      buf.writeInt32BE(u64_high.high, 0);

      // perform two's compliment if necessary
      if ((buf[0] & 0x80) != 0) {
        twosComplimentInPlace(buf, 16);
        return BigInt("0x" + buf.toString("hex")) * BigInt(-1);
      } else {
        return BigInt("0x" + buf.toString("hex"));
      }
    }
    default: {
      throw new Error(
        `Invalid type for scvalToBigInt: ${scval?.switch().name}`
      );
    }
  }
}

/**
 * Perform BE two's compliment on the input buffer by reference
 */
function twosComplimentInPlace(buf, bytes) {
  // iterate from LSByte first to carry the +1 if necessary
  let i = bytes - 1;
  let add_one = true;
  while (i >= 0) {
    let inverse = ~buf[i];
    if (add_one) {
      if (inverse == -1) {
        // addition will overflow
        inverse = 0;
      } else {
        inverse += 1;
        add_one = false;
      }
    }
    buf[i] = inverse;
    i -= 1;
  }
}
