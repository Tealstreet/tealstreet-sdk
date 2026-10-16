# Audited primitives split

This branch starts at integration `311678087ec7fc7272874ad79505742173af2e54`.
The implementation and original oracle files are copied byte-for-byte from
`1fca88c417e65e1f843cb73aa37806703c981c00`, whose reported P5 findings are closed.
The fixtures are committed first at `f57694f8fb`; they are not recaptured to fit Rust.
The unaudited-crate ruling and nonce host contract remain in [CLAUDE.md](CLAUDE.md).

Included: Poseidon2/Goldilocks, ECgFp5 Schnorr, EIP-712/secp256k1, JavaScript
MessagePack, Ed25519, Starknet and zkLink primitives, exact pins, raw-string
regressions, original oracle evidence and mandatory CI oracle replay.

Excluded: P3b runtime backends and their source roots, Hyperliquid API/exchange/WS,
any other venue adapter, Lighter TransactionClient/key-management implementation,
Go/JS loaders, Coinbase JWT and production entropy generation. Source manifest
rows are primitive projections; they do not claim those complete adapters or SDKs.
The Go cryptography has no TS manifest representation: its evidence is the harness.

The split adds 61 scoped TS symbols to integration's 6,904, producing 6,965.
The former 17-row candle-backend scope is absent. The current branch report is
275/6965; it is a branch measurement, not a claim that the venue work landed.
The historical 6,901 denominator predates three exported mutable bindings.

Validation: 48 crate tests, three nonce ownership doctests, plain workspace-strict
Clippy, warnings-denied wasm, fmt, layers, extractor check and 33 extractor tests,
parity invention/status/report/progress, and 135 CI wiring controls pass.
Both root crypto scripts replay successfully, including a newly prepared pinned
native SDK checkout. The SDK compatibility patch changes only lock/time-family
compatibility; its crypto source and pins remain unchanged. Only that external
oracle permits upstream warning noise; production crypto remains strict.

CI named-step occurrences are 52 on integration and 56 on the split, zero missing.
Both live oracle steps and both isolated dependency/native-SDK setup steps survive.
The split's own push branch is pinned with a removal control. Prior scheme,
security, artifact, generator and gate mutations are recorded in
[GATE_EVIDENCE.md](GATE_EVIDENCE.md); the split audit reruns controls separately.

Independent venue-agent oracle: `e08fca01bb` on feat/rust-venue-lighter,
original `tests/fixtures/signing-preimages.json` copied without edits at `1b53e9442f`.
Its 92 cases / 268 calls across four deployment profiles all agree: 100 Poseidon2,
60 Fp5, 16 two-to-one and 92 exact Schnorr signatures. Rust also verifies every
signature. The unmodified original generator regenerates the exact artifact;
source SHA256s, original generator SHA256 and WASM SHA256 are asserted. Package
versions 0.0.1 are explicitly insufficient pins. Both root crypto gates replay
this comparison in CI; removing the independent child fails the runner audit.

Fixture SHA256: `afe399cc424ff3e4e5393ef9c2f8f642b770530328f6cc70d6aefd830383679a`.
Generator SHA256: `5cd5d1a8e266eb30a12262c3abb1dd0d28dd6b901e20a37bf99712b6b18f5eb2`.
WASM SHA256: `4fcb6e608498ddf223ee9d0fadb00ab8dd3736f985ab37eec22a195588a029c3`.

The fixture's repeated entropy is deliberately insecure test-only input under
its public offline key. It is never a production nonce generator or host example.
The host CSPRNG and zeroization contract remains mandatory.

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
