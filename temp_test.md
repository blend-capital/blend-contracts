bombadil
GCI37A76ZJOKLNHZ5BS3JYCLPQPAG2HJSGIASIP2HC2QQI6JAGPDWBXQ
SDBR5CIOXZP26PN2BJSZ6ENB2I5BINUPMXBC6753SFT7IXZARPWY4ZEV

USDC
f1981624c01f74e5b2112d7aa5a48b0b1ce05e2b65325b7a98ad416407345c56

b_token
c48a36ac5d8d5e33430a80c130baf1b395d413ab6977411cf241edaccc9a74e3

d_token
3f9d0daffa62e2f1e2803a9d9d24f5c918dbc55ef53c314f5652998bb05eae2a

emitter

mock_oracle

token
4ecc081e9b200f16f9f976fdb6afb79dba2d21373d9391935255b70455ec033f

localnet
cdb9400fd16804d3af7ebe2993c02113b6b47663763079289b0d14c0c09e0e8b

futurenet
619f3541acc1776443e72156054ad85318845ed9a349d9a62cda5af95012df90



soroban contract invoke \
    --id f1981624c01f74e5b2112d7aa5a48b0b1ce05e2b65325b7a98ad416407345c56 \
    --source SDBR5CIOXZP26PN2BJSZ6ENB2I5BINUPMXBC6753SFT7IXZARPWY4ZEV \
    --rpc-url http://localhost:8000/soroban/rpc \
    --network-passphrase 'Standalone Network ; February 2017' \
    -- \
    initialize \
    --admin GCI37A76ZJOKLNHZ5BS3JYCLPQPAG2HJSGIASIP2HC2QQI6JAGPDWBXQ \
    --decimal 7 \
    --name "[55,53,44,53]" \
    --symbol "[55,53,44,53]"

soroban contract install \
    --source SDBR5CIOXZP26PN2BJSZ6ENB2I5BINUPMXBC6753SFT7IXZARPWY4ZEV \
    --rpc-url http://localhost:8000/soroban/rpc \
    --network-passphrase 'Standalone Network ; February 2017' \
    --wasm soroban_token_contract.wasm

soroban contract install \
    --source SDBR5CIOXZP26PN2BJSZ6ENB2I5BINUPMXBC6753SFT7IXZARPWY4ZEV \
    --rpc-url https://rpc-futurenet.stellar.org:443 \
    --network-passphrase 'Test SDF Future Network ; October 2022' \
    --wasm soroban_token_contract.wasm

soroban contract deploy \
    --source SDBR5CIOXZP26PN2BJSZ6ENB2I5BINUPMXBC6753SFT7IXZARPWY4ZEV \
    --rpc-url https://rpc-futurenet.stellar.org:443 \
    --network-passphrase 'Test SDF Future Network ; October 2022' \
    --wasm-hash 4ecc081e9b200f16f9f976fdb6afb79dba2d21373d9391935255b70455ec033f \
    --salt ffcc081e9b200f16f9f976fdb6afb79d