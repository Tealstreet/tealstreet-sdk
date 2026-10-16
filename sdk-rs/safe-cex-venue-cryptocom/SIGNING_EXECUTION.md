# Crypto.com signing execution repair

Historical snapshot: [subsequent behaviour repairs and current triage](BEHAVIOUR_REPAIRS.md) supersede its open-finding counts; earlier measurements remain attached to their original revisions.

The three maintained TypeScript signing helpers run synchronously. Native helpers
now return `Result` directly and use the same shared HMAC backend as their former
local async signer path. Request/WS clock reads, params coercion, errors and HMAC
calculation occur before return. Method/key fields are owned strings. Rust callers
remove future polling; source function names and preimage order stay mapped in
`signing.rs`.

This repairs the three previously audited scheduling mechanisms. It removes the
finding on `generateSignature` and `generateWsAuthMessage`, which retain their
supported residual scopes. `generateAuthenticatedRequest` remains a **behaviour**
finding: its borrowed params cannot reproduce the source live mutable alias.
No row changes status, target or layer. The original18-row audit cohort therefore
has3 supported explanations and15 open findings: **13 explanation /0 scheduling
/2 behaviour /0 documentation /0 already-known**. The other behaviour finding is
the mutable public-endpoint table. These classifications do not prove behaviour
parity; the original [audit](DIVERGENCE_AUDIT.md) and its frozen receipts retain
their historical measurements.

`tests/auth.rs` retains the independently enumerated42-case maintained-TypeScript
comparison, fourteen cases per helper, ending global index41 WS/deep-error input
with explicit nonce flag. Native signature, fields, number bits, errors and pointer
identity are asserted before source output-shape assertions. The four TypeError
cases remain explicit. `tests/signing_entry.rs` separately asserts request/WS
clock effects before consuming the returned value. `tests/divergence_audit.rs`
retains the observed source/native residuals. These are finite comparison scopes,
not an exhaustive equivalence claim; they add no distinct rows to the original
seven executable-reference rows.

Runtime-control targets are the restored old deferred library against the two
entry tests, and a one-byte HMAC preimage prefix change against the native auth
signature assertion. SHA-bound measured exits and mutation patches are recorded
in the local delivery attestation; an assertion failure only counts after the
mutated target compiles. No compiler error substitutes for runtime proof.

The current execution and remaining alias boundary live only in
[the canonical declaration](../parity/MANIFEST-SPEC.md#cryptocom-signing-execution-boundary).
Governing rules remain [§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy) and
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

No remote signing, HTTP serialization, exchange dispatch, arbitrary executable
JavaScript params, wasm execution or complete venue equivalence is established
by this repair. A shared-cell draft was rejected because it conflated primitive
values and variable rebinding with object identity; no alias fix is claimed.
