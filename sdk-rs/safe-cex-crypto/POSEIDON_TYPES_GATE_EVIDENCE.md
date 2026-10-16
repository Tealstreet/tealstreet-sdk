# Poseidon interface-record port evidence

Moved **2 rows pending -> divergent**, measured with Python 3.14 `tomllib`
over `parity/manifest/poseidon-crypto` against parent 125d0ed78f. Both rows use
the canonical FORCED shared UTF8 bucket for their declared strings. The measured
Poseidon triple is 0 ported / 4 divergent / 18 pending across seven manifests;
this stops at the extracted manifest inventory, not every Go/WASM declaration.

Tested SHA: `96179a4a667e1779db966d0992c9a1e020842b03`.
Actual parent: `125d0ed78fc270352a102bd75dc8ee85d8a1d4cc`.
Integration ancestry remains `3c5044dc1a5cf1f05b7fd34eea0a3b70c8bf747b`,
via inherited `b03cca6515`; no refresh to `2bc83d78d4` is claimed. Fresh
sub-agent review of the committed two-record source/oracle/test/manifest has no
outstanding finding within that floor. The other two source slices also have
recorded sub-agent reviews; no blanket ancestor or full-runtime review is implied.

## Measured exits at the tested SHA

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-crypto
  --all-targets --jobs 2`: **0**.
- The same scoped check for `safe-cex-venue-lighter` after the dependency export:
  **0**. These are two serial crate checks, not workspace compilation.
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-crypto
  --test live_poseidon_types --jobs 2 -- --include-ignored --nocapture`: **0**,
  **2 passed / 0 failed / 0 ignored**.
- Symmetric public/private serde-name swap for privateKey/publicKey compiled;
  selected `poseidon_record_native_identity` exit **101**, **0/1/0**, named
  `SchnorrKeyPair.privateKey native identity` (public-B versus private-A).
  Exact source bytes restored; selected target exit **0**, **1/0/0**. This
  control stops on the first bare-raw native key field, not all three paths.
- Permanent encoded-surrogate child control asserts exit **101** at
  `Poseidon live encoded corpus content`. It replaces d800 with d801, asserting
  changed bytes and equal encoded length. The containing test passed above.
  A refusal of both units alone was a review finding; full live body digest
  `a8880597b3aa2cf85faaf84d55572a6ef43d8b22837650d723977886a7ef5424`
  now discriminates before decoding. No observed control survivor remains.
- Specified Node/Yarn `parity:extract:check`: **0**, output text
  `Would update 0 manifests; 0 added, 0 orphaned symbols.` Writer before this
  commit exited **0**, `Updated 1 manifests; 0 added, 0 orphaned symbols.`

Native fields are checked before wire shape assertions, including both distinct
key fields and the error string. The independent floor is three string leaves
and nine inputs each (27 rows), with bare raw/owned/borrowed accepted-value
checks. Lone-surrogate refusals are raw-only because Value cannot materialize
those units; required/null/map-only refusal tests are owned-Value checks. No
hex or key-length validation is inferred from source comments. Type-only string
records are not cryptographic generators or loaded WASM API wrappers.

Not run: workspace build, host clippy, wasm check/clippy, full SDK semantic
compilation, full monorepo suites, live exchange/network, WASM loader behavior,
random key generation or CI. No PR opened or merged. Evidence commits contain
only prose; exits remain attached to the tested code SHA above.

## Fresh-agent cumulative row movement

Python 3.14 tomllib comparison of tested SHA against inherited b03cca6515,
stopping after 26 SDK + 13 venue + 7 Poseidon manifests: **9 pending -> ported,
6 pending -> divergent (15 rows)**. SDK: **67/73/249**; venue: **132/18/186**;
Poseidon: **0/4/18**, ordered ported/divergent/pending. Relative to requested
integration snapshot 2bc83d78d4 the inherited order-API port adds another two
pending -> divergent rows, giving branch cumulative **9/8 (17 rows)**.
TransactionClient still has **41 pending rows**; loader/random-generator work
remains pending. These are progress counts, not proof of complete lane parity.
