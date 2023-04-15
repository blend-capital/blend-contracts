// Invoke a test script to interact with Soroban

import { Asset, Networks, hash, xdr } from "soroban-client";

let xdrAsset = Asset.native().toXDRObject();
let networkId = hash(Buffer.from(Networks.FUTURENET));
let preimage = xdr.HashIdPreimage.envelopeTypeContractIdFromAsset(
  new xdr.HashIdPreimageFromAsset({
    networkId: networkId,
    asset: xdrAsset,
  })
);
let contractId = hash(preimage.toXDR());

console.log("XLM: ", contractId.toString("hex"));
