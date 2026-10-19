# L1 signing audit

The assigned `signL1Action` row describes a raw-key/Result representation, but
production also installs a Promise-returning signer in the constructed exchange.
`binding::signing_audit` obtains that original import from the real constructor;
it does not replace the wallet, signer, hash implementation or signature parser.
The source observer executes the complete maintained signing module with the
installed ethers and MessagePack packages on every test process.

The fourteen native/source profiles cover both networks and seven numeric
nonces: positive and negative zero, -1, the safe-integer maximum, 2^64, 1.25 and
-0.25. Successful signatures compare native r/s and recovery-bit fields before
serialization. The last two nonces exercise the actual rejection path, including
Promise return versus synchronous throw, exact message, display and native
Error/RangeError membership. Existing BytesLike probes independently retain five
valid vault profiles and three invalid envelopes through the bound signer and
ApiClient transport cause.

This is a bounded signing/diagnostic audit. It does not establish arbitrary
wallet callbacks, JS object actions, nonfinite nonce conversion, Promise job
ordering, stack-text equality or heap identity with an ethers Wallet. The typed
raw-key Result API remains separately documented.

The fresh-master compiled fractional nonce probe failed with generic Error,
while the maintained module returned RangeError. The production fix changes only
that numeric nonce rejection class in `signing_preimage`; successful preimages
and signatures are unchanged. Two native ApiClient preparation cases additionally
check the canonical OwnedCallbackError before serialization, so this loss is
observable beyond the standalone Result helper. `scripts/signing-audit-controls.py`
restores the original nonce wrapper and separately flips network selection; each
must fail named native assertions before exact-source restoration passes.

The native producer boundary is explicit: private `WalletKey` is constructed only
by `realm/imports_venue.rs::bind`'s `createWalletFromPrivateKey` import, and the
installed signer requires that private handle. Actual placement consumers in
`order_request_leaves.rs` and `spot_trading.rs` obtain it from the same import;
ApiClient signs the configured secret directly. These paths do not admit an
external arbitrary ethers Wallet callback object. The audit retains the explicit
raw-key interface distinction rather than inferring arbitrary wallet equivalence
from successful signatures. The legacy Realm adapter itself remains owned by
the separate atomic shared-runtime cutover.

The Node/NativeRuntime probes run only in native test builds, matching the
existing BytesLike signing fixture. Wasm all-target Clippy still checks the
production signing code; it does not execute a Node oracle or Tokio executor.
