# zkLink primitive oracle and reuse

The checked-in signatures were extracted before Rust implementation from unchanged
`zklink-web-sdk-alpha@1.0.2` WASM. `npm-wasm.cjs` pins both artifact hashes before
execution; its output never depends on Rust. No provider request is made: `{}` is
only needed to construct the SDK wrapper, and `initZklinkSigner` receives a fixed
synthetic 65-byte signature seed. `createSignedContract` performs local signing.

Four vectors differ independently in seed, pair byte, or transaction nonce. These
are contract-shaped oracle messages, not Rust venue adapters. Their zero-size,
zero-price 36-byte messages follow official `Contract::get_bytes`; generic Rust
`sign` accepts serialized bytes and knows nothing about contracts. Expected keys
and signatures come solely from the npm WASM. Nonce means transaction payload
nonce; MuSig's cryptographic nonce remains deterministic and is not caller supplied.

Algorithm source: zkLinkProtocol/zklink_sdk commit
`3152525e2615ecbbdb56fd6728ce265ad2dde485`, `signers/src/zklink_signer` and
`types/src/tx_type/contract/contract_matching.rs` (Apache-2.0, notice alongside).
The published npm artifact declares MIT; exact source-to-published-binary revision
is not established. It is an independent pinned oracle, not claimed to be built
from that source commit. Crypto dependency `franklin-crypto` uses revision
`6ba7708bd0db5142fc55800addcf211d30e65cfa` (MIT/Apache-2.0), `default-features=false`
plus `plonk`. Disabling plonk fails because upstream unconditionally imports it.
Its bellman dependency must be locked at official SDK revision
`04e846cad306a1ae479367d2f19d9e53c18986ac`; the moving `dev` branch otherwise resolves
a different revision. Multicore is disabled. Standalone native and wasm builds prove
this dependency shape works without wallet/provider/starknet dependencies.

Actual negative controls (restored afterward): change base seed first byte 01->02:
`npm_wasm_vectors` exit101 `base public key`; change payload pair byte 01->00:
exit101 `base signature`; change payload nonce last byte 01->00: exit101
`base signature`. Logs `/tmp/p5a-zklink-{key,payload,nonce}-control.log`.
The unchanged corpus passes all four keys/signatures. Separate tests also reject
31-byte seeds and 93-byte messages before any upstream assertion.

GRVT's existing signer uses ethers EIP-712, not zkLink; this primitive is for
ApeX's L2 signing need. Rust origin makes reuse easier, but source pinning and
WASM parity remain necessary.

`wire_payload_and_signature_last_preserve_order` parses a raw two-key JSON body
with deliberately nonalphabetical `zPayload`, `alphaNonce` order. It consumes the
transported binary payload, checks its signature against the independent npm
WASM golden, appends signature last, and reparses the outgoing bytes to prove the
received binary preimage is unchanged. This does not claim zkLink signs the JSON
envelope: it signs the decoded binary payload. Actual negative control: standalone
crate without `serde_json/preserve_order` exits101 at `zkLink wire body field
order requires preserve_order`, showing `alphaNonce` moved before `zPayload`.
With the feature, the three-test suite passes.
