# P2d verification evidence

Original P2d commands ran from the repository root on `feat/rust-p2d-crypto`.
This follow-up runs on `feat/rust-crypto-arbitrary-precision`; its Cargo clippy,
test, check and wasm gates explicitly add `--features serde_json/arbitrary_precision`
to reproduce P2b's pending workspace feature at base `f639f665b0`. Cargo used
`/opt/homebrew/opt/rustup/bin/cargo`. Root Yarn scripts used the installed entrypoint
`node /Users/sam/.volta/tools/image/yarn/1.22.22/bin/yarn.js` with the Cargo directory
prepended to PATH: the normal Volta shim recursively spawned itself, so its process
tree was stopped. The scripts themselves were run, not skipped.

| Gate | Restored result | Negative control and observed result |
| --- | --- | --- |
| `cargo fmt --manifest-path sdk-rs/Cargo.toml -p safe-cex-crypto --check` | Pass | Added a space inside `can_sign_payload(`; exit 1 with formatting diff. |
| `cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-crypto --all-targets --locked -- -D warnings` | Pass | Added unused `unused_control`; exit 101 naming unused variable. |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-crypto --all-targets --locked` | 20 passed with workspace arbitrary_precision | Replaced HMAC input with `wrong`; independent vectors and local-payload tests failed, exit 101. Wrong callback key failed request assertion. BloFin raw-digest base64, ApeX raw-secret key, Kraken SHA256 HMAC, and base Kraken fallback each failed their corresponding test, exit 101. |
| Exhaustive output-format dispatch (`cargo check -p safe-cex-crypto`) | Pass | Added `FutureFormat` without a match arm; exit 101, E0004 non-exhaustive patterns. No wildcard exists in the signing match. |
| `cargo build --manifest-path sdk-rs/Cargo.toml -p safe-cex-crypto --target wasm32-unknown-unknown --locked` | Pass locally; included in the configured CI wasm job | Injected wasm-only `compile_error!`; exit 101 with the unique wasm diagnostic. |
| `yarn parity:check` | Pass: invention and status checks | Added unmapped `negative_unmapped_control`; exit 1, invention diagnostic naming that item. |
| `yarn parity:report` | Pass: validated manifest claims, with explicit divergences; Rust discovery and status/invention checks precede reporting | Malformed `payloadSigner.ts.toml`; exit 1 with TOML parse error naming the file. After canonical P0f integration, removing its provenance declaration also failed with exit 1. The older blank-layer pass was historical behavior, now replaced by status enforcement. |

Plan and implementation reviews were native sub-agent calls, read by the
implementer. The original WS review found a mismatch against **native Node** on
UTF-16 low-byte decoding: Node vectors for `Zm😀9v` and `ZmŁ9v` failed against the
original decoder, then passed after the Node-compatible change. This establishes
Node behavior only; it does **not** establish Kraken browser-signing fidelity.
All temporary mutations were restored before the final successful gates.

Kraken's TypeScript `signWsAuth` runs in browser, Node and Bun hosts. The feross `buffer` polyfill
strips non-base64 characters rather than truncating UTF-16 units, so those two
non-ASCII secrets decode differently from native Node. The ten ASCII secret
cases in the fixture agree between Node and that polyfill; real Kraken secrets
are valid ASCII base64. The parity evidence is bounded to those ASCII cases.
The round-1 follow-up settled the binding: Next's webpack configuration injects
feross Buffer into client/edge bundles through `ProvidePlugin`, while Node hosts
use native Buffer. The TypeScript decoding is itself host-dependent for malformed
non-ASCII secrets; neither host is a canonical reference for those inputs. The
Rust decoder currently chooses Node semantics, including in wasm; the non-ASCII
vectors prove only that choice, not browser parity. The remaining open question
is how the eventual browser/wasm port should represent that existing divergence.

Round 1 correlation regression: the wire envelope keeps `id` and raw optional
payload/format strings. Unknown formats now return the exact id-bearing
unsupported-format error, so a caller can remove the matching pending request.
Missing/null/empty formats produce the existing id-bearing missing-payload error.
The enum conversion is fallible and the signing match remains exhaustive.
The six hex HMAC expectations now load from the generated fixture instead of
hand-copied literals. Dropping the unknown-format response id was tested as a
negative control: pending-map removal returned `None` instead of `Some("waiting")`,
and the regression test exited 101; restoring the id passed. Adding `FutureFormat`
still failed with E0004 after introducing the raw envelope.

Golden values come from Node's built-in crypto, separate from the Rust crates.
The five ordinary WS values also match the pre-existing TypeScript goldens.
Routing equivalence is P2a's gate and was not run for this crypto-only chunk.
No venue adapters or HTTP signing transport were implemented; Coinbase JWT
remains an explicit override-required error. HMAC helpers are internal; the
mapped payload and WS signer APIs expose this chunk's behavior. Shared primitive
dependencies were centralized with explicit overseer authorization.

Round 1 corpus hardening added derived enum-variant coverage, exact corpus lengths
(7 WS, 12 primitive, 11 Kraken), and the two base override-refusal goldens. E0004
proves only that an arm exists; fixture coverage and independently expected
outputs check the represented behavior. Stark was removed from this HMAC-only
runtime seam; it remains live in
`paradex.sdk.ts::signParadexTypedData`, which calls the callback directly with
Stark/felt-pair. That venue port must widen the request and signer seam.
Its request and signer type mappings
are marked divergent with explicit notes; the signature/headers result remains
ported.
The three partial-model exemptions were later replaced by real mappings once
the array-rs spec change became available.

CI's native test job now runs `generate.cjs`, followed by
`git diff --exit-code -- safe-cex-crypto/tests/fixtures/node-crypto.json` from
`sdk-rs`. This detects generator/fixture drift, rather than treating generator
provenance as a claim that remains true forever without enforcement.

Negative controls for the new checks were run and restored:

- Added `NewVenueBase64` with a compiling hex-HMAC arm and no fixture: enum coverage
  failed with exit 101, naming the missing `new-venue-base64` case.
- Deleted the Bitunix fixture and replaced its signer with plain hex HMAC: the
  all-target test command failed with exit 101; pinned WS size was 6 instead of 7.
- Changed the generator's shared secret, regenerated, and ran the exact CI scoped
  diff: exit 1. Restored the generator, regenerated, and got exit 0.

Workspace-wide public-item and total-row counts are deliberately not pinned here;
they change as other port chunks merge. `parity:check` and `parity:report` validate
Rust discovery, item existence, provenance, declaration correspondence or explicit
renames, and annotation status. Their validated claim counts do not establish
behavioral signing parity.

## Array-rs mapping integration

Merged gate-hardening tip `fcd79c8aef` (including requested `1e413393cc`) via
`ecded235d4`. The PR 7202 array-rs support is already present in this branch, so these rows
pass its gates without a separate merge prerequisite.
`RemoteSignerRequest` maps both `WsAuthRequest` and `WsAuth`, with a note naming
the envelope/payload split; `RemoteSignerResponse` maps `WsAuthResponse`.
`ExchangeOptions` maps its existing wire type alongside runtime `SigningOptions`.
The three former exemptions and the newer `WsAuthRequest` exemption were removed.
Request/response mappings are divergent because their HTTP fields remain absent.

The crypto report's distinct Rust claims increased from 7 to 11. Its displayed
ported rows moved from 5/7 to 6/10 (the existing ExchangeOptions row now also counts
in crypto); four divergent rows disclose the two HMAC-only and two WS-only
projections. The workspace-wide ported numerator did not increase artificially.

Mapping negative controls were restored after observing these failures:

- Removed the ExchangeOptions array's split note: parity:check exit 1,
  `array rs requires note`.
- Changed its SigningOptions member to `MissingSplitMember`: exit 1 with the
  unresolved-member diagnostic and missing SigningOptions invention diagnostic.
- Removed WsAuth from its request array while leaving its exemption deleted:
  exit 1, `WsAuth: public item has no manifest rs or exemption`.

Historical coverage check: `yarn parity:extract:check` failed at the
TypeScript extractor's string-only rs parser (`ExchangeOptions missing string rs`).
The attempted fabricated-TS-row negative control did not reach coverage validation
for the same reason; it is not counted as a proven coverage control. This upstream
array-rs gap was reported to the overseer; no extractor changes were made in P2d.

## Round 2 oracle and corpus properties

Composition provenance is now checked programmatically against the pre-existing
`base.ws-auth-signing.test.ts` literals. Every non-override enum variant must
have a TS composition golden and a signature-bearing Rust fixture with matching
format, secret, payload and signature. An error-only fixture cannot discharge it.
The generator remains a primitive oracle, not an independent composition oracle.
Kraken vectors must retain non-ASCII, mid-string equals and mixed standard/URL
alphabet secrets (mixed characters before padding), with signatures and payloads.
They also require invalid low-byte input followed by at least two valid base64
sextets before padding, so replacing skip-invalid with termination changes bytes.
Length checks alone never prove these properties survive coordinated edits.

Merged extractor support `d65c7e2b8a` via `b759aa6eaf`. Reconciliation changed
only array spacing in two existing rows, preserving member shape/order and notes.
`yarn parity:extract --check` now passes: no manifest changes, added or orphaned
symbols. The earlier string-only-parser blocker is resolved.

Round 2 negative controls, all restored and all exit 101 for the intended reason:

- Added a new enum variant with a compiling wrong hex arm, matching generated
  fixture and adjusted length: rejected for missing TS composition golden.
- Changed base64's generator composition to hex and regenerated: rejected by
  the pre-existing TS signature comparison.
- Replaced hex's signature fixture with an error: rejected for missing signature.
- Deleted non-ASCII, mid-string-equals, and dual-alphabet vectors in separate
  probes, adjusting lengths each time: each respective semantic-property
  assertion failed. These failures do not depend on count literals.

The restored extractor coverage negative control now reaches validation: adding a
fabricated TS type row fails with exit 1 and an orphaned-symbol diagnostic.

## Round 2 raw wire correlation

The envelope now extracts id independently from raw JSON signing fields, echoes
non-string and explicit-null ids, and omits a missing id as TS JSON serialization
does for undefined. Raw outputFormat values use JS truthiness for the initial
guard; only strings select an enum arm. Other truthy types produce JS String
error diagnostics (including arrays joining elements and ordinary objects).
Kraken never reads the format, including numeric formats. Node-generated wire
expectations independently cover all JSON kinds for ids and formats, array/null
coercion, false/zero guards and JS number exponent formatting. The match remains
exhaustive with no wildcard.

This covers ordinary request envelopes containing finite JSON values and
ordinary JSON objects. Objects shadowing
`toString` remains outside the ordinary-object coercion bound. The text-parser
limits and their NO RESPONSE consequence are specified below. Invalid
toSign/non-object wsAuth is rejected with a correlated missing-payload response;
TS may instead throw a digest type error for some invalid payloads, recorded as
a divergence in the request mapping.

Raw-wire negative controls were restored after these failures (all exit 101):

- Dropped the response id: pending-map removal failed in the new wire regression.
- Collapsed explicit-null request id into absence: exact response JSON mismatched.
- Stringified arrays before enum conversion: ["hex"] incorrectly signed and the
  independent wire response comparison failed.
- Made Kraken reject non-string format: expected signature comparison failed.
- Deleted array-format wire cases: JSON-kind corpus coverage failed.
- Added a format enum variant without an arm: compile failed with E0004.

For a nonempty payload and secret, TypeScript and Rust now agree:

| outputFormat | Both responses' error |
| --- | --- |
| `123` | `Unsupported WS auth outputFormat: 123` |
| `{}` | `Unsupported WS auth outputFormat: [object Object]` |
| `[]` | `Unsupported WS auth outputFormat: ` (empty suffix) |
| `true` | `Unsupported WS auth outputFormat: true` |
| `null` | `Missing wsAuth payload for WS auth signing` |

`String(null)` would be `null`, but the falsy guard runs before conversion.
Every response echoes the raw id, or omits it when absent; Kraken instead signs
all these formats without inspecting them. ECMAScript numeric diagnostics use
workspace `ryu-js` (locked 1.0.3), centralized with explicit authorization; num
and store declarations were left untouched. Final verification uses the actual
workspace declaration, not the restored temporary local declaration.

## Merge-gate F1: actual JSON text entry point

`WsAuthRequest` now consumes fields individually and skips unknown values with
`IgnoredAny`, rather than constructing a whole-envelope `Value`. Unknown metadata
at the envelope or wsAuth level tolerates `1e400`, negative overflow, a 401-digit
integer, and 200-level arrays/objects containing overflow. Tests invoke
`serde_json::from_str::<WsAuthRequest>` on raw text preserved as fixture strings,
then compare both base and Kraken id/signature responses with independent Node
`JSON.parse`/crypto results. Base signatures also match the existing TS hex golden.
JSON.stringify cannot construct those raw tokens; it only stores the wire text
and finite response goldens in the corpus.

The workspace `arbitrary_precision` feature **closes numeric parser refusal**.
Raw `1e400` in outputFormat now parses and returns
`{id, error: "Unsupported WS auth outputFormat: Infinity"}`; Kraken signs instead.
Negative overflow produces `-Infinity`, and scalar/array/nested-array forms keep
correlation. Numeric toSign and scalar wsAuth overflow now parse and return
id-bearing missing-payload errors (Kraken: `Missing challenge to sign`).
Otherwise-signable requests with id `1e400` produce signatures while retaining
Rust's raw JSON number (`1e+400` on serialization). TS's Infinity id serializes
as null; that wire-id divergence remains explicit.

Depth refusal is **not closed**: 200-level arrays in retained id, toSign or
outputFormat still produce a recursion error, so **NO RESPONSE IS PRODUCED**
from either signer. Invalid JSON and non-object roots (null, 123, []) also
produce no response. A 200-level non-object wsAuth array is drained using
IgnoredAny and produces correlated missing-payload errors, as does scalar
wsAuth overflow. Ignored metadata still tolerates deep overflow structures.
These outcomes are tested through from_str, not constructed Values.

Historical pre-arbitrary_precision F1 negative controls were restored after all six exited 101 for the expected
reason: restoring the previous whole-envelope Value parser failed on `1e400`;
materializing unknown fields as Value failed the same parse; dropping id failed
the exact response comparison; deleting deep or overflow wire cases failed their
semantic corpus assertions; replacing a rejected overflow id with finite `42`
failed the assertion that parsing produces no response. No control relies on
`json!` or `from_value` to supply the raw numeric token.

## Merge-gate F2/F3/F6

The skip-invalid corpus property requires an invalid UTF-16 low byte before
padding followed by at least two valid base64 sextets. An all-invalid empty key
cannot satisfy it. Removing both invalid-character keys and lowering the length
pin failed this property with exit 101. Changing the decoder's invalid arm from
continue to break failed the independently expected Kraken signature, exit 101.

Native JS String goldens now include `0.000001` and `1e20`, which remain numeric
across fixture JSON serialization. Replacing ryu-js formatting with Number's
`to_string()` independently for each input failed with exit 101: `1e-6` versus
`0.000001`, and `1e+20` versus `100000000000000000000`. Deleting either vector
also failed its numeric-boundary corpus assertion with exit 101. All controls
were restored. The array-rs infrastructure is already merged in this branch;
PR 7202 landing separately is not a merge prerequisite for these mappings.

## Merge-gate F7: CI wasm coverage

The workflow's existing wasm-core job now includes `-p safe-cex-crypto` alongside
types, num and store. The exact YAML command is run locally with CI's
`RUSTFLAGS=-Dwarnings`. Injecting a wasm-only compile_error in crypto produced
exit 101 with `F7_CRYPTO_WASM_NEGATIVE_CONTROL`; restoring the file and rerunning
the same command produced exit 0. This verifies the command actually builds crypto;
it does not claim a remote Actions run has completed.

## Final-gate hardening

Number diagnostics no longer unwrap `as_f64`: when arbitrary_precision retains an
out-of-range token, JS numeric conversion yields Infinity/-Infinity (or NaN if
conversion fails), and ryu-js renders the JS string. Numeric truthiness also uses
that conversion and treats NaN as false. Native JS raw-text goldens cover positive
and negative scalars, arrays, nested arrays and `1e309`. The workspace feature now accepts these numeric tokens. The full suite requires
arbitrary_precision behavior, including the formerly rejected retained fields;
the targeted CI regression remains an additional check.

Retained-field outcome coverage now derives each field/limit pair from the actual
wire token and requires both id/outputFormat with overflow/depth. Primitive
coverage requires empty inputs, Unicode key/payload, embedded NUL payload and a
key longer than the HMAC block, separately for both algorithms and encodings.
Count literals remain supplementary; they do not discharge these properties.

Final-gate negative controls were restored after all eight exited 101:

- Restored the old numeric unwrap: the feature-enabled array regression panicked
  on None, proving the new feature gate reaches the original defect.
- Removed all id failure cases, all overflow cases, and all depth cases in three
  probes: each failed the actual field/limit coverage set.
- Removed Unicode/NUL, empty-input, and long-key vectors in three probes while
  lowering both old count pins: each failed its semantic primitive property.
- Removed only NUL bytes while preserving Unicode: the NUL property failed.

The targeted CI step first checks that the exact test exists, avoiding a green
zero-test filter if the test is renamed or removed. Renaming it made the exact
workflow command exit 1 at that guard; restoring it produced exit 0 (one test,
18 filtered out). The final implementation sub-agent audit approved the changes.

## Canonical P0f integration

Merged integration tip `5ebe612f0e0a0be4147a734e4450c9d1f01b4401`. Both
`safe-cex-parity` and `parity/extract-ts` match that tip exactly. Four crypto
source manifests now declare `safe-cex-crypto/src/lib.rs` in their per-source
`rust_files`, retaining existing type/store declarations. Six renamed targets
are explicitly named, while split explanations and divergence bounds remain.
The extractor reports 6801 rows, including 542 exported value constants; these
are observations of this merge, not pinned completion criteria.

All seven existing gates passed again, as did native check, the targeted
arbitrary-precision CI command, extraction check and fixture regeneration
determinism. Removing payloadSigner provenance failed both check and report
(exit 1, undeclared Rust source); removing SigningOptions's rename token failed
check (exit 1, exact rename target required). Each mutation was restored before
the successful gates. Earlier gate-specific controls remain documented above.

## Workspace arbitrary-precision follow-up

Based on integration `f639f665b0`. That base still carries the pre-P2b feature
pin; verification explicitly enables `serde_json/arbitrary_precision` until
P2b supplies the shared workspace pin. Crypto runtime is unchanged. The retained
corpus now separates overflow acceptance from depth refusal; invalid payload
coverage requires both toSign/wsAuth and overflow/depth. The tests require the
feature rather than treating its absence as an equivalent passing outcome.

Follow-up controls: restored the old overflow-refusal assertion and old numeric
unwrap; both failed with exit 101 (accepted result and array panic respectively).
Deleted each id/outputFormat/toSign/wsAuth × overflow/depth case separately:
each of eight probes failed its semantic coverage set, exit 101. Dropping error
response ids failed the exact invalid-payload response comparison, exit 101.
Running the targeted test without arbitrary_precision failed its explicit
feature requirement, exit 101; absence no longer discharges the behavior gate.
All mutations were restored. Documentation is bounded by these measured
from_str outcomes, including the still-open depth and raw-id wire differences.

Follow-up gates passed: fmt, strict clippy, 20 all-target tests, native check,
wasm build (CI warnings denied), parity check/report and extractor check
(6801 rows, no drift). The targeted CI test also passes with the feature.
The plain default test is intentionally not green before P2b's pin: numeric
acceptance is now required, and disabling the feature is a negative control.
The known Volta shim loop is bypassed with the installed Yarn entrypoint above.

All six overflow format shapes also assert Kraken signatures against Node crypto.
Making Kraken refuse non-string formats failed that targeted test with exit 101;
restored execution passed (1 test, 19 filtered out). Plan and implementation
sub-agent audits approved the measured bounds and corpus assertions.

## P5 primitive foundation

The registered P5 modules and scheme/security regressions extend the earlier P2-only
gates. See [GATE_EVIDENCE.md](GATE_EVIDENCE.md) for scheme oracles, actual
key/payload/nonce and per-scheme wire-order controls, browser build, source/root
coverage bounds and dependency maintenance evidence. The old HMAC callback seam
remains unchanged; these are generic primitives, not venue or transport adapters.

Lighter nonce ownership: the host supplies fresh secret CSPRNG bytes and handles
entropy errors. `SchnorrNonce::from_bytes` rejects zero and noncanonical scalars;
`schnorr_sign` consumes the private, non-Clone/non-Copy nonce type. This crate
owns validation and the signing boundary, not a global/browser entropy source.
Never reconstruct a consumed nonce from reused bytes. Verification rejects a
neutral nonce commitment; the committed Go zero-nonce signature is deliberately
invalid and demonstrates full secret-scalar recovery from public `(s,e)`.

PR 7215 hardened verification now includes malformed Lighter lengths, the
zero-nonce public-key-recovery demonstration and compile-time nonce ownership,
owned-storage zeroization, and independently replayed zkLink 0/92-byte ends.
The live root `yarn crypto:oracles:check` is mandatory in rust.yml, including
frozen isolated JS installation and a pinned native-SDK checkout. Generator
capture uses exclusive creation; ordinary execution asserts drift. Package pins
hash published files as well as versions. The official native SDK is cross-linked
to four npm-WASM results; its source revision is not asserted to be the binary's
provenance. See GATE_EVIDENCE.md for actual per-control failures and the external
SDK time-family compatibility patch/warning policy.

The primitives-only split is recorded in [SPLIT_EVIDENCE.md](SPLIT_EVIDENCE.md).
It excludes the inherited runtime backend and venue implementations.

## PR 7224 review repairs

Uniform 40-byte nonce candidates reject about half the time (real Node CSPRNG:
4057/8192 = 49.52%, repeat 4020/8192 = 49.07%). The host must resample fresh
secret candidates, scrub its buffer after every constructor outcome, stop on
actual entropy failure, and bound a broken provider. The worked CLAUDE example
is bound byte-for-byte to the executed host helper. A looping host completes
and verifies all 128 requested signatures. Independent controls: reducing the
loop to one attempt fails 101 at looping-host success; removing only the
documented loop fails 101 at the executable-contract binding.

Go-first commit 6ede3a7b24 captures canonical and left/right/both noncanonical
two-to-one inputs. Go hashes the latter; Rust deliberately refuses them. Only
raw hash/Fp5 paths claim Go noncanonical reduction. Deleting each noncanonical
case independently fails 101 at its specific canonicality coverage property.
The live Go replay runs in the crate suite and the all-oracles CI runner.

The zkLink Rescue output is checked with first/ok_or before signing.
RETRACTION: the previously claimed unchecked-index control mutated the helper,
not the production call site. Restoring indexing at sign stayed green under
the zkLink lib tests; full-crate failure was only denied dead-code. The earlier
claim did not prove call-site binding and must not be used as evidence. A
production-entrypoint empty-output witness is required. Stark public-key,
explicit-nonce and verify helpers are private/test-only: they do not implement
signParadexTypedData. That row covers only hashing and deterministic signing;
no adapter completion is claimed. The Go zero-nonce key leak is marked ref-bug:
in source and its divergent owning row, including the public key-recovery proof.

All three locked getrandom versions have browser backends: 0.2.17/js and
0.3.4+0.4.3/wasm_js. A resolved wasm feature-graph test pins them. Disabling
either new backend independently fails 101 with a version-specific diagnostic.
No production RNG or host entropy implementation was added. Crypto oracles use
the existing isolated CI install; they do not require tsx or ts-node. The
reported integration HOW tsx environment failure was left to its owner.

## PR 7224 felt-range repair

Oracle-first commits 4ce34ac5c5, 7e51aecf44 and a2542f4740 captured 109
raw SDK cases before the corresponding source repairs: 79 field/i128/extra-key
cases, four malformed-schema cases, and 26 negative-zero/domain/enum/merkletree
cases. The field guard validates declared values before Felt can reduce them;
`p`, `p+1`, hex `p` and nonzero negatives return errors, including nested and
array fields. Negative decimal zero remains valid. The SDK's i128 coercions
and ignored undeclared message keys are retained in the captured cases.

Scoped verification: eight Stark tests, strict all-target clippy, warnings-denied
wasm build, three deterministic SDK generators, and 135 CI wiring controls
passed. This is a felt-range repair, not a claim of complete SDK parity:
pre-existing `+1`/`0X1` WRONG-HASH differences on felt/shortstring remain a FINDING: Rust encodes shortstrings while the SDK parses numeric one.

Independent reviewer: MERGE for this scoped repair. Removing the production
message guard and its now-unused primary binding exits 101 at the runtime
assertion `rev0/felt/p SDK refuses, Rust signs`; composite-range checks fail
too. Removing only the guard would yield an unused-variable compile failure,
which is explicitly not the negative-control evidence. Additional +p,
whitespace/BOM-prefixed p, uppercase 0X p and negative-hex probes found no
modular bypass.

## Rescue control rebound to the public signer

The new `public_sign_propagates_empty_rescue_output` enters public `zklink::sign`
with a valid seed and empty payload. A private cfg(test), thread-local injection
provides empty outputs at the checked boundary; a Drop guard restores the flag.
Six zkLink tests pass. An independent reviewer replaced the actual sign call
with `rescue_hash(...)[0]`: exit 101 at this runtime assertion, with the five
sibling tests passing. No dead-code compile error contributes to this control.
The earlier retraction above remains part of the record.

## Canonical WS payload ownership

`WsAuthRequest`, `WsAuth`, and `WsAuthResponse` moved to
`safe-cex-types/src/ws_auth.rs`; crypto re-exports the same types. No duplicate
record or lateral services dependency was introduced. Manifest targets and the
per-source rust_files declaration moved together. The previous extraData drop
was a projection defect; raw metadata retention now preserves unknown values,
including explicit null, overflow tokens and 200-deep arrays. Type is retained
raw too. HTTP config, exchangeName and accountName are still omitted from this
WS-only projection. All 17 existing signing/text-parser tests pass unchanged;
retained signing fields still have their disclosed depth limits.

The actual Bybit envelope pins type=ws-auth, toSign, outputFormat and
extraData.expires, plus an id-bearing wsAuthSignature response. The canonical
record decoders refuse positional arrays, with Type::deserialize caller tests
so a future permissive inherent decoder cannot silently bypass the trait.

The pre-existing correlated refusal for truthy non-string toSign is explicitly
marked ts-bug: BaseExchange/KrakenExchange reject their promise and leave auth
unsettled. A live-source TS rejection oracle remains a FINDING; the move does
not claim it closes that reference-evidence gap.

Scoped reviewer MERGE. Four controls fail at runtime101: dropping extraData
fails the Bybit-envelope assertion; substituting eager Value fails metadata
lexical retention and the separately isolated deep-metadata case; deriving the
request decoder fails three legacy correlation/parser tests; adding a public
permissive inherent decoder fails the qualified-caller rejection test. The
initial trait-forced regression was corrected before this verdict because it
could prove only that the strict implementation was present. Strict clippy,
warnings-denied wasm and explicit-workspace parity also pass.

Validation caveat discovered during review: reusing a Cargo target directory
across scratch checkouts can leave parity-check's compiled CARGO_MANIFEST_DIR
pointing at the scratch tree. This run supplies --workspace explicitly; the
initial default-root invocation inspected the mutation tree and was rejected
as evidence for this worktree.

The earlier 109-case range repair did not cover implicit preset members or
unrecognized types. Its scoped success must not be used as proof that F1 was
closed. The reopened repair replaces permissive fallback with refusal and
checks preset aggregates through every leaf. Current crypto taxonomy after
the canonical WS move is 10 CHOSEN / 2 MIXED / 1 FINDING (13 rows); the earlier
11 / 1 / 1 report predates that re-bucketing and is superseded.

## Reopened F1: closed kind dispatch and composite ingress

Oracle-first commits 3335e4818e, 6b5c263234 and 1330cd4f90 add 84 raw
SDK cases: 72 closed-kind/preset cases (44 SDK refusals), eight positive
encoding-exemption controls and four malformed-composite cases. Existing
fixtures remain unchanged. Dispatch now matches the dependency's closed
InlineTypeReference enum exhaustively; undeclared custom types refuse. Known
preset aggregates recurse through every leaf without hand-written field-name
schemas. A future enum variant requires an explicit compile-time decision; a
future unrecognized runtime name refuses. Booleans, bounded signed i128,
revision-1 byte-array strings and nonhex hashed selector names are the named
exemptions, each with independent SDK positive hash controls.

Custom composites require actual JSON objects and arrays require arrays before
traversal. This also closes the raw-number-to-synthetic-record conversion: the
reviewer measured p and 2p hashing identically when the declared custom field
was serde's private Number marker; both now refuse before dependency decoding.

Scoped reviewer MERGE. Twelve Stark tests, strict all-target clippy,
warnings-denied wasm, explicit-workspace parity (6965 rows/420 public items)
and 135 CI wiring controls pass. Three new live SDK generators reproduce their
committed fixtures and are mandatory children of the existing CI oracle step.

Negative controls all fail at runtime101 by name: ignoring preset aggregate
validation fails TokenAmount/p; making the unknown-kind branch succeed fails
the structural unknown-member test; removing custom-object validation fails
the public raw-composite-shape assertion; disabling reserved-shadow refusal
fails the public refusal-owner diagnostic. The last assertion pins Rust's
deliberate pre-encoding validation owner, NOT the SDK's RangeError text.
The shadow-felt reference stack overflow is marked ref-bug on the Paradex row.
Wrong hashes for +1/0X1 remain explicitly disclosed; no full SDK-parity claim.
