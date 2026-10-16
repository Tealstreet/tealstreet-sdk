# safe-cex-crypto

Behavioral reference: [WS signing invariants](../../packages/safe-cex/src/exchanges/CLAUDE.md)
and [payloadSigner](../../packages/safe-cex/src/utils/payloadSigner.ts).

`SigningOptions` borrows runtime credentials and a `PayloadSigner`; the wire-only
capability placeholders in `safe-cex-types` do not carry callbacks. Signer futures
need not be `Send`, so a browser host can retain its own callback handles. This
crate's `sign_payload` requests HMAC only. Paradex's TypeScript SDK directly calls
`payloadSigner` with the live Stark/felt-pair scheme; that venue callback path is
omitted here, recorded as divergent, and requires widening this seam in its port.

Tests require each algorithm/encoding corpus to retain independent Node `crypto`
outputs for empty inputs, Unicode,
embedded NULs and keys longer than the digest block. They never compute expected
signatures through the Rust implementation.

`sign_ws_auth` mirrors the base signer and explicitly refuses Kraken and Coinbase
formats. `sign_kraken_ws_auth` mirrors the Kraken override, including ignoring the
format and Node's permissive base64 secret decoding. This is corroborated against
browser decoding only for the ten ASCII secrets in the fixture. Next injects
feross Buffer into client/edge bundles, while Node uses its native Buffer; malformed
non-ASCII secret decoding is host-dependent in TypeScript. The Rust currently
chooses Node semantics, without claiming those are canonical. Coinbase JWT remains the
venue override's responsibility. The request/response mappings represent WS-only
projections; HTTP fields remain unimplemented.

`WsOutputFormat` has an exhaustive match with no wildcard: adding a variant
without a signer fails compilation. `WsAuthRequest` parses the id and optional
raw `WsAuth` JSON fields first, preserving missing versus null and non-string ids. The base signer converts the format to the enum
fallibly from actual strings, returning an id-bearing error for unknown or wrong-type
formats with JavaScript String diagnostics; missing/null/empty
format returns the existing missing-payload error. Kraken ignores the format.
Request deserialization skips unused `extraData` and other unknown fields without
materializing them, tolerating overflowing numbers and deep nesting there.
The workspace's `arbitrary_precision` feature closes numeric parser refusal:
retained overflowing formats return id-bearing Infinity/-Infinity diagnostics,
including arrays; Kraken signs without inspecting them. Overflowing numeric
toSign or scalar wsAuth returns a correlated missing-payload error. An overflowing
id is preserved as a raw JSON number and otherwise-signable requests sign;
TypeScript JSON.stringify instead serializes its Infinity id as null, a remaining
wire divergence. Excessive nesting in retained id/toSign/outputFormat, invalid
JSON and non-object roots still fail parsing: **NO RESPONSE IS PRODUCED**, including
for Kraken. Deep non-object wsAuth arrays are drained and return correlated
missing-payload errors. Ignored metadata remains tolerant of deep nesting.
Raw-text tests exercise the host's `serde_json::from_str` entry point.

The checked-in [Node oracle](tests/fixtures/generate.cjs) regenerates static
[golden vectors](tests/fixtures/node-crypto.json). Existing WS vectors match
`base.ws-auth-signing.test.ts`; additional vectors exercise SHA512 and Kraken's
base64 decoder independently of Rust. Run crate-local fmt, strict clippy,
all-target tests, wasm build and root `yarn parity:check` / `yarn parity:report`.

The WS corpus contains one case for every enum variant, including the two base
signer's required-override refusals. A test captures serde's derived variant list
and compares it to fixture coverage; a new variant with a compiling arm still
needs a signature-bearing case and a matching composition golden in the existing
TypeScript test. A Rust test reads those TS literals and compares each complete
format/secret/payload/signature tuple to the corpus. Kraken corpus properties
require non-ASCII, mid-string equals, dual-alphabet secrets, and invalid input
followed by decodable data before padding. Length checks
are supplementary drift checks, not protection against coordinated edits. The native CI test
job regenerates the corpus and rejects drift with a scoped Git diff check.

The manifest uses array-rs mappings to split `RemoteSignerRequest` into its
id-bearing `WsAuthRequest` envelope and raw `WsAuth` payload, and `ExchangeOptions`
into wire data and borrowed runtime `SigningOptions`. `WsAuthResponse` maps the
WS-only response shape. Request/response rows remain divergent until HTTP fields
are represented; these behavior-carrying models no longer use exemptions.
The array-rs gate and extractor support from PR 7202 are already merged into this
branch; the mappings pass parity validation here.

P5 primitive modules are prepared in `lighter`, `eip712`, `msgpack`, `ed25519`,
`starknet` and `zklink`. Their independent oracle fixtures were committed before
Rust integration, including existing-Go-WASM hashes and controlled Schnorr
signatures. Lighter's raw-message API explicitly hashes to Fp5 before signing;
Stark takes a precomputed hash, and zkLink takes binary bytes, not an implicitly
authenticated JSON envelope. EIP-712 signs its declared typed structure.

Modules are public and consume exact workspace dependency pins.
[GATE_EVIDENCE.md](GATE_EVIDENCE.md) records the SDK gates, actual controls and
dependency provenance. Source rows describe primitive dependency projections;
loader rows cover loading only, while Go cryptography is validated solely by the
Go-WASM vector harness. Coinbase JWT is unchanged.

Lighter nonce ownership: the host supplies fresh secret CSPRNG bytes and handles
entropy errors. `SchnorrNonce::from_bytes` rejects zero and noncanonical scalars;
`schnorr_sign` consumes the private, non-Clone/non-Copy nonce type. This crate
owns validation and the signing boundary, not a global/browser entropy source.
Never reconstruct a consumed nonce from reused bytes. Verification rejects a
neutral nonce commitment; the committed Go zero-nonce signature is deliberately
invalid and demonstrates full secret-scalar recovery from public `(s,e)`.

All scheme oracles replay in CI via root `yarn crypto:oracles:check`, with frozen
isolated JS dependencies and package artifact pins. Capturing a new golden is
explicit and refuses overwrite. zkLink's additional 0/92-byte vectors call the
pinned official native SDK, cross-linked to the four existing npm-WASM vectors.
The external SDK's time-only compatibility lock patch does not change crypto
sources or pins. Nonce-owned storage is zeroized on Drop; hosts must separately
scrub their borrowed input buffers immediately and obey the contract in CLAUDE.md.
