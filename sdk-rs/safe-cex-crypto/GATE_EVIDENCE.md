# P5a gate evidence

Lighter oracle fixtures were captured before Rust implementation from the existing
Go WASM, with artifact/runtime SHA256 and actual entropy transcripts. The harness
injects test-only entropy into Go's runtime; one 40-byte big-endian scalar request
per signature is asserted. This controls the nonce without rebuilding Go.

`node tests/fixtures/lighter/go-wasm.cjs` replays all outputs and does not overwrite
fixtures. Negative control: altered a committed expected hash; check exited 1
with `Go WASM oracle drift`. Restored replay exited 0. Go independently rejects
mutated keys and payloads; changing the nonce changes the signature while retaining
valid verification. Raw JSON fixture strings are the Rust test entry point.

## Primitive implementation evidence

All new fixture readers parse raw strings. No venue adapters or live calls exist.
The combined isolated dependency probe ran the existing 20 tests plus 16 new
scheme tests (36 passed), strict clippy and a wasm32 build. These were preliminary
checks before workspace registration. The registered SDK results below supersede
the isolated counts; primitive modules are now public and consume workspace pins.

| Scheme | Independent oracle | Actual negative controls (all exit 101) |
| --- | --- | --- |
| Goldilocks/Poseidon2/ECgFp5 | Existing Go WASM with frozen artifact hashes and entropy | Key mutation: Go public key mismatch; payload and nonce mutation: Go Schnorr mismatch. Clearing specialized corpora, removing zero or over-order scalar cases: semantic-property diagnostics. Removing preserve_order: body re-keyed after signing. |
| EIP-712/secp256k1 | Installed ethers 6.17.0 | Key: secp256k1 signature mismatch; payload/application nonce: EIP-712 payload hash mismatch. Removing preserve_order: signed two-key body wire order diagnostic. RFC6979 signing nonce is internal, not caller-configurable. |
| MessagePack | Installed @msgpack/msgpack 3.1.3 | Removing preserve_order: signed body must not re-key. Codec has no key or signing nonce; signing-scheme controls cover encoded application data. |
| Ed25519 | Node/OpenSSL | Seed, payload and application nonce mutations: Node public-key/signature diagnostics. Removing preserve_order: signed key order diagnostic. No independent signing nonce exists. |
| Stark curve | Installed Starknet SDK with independently recovered signing nonces | Key/hash/nonce: SDK public-key/signature diagnostics. Removing preserve_order: prehashed envelope key order diagnostic. Envelope context is not authenticated by this prehashed primitive. |
| zkLink Rescue/MuSig | Installed npm 1.0.2 WASM, SHA256 bound | Key: base public key; payload and application nonce: base signature diagnostics. Removing preserve_order: wire body field order diagnostic. Cryptographic nonce remains deterministically derived inside the primitive. |

Read-only implementation audit found and fixed vacuous Lighter specialized-corpus
coverage and missing explicit-nonce caller obligations. Noncanonical word and
scalar behavior is modeled from Go; no expected vector was changed to match Rust.
Go-style raw-message hashing is explicit before the candidate's prehashed Schnorr
API. Additional edge fixtures were captured independently, preserving old goldens.

zkLink's upstream SDK is Rust with wasm-bindgen JS bindings. Narrow primitive code
is adapted from revision 3152525e2615ecbbdb56fd6728ce265ad2dde485 (Apache-2.0;
notice retained), with franklin-crypto 6ba7708bd0db5142fc55800addcf211d30e65cfa,
plonk enabled and multicore disabled, and bellman_ce 04e846cad306a1ae479367d2f19d9e53c18986ac.
Native and browser probes pass against the npm artifact; upstream HEAD is not
claimed to be the proven source revision of that published binary. GRVT's current
TS code uses EIP-712, while ApeX uses the zkLink binding. Coinbase JWT remains
untouched; completing the required primitives takes precedence over that optional
ES256/EdDSA key-parsing override.

The final combined isolated probe passes 39 tests (including all existing crypto
tests and every scheme's wire-order regression), strict clippy and wasm32 with
warnings denied. This validates feature coexistence under serde_json 1.0.152 with
float_roundtrip/preserve_order/arbitrary_precision, not final SDK registration.

## Registered SDK gates

The complete SDK crypto test gate passes 42 tests with `--all-targets --locked --
--include-ignored`. Native tests execute the actual unchanged Go WASM replay in
all three fixture modes, so that vector-provenance check is also reached by CI's
normal cargo test. Other JS/npm oracle scripts are replayed locally against the
installed dependency versions; their Rust fixture assertions run in CI, but this
change does not add npm-oracle dependency installation to the Rust CI job.

| Gate | Actual negative control | Observed failure |
| --- | --- | --- |
| fmt | Added extra space to native hash declaration | exit1, formatting diff |
| strict clippy | Added unused variable in native hash | exit101 naming unused_p5a_control |
| full crypto tests | Each scheme's key/payload/nonce fixture mutations above | exit101 with scheme oracle mismatch; no expected value changed to fit Rust |
| wasm32 build | wasm-only compile_error in Lighter module | exit101 P5A_SDK_WASM_CONTROL |
| Go-WASM vector replay through Rust test | Altered expected Go hash temporarily | Rust exit101 with Go WASM oracle drift; Node exit1 |
| extractor check | Fabricated pending type in real primitive-source manifest | exit1 naming P5aMissingSymbol as orphan |
| parity check | Removed hasher's per-source Rust-file declaration | exit1 undeclared Rust source |
| parity report | Same missing provenance | exit1 undeclared Rust source, no validated progress table |

The arbitrary_precision-removal control exits101 at its explicit requirement.
Removing preserve_order makes every scheme's two-key/signature-last assertion
fail with its own diagnostic, exit101. The preserve_order experiment needs
unlocked/offline resolution because removing its optional indexmap edge changes
Cargo.lock; both declaration and lock were restored byte-for-byte. An initial
locked attempt failed at lock validation and is not counted as behavioral proof.
Likewise removing the entire primitive root first failed manifest path validation;
coverage proof instead uses the orphan-type control above.

Extractor package lint/typecheck/tests pass (32 tests). Their separate controls
introduce an unused binding, an incorrect numeric assignment, and a broken
manifest suffix respectively; measured exits1,2,1 with their intended diagnostics. Temporary
changes are restored before final successful runs.

All new mappings are explicitly divergent primitive projections; no venue adapter
is counted ported. Poseidon loader rows state: the TS surface is browser-loader,
node-loader, loader and stub around a Go WASM blob in src/wasm. Those rows cover
LOADING, not cryptography. Go has no manifest representation; its only parity
evidence is the Go-WASM vector harness. Signer/client lifecycle and transaction
builders remain unported. No invention exemption was added.

Extraction denominator: 6901 -> 6979 (+78): P5a roots add61 rows (poseidon22,
lighter signer39), while the verbatim P3b candle-root group adds17 pending rows.
The source groups remain disjoint and lighter-sdk/src/client is excluded.
The current branch report is263/6979; the supplied277/6901 reference includes
sibling utility work not present at this branch's bfe57deb48 parent. P5a adds no
ported venue numerator, only9 divergent rows and19 additional distinct crypto
claims (crypto6/19,30 distinct,13 divergent). Do not present these counts as
behavioral signing proof.

## Dependency provenance, measured 2026-10-12 08:21 UTC

| Crate | Last release | Total / API recent / pinned-version downloads | Published Rust size |
| --- | --- | --- | --- |
| poseidon-hash0.1.4 | 2026-03-04 06:21:19Z | 32682 /21210 /24627 | src:4 files,2285 physical lines; archive26571bytes |
| goldilocks-crypto0.1.2 | 2026-03-04 06:22:27Z | 30844 /19979 /22990 | src:6 files,2662lines; all Rust:13files,3868lines; archive43237bytes |

Both have sole registry owner/publisher Bvvvp009. Their upstream repository has
three commits and one contributor; last commit88e458b2771c12624649a1218d261b47f4ddb84c
is March4 2026. Published READMEs and upstream explicitly disclose **unaudited**;
vector equality is not an external cryptographic audit. Physical source counts
include comments and blank lines and exclude dependencies.
Sources: [poseidon registry](https://crates.io/api/v1/crates/poseidon-hash),
[goldilocks registry](https://crates.io/api/v1/crates/goldilocks-crypto), their
`/owners` and version `/download` endpoints, and
[upstream](https://github.com/Bvvvp009/goldilocks-poseidon2).

A [registry zklink search](https://crates.io/api/v1/crates?q=zklink&per_page=100)
returned zero results. Exact names zklink_sdk_signers, zklink-sdk-signers,
zklink-crypto and zklink returned404. The official Rust SDK's signer package is
unpublished; no matching published primitive crate was found. This does not rule
out a differently named publication. Rust-origin is confirmed by the upstream
SDK/wasm binding; the assumption of a published narrow crate is not borne out.
This port reuses its small Apache-2.0 signer glue over the exact crypto fork,
not a fresh curve/Rescue implementation or the provider-heavy whole SDK.

`cargo tree -p safe-cex-crypto -i alloy-primitives --locked` and lock inspection
show one alloy-primitives1.7.3 shared by alloy-dyn-abi1.7.3 and both2.5.0 signers.
All original lock package versions remain present; extra versions belong to new
transitive dependencies. Shared declarations were appended with explicit grant,
without editing any existing workspace dependency entry.

The key/payload/nonce controls were also repeated against the registered SDK for
all five signing schemes (15 separate probes): every probe exited101 at the
corresponding public-key, hash, or signature assertion. All fixture bytes were
restored exactly. The extra Go replay test is unignored and runs from cargo test;
its temporary expected-hash mutation failed through that Rust entry point.

Initial candidate API finding: calling goldilocks-crypto sign_with_nonce on Go's
8-byte raw-message vector returned InvalidMessageLength(8), because that API
expects an already-hashed 40-byte Fp5 value. The port uses Go's explicit
hash-to-Fp5 step and sign_hashed_message; every final vector agrees, including
the original 8-byte case. No expected output was altered to resolve this finding.

MessagePack payload control: changed the two-key raw payload while preserving its
independent expected encoding; SDK test exited101 with MessagePack exact wire
bytes: two-key. Codec has no cryptographic key or nonce parameter. Final
implementation audit approved registration, mappings and the native Go replay.

## Required CI oracle follow-up

Merged P3b `956cca4735` in `38a67762af`, after deleting the independently
copied candleCache roots and pending manifests in `faf890c4cf`. The three roots
and annotated backend manifests now come from that parent, exactly once. The
lockfile resolution retains both parents' pinned packages. Coverage remains
6979: +61 P5 symbols and +17 P3b-owned symbols from the 6901 baseline; the
report remains 263/6979 (backend rows are divergent, not newly ported).

Root `yarn crypto:lighter:check` passes all three actual Go-WASM replay corpora
and all four native Rust Lighter tests. CI invokes this mandatory root script
in the native test arm. The dependency-free workflow audit passes 40 tests,
including negative controls that remove the oracle step or script, append
`|| true` to either, disable its condition, or omit edge/noncanonical/Rust
comparison components. Each mutation is rejected at the intended assertion.
Both workflow path filters include the Go package and Lighter signer sources.
The normal Cargo suite still independently invokes the actual Go replay.

## PR 7215 F1: zero-nonce secret-key recovery

Go `--security` vectors were captured with `wx` in d467398f29 before the fix.
Two fixture keys each have a zero-nonce signature rejected by Go and a valid
nonzero control. The Rust test recovers the complete key as `-s * e.inverse()`
from each zero signature, then requires rejection by the public verifier.
The signer now consumes `SchnorrNonce`, whose private constructor checks a
canonical nonzero scalar; the type has no Clone, Copy, Default or deserializer.
Host code owns fresh secret CSPRNG entropy and handles failure; generation is
not hidden behind a browser/global RNG. Never reconstruct previously used bytes.

Actual controls, all restored: disabling the constructor zero guard fails101
at “zero nonce must never reach signing”; disabling neutral-commitment rejection
fails101 at Go verification parity (Rust true vs Go false). The isolated reviewer
removed the canonical-range guard and failed101 at over-order refusal. Making
the tuple field public makes the privacy compile-fail doctest compile, failing101.
Adding Clone+Copy makes both clone and moved-reuse compile-fail doctests compile,
failing101. No reliance on an inert annotated compiler error code.

43 unit tests, three compile-fail doctests, strict clippy, warnings-denied wasm,
and parity invention/status pass; the native test replays all four Go modes.
An isolated adversarial audit returned MERGE with all guard controls caught.

## PR 7215 remaining findings and nonce hygiene

F2: PR branches/branches-ignore restrictions and removal of the P5a push branch
are rejected by the workflow audit. F3: the Go corpus refuses 4, 9 and 15 bytes
for both hashes, signing and verification; deleting `words`' alignment guard
fails101 at “unaligned hash must not discard trailing bytes”. Required-length
properties prevent deleting these cases and lowering a count.

F4: the root `crypto:oracles:check` runner executes HMAC, all five Go modes,
Ethereum, MessagePack, Ed25519, Stark signing and typed-data, npm zkLink and the
native SDK boundary oracle. CI installs isolated JS packages from the frozen
root lock. Ethereum/MessagePack/zkLink `--capture` against existing fixtures
fails1 with EEXIST; corrupting each fixture fails1 at its default drift assertion.
Changing same-version ethers/MessagePack JS bytes fails1 at the package-file
SHA pin. Every child failure and final git-diff failure is injected in the VM
runner audit; step/script removal, masking, disabling, setup removal and dropping
the frozen-lock option all fail. The wiring suite passes 53 tests.

F6: bc673eb842 commits independent native-SDK boundary fixtures before their
Rust regression. Official SDK 3152525e's public sign_musig/verify_musig produces
0- and 92-byte vectors, with nonzero last byte; first it matches all four prior
36-byte npm-WASM signatures exactly. The npm export surface has no generic
raw-byte signer: these are native SDK vectors, not alleged npm-binary provenance.
A fixed compatibility patch updates only lock-format/time-family dependencies;
SDK crypto sources/pins remain unchanged. Original/compatible lock SHA hashes,
Git revision, tracked-source cleanliness and harness contents are checked before
replay. Upstream warnings are allowed only in this external oracle build;
production crypto clippy and wasm remain warnings-denied. No toolchain installed.
Changing padding to288bits fails101 at the boundary signature; deleting the max
case fails101 at the corpus property. The isolated reviewer changed it to728bits
and deleted the >92 guard: both fail101 at signature/93-byte refusal respectively.
Native fixture --capture is create_new, and default mode compares complete output.

Nonce hygiene: the owned bytes zeroize on Drop via pinned zeroize1.9.1 (already
in the lock). Disabling that write fails101 at a retained-allocation byte check;
the allocation is inspected as initialized u8 memory before manual deallocation.
This helps native and wasm linear memory but cannot erase compiler/dependency
copies or external snapshots. The borrowed host input is NOT automatically
scrubbed: the explicit CLAUDE host contract requires fresh CSPRNG bytes per
signature, no deterministic derivation/retention, error propagation and immediate
host-buffer zeroization on either constructor result. Isolated audits returned
MERGE for all these fixes; scratch mutations were restored.

Primitives-only split: base311678087e, audited production source1fca88c417 unchanged; 46tests,3doctests,134wiring controls and33extractor tests pass. New push-branch removal is rejected with isolated primitives push CI missing. The P5 roots reconcile without client/backend scope. Named CI steps52->56, zero missing. Fresh native SDK preparation and both root oracle scripts pass. [SPLIT_EVIDENCE.md](SPLIT_EVIDENCE.md) separates this result from the independent Lighter trace.

Split reviewer: zero nonce guard removal fails101 at zero nonce must never reach signing; alignment guard removal fails101 at unaligned hash must not discard trailing bytes; restored7lighter tests pass in its detached checkout. Independent e08fca01bb trace compares all268 calls (100hash/60fp5/16two-to-one/92signatures) exactly, with original generator, fixture and source artifacts SHA-pinned. Both root scripts now include live independent regeneration plus Rust comparison.

Final independent-link audit (own checkout): changing the first captured crypto preimage fails101 naming lighter/livenet/create-missing-optionals/poseidon2Hash, with only the comparison test selected, so the failure is not merely the fixture SHA pin. Removing the live independent child fails wiring1 (required calls14 versus15). Restored268 comparisons, original live regeneration and135controls pass. Nonce0x42 is below the scalar order and invariant under Go high-bit masking and byte reversal;92 exact signatures corroborate it. Scoped reviewer returns MERGE.

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

Enum cardinality probe: twelve actual starknet@7.6.4 cases were captured by
the source program at ae1f69d11a before the repair. Extra tuple elements were
already refused downstream in the native encoder; the old zip is not evidence
of a live extra-leaf signing defect. Empty tuple encoding emitted a wrong hash
and now refuses explicitly. SDK-ignored extras remain a CHOSEN refusal.
Removing the empty-tuple production guard compiled and failed runtime101 at
STARK-ENUM empty tuple must refuse unsupported encoding. Restoring unchecked
zip traversal without the exact-count guard compiled and failed runtime101 at
STARK-ENUM declared/native payload count. Simply removing the count guard
while retaining indexing panicked instead; that run is not named-assertion proof.
Both repaired scoped targets restored2/2 after integration a09ea6489b.

Domain identity cardinality: fixed four-case traversal and independent native
count replace an unbounded fixture loop. Bypassing the source-total assertion
and emptying the source compiled and failed runtime101 at DOMAIN-IDENTITY
expected/native index coverage. Adding an extra native output compiled and
failed at DOMAIN-IDENTITY independent native case total. Original fixture
bytes remain unchanged. Empty/subset and consuming-decoder sweep coverage
is not claimed complete.


## Paradex source signing inputs at e1799b07c68935f840ef9127685992de701c32c5

The venue adapter now consumes shared Stark primitives through checked original
address/key ingress. Numeric strings use shared js_bigint before the existing
range guard; failed numeric parsing retains the original shortstring. Reordered
revision-0 domains keep their original authenticated definition/type hash while
a canonical metadata copy constructs the native encoder. Native encoder and
Pedersen APIs perform the composition; no private venue cryptography is added.

At this committed SHA, two-crate all-target cargo check and strict clippy,
the venue signing target, stark_numeric_strings, stark_enum_cardinality and
the filtered existing Stark unit regressions all passed. The new SDK target
pins 108 spelling cases, forty nested/domain/range cases, four original account
boundaries and six distinct domain orders. Twelve actual source caller/account/
key combinations match Account.signMessage. The four shared controls removed
numeric normalization, substituted the domain type hash, substituted domain
field order and accepted exact-prime account input. Each compiled, failed 101
at its required named native assertion, and restored with exit 0. Nine separate
venue controls completed the same protocol. No survivors among thirteen
attempted controls; no compiler-only result contributes to that count.

The existing empty-enum-tuple encoding remains an unimplemented FINDING at this
SHA. It is not changed to CHOSEN merely because other causes are repaired.
The venue gate sheet records exact gate/control names, scope and pre-commit
instrument failures. Evidence is attributed to this SHA, not a later document
or delivery SHA. No full SDK acceptance or full workspace suite is claimed.

## Source-first follow-through after e1799b07c6

The Paradex venue gate sheet records withdrawn and rejected attempts at
62e9c7e45d, d58a6ce5fb, 17b8343df0 and 9c74ed87b6, including precisely which
controls completed and where restore testing did not occur. No partial sequence
is called a complete committed gate sequence.

Additional live SDK witnesses keep their original raw input and independently
pinned inventories: 14 exact empty tuples, 129 partial slots, 33 unused enum
schemas, 401 raw numeric/coercion/recursive cases, 48 selector name/hex cases
plus 24 selector coercions, 110 text/chunk cases, 27 enum-entry cases and 30
original domain revisions. The slot witness permanently pairs three ignored-value
hash identities with three changed hashes when the original authenticated type
definition is canonicalized. Only parser metadata is canonicalized in production.

The source-first native failures and repair commits are recorded in the venue
sheet. Source-side acceptance does not erase the inherited deliberate array
cardinality refusal or the canonical UTF-8 boundary. No universal SDK acceptance,
full workspace, or historical native-control revalidation is claimed.

## Complete shared signing proof at 846f57205fbaea2dad39b63c725107a4da9cede6

The complete fifteen-gate sequence passed at this committed SHA. Twenty-three
shared crypto controls and nine venue controls each compiled, failed 101 at
their required native assertion, and restored with exit 0. The venue gate sheet
names all thirty-two controls and fifteen exits. Earlier partial or rejected
sequences do not contribute to this completed count.

The 986-case source inventory and explicit residual refusals are listed in the
venue sheet; it is scoped evidence, not universal SDK acceptance. Original
caller hashes and authenticated definitions remain distinct from canonical
metadata. The Paradex row now closes the measured unimplemented finding while
retaining its explicit CHOSEN cardinality/schema/resource/text causes. Native
Pedersen, Poseidon, Keccak, field parsing and Stark signing perform the crypto;
there is no venue-local replacement primitive. The source TypeScript remains
unchanged and maintained. Full workspace/crypto/venue suites were not run.

## Lighter provenance repair at e485140dd5be22e238700f8b0619c93837a8f23e

Both `crypto:oracles:check` and `crypto:lighter:check` were blocked by the same
stale whole-file provenance pin. Classification: one documentation/provenance
defect affecting two commands; zero observed crypto behaviour defects in this
corpus. The source change at `dc94458895ce1a436887b7b8f52e6cce9db9c15a`
adds only optional `Market.isPerpetual`. Source history inspection stopped at
iteration 2, `127ab2432e8a5961a31c9c12c2040a68020bf69e`; its blob matches the
original source hash. Independent transpilation of the before/after source
produces identical runtime JavaScript.

The original 92-case fixture and both capture generators remain byte-identical
to merged base `694d5d0733d489d021f6e81be756ca239a77f218` (scoped diff exit 0).
Fresh captures differ only in that source hash. Restoring that metadata leaf
reproduces the original fixture and both original supplement capture hashes.
The separately labeled `reviewed-source-provenance.json` pins exact original
and current source hashes. Live captures must supply the reviewed current hash;
only that leaf is normalized before complete capture comparison. Other sources,
case payloads, primitive results and immutable artifact pins remain enforced.
This later review adds no original independence claim. Native crypto assertions
now compare actual bytes against independently decoded expected bytes.

Controls below started from committed `e485140dd5be22e238700f8b0619c93837a8f23e`;
negative rows describe deliberate temporary mutations, not clean committed trees.
Each mutation was restored before its positive run. No control survived.

| Control and assertion | Negative exit | Restored exit |
| --- | --- | --- |
| Append an unreviewed comment to `types.ts`; direct checker reports `unreviewed independent source drift: packages/safe-cex/src/types.ts` | 1 | 0 |
| Inject captured SDK `marketId` 1→2 in memory; actual checker reports `independent venue oracle drift` | 1 | 0 |
| Flip native `poseidon2_hash` digest byte 0; full `crypto:oracles:check` reaches `lighter/livenet/approve-distinct-fees field-identity "poseidon2Hash"` | 1 (native test 101) | 0 |
| Same native digest mutation; full `crypto:lighter:check` reaches `go_wasm_hash_and_scalar_vectors` / `Go HashNoPad mismatch` | 101 | 0 |

Restored oracle command: exit 0, including five native tests across
`field_identities`, `independent_lighter_field_identity` and
`stark_enum_cardinality`. Restored Lighter command: exit 0, with 8 scoped unit
tests and 2 original plus 3 supplement tests. Original native inventory remains
268 primitive calls over 92 cases, stopping at
`lighter-robinhood/testnet/auth`; supplement inventory is 48 primitive calls
over 24 cases, stopping at
`lighter-robinhood/testnet/approve-field-maxSpotMakerFee`. These are bounded
corpus floors, not complete-function parity or universal input coverage.

Sub-agent review: `account_read_review` identified the normalized-identity
wording distinction, corrected in this pin; `bitget_findings_audit` verified the
exact committed pin and source/runtime equivalence. Neither found a blocking
semantic bypass. The parent ran the command and mutation exits above serially.
No production crypto or parity manifest changed. The existing divergence
classification remains governed by
[the canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

## zkLink ContractBuilder and seed-only signer supplement

The imported SDK capability in `zklink/contract.rs` is adapted from public
zkLinkProtocol/zklink_sdk revision3152525e2615ecbbdb56fd6728ce265ad2dde485.
`tests/fixtures/zklink/contracts.cjs` captures npm1.0.2 WASM signatures before
native implementation and refuses overwriting the new golden. Original fixtures
remain unchanged. Its full-package pin plus JS SHA256 d20354a3ea5f13321312b36a0b6d5fc4e13eed71c3eda3ba1b2404c8b7db560e
and WASM SHA256 4fc3cd426eef58022fb7228a8fcd6e268f6f243af36c848a55cabcbd1ab96c7e
are enforced live. The pinned source revision and published binary are separate
references; this does not claim the binary was built from that revision.

The self-authored supplement has24 contracts: eleven independently changed
fields, nonzero amount/price, both directions, distinct fees, subsidy, high bits
for slot/nonce/pair truncation, zero/max/rounded sizes,120-bit price maximum,
BigUint decimal spellings, and four different recovery bytes. It also compares
18 amount/fee rounding answers through the actual exported npm functions.
Every contract runs through the actual TypeScript `signContract` helper with
the real SDK too; the harness supplies imports and transpiles its original
assertions/body. Native signatures, public keys, full signed records and wire
property order match. Raw JSON tests include numeric overflow tokens and
explicit native range refusals. These bounds are described in CLAUDE.md; no
identical panic/error claim or other-transaction capability is implied.

Measured gates on this change:
- `cargo test -p safe-cex-crypto --test zklink_contract --jobs1`:5 pass.
- Existing `cargo test -p safe-cex-crypto --lib zklink --jobs1`:6 pass.
- Strict all-target host Clippy and wasm32 library Clippy pass.
- `scripts/zklink-contract-controls.py`: maker/taker serialization swap,
  always-next-exponent packing, and recovery-byte replacement each compile and
  fail their exact named test/assertion (exit101); restoration passes (exit0).
- Global parity:0 findings (7459 rows/17054 public items). Imported SDK roots
  are outside TS extraction; each new public primitive has its own explicit
  exemption, with venue transaction bodies still mapped separately.
- Read-only extraction:0 added/0 orphaned symbols; only the existing shared
  base.ts and safeCexAxiosCreate.ts metadata drift remains. No owned drift.

The signer owns zeroizing65-byte seed storage without Debug, Serialize, Clone
or export. Decoding failures drop and erase partial storage. Caller-owned
strings and scalar temporaries in the existing signing primitive are outside
this erasure guarantee. SDK Apache-2.0 notice is retained. No dependency, root
script, workflow, deployment or TypeScript production file changes.

## Aster V3 source pin after V1 retry integration

Reviewed TypeScript `8fab4380ad15..9db593a23388`: surrounding imports and V1
signature-rejection retry handling changed. AST-selected buildV3SigningPayload
and generateV3Signature declarations are byte-identical (SHA256 b68630d0de74f356d8ffa2a0c7aa095968aeb37eebda726c33fc6ab5fccd706f and b25c339f37d1fed250ff7735f0ad3fd0f1304a207d4013ac31027b5ed395841f).
The unchanged live oracle produces the same four complete case records, including
ABI preimages, hashes and signatures. Only its whole-file source pin changes from
5ade2118df55617de0f4443bc213b8e10bf9dee5058eca1297e2b3e0d23d9ed6 to
80a7fd744fc6ff268685fbff008e9312306705017ada6adb29bede875c9468be.
No fixture input, output or comparison is regenerated or weakened.

The old pin fails the live Node oracle with exit1; the reviewed pin passes.
Scoped native `ethereum::tests::actual_typescript_aster_signing_preimages` passes
all four records. Swapping the two address words in the production ABI encoder
compiles and fails101 at `live TS ABI preimage "complete"`; exact source restoration
passes again. The live Node oracle also passes after restoration. Logs are
`/tmp/ydwdec-aster-pin-{before,after,native,abi-control,abi-restored}.log`.
