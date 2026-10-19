# Lighter gate evidence

The standalone SDK key-management fixtures below are historical after the
unused SignerError/TransactionSigner cleanup. Shipped signer composition remains
TransactionClient; see `../safe-cex-lighter-sdk/SIGNER_RETIREMENT_REVIEW.md`.

Protocol data slice, based on `6922766a6c`. All commands run in this worktree.
Cargo uses `/opt/homebrew/opt/rustup/bin` on PATH; Yarn uses
`node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`.

- Live `lighter_protocol_tables_match_live_typescript`: passed. Rust-only
  `RECV_WINDOW` 20000 → 20001 failed by name, exit 101; restored. TS-only
  mutation of the same value failed by name, exit 101; restored and passed.
- Scoped fmt passed; broken enum indentation failed, exit 1; restored.
- Scoped all-target strict Clippy passed; unused `lighter_unused_negative_control`
  failed, exit 101; restored.
- Scoped wasm build passed; wasm-only `compile_error!` failed by name, exit 101;
  restored.
- Invention/status passed (6901 rows at this base). Changing the mapped
  `RECV_WINDOW` row back to pending failed both status and invention, exit 1;
  restored.
- CI/layer audit: 51 tests passed, including removed, masked, and skipped
  Lighter oracle step controls and the exact root script pin.
- `parity:report` and `parity:progress` ran successfully. Protocol slice:
  323 pending / 13 ported / 0 divergent.

The independent reviewer reproduced two missing negative controls: additional
Rust enum variants and narrowing the chain ID to f32 passed the initial oracle.
The strengthened test exhaustively matches both Rust enums and checks their
serialized inventories against live TS unions; profile roundtrips now include
fractional, negative, f32 precision-boundary, and large finite numbers.

Independent review verdict: MERGE after fixes. Reviewer mutated both enum
inventories (named compile failures, exit 101), narrowed chain ID to f32
(`number domain.signerChainId`, exit 101), and changed the live TS Robinhood
chain ID (`LIGHTER_DEPLOYMENT_PROFILES.robinhood.signerChainId`, exit 101).
Restored oracle and 51-test CI audit passed in the isolated checkout.

Coverage passed after a worktree-local frozen dependency install (no build scripts).
Adding `LIGHTER_COVERAGE_NEGATIVE_CONTROL` to TS failed by name at exit 1;
source restored. The initial borrowed dependency tree lacked smol-toml and was
removed before the local install; nothing was installed in the main checkout.

Deployment, OHLCV, and public-parser slice: 308 pending / 28 ported / 0 divergent.
Independent Rust and TS mutations of recovery `>=` to `>` failed
`shouldAttemptLighterOhlcvRecovery` at exit 101, then restored pass. Review found
that finite-only fixtures missed NaN-sensitive inversion and JSON erased -0;
tagged special-number input now covers NaN, infinities, and signed zero.
The review's `>=` to `!(<)` mutation then failed by name at exit 101.
10,000 deterministic parser strings and underflow/rounding extremes passed in the
review checkout. A shadowed bestPrice fixture was replaced with sole-key and
conflicting-key cases, pinning every fallback and its precedence.

Pure-slice final isolated verdict: MERGE. Removing bestPrice now fails by
`parseLighterWsBookSidePrice` at exit 101. Scoped native tests, strict all-target
Clippy, fmt, wasm, and invention/status all pass after restoration.

Merged shared numeric primitive commit `a26c0ac381` without whole-side conflict
resolution. Named workflow steps: local 55, incoming 55, resolved 56; union of
both sets retained with no removals. Both js_number_string provenance and the new
number_from_string exemption retained. CI/layer audit passed 53 tests; external
number_from_string reachability test, both Lighter live oracles, and parity
invention/status passed (6901 rows at this merge).

Independent signing capture (no Rust signing parity claim): 92 offline cases across
all four deployments, recording outer signer inputs, actual TransactionClient
inputs/results, real WASM preimages/results, and API serialization. Every signature
is verified by the real WASM. Fixed test key, time, entropy, source hashes and WASM
SHA256 are in the fixture header. Both packages report 0.0.1; hashes are the pins.
Fixture preimage mutation failed `lighter/livenet/create-missing-optionals/crypto`
at exit 101; separately mutating live SDK chain ID failed the same case's
`sdkResults` at exit 101. Restored captures pass. Isolated sub-agent review of the
expanded 92-case capture: MERGE, no blocking findings. Counts remain
308 pending / 28 ported / 0 divergent.

Utility slice: getPrecision, dropPrefix, addPrefix, clampDecimals; 304 pending /
32 ported / 0 divergent. Independent Rust and TS precision mutations each fail
getPrecision by name at exit 101. Review exposed a missing ordinary trailing
whitespace case; space/tab/newline now pin the preserved TS digit-counting bug,
and independent trimming mutations fail at 101. Final isolated verdict: MERGE.
All live venue tests, strict Clippy, fmt, wasm, parity check/report/progress pass;
extractor coverage: 0 updated, 0 added, 0 orphaned.

Review also found decimalsFromScale CANNOT use native f64::log10: expanding
power-of-ten neighbours from +/-1 ULP to +/-256 ULP exposed rounding differences.
Named witnesses: 100.00000000000006 -> TS 2 / Rust None;
9.999999999999672e-254 -> TS -253 / Rust None. Implementation removed and row
left pending awaiting shared V8-compatible Math.log10; numeric seam routed.

Close classification slice: 298 pending / 38 ported / 0 divergent. Two predicate
constants preserve non-Unicode JS regex case folding, ASCII word boundaries, and
JS whitespace (including BOM, excluding NEL); required bool fields are schema
checked against live TS. All code/early-state axes and auth-policy precedence
pass. Rust and TS policy mutations fail LIGHTER_POLICY_REASON_RE at exit 101;
reviewer separately mutated auth suppression on both sides, each failing
classifyLighterPrivateClose at 101. Restored oracle passes. Isolated MERGE, no
blocking findings. Scoped strict Clippy, fmt, wasm and parity check pass.

Builder-fee slice: 290 pending / 46 ported / 0 divergent. Five constants, mode
union, exact-static selection and deployment default routing match maintained TS.
Independent Rust and TS taker-fee mutations fail LIGHTER_INTEGRATOR_TAKER_FEE_VALUE
at exit 101. Review found serde alias broadening bypassed output-only tests;
accepted deserialize_enum metadata now matches live TS, plus invalid decoding
cases. This also strengthens existing deployment/quote unions. Reviewer STATIC
alias fails at 101; independent TS resolveMode mutation fails by name at 101.
Restored gates pass; isolated verdict MERGE. Scoped strict Clippy, fmt, wasm and
parity checks pass. BigInt conversion/config remains pending.

Account-tier data/predicate slice: 283 pending / 53 ported / 0 divergent. Tier
accepted enum inventory, all messages, object/array record acceptance, eligibility,
and finite nonzero fees match live TS. Rust and TS finite-fee guard removals each
fail hasNonZeroLighterIntegratorFee at exit 101; restored pass. Reviewer independently
removed array acceptance on both sides: asRecord fails by name at 101. Isolated
MERGE, no blocking findings. Five live oracles, strict Clippy, fmt, wasm and parity
check pass; coverage 0 updated/added/orphaned. Four String(any)-dependent rows
remain pending; no private coercion implementation is included.

Accepted-input audit: all four literal enums used serde's default unit decoder,
which also accepts tagged objects. The new rejection check reproduced this with
{"dynamic":null}, exit 101. Decoders now require String before private serde
variant decoding. Inventory still comes from serde metadata (unknown_variant on
a sentinel after the string gate), so arbitrary wire aliases remain detectable.
Every live TS literal is tested wrapped in an object; arrays/null/numbers/bools
are also rejected. Reviewer removed string gate and injected a private-wire alias;
both controls fail at 101, restored type/builder/tier oracles pass. Isolated MERGE.
Five live oracles, strict Clippy, fmt, wasm, parity and 54-test CI audit pass.
This corrects Rust input widening; no TS behavior or manifest status changed.

Follow-on accepted-set audit reproduced positional-array acceptance in all six
object records (deployment profile, both close classifications, cancel target,
minimums, approval fee caps). All six rejection probes failed at 101. Public
decoders now require maps before private derived wire structs. Required fields,
null rejection and binary64 domains remain live-schema checked. Enum tests also
call concrete Type::deserialize, so a permissive public inherent decoder cannot
hide behind a strict trait path. Reviewer bypassed a map gate and separately
injected a public inherent enum decoder: each failed a named test at 101.
Restored live gates pass; isolated MERGE. No alternative public From/TryFrom
bypass found. Decoder corrections change no parity counts.

Approval/data slice: 276 pending / 60 ported / 0 divergent. Cancel/minimum/fee-cap
records preserve required non-null fields and binary64 values, while rejecting
positional arrays. Approval constants match every live table key/value. Fee
predicate covers all 8^4 finite/nonfinite/signed-zero combinations. URL helper
preserves deployment/proxy lookup, JS trim and single trailing-slash removal.
Reviewer independently replaced single removal with remove-all in Rust and TS:
`approval getBaseUrl` failed at 101 both times; restored pass. Isolated MERGE
after map-only corrections. Native tests, live oracles, strict Clippy, fmt, wasm
and parity check pass.

Market-symbol parser: 275 pending / 61 ported / 0 divergent. Live TS AST includes
its actual quote constants; corpus derives suffix probes from every known quote,
plus slash precedence, repeated slashes, empty fallback, case, BOM and astral
inputs. Independent reviewer removed USDG from Rust and TS separately; both failed
parseLighterMarketSymbol at exit 101. Restored pass; isolated MERGE. Full native
and live tests, strict Clippy, fmt, wasm, parity check pass.

Merged shared coercion branch 5cf6b4d8d8. Named CI steps: local parent 56,
incoming parent 55, resolved 56; exact union with no removals from either parent.
No conflicts required resolution. All six Lighter live oracles plus three native
record-shape regressions pass; shared p4_coercion five tests pass (distinct Number,
parseFloat, parseInt, fallible recursive String and real bitget wrapper corpora).
CI/layer audit 54/54 and parity invention/status pass. Counts unchanged:
275 pending / 61 ported / 0 divergent.

Tier coercion slice: 271 pending / 65 ported / 0 divergent. Four helpers use
shared fallible js_string with venue nullish defaults. Shared parse_float replaces
the local prefix scanner. Isolated reviewer: MERGE; independent Rust and TS
eager-message-coercion mutations each fail isLighterPartnerAttributionTierRejection
at exit 101, protecting early-return error timing. Restored tier and pure oracles
pass. The preceding shared-coercion merge also received isolated MERGE.

Utility and builder-fee coercion slice: 261 pending / 75 ported / 0 divergent.
Seven utility rows and three builder-fee rows cleared isolated adversarial review.
Rust and TS truncation-to-floor mutations fail parseLighterApiKeyIndex at 101;
Rust and TS trim-removal mutations fail parsePositiveIntegerBigInt at 101. All
restored. Corpus covers BOM/NEL, throwing String coercions, prefix numeric parsing,
negative zero, nullish field precedence, input cancellation order/duplicates, and
prototype keys. BigInt guard and 400-digit values are checked before direct
arbitrary-precision parsing. Config uses signed BigInt with no JSON decoder;
negative BigInts and full binary64 fee fields have a direct construction test.
Seven live oracles and native tests pass, strict Clippy and wasm build pass.

Market helpers: 257 pending / 79 ported / 0 divergent. Four pure helpers use
shared fallible String/parseFloat; live AST oracle retains constants, error timing,
nullish precedence, ratio boundaries, signed zero and subnormal-to-infinity.
Independent Rust and TS early-status-return mutations fail isLighterPerpMarket
at exit 101; restored pass. Isolated MERGE. Clippy/fmt and parity check pass;
coverage reconciliation reports zero changed, added or orphaned rows.

Signer request types: 251 pending / 85 ported / 0 divergent. Six rows retain
signed arbitrary BigInt, binary64 values and optional non-null domains. Closed
string unions have private wire decoders and metadata inventories; numeric
grouping accepts exactly 1/2/3, while checked margin mode preserves IEEE -0.
BigInt records expose no JSON codec; signed/wire projections remain unclaimed.
Review found a false-green decoding 2 to grouping variant 1; decoded roundtrips
now fail that mutation by name at 101 on both numeric decoder paths. Independent
TS marketId number-to-string mutation fails named field schema at 101. Restored
gate passes; isolated MERGE. Strict Clippy and all native/live tests pass.

Panic-expression audit: no venue-input-triggerable panic reproduced in the six
remaining unwrap sites. Removed all six guarded expressions using exhaustive
profile-array destructuring, matched string splits and serialization-error
propagation. Production source has no unwrap/expect/assert/panic expressions.
Isolated reviewer MERGE; swapped profile bindings fail named approval getBaseUrl
at 101, then restored pure/exchange helper gates pass. Counts unchanged.

Approval domain slice: 247 pending / 89 ported / 0 divergent. Context, check
result and submission result retain every live TS field/domain; closed approval
status uses private serde metadata and string-only concrete decoder checks.
Default integrator helper reuses equivalent guarded constants/parser and preserves
exact Error text. Isolated MERGE: independent Rust error-text and TS context
field-type mutations fail their named cases at exit 101, restored oracles pass.
Scoped Clippy and parity check pass with cargo concurrency capped at two jobs.

Signer pure helpers: 245 pending / 91 ported / 0 divergent. Raw provider values
remain unchecked until caller validation (TS defect marked). Nonce classifier
coerces both code and message before OR; shared TypeErrors propagate. Independent
Rust and TS early-return mutations fail isLighterInvalidNonceError at exit 101,
then restored live oracle passes. Isolated MERGE; strict Clippy/parity check pass.

Integration merge 7fcffd57e8: local parent 6d36c850f3 has 56 named CI steps;
incoming has 52; resolved has 56, the exact multiset union with no removals.
Six conflicts resolved per hunk: branch triggers and CI assertions unioned,
Lighter Cargo dependencies retained, exemptions unioned, and shared numeric
formatter delegation retained. Scoped native/live Lighter tests pass (10 live
oracles plus 4 native tests); CI/layer audits pass 184 assertions. Invention/
status pass at 6906 rows; coverage shows zero changed/added/orphaned. Scoped
Clippy, fmt and wasm build pass. Isolated reviewer MERGE after formatting cleanup.
The known HOW/tsx CI installation gap is owned separately; no skip added.
Counts unchanged: 245 pending / 91 ported / 0 divergent. Oracle as-const sweep
found runtime-value or explicit-AST comparisons only, no widened assignability
checks; existing independent Rust/TS controls remain mandatory.

L1 signature helper URL: 244 pending / 92 ported / 0 divergent. Exact
URLSearchParams pair order, form encoding, raw origin and optional wallet handling
match 480 live TS cases; parameter schema covers all 13 fields. Isolated reviewer
MERGE after independent Rust and TS wallet-trimming controls each failed the
named createLighterL1SignatureHelperUrl case at exit 101. Restored live pure gate
passes with two Cargo jobs.

Client ID and BigInt formatting: 242 pending / 94 ported / 0 divergent.
Runtime clock/random samples are injected in source order; signed BigInts retain
arbitrary precision. Live TS cases preserve the source's empty zero suffix and
short exponent suffix defect. Independent Rust and TS suffix-length mutations
fail generateLighterClientId at 101, then restored live pure oracle passes.
Isolated reviewer MERGE; scoped strict Clippy and parity invention/status pass
(6906 rows, 1181 public Rust items). Runtime comparisons do not depend on narrow
TypeScript inferred tuple types.

Shared update merges: utility parent 3bbc52657f preserves the exact named CI
union 56 local / 55 incoming / 56 resolved, and integration dfa2ff2318 preserves
56 local / 52 incoming / 56 resolved. Both automatic merges independently
reviewed MERGE, exemptions and dependencies retained. All 10 Lighter live
oracles plus 4 native tests pass after shared strict decoder integration. The
transport raw_parser tests pass, including the correction control at 101; missing
TypeScript compiler fails loudly. Canonical manifest spec is now committed at
parity/MANIFEST-SPEC.md. Counts remain 242 pending / 94 ported / 0 divergent.

Four stateless error classifiers: 238 pending / 98 ported / 0 divergent.
Actual TS member initializers are evaluated with their account-tier dependencies.
Reviewer found nested data.code/error.code precedence swap falsely passed;
72 conflict cases now cover empty/false/zero, both matched codes and throwing
coercions. The same TS swap now fails named TierOrderError at 101; independent
Rust fallback mutation fails named ApprovalMissingOrderError at 101. Restored
scoped live oracle passes; isolated MERGE. No exchange lifecycle claim.

Core ABI merge ed304ce751: 238 pending / 98 ported / 0 divergent unchanged.
Four conflict files resolved per hunk; exact CI union 56 local / 55 incoming /
56 resolved, no exemptions removed. Retained transport heartbeat witness now
uses the incoming test-only leaf_error projection for its leaf-provenance
assertion. All Lighter live/native tests pass. Isolated MERGE: heartbeat, REST
failure compile controls and live axios-retry oracle pass. No lossy REST wrapper.

Price candidate and native-batch predicate: 236 pending / 100 ported / 0 divergent.
Live member bodies preserve first positive finite price and the length, symbol,
limit-type, reduce-only, missing stop/take-profit and positive finite price guards.
Independent Rust and TS mutations treating stopLoss -0 as absent fail by name at
101; restored oracle passes, isolated MERGE. Scoped Clippy and parity checks pass
(6906 rows, 1210 Rust items); coverage reports 0 changed/added/orphaned manifests.

Leverage tier and initial tick helpers: 234 pending / 102 ported / 0 divergent.
The live static tier table drives all tier and midpoint neighborhoods, plus
nonfinite and generated inputs; the cap is loaded from source and interval
lookup count is pinned to one. Independent Rust and TS strict-tie mutations
(< to <=) fail snapToLeverageTier by name at 101; restored gate passes. Isolated
MERGE, scoped Clippy and parity pass (6906 rows, 1212 public Rust items).

Margin helpers: 231 pending / 105 ported / 0 divergent. Strict numeric and
string branches, non-trimmed coercion, defaults and positive-zero results match
the live member bodies. Reuse the checked signer numeric domain; new canonical
literal constructors map to its existing type row and do not widen decoding.
Independent Rust and TS negative-zero mutations fail by name at 101; restored
exchange-helper and signer-type oracles pass. Isolated MERGE; scoped Clippy and
parity pass (6906 rows, 1217 Rust items). Nonfinite JSON tags do not prove native
serde Value numeric coverage; the unknown-value boundary remains JSON-shaped.

Numeric integration merge 7738215fb0 plus canonical math registrations copied
from a6f317fbc9: exact CI union 56 local / 55 incoming / 56 resolved; no lost
exemptions. Identical duplicate LoadedChunks entry deduplicated. Crate dependencies
and layer policy unchanged; layer audit passes at 37 classified crates. All
Lighter live/native tests compile and pass. Review first rejected two missing
math registrations; after importing their exact canonical entries, parity passes
and isolated reviewer returns MERGE. Committed counts remain 231/105/0.

Precision metadata helpers: 227 pending / 109 ported / 0 divergent. Shared
fallible Number(any) retains source key and throw order; shared math_log10
retains the measured Node profile. Live corpus probes +/-256 ULP around every
decimal power, including both original platform-libm witnesses. Independent
Rust platform-log10 and TS allowZero mutations fail named cases at exit 101;
restored live oracle passes. Isolated reviewer: MERGE. Scoped Clippy passes.

Approval numeric coercion: 226 pending / 110 ported / 0 divergent. Shared
Number(any) preserves negative zero and propagates TypeError; only nonfinite
results become None. Live source corpus covers null/undefined, radix and partial
strings, arrays and throwing objects. Independent Rust and TS Number-to-
parseFloat substitutions fail toNumber by name at 101; restored gate passes.
Isolated reviewer: MERGE. Scoped Clippy and parity pass (6906 rows, 1241 items).

BigInt integration merge bf9e30f7c1: CI exact union 56 local / 55 incoming /
56 resolved. All exemption targets retained; identical LoadedChunks duplicate
removed and existing local js_number_string explanation retained. Dependencies
intact, layers unchanged, layer audit passes (37 crates). All Lighter tests
including ten live oracles pass. Isolated review: MERGE; parity 6906 rows /
1263 Rust items and CI audit 106 tests pass. Counts remain 226/110/0.

BigInt helpers: 223 pending / 113 ported / 0 divergent. stringToBigInt,
approval toBigInt and toHex10 use shared conversions, preserving String-first
catch behavior versus direct numeric conversion and uint64 Error checks. Live
corpus compares values and error names/messages/UTF-16 units. Independent Rust
Number-intermediary, direct boolean conversion, and ten-digit padding controls
fail named assertions at 101. Permanent child TS mutations cover all three; a
no-op mutation makes the control itself fail at 101. Restored three-test gate,
Clippy and parity (6906 rows / 1268 items) pass; isolated reviewer: MERGE.

Node isolation evidence: all ten scripts ran in a disposable tracked-source
mirror under /private/tmp with no ancestor node_modules, using ci-deps.cjs and
a copied frozen root lockfile. Each script failed MODULE_NOT_FOUND under a
missing compiler-path control; this sweep is now a maintained test. Only the
compiler plus Node builtins and repository sources execute, so no runtime npm
selectors were added. Mandatory Lighter gate is in types after root install;
isolation proves independence of that install, not a reproduced Lighter CI red.
Updated BigInt corpus also passed in the mirror; mirror deleted afterwards.

Scaled integers: 222 pending / 114 ported / 0 divergent. Both source guards,
clamping and shared math_pow/Math.round/BigInt preserve overflow error order.
Live corpus includes every exponent 0..30, negative half-neighbors, subnormals
and product overflow. Rust round-away-from-zero mutation fails -0.5 by name at
101; maintained TS round-to-trunc control fails by name at 101. Restored tests,
Clippy and parity (6906 rows / 1269 items) pass; isolated reviewer: MERGE.

Approval L1 message builder: 221 pending / 115 ported / 0 divergent. Required
bigint/number schema, exact labels/newlines and evaluation order match live TS.
Every field and conflicting invalid-field pairs are covered. Independent Rust
account-before-nonce mutation fails by name at 101; permanent TS label mutation
fails by name at 101. Restored three-test gate and parity (6906 / 1271 items)
pass; isolated reviewer: MERGE. No signing integration claimed.

Funding normalization: 220 pending / 116 ported / 0 divergent. Exact hourly
floor and multiplication-before-division match live TS over 2527 binary64
cases. Rust algebraic folding to multiply by 0.08 fails the overflow witness
by name at 101; permanent TS floor mutation fails by name at 101. Restored
three-test gate, Clippy and parity (6906 / 1272 items) pass. Isolated reviewer:
MERGE. Public websocket class remains pending.

Account parsing/address helpers: 218 pending / 118 ported / 0 divergent.
Account conversion delegates the identical source approval body; independent
member oracle still catches source drift. Address extraction retains JS trim,
case-sensitive unanchored ASCII matching and first candidate. Rust direct-
BigInt and uppercase-prefix controls fail named assertions at 101; maintained
TS controls do too. Restored two-test gate, Clippy, parity (6906 / 1274 items)
and coverage (0 changed, added, orphaned) pass. Isolated reviewer: MERGE.

Reduce-only/minimum helpers: 214 pending / 122 ported / 0 divergent. Both
side unions, explicit position snapshots, strict-positive contracts and source
minimum comparisons match the member oracle. Minimum messages retain base-before-
quote order and JS spelling. Rust >= boundary mutations fail named assertions
at 101; permanent TS controls cover all four helpers. Restored two-test gate,
Clippy and parity (6906 / 1278 items) pass; isolated reviewer: MERGE. Runtime
default-store selection remains pending and is explicit in the row note.

Rate-limit classifier: 213 pending / 123 ported / 0 divergent. Shared Number
then String preserve nullish status/code precedence and pre-match error order.
Rust early-return on 429 fails by name at 101 for throwing code; maintained TS
Number-to-parseFloat control fails by name. Restored two-test gate, Clippy and
parity (6906 / 1279 items) pass. Isolated reviewer: MERGE.

Stop-slippage fallback: 212 pending / 124 ported / 0 divergent. Shared
normalizer retains order value, options-extra fallback, then default 1. Zero
is accepted and unused fallback coercion stays unevaluated. Rust eager fallback
mutation fails explicit -0 plus throwing fallback at 101; maintained TS falsy
fallback mutation fails by name. Restored two-test gate, Clippy and parity
(6906 / 1280 items) pass. Isolated reviewer: MERGE.

Configured margin and rejection message: 210 pending / 126 ported / 0
divergent. Four alias fields use source nullish priority before parsing, so
invalid non-null values mask later candidates. Independent Rust priority and
message-label swaps fail named assertions at 101; permanent TS controls do too.
Restored two-test gate, Clippy and parity (6906 / 1282 items) pass. Isolated
reviewer: MERGE. Runtime store integration remains pending.

OHLCV fallback key: 209 pending / 127 ported / 0 divergent. Exhaustive
shared Timeframe mapping matches every live TS alias and retains raw symbol
text. Rust M1-to-3m and permanent TS delimiter mutations fail by name at 101;
restored two-test gate, Clippy and parity (6906 / 1283 items) pass. Isolated
reviewer: MERGE.

Integration extrema/collation merge 35e1b03e7f: exact CI union 56 local /
62 incoming / 63 resolved. All 246 exemptions unique, nonblank and neither
parent loses targets. Venue/core dependencies retained, layer policy unchanged,
layer audit passes (37 crates). All Lighter tests including live controls pass.
Isolated reviewer: MERGE; parity 6906 / 1288 items and CI audit 116 tests pass.
Committed venue count remains 209/127/0.

Backoff policy: 206 pending / 130 ported / 0 divergent. All three remaining
policy functions use shared math_min/math_max/math_pow. Native and source
parameter schemas, overrides, NaN/signed zero and rate-limit flags are pinned.
Rust std-min, powf and private OR-to-AND controls fail by name at 101; permanent
TS controls cover all three functions. Restored three-test gate, Clippy and
parity (6906 / 1292 items) pass. Isolated reviewer: MERGE. Shared pow platform
profile limits are explicit; no venue-local numeric shim remains.

## Reduce-only limit inference

Ported the pure inferStopLossTypeFromReduceOnlyLimit body with an explicit ticker
lookup result; runtime store dispatch remains pending. Live comparison covers all
order types/sides, reduce-only flags, nonfinite and nonpositive prices, ticker
fallback order and inclusive ties. Scoped live_exchange_helpers tests passed.
Isolated sub-agent verdict MERGE: replacing inclusive comparisons with strict
comparisons fails the helper by name at exit 101; restored tests and permanent
TypeScript strict-tie control pass. Counts: 205 pending / 131 ported / 0 divergent,
before the ongoing shared-string representation audit.

## BigInt diagnostic UTF-16 witness

A valid UTF-8 input (999 ASCII x characters followed by an astral emoji) makes
TypeScript BigInt truncate its SyntaxError diagnostic through the surrogate pair.
The oracle transports exact UTF-16 units and compares shared message_utf16; the
convenience display field is checked separately against its documented lossy view.
See sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary. This does not widen
Rust ingress: the input is scalar Unicode and the error field already carries u16.
The isolated reviewer substituted display.encode_utf16 and obtained a named
stringToBigInt equality failure at exit 101. Its first review caught an assertion
ordering issue masking the existing TS mutation control; repaired and all three
live_pure tests pass. Follow-up verdict MERGE. No parity-count change: 205/131/0;
input-representation classification audit remains in progress.

## Integration a99f1fffe1: canonical string boundary and Date primitives

Merge preserves exact named CI-step union: 63 local / 63 incoming / 64 resolved.
All 249 exemptions have unique targets and nonblank reasons; neither parent loses
a target. Lighter Cargo dependencies and layer policy remain unchanged. Layer
audit: 37 classified crates. All scoped Lighter targets including ignored live
oracles pass; CI wiring 121 tests pass. Initial layer/wiring attempts lacked Cargo
in PATH (ENOENT); both rerun end to end with the required toolchain path.
Isolated reviewer MERGE, independently parity-green at 6906 rows / 1296 public
items, canonical spec and incoming Date files retained. Counts unchanged 205/131/0.

## First shared-string boundary classification batch

Reclassified dropPrefix, addPrefix, stringToBigInt and parseLighterApiKeyIndex as
CHOSEN with the canonical shared-utf8-string-boundary citation. The maintained
44-case live corpus retains double-escaped valid JSON ingress and TS outputs for
lone high/low surrogates, while scalar Unicode, astral pairs, BOM and NEL remain
positive Rust comparisons. Refusal is a representation boundary, not invalid JSON.
Isolated reviewer replaced refusal with replacement-character ingress: named
 dropPrefix failure at exit 101. Restored all three live_pure tests pass; MERGE.
Counts: 205 pending / 127 ported / 4 divergent. Remaining string-bearing rows are
still being audited; this batch is not a claim that their domains are complete.

## Pending mapOrder inherited table witnesses

Census: 13 syntactic protocol/profile/local-table reads: four closed deployment
profile reads, one closed-Timeframe INTERVAL read with a negated guard, and eight
order-table reads. Six named order-table reads uppercase keys and are immune;
two raw numeric-code fallback tables retain all 12 Object.prototype own names.
All order reads except the fetchTrades side return feed truthiness or ||.
Actual AST-extracted mapOrder result expressions preserve inherited truthiness,
identity and ownership before JSON can erase functions; __proto__ returns the
VM Object.prototype itself, the other eleven names return their exact functions.
Row remains pending with ts-bug marker; shared TableHit/output handling is awaited.
Initial review found member identity was inferred from input: a wrong own function
passed. Repaired with actual reference and own-key checks plus exact ordinary
values. Both own-only lookup and wrong-own-function controls now fail named cases
at exit 101. Isolated review MERGE; three scoped table tests pass, including missing
compiler refusal. No parity-count change (205 pending /127 ported /4 divergent).

## Five additional utility string boundaries

Reclassified getPrecision, toLighterApiKeyIndexString,
parseLighterPositiveIntegerString, isLighterL1Address and normalizeNonNegativeNumber
with the canonical shared boundary citation. The maintained corpus now has 99
escaped witnesses with scalar/astral/whitespace positive comparisons. Source
callability is checked outside exception capture (the private normalizer is
explicitly exposed). Isolated lossy-ingress mutation fails getPrecision at 101;
restored scoped tests pass, reviewer MERGE. Counts: 205 pending /122 ported /9 divergent.

## Market-style reference price

Ported resolveMarketOrderReferencePrice with explicit ticker lookup input and
shared ExchangeError. Maintained source corpus pins side/mark/last/index/midpoint/
opposite priority, finite-positive filters, native nonfinite inputs and both exact
Error messages. Escaped symbol witnesses declare only the shared string boundary;
runtime store dispatch remains pending. Initial compile exposed incorrect root
import; corrected to the core's public runtime::ExchangeError path. Both scoped
exchange tests pass. Independent Rust mark-before-side mutation fails by name at
101 (including positive subnormal ask); TS side-swap control fails separately.
Isolated reviewer MERGE. Counts: 204 pending /122 ported /10 divergent.

## Integration 6eb262afc5: TableHit and lodash primitives

Exact CI union 64 local /66 incoming /67 resolved. All 259 exemptions unique,
nonblank, with no parent-target loss; Lighter dependencies and layer policy unchanged.
Scoped Lighter all-target tests including ignored pass, CI wiring135 pass, layers37.
Reviewer found the previous reference-price row omitted its exact renamed Rust
path in the note: corrected this registration in the merge. Independent parity
now passes6906 rows/1307public items; follow-up MERGE. No shared primitive edits.
Counts unchanged204pending/122ported/10divergent.

## Shared TableHit checked against pre-integration Lighter capture

The existing actual-TypeScript witnesses now independently compare shared lookup's
all-twelve inherited identities, function/object distinction, truthiness, presence
and nullishness. No mapper row promoted. Isolated reviewer replaced inherited
results with Missing and got named mapOrder shared TableHit failure at101. Restored
all three scoped table tests pass, including own-only and wrong-own-function TS
controls. MERGE; counts remain204/122/10.

## Public REST payload parsers

Implemented parseRestOrderBookLevels and parseRestTickerItems with borrowed values,
source candidate order/nullish masking, empty-container behavior and shared JS own
key ordering. Both rows cite the shared string boundary with escaped payload/output
witnesses; runtime fetching remains pending. Initial review found swapping deeper
book aliases still passed: expanded to every alias pair with distinct values and
earlier null/false/zero/string/empty masks. The same Rust swap now fails the named
book comparison at101; reversing key order separately fails ticker comparison101.
Permanent TS controls cover first/deep book priority, deep ticker priority and
Object.values reversal. Restored all three pure tests pass; scoped Clippy clean;
parity6906/1310 green. Follow-up MERGE. Counts202pending/122ported/12divergent.

## Crypto integration merge 35b7ec85f9

Both-parent CI inventory is the exact 67/70 -> 71 union. Crate dependencies retained;
layer audit37 passes; 259 exemption targets unique with nonblank reasons and no
parent losses. Cargo disambiguates indexmap/quote/num-bigint versions. All scoped
Lighter targets pass. CI wiring171/171 passes after its removal control was updated
to remove every duplicated crypto path trigger rather than just the first.
Independent signing validation BEFORE venue integration: both independent_lighter
tests pass, covering all268 original WASM calls. Imported fixture is byte-identical
(SHA256 afe399cc424ff3e4e5393ef9c2f8f642b770530328f6cc70d6aefd830383679a).
Isolated merge reviewer returns MERGE: exact gate union, no parent audit/exemption
losses, unchanged layer policy and parity6967/1327 exit0. Re-read corrected section15:
whole-record refusal does not converge with omitted fields; inherited stringification
is member-specific. Counts unchanged202pending/122ported/12divergent. No transaction
signer integration claimed by this merge.

## Twelve pure websocket state decisions

Public/private pong policy, watchdog/ping thresholds, readonly reconciliation,
rate-limit cooldown and forced-toast decisions match actual TS members and static
constants. Pinned corpus has4 boolean states,196 deadline pairs and2744 toast triples,
including NaN/infinities/signed zero. All3 scoped tests pass, including four source
mutations requiring named assertion exit101 and missing selected compiler rejection.
Scoped all-target Clippy is clean. Isolated reviewer independently changed Rust ping
10000->10001 and toast >= to >: each compiles then fails its own named assertion101.
Restored tests3/3; parity6967/1339 passes; MERGE. Runtime clock acquisition and state
ownership are explicitly left on pending class rows. Counts190/134/12.

## Stream freshness and ticker hydration predicates

Actual TS member oracle checks2028 cases with distinct wall/virtual clocks and212
price cases covering all five fields, empty/multiple tickers, zero/negative and
nonfinite values. Three scoped tests pass with six named source controls; scoped
Clippy clean. Reviewer changes post-connect > to >= and positive price >0 to >=0:
both compile then fail named assertions101. Restored3/3; parity6967/1341; MERGE.
Counts188pending/136ported/12divergent.

Both new ws-state and table-inheritance scripts also execute in an external
tracked-source mirror with no ancestor node_modules, using the frozen isolated
compiler selected by ci-deps.cjs and copied root lockfile. Each missing-compiler
control fails MODULE_NOT_FOUND; mirror deleted. Ambient-root missing-compiler
controls also pass, preventing fallback to the root install.

## Private market payload flatteners

Both source helpers preserve own-key order, one-level array flattening, singleton
object handling and borrowed wire values.514 source cases and12 escaped Unicode
witnesses cover arrays, scalars, null/missing, nested values, own prototype-name
keys and index/insertion ordering. Lone-surrogate outputs stay present on TS;
Rust ingress refusal cites the shared CHOSEN string boundary. All3 scoped tests
pass including reverse-order and missing-singleton source controls101; scoped
Clippy clean. Reviewer independently changes Rust ordering/singleton behavior:
each compiles then fails by name101. Restored3/3, parity6967/1343; MERGE.
Counts186pending/136ported/14divergent.

## Consolidated core integration 590f1a4d7e

CI71/70 ->71 exact union;268 exemption targets unique/nonblank with no parent
losses; layer37 passes and Lighter dependencies retained. CI177/177 passes.
Integration itself declared candle-cache native/wasm backends absent from its lock:
offline reconciliation adds36 packages, removes0, updates candle-cache dependency
membership and two windows-sys disambiguations only. All scoped Lighter targets
pass including ignored oracles; subsequent locked no-run compile also passes.
Reviewer found the heartbeat test applied leaf_error twice after both parents'
conversion sites survived. E0308 reproduced; removed the creation-site conversion,
retaining final boundary conversion. Entire tunnel test target compiles and exact
heartbeat regression passes after repair. Follow-up MERGE; parity6984/1377 passes.
Counts186pending/136ported/14divergent. New extractor expansion remains separate.

## SDK dependency-closure extraction

Explicit scope ruling assigns TransactionClient composition/nonce/auth to this venue.
Expanded signer-only root to lighter-sdk/src, keeping all39 signer rows byte-unchanged
and covering client/constants/types/utils plus actual default API declarations.
Extractor adds350 pending rows in21 manifests, zero orphaned: global6984->7334.
SDKtotal389pending =client189/types92/constants33/utils32/index4/signer39.
Scoped extractor38 tests and typecheck pass; subsequent coveragecheck0changed/added/
orphaned. Isolated reviewer restores signer-only root: new closure test fails by
name at exit1; restored passes. Parity7334/1377 green; MERGE. Venue counts stay
186pending/136ported/14divergent; no new SDK behavior claimed by this inventory.

## SDK constants: 29 exported values

Maintained live-source oracle compares every table entry in declaration order,
all binary64 bits and exact BigInt bounds. Source package-version inconsistency
is preserved and marked on SdkVersion/UserAgent. Three source controls (chain,
version, BigInt) each assert-fail by name at101; selected missing compiler fails
MODULE_NOT_FOUND even with ambient packages. Tracked-source external mirror with
isolated frozen compiler passes. Reviewer independently mutates Rust ChainId and
MaxUint64: named101. Review found object projection erased order and duplicates;
changed both transports to ordered $table arrays, then independent reorder and
duplicate mutants compile and fail SDK constant ChainId at101. Restored3/3,
parity7334/1406 passes; review MERGE. SDK360pending/29ported/0divergent;
venue186pending/136ported/14divergent. Host Clippy measured separately from wasm:
pre-integration wasm lint blocked by upstream candle-cache indexed_db collapsible_if.

## Shared base 9222ed2454

Real TOML parsing verifies280 unique/nonblank exemptions, retaining both parents'
targets. CI71/73->74 exact multiset union;190 wiring tests pass. Own dependency
manifest and expanded SDK extraction remain intact; layer37 and locked crate check
pass. Strict all-targets host Clippy and wasm32-unknown-unknown Clippy each exit0
separately, dependencies included, after upstream candle-cache repair. Reviewer
confirms incoming lock matches except retained own dependencies, scoped constants
3/3, parity7334/1464, MERGE. Venue186/136/14; SDK360/29/0 unchanged.

## SDK constant aliases: four closed domains

Live TypeScript checker resolves actual indexed-access unions; source lost-as-const
and changed-string-literal controls fail named101. Accepted metadata and exhaustive
Rust variants pin strings; concrete Value and F64Deserializer roundtrips pin numeric
decoding independently. Qualified public decoders reject tagged objects and other
nonliteral shapes. Isolated tracked-source compiler proof passes and missing compiler
fails loudly. Host Clippy and3/3 tests pass. Reviewer independently substitutes wrong
numeric decoding, an extra serde alias and a permissive public inherent decoder;
each compiles then fails named101. Restored3/3, parity7334/1468; MERGE.
SDK356pending/33ported/0divergent; venue186pending/136ported/14divergent.

## SDK protocol constants: twelve values

Live ordered-table/binary64/BigInt oracle covers TxType and eleven scalar constants.
Three TS mutations(tx/gas/size) fail named101; isolated tracked-source compiler
proof passes and missing compiler fails loudly. All3 tests pass. Host and wasm
strict all-targets Clippy independently pass. Reviewer narrows gas multiplier
through f32, changes MaxOrderSize and reorders TxType: each compiles then fails
named101. Restored3/3. Review caught missing exact target citations for renamed
constants; all12 notes repaired and independent parity7334/1480 passes; MERGE.
SDK344pending/45ported/0divergent; venue186pending/136ported/14divergent.

## SDK protocol domains: eleven types

Actual TypeScript checker demonstrates numeric enums accept number-typed variables
beyond named literals; independent reviewer confirms literal999 rejected versus
incoming:number accepted. Numeric newtypes preserve that domain, named constants,
NaN/infinities and signed zero. TxTypeValue remains a closed literal union; two
string enums retain strict private wire decoders and accepted metadata. Member
values compare binary64 bits (not JSON numeric spellings). Three source controls
(widened alias, numeric constant, string member) fail named101; missing selected
compiler is loud both with ambient packages and in tracked-source mirror.
Reviewer changed constant, normalized -0, added string alias and public permissive
decoder: every mutation compiles then fails named101. Restored3/3, parity7334/1529,
host and wasm strict all-targets Clippy each pass; MERGE. SDK333pending/56ported/
0divergent; venue186pending/136ported/14divergent.

## SDK common leaf records and canonical numeric ingress

Five types: Balance, Ticker, OrderBookLevel, PaginationParams, MarketStatus.
Exact source schemas, typed field references, required/optional/null and shape
checks. Three unrestricted-string rows cite CHOSEN shared UTF8 boundary with
24 pinned code-unit witnesses covering scalar/astral/BOM/NEL and lone-surrogate
JSON retention versus Rust ingress refusal. Reviewer found concrete decoder
control used only empty arrays: repaired all4 to complete positional arrays;
public permissive decoder now compiles then named101. Empty Unicode corpus also
fails named101; optional-null removal fails PaginationParams.page.

Both author/reviewer measured raw serde typed parsing refuses1e400 while Node
and Value-first serde retain Infinity. Overseer ruled Value-first canonical,
matching core body_value. Permanent live +/-overflow bit witnesses cover all
numeric SDK enum types and common numeric fields; direct typed parsing limitation
is explicitly outside that ingress claim and separately pinned. Substituting the
wrong ingress compiles then fails Balance canonical Value-first overflow ingress
at101. Restored6/6 tests, parity7334/1534; host and wasm strict all-targets Clippy
separately pass. Review MERGE. SDK328pending/58ported/3divergent;
venue186pending/136ported/14divergent. Shared f64 serde repair remains routed.

2026-10-13 integration91db928805 merge consolidation: both-parent CI step
multisets74/76 resolve to77 exactly; dependency manifest retained unchanged;
real tomllib301blocks/301distinct targets, no empty reasons or parent losses.
Full scoped Lighter all-target tests including ignored live oracles38passed,
0failed/ignored. Strict host and wasm all-target Clippy separately exit0.
Parity7334rows/1692public items; extractor0changed/added/orphaned after only
canonical array whitespace normalization (parsed manifest identical). CI197/197,
layer78/78. Independent merged-state review MERGE: repeats275CI/layer tests and
parity, verifies dependencies, full SDK root350 added rows, exemption reasons and
both-parent gate union. Counts venue186pending/136ported/14divergent;
SDK328pending/58ported/3divergent. No stash used.

Follow-on integrationb1ef25ef36 (Kucoin and GRVT): actual-parent CI multiset
77/78 resolves79; CI205/205, layers78/78, locked own-crate all-target compile0,
extractor0changed/added/orphaned; exemptions301blocks/301distinct and dependencies
unchanged. Adversarial review caught an obsolete signer-only extraction test
and contradictory paragraph reintroduced at EOF by the otherwise clean merge.
Named old test failed4versus3 while full SDK closure test passed. Removed only
those obsolete copies; the replacement closure test retains signer/poseidon
assertions and adds client/types/constants/utils/default API coverage.
Repaired full extractor suite38/38; reviewer verdict MERGE. Incoming Kucoin and
GRVT lock dependencies and both parents' script assertions retained.

Shared get-key integration6bcff79504: clean eight-file upstream delta independently
verified byte-identical by reviewer, MERGE. CI79/78→79,205/205 tests; locked own
all-target compile0; parity7334/1808; tomllib303blocks/303distinct nonblank targets.
Cargo files, lock and layer policy unchanged. Venue/SDK counts unchanged.

Integration8a6ce34ca3 Binance consolidation: CI79/79→80 exact multiset union,
209/209 wiring tests, layers78/78, own locked all-target compile0,
parity7334rows/1920items, extractor0changed/added/orphaned, real tomllib303blocks/
303distinct. Additive CI conflict union retains Lighter and Binance assertions.
Independent merged-state review MERGE confirms both-parent scripts, gates,
lock dependencies, layer policy and full SDK closure; obsolete signer-only
regression remains absent. No parity-count changes.

Integration8cbfb8b5f9 Bybit consolidation: CI80/80→81 exact multiset union,
214/214 wiring tests, layers78/78, own locked all-target compile0,
parity7334rows/2027items, extractor0changed/added/orphaned, real tomllib303blocks/
303distinct nonblank targets. Package script conflict resolved by retaining both
Lighter and Bybit commands. Counts unchanged.

Signed-zero amendment audit: maintained common and SDK numeric type oracles
now pin Node JSON.parse bits for raw1e400,-1e400,-0,-0.0,0, and already-built
Value(-0) inputs. All six common numeric fields and eight numeric SDK newtypes
retain exact bits through the actual private remote/map decoders. Both targets
6/6 pass. No production decoder change or row reclassification was needed.
Independent reviewer inserted compiling BalanceWire.decimals n+0.0 mutation:
exit101 at Balance.decimals existing Value signed zero. Restored both targets
6/6 including maintained TS controls; no real signed-zero defect reproduced.
Merged-state and signed-zero review verdict MERGE, all parent protection retained.

2026-10-13 existing venue numeric-ingress census: exactly six older Map<Value>
rebuffer decoders, two boolean-only and four numeric records/eight fields.
DeploymentProfile signerChainId, BuilderApprovalFeeCaps all four fees,
CancelTarget marketId and OrderMinimums both amounts flatten parsed lexical -0
from8000000000000000 to0000000000000000. Live Node witnesses pin every field,
with raw typed -0, lexical -0.0, native Value(-0.0), positive zero and +/-overflow
remaining intact. Four previously ported rows now FINDING. This does not change
the clean SDK strict-map/newtype audit, whose scope is explicitly separate.
Reviewer independently confirms census; temporary strict-object repair compiles
and flips the named CancelTarget finding at101. Shared repair remains held.
Venue186pending/132ported/18divergent; SDK committed328/58/3 at this checkpoint.

2026-10-13 SDK common records: LighterConfig, Account, Market, Position, Trade
and OrderBook mapped with private derived remote decoders behind the existing
strict object guard; no numeric shim or Value map rebuffer. Shared PositionSide
and OrderSide retain exact accepted domains. All fields pinned to live TS schema,
required/optional/null, nested required fields and object-only shapes; all22
common numeric fields retain raw +/-overflow, lexical -0/-0.0/+0 and native
Value(-0) bits. Nine string-bearing records carry complete eight-case UTF16
witness inventories; six new rows declare CHOSEN shared UTF8 boundary only.
Nine maintained TS mutation controls fail named101. Reviewer mutations accepting
optional null, public positional arrays and defaulted required balances compile
then fail101; restored common+older-ingress tests4/4, review MERGE.
Strict host and wasm all-target Clippy separately0; parity7334/2033;
extractor0changed/added/orphaned. SDK322pending/58ported/9divergent;
venue186pending/132ported/18divergent.

Re-consolidation against integrationb8fc4dfbaa: clean textual merge; both-parent
CI81/82→82 exact multiset union; tomllib312blocks/312distinct nonblank targets;
Lighter dependencies unchanged. Fresh full scoped tests39passed/0failed/0ignored,
strict host and wasm all-target Clippy separately0, parity7334rows/2176items,
extractor0changed/added/orphaned, CI217/217, layers78/78. Asterdex's existing
oracle.cjs trailing whitespace is byte-identical to incoming source, not authored
or altered here. Venue186pending/132ported/18divergent; SDK322/58/9.
Independent merged-state review MERGE confirms both reviewed slices byte/semantic
unchanged, exact gates/scripts union, exemption target preservation, incoming
Asterdex lock dependencies and retained SDK extraction closure.

2026-10-13 resume correction to extractor evidence above: those local extractor
passes relied on a hand-installed smol-toml dependency. They establish local
source drift only, not clean-install reproducibility. RESUME.md records that the
extractor package is outside the root workspace and smol-toml is absent from the
root lock; the shared gate owner holds that wiring repair. No local install or
skip is used to disguise that limitation in resumed validation.

SDK generic responses resumed: ApiResponse<T> and PaginatedResponse<T> retain
strict record shapes, optional presence versus nullable T, and no T:Default
constraint. Numeric and nested typed payload bit witnesses remain distinct from
the generic Value lexical -0 FINDING (owned and borrowed); rows are MIXED and
FINDING respectively. SDK320pending/58ported/11divergent; venue186/132/18.
Reviewer caught a length-only corpus guard: replacing -0 with0 still passed.
Exact lexeme inventory plus source negative-zero bits now pin that witness, and
its maintained erasure control fails named101. Missing Paginated numeric and
nested typed payload witnesses were added in both ingress directions.
Final review MERGE; restored3/3 tests, four maintained source/corpus controls;
compiling Rust nullability, public positional decoder and zero-flattening
mutations each fail named101. Missing compiler fails MODULE_NOT_FOUND.
Resumed scoped host and wasm strict Clippy separately0 (library and generic test
target); parity7334rows/2178items. Extractor locally reports0changed/added/orphaned
under the pre-existing local dependency install, with the clean-install limitation
above retained explicitly rather than claimed closed.

SDK order record slice: CancelOrderParams, CancelAllOrdersParams, QueryFillsParams
and OrderStats add four CHOSEN shared UTF8 rows. Thirteen numeric fields each
cover eight exact lexemes with owned/borrowed binary64 bit checks and separate
native -0; all required/optional/null, public qualified shape and optional absent
serialization paths are covered. Per-record eight-case Unicode witnesses include
lone surrogates and accepted scalar/astral/BOM/NEL strings. Three live tests pass,
including two named TS schema mutations at101 and missing compiler rejection.
Review initially found swapped totalVolume/totalFees survived identical fixture
strings. Distinct per-field strings now make that compiling swap fail101 at
OrderStats complete roundtrip. Optional null widening also compiles then fails101
at CancelOrderParams.orderId. Restored3/3; final isolated review MERGE.
SDK316pending/58ported/15divergent; venue186pending/132ported/18divergent.
Host and wasm all-target strict Clippy separately0; parity7334/2182, CI217/217,
layers78/78. Real TOML312blocks/312distinct, no blank reasons. Updated distinct-value
oracle matches byte-for-byte in a tracked-source mirror without ancestor packages;
selected missing compiler fails with and without ambient root packages.

Resumed integration merge targets verified published56e818d52b (MEXC and WOO),
replacing an uncommitted200f57e95b merge after integration advanced. Both-parent
CI82/86→86, no missing names; tomllib312blocks/312distinct, no blank reasons or
missing parent targets. Lighter dependencies unchanged. Full scoped suite45passed
across22binaries,0failed/0ignored; host and wasm all-target strict Clippy each0;
parity7334rows/2511items; CI230/230. Local extractor0changed/added/orphaned after
canonical spacing correction in orders.ts.toml; its pre-existing hand-installed
smol-toml dependence remains a clean-install limitation, not a closed gate issue.
Venue186pending/132ported/18divergent; SDK316pending/58ported/15divergent.
Standalone layers78/78. Independent merged-state review MERGE verifies exact
CI multiset union and retained assertions, all exemption targets/reasons, incoming
lockfile graph, worker exception placement and grants, unchanged Lighter code and
manifest semantics, and retained full SDK extractor root.

SDK OrderInfo/Fill: two CHOSEN shared UTF8 rows. Thirteen additional numeric
fields include SDK enum wrappers and retain eight exact lexemes through owned
and borrowed ingress plus native negative zero. Distinct per-field strings and
independent boolean variations protect field placement. All39 public field types
are pinned using compiler-derived type names: reviewer changing OrderType to
OrderStatus compiled and initially passed, then failed101 by field name after
the guard. Boolean reduceOnly/postOnly swap and nullable triggerStatus mutations
also compile and fail named101. Source schema checks precede fixture decoding so
source controls retain their intended assertion rather than an unwrap failure.
Restored3/3 tests; host and wasm all-target strict Clippy separately0;
parity7334/2513; local extractor0changed/added/orphaned, with its existing
clean-install dependency limitation retained. External tracked-source Node mirror
matches bytes; selected compiler missing fails with and without ambient packages.
SDK314pending/58ported/17divergent; venue186pending/132ported/18divergent.
Final isolated review MERGE: wrong-wrapper control still compiles and fails101
at OrderInfo.type public field type, maintained TS controls pass independently,
and restored baseline3/3 exits0 after the ordering repair.

Six SDK create/query/modify request records:52 fields, including26 numeric fields
with eight exact lexical cases through owned/borrowed ingress and separate native
negative-zero checks. Public field type names pin numeric enum identity; optional
booleans and strings retain absence versus null. All-optional records also roundtrip
an empty object. No extra price, quantity or index validation is introduced.
Three live tests pass, including four named TS controls and compiler absence.
Isolated review MERGE; defaulted required ModifyOrderParams.orderId, nullable
CreateLimitOrderParams.postOnly and wrong QueryOrdersParams.status enum mutations
all compile then fail named101; restored3/3. The updated oracle is byte-identical
in an external tracked-source mirror and fails a missing selected compiler both
with and without ambient root packages. Local extractor0changed/added/orphaned;
its clean-install dependency limitation is unchanged. SDK308pending/58ported/
23divergent, venue186pending/132ported/18divergent.
Host and wasm all-target strict Clippy separately0; parity7334rows/2519items.

BatchCancelOrdersParams adds one CHOSEN shared UTF8 row with optional ID arrays.
Fixtures preserve unsorted adjacent and non-adjacent duplicates, and distinguish
empty arrays from absent keys. Non-string elements and explicit null reject;
all-optional empty object remains valid. Unicode inventory now has nine cases,
including empty string, for scalar and nested array strings. All five public
field types and both numeric fields are pinned. Three live tests pass including
five named TS controls and missing selected compiler. Isolated review MERGE:
duplicate-removal and null-array mutants compile then fail named101; restored3/3.
Host and wasm all-target strict Clippy separately0. Local extractor0changed/
added/orphaned with the documented clean-install limitation retained. External
tracked-source mirror produces identical oracle bytes; missing compiler fails
with and without ambient packages. SDK307pending/58ported/24divergent;
venue186pending/132ported/18divergent.
Parity7334rows/2520items passes.

### Shared numeric ingress integration and owner opt-in (2026-10-13)

Merge `449c2cb01b` incorporates published integration `b283acad08`. Isolated
merged-state adversarial review returned MERGE: CI named-step multisets
86/87 -> 87 with no parent losses; parsed exemption blocks/distinct 323/323,
zero blank reasons and no missing complete parent blocks; layer containers,
Lighter dependencies and workspace serde_json declaration unchanged. Lock delta
adds only the incoming num crate's serde/trybuild edges. Scoped venue suite:
45 passed in 22 binaries, zero failed/ignored. Host and wasm all-target strict
clippy passed separately; fmt passed; parity 7334 rows / 2538 public items;
wiring 237/237 and layer audit passed. Local extractor reported zero changed,
added or orphaned rows; its hand-installed dependency remains a clean-install
limitation, not hermetic extractor proof.

The opt-in replaces explicit Value buffers in approval fee caps, cancellation
targets and order minimums with shared JsNumberInput, registering every output
field through explicit JsNumberOutput impls and the exhaustive expression macro.
The old maxPerpsMakerFee loss assertion first failed by name at exit 101 with
actual bits 8000000000000000 against its former 0000000000000000 expectation.
The replacement corpus compares seven fields against Node on raw, owned,
borrowed and nested paths, including -0/-0.0/-0e0, decimal/exponent, overflow,
negative underflow and binary64 rounding. DeploymentProfile is audited separately
and has not yet adopted the newly published Cow eligibility.

Generic responses opt into whole-entry ingress with JsNumberInput payloads and
then payload.decode, while preserving the legacy serde Value rebuffer diagnosis
as a separate observation. Numeric metadata and payloads compare bits across
all three entry paths. API message/error, retained generic strings and CancelTarget
orderId retain explicit scalar/astral/BOM/NEL positive witnesses and valid-JSON
lone-surrogate representation refusals; the numeric repair does not erase those
independent CHOSEN string restrictions.

The changed generic Node oracle produced byte-identical output in an external
tracked-source mirror. A missing selected compiler failed MODULE_NOT_FOUND both
there and in the ambient-package worktree. The temporary mirror was deleted.

Isolated adoption review: MERGE. A compiling zero-flattening mutation failed at
exit 101 naming `LighterCancelTarget.marketId raw -0`. Restored ingress, generic
and pure-helper binaries passed 8/8, including maintained TS source controls.
An independent canonical-ingress probe also retained nullable versus absent
payload presence and nested null/negative zero while rejecting non-records.

Adoption final gates: host and wasm all-target strict clippy each exited 0;
parity passed 7334 rows / 2538 public items. Local extractor reported 0 changed,
0 added and 0 orphaned (same clean-install caveat). Counts after this opt-in:
venue pending/ported/divergent 186/134/16; SDK 307/58/24. Only the two numeric-only
venue records change to ported; independently witnessed string restrictions stay.

### DeploymentProfile numeric ingress and shared Cow eligibility

The numeric owner's `9e59a52b29` was verified at the published remote head before
merging. Shared eligibility is imported unchanged: str, references recursively
bounded by JsNumberOutput, and Cow<str>; the explicit &Value refusal remains in
the owner's pinned compile-fail corpus. No local orphan-rule workaround or
representation change is used.

Both parents retain 87 named CI steps in the result. Exemption blocks/distinct
remain 323/323, with every complete parent block retained. Cargo manifests,
lockfile, workspace serde_json declaration and layer policy are unchanged.
DeploymentProfile now uses JsNumberInput internally, with exhaustive output
registration for the record and its two local closed enums.

The old signerChainId loss witness first failed by name at exit 101: actual
8000000000000000 versus its old expected 0000000000000000. Its replacement
compares raw/owned/borrowed/nested bits against Node across the same ten spellings
as the other seven repaired fields. All four profile constants remain borrowed
Cow values and decoded profiles remain owned Cow values. Separate live Node
witnesses pin both URL fields' scalar/astral/BOM/NEL retention and lone-surrogate
refusal. The row remains CHOSEN for the shared UTF8 boundary, not numeric loss.

Full scoped suite after adoption: 47 passed across 22 binaries, 0 failures,
0 ignored. Local extractor: 0 changed/added/orphaned (hand-installed dependency
caveat still applies). Counts remain venue 186/134/16 and SDK 307/58/24 in
pending/ported/divergent order; closing the numeric finding does not erase the
independent measured URL-string restriction.

Profile merged-state review: MERGE. Compiling Rust zero-flattening failed 101
naming `LighterDeploymentProfile.signerChainId raw -0`; an independent live
TypeScript chain-ID mutation failed 101 naming
`LIGHTER_DEPLOYMENT_PROFILES.livenet.signerChainId`. Restored numeric/protocol
binaries passed 4/4. Reviewer independently verified both-parent gate, exemption,
manifest, lock and layer preservation plus byte-identical imported shared files.
Host and wasm all-target strict clippy passed separately; fmt passed. Parity
initially caught a dropped rename target in the rewritten profile note; restoring
the exact LighterQuoteCurrency target citation passed 7334 rows / 2538 items.
Wiring and layer assertions passed 237/237. No code suppression or shared edit.

### Six SDK response records (2026-10-13)

Result, NextNonce, ApiKeyInfo, TxHash, ServerTime and RateLimit cover 22 fields,
including 14 numeric fields. The maintained responses.ts oracle checks exact
schemas, required/optional presence, null/wrong-shape refusal, distinct string
fields, booleans and compiler-derived Rust field identities. Numeric witnesses
cover ten spellings per field across BOTH bare serde and shared whole-entry raw,
owned and borrowed paths, comparing bits before serialization. Public Deserialize
captures JsNumberInput and delegates to a private strict-map companion; each
record explicitly registers every field with the shared output expression macro.

All six rows retain CHOSEN shared-UTF8 restrictions, with empty/scalar/astral/BOM/
NEL positive cases and valid-JSON lone-surrogate refusals. No positivity,
integrality, bitmap, nonce or timestamp validation is added to the source types.
Three maintained TS schema mutations fail named assertions at exit 101. Isolated
adversarial review returned MERGE: compiling null-widening and required-field
mutations failed 101 naming `ApiKeyInfo.lastUsedAt rejects null/wrong type` and
`NextNonce.account required/optional`. Both restored; baseline 3/3 passed,
including the two-depth ingress corpus and TS controls.

New Node oracle: external tracked-source mirror output byte-identical; missing
selected compiler fails MODULE_NOT_FOUND with ambient packages and without them.
Mirror deleted. Existing mandatory all-target venue gate discovers the new test
binary, and existing full lighter-sdk source triggers cover responses.ts.
Parity passed 7334 rows / 2544 public items; local extractor 0 changed/added/
orphaned, retaining its hand-installed dependency caveat. Counts: venue
186/134/16; SDK 301/58/30, pending/ported/divergent.
Final response-slice fmt and host/wasm all-target strict clippy each passed after
the expanded bare-serde corpus was included; host and wasm measured separately.

### Explicit two-depth ingress evidence

Following the section 17 correction, the older eight-field audit now compares
bare serde raw/owned/borrowed calls and bare nested raw records alongside all
shared entry paths. All six ingress/generic tests pass. The four older records
really rewrote public Deserialize; their notes now name that deeper adoption.
Generic wrappers only registered the retained-payload shared entry API, so their
notes remain MIXED: shared UTF8 plus the residual bare-Value lexical-zero FINDING.
No production SDK decode caller is ported yet and no generic Deserialize rewrite
is claimed. Counts unchanged, venue 186/134/16 and SDK 301/58/30.
Host and wasm all-target strict clippy each passed separately; parity passed
7334 rows / 2544 public items. This edit changes tests and claims, not production.
Isolated review returned MERGE. A mutation affecting only bare raw input failed
101 naming `LighterDeploymentProfile.signerChainId bare raw repaired bits -0`
while shared paths were untouched. Restored targeted suite passed 6/6. This pins
execution of the added bare path rather than only presence of shared API tests.

### Consolidation on integration 98102913bb

Both-parent CI step multisets are 87/87, merged 87, with no missing step.
Real tomllib parsing finds 327 exemption blocks / 327 distinct targets, no blank
reasons, and every complete parent block retained. Lighter dependencies, Cargo.lock
and layer configuration are unchanged. Full scoped live suite: 50 passed across
23 binaries, zero failed/ignored. Host strict all-target clippy and wasm32 strict
all-target clippy passed separately, with jobs=2. Fmt passed. Wiring plus layer
audit passed 237/237. Parity passed 7334 rows / 2549 public Rust items. Local
extractor reports zero changed/added/orphaned; its hand-installed dependency still
means this is not clean-install proof. The first wiring command omitted Cargo's
PATH and failed ENOENT; correcting the invocation produced the full passing run.

Store notification sweep: zero already-ported rows cross Store.update/setSetting.
The live TS AST identifies nine calls in four methods: changeAccountTier, start,
fetchAndSetPrivateSnapshotHelper and fetchAndSetBalance. All four rows remain
pending. No current Rust venue source references Store or performs store writes.
The routed subscriber-throw seam therefore changes no current row claim; it must
be witnessed when these pending runtime methods are implemented. Counts remain
venue pending/ported/divergent 186/134/16, SDK 301/58/30.
Isolated merged-state review returned MERGE against incoming 98102913bb after
correcting overly broad SDK Value-first wording in CLAUDE.md. Reviewer independently
verified both-parent CI/scripts/assertions, complete exemption reasons, dependencies,
lock/layer preservation and the zero-ported-row Store sweep. Integration was then
frozen for the core merge; no newer integration merge was attempted during that hold.

### Five SDK response leaf records

DepositAddress, TransferHistoryItem, MaintenanceWindow, FeeTier and FundingRate
add 28 fields, including eight binary64 fields, against live responses.ts.
Both bare serde and shared raw/owned/borrowed paths compare numeric bits over ten
spellings per field. Required/optional/non-null presence, compiler-derived field
types, qualified map-only decoding and string-array order/duplicates/empty values
are pinned. All five rows are CHOSEN shared-UTF8 with scalar/astral/BOM/NEL and
valid-JSON lone-surrogate witnesses. Amounts/rates remain strings; no business
validation is invented. The existing compiler-only Node oracle is extended, with
no new runtime dependencies or selector requirements.

Response suite passes 5/5 including six maintained TS source mutations total;
three new mutants fail named DepositAddress, MaintenanceWindow and FundingRate
schema assertions at exit 101. Host strict all-target clippy and wasm strict
all-target clippy each pass separately. Parity passes 7334 rows / 2554 public
items. Local extractor drift is zero, still not clean-install evidence.
Counts pending/ported/divergent: venue 186/134/16; SDK 296/58/35.
Isolated review returned MERGE. A compiling Rust deduplication mutation failed
101 naming `MaintenanceWindow complete roundtrip`; restored baseline passed 5/5
including all source controls. Thus the adjacent-duplicate fixture is load-bearing.

### SDK account-key and exchange-info containers

AccountApiKeys and ExchangeInfo add eight outer fields, including serverTime,
and retain typed ApiKeyInfo/RateLimit arrays. The maintained oracle separately
walks all seven child numeric fields, checking ten spellings through bare and
shared raw/owned/borrowed entries. Nested Unicode has an exact inventory rather
than only a case count. Distinct child records and adjacent duplicates make
array ordering and multiplicity observable; empty lists remain valid. Required
fields, rejected scalar/array-element shapes, exact compiler child types and
public qualified map-only decoding are pinned. Both rows retain CHOSEN shared
UTF8 and make no business validation or runtime-caller claim.

Response suite passes 8/8. Two new TS array-to-scalar mutations fail named
AccountApiKeys and ExchangeInfo schema assertions at exit 101. Host and wasm
strict all-target clippy pass separately. Parity passes 7334 rows / 2556 public
items; local extractor reports no drift with its existing clean-install caveat.
Counts pending/ported/divergent: venue 186/134/16; SDK 294/58/37.
Isolated review returned MERGE. A compiling Rust mutation reversed
AccountApiKeys.apiKeys and failed 101 in its named container roundtrip assertion.
Restored baseline passed 8/8, including nested numeric paths and source controls.

### Core refresh at aa632b5578 and indirect notification audit

The integration freeze lifted after the preceding full merged-state MERGE verdict
on d245e05334 / incoming 98102913bb. Refreshed to aa632b5578 without conflicts.
Both-parent CI named steps remain 87/87 -> 87; exemptions parse as 327 blocks /
327 distinct targets, with complete parent blocks/reasons retained. No script
names disappear. KuCoin's script and matching assertion gain three incoming
scoped test binaries verbatim. Own Cargo dependencies, lock and layers do not
change. The refreshed full Lighter suite passes 55 tests in 23 binaries, zero
failed/ignored; wiring plus layer audit passes 237/237; local extractor drift is
zero with the same hand-installed-dependency caveat.

STORE_NOTIFICATION_AUDIT.md supersedes the earlier direct-call-only zero. All
94 ported callables resolve through TS AST and their fixed internal helper/core
paths were followed; none reaches notification. The 98 ported data/type/enum
rows do not add a callable path. All 14 divergent callables were checked too.
No row reclassification is needed. Arbitrary host callback effects are outside
this point-in-time audit, explicitly not claimed effect-free. Counts remain
venue 186/134/16, SDK 294/58/37 pending/ported/divergent.

Before commit, integration advanced to eefa54442a. Repointed the uncommitted
merge, preserving only our audit/evidence files, then checked every incoming
changed file byte-for-byte against MERGE_HEAD. CI steps 87/88 -> 88, exemption
blocks/distinct still 327/327, no missing complete parent entry. The newer delta
adds cancellation-discovery wiring, OKX changes and candle-cache changes. Native
runtime gate inputs are unchanged by that delta (owner.rs adds a cfg(test) test);
the wasm cursor path changed, so wasm all-target strict clippy was rerun and
passed. Wiring/layers now pass 241/241. Parity passes 7334 rows / 2558 public
items. The 55-test full native result above remains explicitly scoped to the
immediately preceding aa632b5578 tree; it is not a browser execution claim.
Final host strict clippy also passed independently on eefa54442a; local extractor
again reports zero drift. Incremental isolated review returned MERGE, independently
confirming all 74 incoming files exact, all parent gates/exemptions retained,
241 wiring/layer tests and parity 7334/2558. The reviewer independently matched
94 ported and 14 divergent callable inventories and agreed with the explicit
manual-reachability scope. This follows the earlier full merged-state verdict.

### Incremental P6 consolidation refresh at 1f6698344c

Fresh remote inspection after the eefa54442a verdict found P6 consolidated.
Merged its 36 changed files byte-identically. Named CI remains 88/88 -> 88;
real TOML parsing gives 333 exemption blocks / 333 distinct targets, with every
complete parent block/reason retained (large textual deletion is whitespace
normalization, not a lost exemption). Own dependencies, lock and layers remain
unchanged. The risk API changes are compiled through the Lighter dependency
graph by separate host and wasm strict all-target clippy, both passing. Our
reachability audit contains no risk or Store mutation caller, so no local API
migration or row change follows. Wiring/layers pass 241/241. Earlier full native
55-test evidence stays scoped to aa632b5578, with incremental checks stated
separately rather than claiming that suite ran on this newer tree.
Parity on this refresh passes 7334 rows / 2568 public items; local extractor
again reports zero changed/added/orphaned (not clean-install proof). Isolated
incremental review returned MERGE: all 36 incoming files exact, CI/scripts/audit
assertions retained, 333 unique complete exemption targets with both parents'
reasons, and Lighter dependency/layer/source/manifest preservation verified.
No additional full-suite claim or new parity row is made by this merge.

### Shared container eligibility refresh at b0e0b9c68b

All 28 incoming changed files match upstream byte-for-byte. Named rust.yml CI
steps remain 88/88 -> 88 (all workflow names separately 348/348). Real TOML
parsing retains all complete parent exemptions: 333 blocks / 333 distinct.
Own Cargo dependencies and layers are unchanged. Lock package/version inventory
is unchanged; the two incoming dependency edges are num -> existing indexmap
2.14.2 and GRVT -> num. Shared JsObjectMap and IndexMap numeric-output eligibility
is now present in integration, releasing the three pending SDK dictionary records.

Fresh full scoped Lighter tests on this merged tree pass 55 tests across 23
binaries, zero failed/ignored. Host and wasm strict all-target Clippy each pass
separately. Wiring and layer tests pass 319 total (241 wiring plus 78 layer).
Parity passes 7334 rows / 2579 public Rust items. Local extractor drift is zero
with the previously disclosed hand-installed-dependency caveat, not proof of a
clean install. Counts pending/ported/divergent remain venue 186/134/16 and SDK
294/58/37. This supersedes the older full-suite scope for this tree only.
Incremental isolated review returned MERGE, independently verifying all incoming
bytes, complete exemption reasons, dependency deltas and the fresh gate logs.
A final docs-only refresh to 7772471f39 adds 17 lines to MANIFEST-SPEC about
inherited lookup -> write consequences. Incremental reviewer returned MERGE:
no executable gate inputs change, current Lighter claims remain consistent,
and future lookup-to-write paths require whole-path witnesses.

### SDK dictionary responses

ErrorResponse, FeeSchedule and InsuranceFund adopt the landed shared JsObjectMap
eligibility, without a local wrapper. Thirteen outer fields include four string
dictionaries, three direct numeric fields and two nested fee-tier numeric paths.
The source oracle pins strict shape, optional absence versus null, compiler field
identity, integer-key/insertion order, duplicate replacement, own prototype-named
keys and empty dictionaries. Keys and values have separate scalar/astral/BOM/NEL
and lone-surrogate witnesses; the latter travel as escaped raw JSON plus UTF16
units, avoiding a malformed Rust oracle envelope. Numeric bits cross both bare
and shared raw/owned/borrowed entries before serialization. Distinct dictionary
and tier contents ensure swapped fields cannot compare equal accidentally.

The focused response suite passes 11/11, including three new TS schema controls
(error field values, fee-tier array and insurance balances), each failing by
name at exit 101. Host and wasm strict all-target Clippy pass separately; parity
passes 7334 rows / 2582 public items. Local extractor drift remains zero with
its hand-installed-dependency caveat. Counts pending/ported/divergent: venue
186/134/16; SDK 291/58/40. The three new divergent rows cite the shared UTF8
boundary for strings including dictionary keys and values.
Isolated adversarial review returned MERGE. Swapping the two FeeSchedule fee
maps in a compiling Rust decoder failed `FeeSchedule complete roundtrip` at
exit 101; restored baseline passed all 11 response tests, including the source
controls. No sibling checks were weakened.

### Generic retained-payload depth acceptance cost

ApiResponse and PaginatedResponse now carry an independent CHOSEN depth note
alongside UTF8 and the residual legacy numeric FINDING. Live Node accepts nested
payload arrays at depths 0, 2, 124, 125, 126, 127, 128 and 140. Legacy bare
owned/borrowed Value decoding retains every measured depth and leaf bits.
Shared raw/owned/borrowed ingress accepts up to payload depth 126 for ApiResponse
and 125 for PaginatedResponse; the items array consumes one more level. Refusals
are explicitly recursion-limit errors, with shallow positives guarding against
an unrelated decoder failure masquerading as the declared cost.

The generic suite passes 4/4, including existing live source controls. Host and
wasm strict all-target Clippy pass separately; parity remains 7334/2582. This is
witness/classification work, not a production decoder change or new row claim.
Counts remain venue 186/134/16 and SDK 291/58/40 pending/ported/divergent.
Local extractor returns zero changed/added/orphaned after canonical whitespace
restoration, with the existing portability caveat. Isolated review returned
MERGE. Raising both shared depth guards to 256 compiled and failed
`PaginatedResponse raw depth 126 acceptance cost` at exit 101. The reviewer
restored the shared file and all four generic tests passed, including source
controls. No shared production edits are part of this commit.

### SDK health and component status records

HealthResponse and ComponentStatus add ten record fields, including three numeric
fields and three closed inline unions (health, connection and component state).
Public union decoding requires a string before private serde remote companions.
The live oracle checks accepted metadata, exhaustive mappings, public qualified
object rejection, all declared literals in their enclosing fields, optional
absence versus null and compiler-derived field identities. Public record numeric
capture retains bits on both bare and shared raw/owned/borrowed paths. Open
strings carry the separately witnessed shared UTF8 restriction.

Focused response tests pass 13/13, including health-union extension, connection
type-change and component-union extension controls failing named source schemas
at exit 101. Host and wasm strict all-target Clippy pass separately. Parity
passes 7334 rows / 2587 public items; local extractor reports zero drift with
the existing hand-installed-dependency caveat. Working counts pending/ported/
divergent: venue 186/134/16; SDK 289/58/42.
Isolated adversarial review returned MERGE. Both compiling Rust widening controls
failed at exit 101: adding an alias failed `HealthResponse.status accepted union
metadata`; bypassing string-first decoding failed `HealthResponse.status rejects
tagged object {"healthy":null}`. Restored response suite passed 13/13, including
TS controls. No permissive public decoder was introduced.

### Remaining SDK account, history and system response records

AccountInfoResponse, WithdrawalHistoryItem, DepositHistoryItem and SystemStatus
add 32 fields, ten direct numeric fields and four inline status unions. KYC
absence, literal none and rejected null stay distinct. Typed system component
and maintenance arrays retain order/duplicates/empty lists and cross three nested
numeric field paths. The oracle's nested fixture builder now retains child union
literals and string arrays when constructing distinct siblings; closed status
values are never replaced with arbitrary fixture strings.

All 16 response tests pass, including four new source controls failing named
account, withdrawal, deposit and system schemas at exit 101. Host and wasm
strict all-target Clippy pass separately; parity passes 7334 rows / 2595 public
items. Local extractor drift is zero with the same hand-installed-dependency
caveat. Counts pending/ported/divergent: venue 186/134/16; SDK 285/58/46. All
responses.ts rows are now implemented and classified as CHOSEN for their shared
open-string UTF8 boundary; this is not a production SDK client decoding claim.
Isolated review returned MERGE. Removing the optional-non-null guard for
SystemStatus.scheduledMaintenance compiled and failed the named null/wrong-type
assertion at exit 101. The reviewer restored the decoder and all 16 response
tests passed, including source controls. Nested fixture changes preserve the
sibling accepted-set and array checks.

### Fifth consolidation: reviewed basis during integration hold

The merge basis is cd122af5d2. The full scoped run on the preceding merged
0968312f8d tree passed 64 tests across 23 binaries with zero failed or ignored,
and strict host and wasm all-target Clippy passed separately. The eight-file
0968312f8d-to-cd122af5d2 delta touches KuCoin, package scripts and wiring only;
it does not change Lighter's native test or dependency inputs. Wiring/layers
were rerun on cd122af5d2 and passed 329/329; parity passed 7334 rows / 3215 public
Rust items. These incremental checks do not claim a second full native run.

The extractor reported one changed manifest, zero added and zero orphaned rows:
Coinbase's inherited starbase-orders manifest lacked seven canonical blank
separators. Parsed data is unchanged by the upstream repair at 18261b604c.
This basis retains that red during the integration freeze; it is not merge-up
ready until the repair is incorporated and extractor drift rerun. The extractor
still uses the previously disclosed hand-installed dependency, not clean-install
proof. Pending/ported/divergent counts are venue 186/134/16, SDK 285/58/46.

The Store notification conversion sweep follows callees and indirect paths,
not only map_err spellings. Implemented Lighter source currently has no Store,
BaseExchange or ExchangeContext reference and no Store mutation result to
convert, discard or stringify. The prior transitive callable inventory remains
applicable; subsequent work adds DTO decoders only. Future writers must retain
the error type explicitly so the notification ABI change cannot be hidden by
inference or stringification.

### Fifth consolidation refresh after shared eligibility landing

After the hold lifted, the e92bdd9b04 refresh imported 39 files exactly from
upstream. Named CI steps are 90/90 -> 90, and a real tomllib parse reports 338
exemption blocks / 338 distinct targets, with the complete parent union intact.
Cargo.lock, layers.json and Lighter's Cargo.toml are unchanged by this refresh.
Because shared numeric capture changed, the full scoped suite was rerun rather
than carrying forward the earlier result: 64 tests across 23 binaries, zero
failed or ignored. Strict host and wasm all-target Clippy passed separately.
Wiring/layers passed 329/329. The first wiring invocation lacked Cargo on PATH;
it failed before testing and was rerun with the required rustup path.

The inherited Coinbase extractor witness flipped after 18261b604c: zero changed
manifests, zero added and zero orphaned rows. This still is a local run with the
previously disclosed hand-installed extractor dependency, not clean-install
proof. Counts remain venue 186/134/16 and SDK 285/58/46 pending/ported/divergent.

### SDK transaction request and signature records

TransactOpts, CreateOrderTxReq, CancelOrderTxReq, TransferTxReq, WithdrawTxReq,
UpdateApiKeyTxReq and TxSignature cover 51 source fields, including 30 numeric
fields. Schemas were captured from the live TypeScript before implementation.
Public decoders capture JsNumberInput before strict private map companions;
optional non-null strings, numbers and booleans retain absence and refuse null.
SDK OrderType, OrderSide and TimeInForce now have explicit exhaustive output
registrations in their owning venue module, without changing their decoders.

The focused suite passes 3/3. Every numeric field compares bits on bare and
shared raw/owned/borrowed paths, with overflow, lexical/native negative zero,
underflow and binary64 rounding. Distinct finite fields and compiler-derived
field types protect against swaps and numerically identical wrapper substitution.
Three source controls change required nonce type, SDK order wrapper and optional
gas-limit presence; each fails its named schema assertion at exit 101. The
compiler-absence test runs with ambient packages present and fails MODULE_NOT_FOUND.
An external tracked-source mirror also runs all seven schemas without ancestor
node_modules, then fails the missing-compiler control. It was deleted afterwards.

Host and wasm strict all-target Clippy pass separately, with one Cargo job.
Parity passes 7334 rows / 3234 public items. Working pending/ported/divergent:
venue 186/134/16; SDK 278/58/53. The seven new divergent rows carry CHOSEN shared
UTF8 string boundaries with scalar/astral/BOM/NEL and lone-surrogate witnesses;
no transaction signing, nonce runtime or SDK client integration is claimed.
Local extractor drift is zero, with the existing installation caveat. Isolated
adversarial review returned MERGE. Removing gasLimit's optional-non-null guard
compiled and failed `TransactOpts.gasLimit rejects null/wrong type` at exit 101.
A separate nonce-zeroing mutation compiled and was rejected earlier by shared
output bit validation, so it is not counted as the named assertion control.
Both mutations were restored and the serial baseline passed 3/3, including TS
controls. No review changes remain.

### SDK L2 transaction info and closed numeric discriminators

Eight L2 info records retain 47 outer fields and each record's distinct numeric
literal txType (14 through 21), represented by its own single-variant type.
The live TypeScript checker proved the NumberLiteral domains before production
implementation. Accepted numeric spellings, F64Deserializer roundtrips, other
transaction numbers, non-finites and nonnumeric/tagged inputs are tested. Public
qualified calls and exhaustive variant matches protect the concrete decoder and
variant surface. A broad TxTypeValue field is not an equivalent representation.

The expanded transaction suite passes 5/5. Four hundred nested TransactOpts
numeric witnesses cross six bare/shared raw/owned/borrowed paths (2400 bit
comparisons), alongside direct field witnesses and nested optional gas-price
Unicode/refusal cases. Three additional TS controls remove as-const, widen the
discriminator to TxTypeValue and change a constant; each fails a named assertion
at exit 101. The external source mirror now includes the actual constants file:
15 schemas and eight checker-derived literals pass with no ancestor modules;
the missing compiler fails MODULE_NOT_FOUND. The mirror was deleted.

Host and wasm strict all-target Clippy pass separately, one Cargo job each.
Redundant single-arm matches in numeric-free test projections were simplified
after Clippy caught them. Parity passes 7334 rows / 3250 public items after
mapping notes were corrected to name exact auxiliary Rust discriminator targets.
Working pending/ported/divergent: venue 186/134/16, SDK 270/58/61. These are DTO
shapes with shared UTF8 CHOSEN notes, not SDK signing or transaction execution.

The first L2 review returned DO NOT MERGE on incomplete claims: whole-record
capture also refuses deep unknown fields that ordinary serde discards. A compiling
discriminator widening separately failed `L2CreateOrderTxInfo.txType accepts only
its literal: 15.0` at 101, restored baseline 5/5. No discriminator defect remained.

The follow-through now covers all 41 public capture records, including older
slices: 22 SDK responses, 15 transaction records and four venue records. Live
Node plus an ordinary serde unknown-field-skipping alternative accept 140 nested
arrays in extra; six public/shared raw/owned/borrowed entry paths accept root
126 and refuse 127. L2 opts.extra accepts 125 and refuses 126. Positive cases
0/2/125 and explicit recursion-limit messages prevent unrelated rejection from
passing. Known field values remain under separate source/bit assertions; this
helper compares accepted output against the same decoder's shallow projection
so integer-versus-float JSON spelling is not mistaken for a depth defect.

All 24 tests across the three affected binaries pass. Forty-one notes now price
this independent CHOSEN policy; fee caps and order minimums moved from ported to
divergent, with no production logic change. Counts pending/ported/divergent are
venue 186/132/18 and SDK 270/58/61. Retaining shared bounded capture avoids an
unbounded venue-local numeric fork; accepting deeper input is expressible and
is not claimed intrinsically wrong or impossible.

The second review closed the depth blocker: raising both shared capture guards
128 -> 256 compiled and failed `L2CreateOrderTxInfo bare raw unknown-field depth
127 nested=false acceptance cost` at 101; restored baseline passed 24/24.
It withheld MERGE on an adjacent independently measured restriction: numeric-only
fee caps and minimums also refuse unknown lone-surrogate string values. The
maintained helper now witnesses high/low/embedded surrogates and six Unicode
positive cases on all 41 root records plus nested opts. Node accepts the valid
JSON and ordinary serde skips these extras; bare/shared raw capture refuses the
unpaired UTF16 cases. The notes cite the canonical shared UTF8 declaration for
this independent cause, including the two numeric-only records. No owned/borrowed
surrogate witness is claimed, because serde Value cannot carry those inputs.
Final isolated review returned MERGE: both unknown-field blockers are closed,
all 41 notes retain both costs, and the reviewer independently reran 24/24 tests
across three binaries. Host and wasm strict Clippy pass separately after the
final witness changes; parity remains 7334/3250 and extractor drift is zero
with the established installation caveat. Earlier compiling discriminator and
depth controls remain valid; no review edits or shared production mutations remain.

### Interface heritage census guard

The SDK common, generic, order, response and transaction censuses previously
walked only InterfaceDeclaration.members. All five now refuse heritageClauses
before emitting a schema. Each has a maintained source mutation adding an
ExtraNumeric base with required hiddenNumeric:number, and its Rust child test
asserts exit 101 plus the named unsupported-interface-heritage assertion.
All 31 tests across the five affected binaries pass, with no failed or ignored
tests. The actual current interfaces have no heritage; this repairs a detector
blind spot, not a current production shape or manifest classification. Other
member walks inventory type literals, enums or explicitly selected class members,
not interface field schemas. Counts remain venue 186/132/18 and SDK 270/58/61
(pending/ported/divergent).
The isolated reviewer returned MERGE and independently removed the generic
guard: the mutated child incorrectly passed, making the permanent control fail
at 101. Restoring the guard made the child fail at 101 with ApiResponse's named
heritage assertion; the restored five-binary baseline passed 31/31.

### L2 transaction union

L2TxInfo uses the eight source record alternatives, captured before implementation
and retained as a live AST union inventory. Each variant has an independent
selected-name and complete-roundtrip assertion; required fields and invalid tags
remain rejected. Direct and nested numeric witnesses compare bits through all
six bare/shared raw, owned and borrowed paths, including untagged buffering.
Declared and unknown strings retain the separate UTF8 witnesses. Root and opts
unknown-depth costs remain 126 and 125 respectively, measured on each variant.
The source member-removal control fails the named union inventory at exit 101.
The seven-test transaction binary passes with zero failed or ignored tests.
No TransactionClient integration or production SDK decode caller is claimed.
Host and wasm strict Clippy passed separately; parity passed 7334/3251 and
extractor drift was zero (existing local dependency installation, not a clean
install proof). The first review refused merge: swapping reduceOnly/postOnly at
the union boundary compiled and passed seven tests because both were true.
Independent per-boolean roundtrips and per-field null/wrong-type assertions now
cover that boundary rather than relying on the inner record's tests.
The subsequent symmetric-serde-name ruling is independent: encode can undo a
wrong decode. The union test now reads every native string field and each
independently varied native boolean directly; numeric witnesses already read
native fields before serialization. Roundtrips remain shape checks, not the
sole evidence of field identity.
Final isolated v3 review returned MERGE. Symmetric public/private serde-name
swaps compiled and failed at 101 on L2CreateOrderTxInfo.price native string
identity and L2CreateOrderTxInfo.reduceOnly native boolean identity false.
Restored seven-test baseline passed, and final strict host and wasm all-target
Clippy each passed separately. No reviewer mutations remain. Counts
pending/ported/divergent: venue 186/132/18, SDK 269/58/62.

### Common record native field identity

Ten older common record shapes now vary individual leaf values and assert on
native fields recursively. The isolated reviewer returned MERGE after symmetric
serde renames (with wire order retained) compiled and failed Balance.available
native field identity and Account.balances[0].available native field identity at
101. A zero-only flattening mutation separately failed Balance.decimals existing
Value signed zero; finite corruption failed Balance.decimals native field
identity. Restored 3/3 tests and scoped strict Clippy passed. This is a partial
field-identity sweep, not completion of the 60-record or whole-venue inventory.
No production or manifest changes; counts remain venue186/132/18 SDK269/58/62.

### Signed transaction wrapper

Source capture preceded SignedTx implementation: three required fields and all
8 L2 variants, 530 numeric cases, 216 declared-string cases, 144 nested-depth
cases and 216 unknown-string cases. Inventories are asserted explicitly. Native
strings, booleans and signature/transaction numeric fields are checked across
the outer boundary, independently of the inner types' tests. Numeric witnesses
cross six bare/shared raw/owned/borrowed paths. Unknown depth accepts root126,
tx/signature125 and tx.opts124 arrays; Node and ordinary ignoring serde accept140.
These and unknown-string UTF8 refusal remain independent CHOSEN costs.
The transaction binary passes9/9 with zero failed or ignored; source signature
widening and inherited-field controls fail named assertions at101. Extractor
is zero changed/added/orphaned with the established local-install caveat.
No production signing consumer or TransactionClient runtime claim is made.
Isolated SignedTx review returned MERGE: symmetric r/s serde-name changes
compiled and failed SignedTx.signature.r native identity at exit101. Restored
9/9 tests passed; strict host and wasm all-target Clippy passed separately at
jobs1. Parity7334/3252. Counts pending/ported/divergent: venue186/132/18,
SDK268/58/63. No production signing or decoder-consumer claim.

### NaN capture normalization

The capture-only powf input helper canonicalizes NaN alone. Focused tests2/2
passed, including named child failures for signed-zero flattening and finite
corruption. An isolated reviewer checked16 IEEE inputs and killed removal of
canonicalization with the payload-NaN witness; MERGE, scoped strict host Clippy
passed. Historical report and shared signing fixture bytes remain unchanged.
The 21-producer census and its limits are in NAN_CAPTURE_AUDIT.md.

### Generic wrapper identity

Two records now consume distinct Node-side fixtures and assert all12 native
fields, including nested payloads and four pagination boolean combinations.
Isolated MERGE: symmetric public/private hasNext/hasPrev renames compiled and
failed PaginatedResponse.hasNext native identity at101. Restored4/4 and scoped
strict host Clippy passed; no production or manifest changes.

### SDK order native identity

All13 older order records now read native fields and vary each axis independently.
Isolated MERGE: symmetric public/private reduceOnly/postOnly names compiled and
failed OrderInfo.reduceOnly native field identity at101. Restored3/3 and scoped
strict host Clippy passed; production source restored byte-identically.

### SDK response native identity

Twenty-two native projections recurse into child arrays and name closed enum
variants exhaustively. Isolated MERGE: compiling symmetric serde names failed
HealthResponse.database native field identity and FeeSchedule native field
identity (nested FeeTier minVolume/feeRate) at101. Restored16/16 and scoped host
strict Clippy passed. Same-corpus Node captures reduced9to1 per process without
combining independently mutated source-control processes. Counts unchanged.

### Transaction leaf identity and capture reuse

Fifteen record projections cover native fields and nested opts independently of
outer union checks. Isolated MERGE: compiling symmetric reduceOnly/postOnly names
failed CreateOrderTxReq.reduceOnly native field identity at101. Restored targets
passed separately9/9 and4/4; each scoped strict host Clippy passed. Same-corpus
Node invocations fall4to1 for transactions and2to1 for generics. Counts unchanged.

### Later signing field-identity supplement

The mandatory venue gate captures24 later cases and48 real WASM calls with distinct
fees and compares each raw preimage fee word directly with the outer request.
The actual source maker/spot-taker swap fails maxPerpsMakerFee preimage identity;
a reviewer Rust consumer word8/9 swap compiled and failed the distinct-fee
Poseidon assertion at101. Restored3/3 and scoped strict host Clippy passed after
repairing the hex-iterator lint; isolated verdict MERGE. No wasm execution claim.

Original92-case bytes remain SHA256
afe399cc424ff3e4e5393ef9c2f8f642b770530328f6cc70d6aefd830383679a, including
under the actual source fee swap: independently reproduced byte-identical cmp0.
This is the original corpus's measured blind spot, not a signer-defect claim.
The later generated artifact is ignored at .aimux/captures/signing-field-identity.json,
SHA256 fe35cc51111d42ce88c35268a5ae105783a661a7101855822b91285b35f1d12f,
captured on darwin arm64. The committed live capture/test is the portable gate;
At that checkpoint shared adoption remained a separate crypto-owner action;
it was subsequently published at 74aa2775f1, recorded below.
There is no retroactive independence or production signing-parity claim.

### Native-only signer and approval record identity

Eight records examined, seven with repeated same-typed edge values. Added distinct
construction/Clone cases and all eight create-order boolean combinations. Isolated
MERGE: production Clone fee swap compiled then failed the named maker-fee identity
at101; source restored. Each new test and each scoped strict host lint passed
separately. New Clone cases cover present optionals; old construction cases retain
absence coverage. No request-decoder or signing-runtime claim.

### Older numeric-ingress capture batching

Selected Node invocations fall from 104 to 1, preserving all numeric lexemes and
Unicode raw strings. Eight separate unknown-depth captures remain, so the whole
target falls from 112 to 9. Scoped target passed 3/3; scoped strict host Clippy
passed. Isolated reviewer returned MERGE after checking all 104 exact inputs,
lexical negative zero, escaped surrogates and unchanged entry-path assertions.
No production behavior or parity status changed.

### Deployment profile decoded identity

Seven native fields and all four proxy-flag combinations are asserted directly.
Isolated MERGE: private-Wire-only proxy serde-name swap compiled and failed
DeploymentProfile.usesRestProxy native identity at 101, leaving public serialization
and constant tables unchanged. Exact source restoration returned 1/1; scoped
strict host Clippy passed. No status changes or wasm-execution claim.

### Older scalar record consumer identity

Three record checks in live_pure previously re-encoded decoded records. They now
read every native field from distinct inputs; the older same-value edge corpus
remains. Isolated MERGE: symmetric public/private minimum-field serde names
compiled and failed LighterOrderMinimums.minBaseAmount native field identity at
101. Exact source restored, scoped baseline 1/1 and strict host Clippy passed.
The separate numeric-ingress gate already read native numbers; no production
defect or parity-status change is claimed.

### Utility result identity

Source capture contains 241 minimum cases, including two added distinct snake/camel
inputs. Native minimum/cancel fields are now checked before serialization.
Isolated MERGE: returned base/quote swap compiled and failed
getLighterOrderMinimums.minBaseAmount native identity at 101, first on the existing
7.5x/8.5x fallback case. Source restored, target 1/1 and scoped strict host Clippy
passed. This strengthens this consumer; no production defect is asserted.

### Close-classification native identity

Two older decoders gain successful identity coverage: all 4 public and 64 private
boolean combinations, plus native classification result projections. Isolated
MERGE: private decoder handshake/rate-limit swap compiled and failed at 101 on
LighterPrivateCloseClassification.isHandshakeFailure native decode identity mask 1.
Source restored, scoped test 1/1 and strict host Clippy passed. Manual assertion-
shape inventory covers all 24 top-level targets and remains a partial fixture
sweep. Reviewer wording correction distinguishes order ID arrays from typed
child records. No production or status changes.

### Published shared supplement adoption

Inspected 74aa2775f1 on origin/feat/rust-venue-hyperliquid: the adoption document,
nonignored independent_lighter_field_identity test and both root crypto gate
entries. Compared generator bytes against ab2aadb8f5: identical, SHA256
99a7a0b050d6b86f7a74c1b6072fd93ed76035712e89ffa956f5518b27473cba.
Future local supplement headers name this adoption; original capture mode and
the archived ignored supplement are unchanged. The crypto owner's 3/3, Node24
comparison and mutation results are owner-reported, not rerun by this venue.
No integration, live CI or retroactive independence claim.

The updated local supplement header passed 3/3 native tests, including the
permanent compiling child source-swap control. Metadata-only sub-agent review
returned MERGE and confirmed the default 92-case capture branch is unchanged.

### Store notification cutover merge-down

Integration parent 9419b340d8a6: 248 incoming files matched byte-for-byte before
local audit documentation updates. CI named steps 90/90 to 90; real tomllib
359 exemption blocks / 359 distinct targets, complete values and reasons from
both parents retained. Lighter Cargo.toml and layer policy unchanged. Lock delta
is upstream dependency edges for Binance and Phemex, with no package versions
changed. Scoped all-target cargo check passed at jobs1 without venue migrations.

Manual reachability remains zero over the same 94 ported and 14 divergent
callables. A one-process source AST scan broadens the pending call-site inventory
to 25 mutations across 14 pending rows. No notification-related FINDING exists
to promote. See STORE_NOTIFICATION_AUDIT.md; counts remain venue132/18/186 and
SDK58/63/268. No subscriber failure is discarded or converted by venue code.

The real Yarn entrypoint ran parity:extract:check and returned 1: seven added
symbols, zero orphaned, seven manifests would update (base, base.ws, Phemex API
and types, unknownSendOutcome, ChaserOrderManager and ChaserOrderOptions). All
reported paths match the incoming parent; no Lighter source drift was reported.
This inherited red is routed, not repaired in another owner's files. The local
extractor dependency-install caveat remains; no clean-install proof is claimed.
This is merge-down verification, not a full merge-up sheet or host/wasm lint
claim. No new runtime notification parity is claimed by a zero-site migration.

### Subscriber identity merge-down

Parents aa703c3614 and integration 9af82bfeae: all 66 incoming files match the
incoming parent exactly. CI named steps 90/90 to 90; real tomllib reports 359
blocks / 359 distinct exemption targets with every complete parent entry retained.
Own Cargo.toml and layers.json are unchanged. The lock adds only incoming BingX
dependency edges; no package version changes. Scoped all-target Lighter cargo
check with --locked and jobs1 passes. Workflow and wiring inputs are unchanged.

Inspected both shared conversion branches reading subscriber_id() and appending
inner-to-outer metadata. STORE_NOTIFICATION_AUDIT.md closes the historical missing-
identity finding, with zero local boxes or runtime Store conversion sites. Counts
remain venue132/18/186 and SDK58/63/268. Core's reported 20 tests and independent
empty-list mutations were not rerun here; this is source verification plus venue
compilation, not a new full scoped sheet or host/wasm Clippy result. The earlier
extractor result remains scoped to its recorded tree and is not relabeled here.

### Integration refresh and borrowed-collection identity bucket

Merge f3eb5d91a1 adopts fetched integration 710e663156. All 35 incoming files
match that parent. CI named steps 90/90 to 90; real TOML 359 blocks / 359 distinct,
with complete parent entries retained. Own Cargo.toml and layers unchanged;
the only lock delta adds Bitget's candle-cache edge. Scoped locked all-target
check at jobs1 passes with no new diagnostics. No full readiness sheet claimed.

The 24 target shapes are partitioned in FIELD_IDENTITY_AUDIT.md. This slice
strengthens four collection outputs in two consumers: native bid/ask slices and
ordered Value references replace re-encoded collection comparisons. One shared
source supplement contributes eight public and eight private inputs to existing
captures. It adds no Node subprocess. Public REST inventory is 2298; private
flatten inventory is 530. Baselines passed live_pure 5/5 and live_ws_state 3/3.
The new public inventory assertion was added afterward; its verification is
recorded separately below. No old surviving bid/ask mutation or new production
defect is inferred from the former comparison shape.

Independent sub-agent review returned MERGE. Compiling bid/ask swap failed101
at parseRestOrderBookLevels.bids native identity; flatten reversal failed101 at
flattenByMarketPayload. Both captures rejected collapsed supplement sides and
booleans by name. The pre-repair bid/ask swap also failed101, so no historical
blindness is claimed there. Restored baselines passed5/5 and3/3, including the
new inventory assertion; scoped strict host Clippy passed separately. All
mutations restored. No wasm lint or full merge-up readiness claimed for this
slice. Counts remain venue132/18/186, SDK58/63/268; broader sweep stays open.

### Scalar and opaque-helper bucket

Three targets now pin their source-case inventories; signer provider and tier
asRecord comparisons read native borrowed Value results, and tier successful
Deserialize/normalization results use exhaustive native variant names. Two
existing distinct nested records supplement the signer/tier inputs without new
Node processes. Exchange-helper corpora already have competing distinct axes;
no equal-step or equal-field blindness is inferred from repeated edge values.

Independent sub-agent MERGE: compiling signer code/message swap failed101 at
isLighterInvalidNonceError; private tier wire mapping swap, retaining variant
inventory order, failed101 at LighterAccountTier native decoded identity.
Empty-corpus controls failed101 at referencePrices, nonce and normalized
inventory assertions. Restored baselines utils1/1, exchange2/2, signer1/1,
tier1/1 passed; strict host Clippy passed separately on the three changed targets.
No new wasm lint or full merge-up sheet is claimed.

The bracket-class audit mutated the two production scalar precision fallback
reads against the unchanged utils corpus. Compilation succeeded; the assertion
getLighterAmountDecimals failed101 on amount0.01/price0.1, expected2 versus1.
Production bytes restored and baseline1/1 passed. No surviving mutation or
formatter immunity is claimed. The three pending source formatter rows and
trigger/amount/price call sites are inventoried in FIELD_IDENTITY_AUDIT.md.
Counts remain venue132/18/186 and SDK58/63/268.

### Independent precision fallback controls

At 77cf56f03c, sub-agent tested amount-only and price-only fallback mutations
separately. Both compiled and failed101: getLighterAmountDecimals (got1 expected2)
and getLighterPriceDecimals (got2 expected1), using unchanged amount0.01/price0.1.
Restored production bytes exactly and scoped target passed1/1. The generator
hash is recorded in FIELD_IDENTITY_AUDIT.md; zero fixture-value repairs. This
supersedes any inference of independent price coverage from the earlier combined
swap. Coverage is current and per scalar site, not historical or per venue.
No new lint or full readiness claim; no manifest classification changes.

### MEXC integration repair merge-down

Fetched integration e53d89eddeef1fc1dddf919fed994d0c8b9df48b. All 73 incoming
files match that parent exactly; named CI steps90/90 to90; real tomllib359 blocks
and359 distinct targets, complete parent entries retained. Own Cargo.toml and
layers unchanged. The only lock delta adds MEXC's existing indexmap2.14.2 edge;
no package versions change. Inspected the failed open-orders match: its log arm
now propagates with ?, rather than discarding the Result to make both arms unit.

Scoped locked all-target Lighter check, jobs1, passed after this merge. Earlier
Lighter checks did not include MEXC or a wasm workspace build; they are not
workspace-build evidence. No MEXC test, wasm workspace build, new host/wasm lint
or full merge-up readiness claim is made here. Lighter manifests/code are unchanged.

Cardinality audit based on 8ad81895ba (in progress; no merge-up sheet): the
24-target inspection is recorded in FIELD_IDENTITY_AUDIT.md. The reviewer
reproduced a real false green by setting utility capture scalars.length=0:
the old test compiled and passed1/1, while the new test failed101 at
UTILS-CARDINALITY scalars source cases, 0 versus456. Permanent empty and subset
capture controls retain this distinction without changing normal corpus values.

Six cancel-output mutants compiled and failed101 at
resolveLighterCancelTargets inventory: empty, subset and extra outputs, each
with source guards present and bypassed. Fifteen additional output mutants
compiled and failed101 with source totals bypassed: bids and asks independently
at REST-CARDINALITY native bid/ask count, ticker items at native ticker count,
and both flatteners at FLATTEN-CARDINALITY native payload/orders count.
Restoration was byte-checked; restored primary pure and ws targets passed1/1
and the utility target passed2/2 before later permanent controls were added.
These are post-computation mutations, not consuming-deserializer evidence.

The immutable92-case signing artifact still has SHA256
`afe399cc424ff3e4e5393ef9c2f8f642b770530328f6cc70d6aefd830383679a`.
Neither signing generator changed. Later consumer cardinality assertions do not
retroactively change the original independence claim or claim shared adoption.
TxReceipt remains separate uncommitted WIP, excluded from this audit's claims.

Integration a09ea6489b was fetched and merged at d922626f79. All three incoming
manifest files match the incoming commit byte-for-byte and parse with tomllib;
CI remains90/90/90 and exempt.toml359 blocks/359 distinct, with complete parent
bytes preserved. Own Cargo, lock and layers are unchanged on both parents.
Incremental sub-agent verdict MERGE. Real-Yarn parity:extract:check exits0:
0 manifests updated, 0 added, 0 orphaned. No clean-install claim is made.

The nine cardinality targets passed29 tests with zero failed/ignored after
updating source-control names for their newly earlier total-count failures.
Strict host Clippy over those nine targets passes after replacing singleton
loops with direct assertions. This is scoped test evidence with separate SDK
TxReceipt WIP present, not a whole-crate readiness sheet or wasm build claim.

## SDK consuming collection audit

The 15 public Vec fields have 75 covered field/entry combinations: nine response
fields across six entries, five common/order fields across three bare entries,
and paginated items across six entries. Fixed native item comparison totals
are 24 per response field, six per bare-only field and 12 for paginated items.
The later six-field Node supplement checks collection declarations only;
it is not a complete schema oracle and changes no historical fixture bytes.

The permanent runner's nine-response and six-field consuming mutations both
compiled, exiting 101 at every named SDK-CARDINALITY Type.field assertion.
Successful decoding preceded the failures; no trailing-input error counted.
Both retained-mutant/native-count-guards-removed runs also exited 101 through
native identity. Count guards are redundant for these mutants, not proven
load-bearing or generally removable. Restored baselines passed 9/9 and 6/6,
with shared Array and both private Wire production files restored byte-for-byte.

Proof was run with the separate TxReceipt WIP present; it remains excluded from
this audit commit and from any readiness claim. No workspace build or live CI
claim is made. Real tomllib counts remain venue132/18/186 and SDK58/63/268
(ported/divergent/pending). No manifest or production behavior changed.

Strict host Clippy and separate wasm32-unknown-unknown Clippy passed for
live_sdk_responses and live_sdk_collections with --locked --jobs 1 and
-D warnings. Scoped rustfmt --check passed for both test files. The runner's
interruption cleanup was also exercised with a sleeping child: it terminated
and waited for its own process group before propagating KeyboardInterrupt.

## TxReceipt SDK record

The live source census pins eight fields and nine closed numeric transaction
tags. Status and both optional block numbers retain bare/shared raw, owned and
borrowed numeric bits. Optional metadata rejects null; data distinguishes
absence from present null and retains its numeric payload through JsNumberInput.
Exact inventories pin 30 field/spelling numeric cases, 20 payload-location/
spelling cases, 45 field/UTF16 string cases and six payload depths. Accepted
depths traverse every native array, assert one item per level, then read leaf17
as binary64. Unknown extras retain the separate bounded-capture/UTF8 witnesses.

An independent reviewer reproduced an UNCOMMITTED test-gap false green:
injected tx_hash.trim() compiled and passed 2/2 because the prior Unicode loop
checked acceptance only. Baseline production did not trim. The repaired tests
read retained native strings and payload strings, rather than treating successful
decoding as identity. The repaired baseline passes 2/2.

The row is MIXED: bounded capture and the shared UTF8 boundary remain independent
of the non-JSON any finding. Actual-source data properties with undefined,
bigint, function and symbol values are witnessed before serialization; the
Rust JSON carrier cannot admit them. No cycles/alias proof or production
TransactionClient integration is claimed. Working counts from tomllib are
venue132/18/186 and SDK58/64/267 (ported/divergent/pending).

Both repaired Rust controls compile and fail at exit 101: symmetric public and
private blockNumber/blockTimestamp serde-name exchange fails
TxReceipt.blockNumber native identity, and the previously surviving trim fails
TxReceipt txHash positive Unicode native identity. These distinguish native
field identity from symmetric roundtrip agreement and mere string acceptance.

Restored receipt tests pass 2/2. Strict host and separate wasm32-unknown-unknown
Clippy pass for live_sdk_transactions with --locked --jobs 1 and -D warnings.
Scoped rustfmt passes for the three changed Rust files. Real-Yarn parity:check
passes at 7341 rows / 3386 public Rust items. This is local slice evidence;
PR7334 remains pinned at d41c2f06c7, and no current-integration merge-up readiness
or workspace build is claimed.

## SDK batch records — separate follow-on branch

Three type rows: BatchTxReq, BatchTxResponse and BatchCreateOrdersParams.
The source checker measures overlapping structural unions, not runtime dispatch.
Private retained-input wrappers validate membership without projecting a selected
member. The response error map independently records its narrower string-value
contract: source Record<number,string> accepts a nonnumeric foo:123 extra.
Eight source-checker and post-return typeof witnesses retain own undefined,
bigint, function and symbol child extras; the Rust JSON carrier excludes them.
No TransactionClient operation or production SDK decoder adoption is claimed.

The later oracle has 14 primary cases, 340 numeric cases (34 exact paths times
10 pinned lexemes), 81 Unicode cases, 40 depth cases and eight non-JSON witnesses.
All nine source schemas are pinned. A complete primary-case digest pins values,
not only counts; changing quantity 12.5 to 12.75 leaves counts intact and fails
`batch independent complete fixture contents`. Eight permanent source controls
run as separate changed corpora in one selected test; these are named Node
failures, not claims of eight separate Rust exit-101 runs. Each ordinary test
process captures its source corpus once. Capture metadata names Node version,
architecture and platform. These JSON numeric spellings cannot produce NaN;
negative zero, finite numbers, overflow and negative underflow remain bit-exact.

`live_sdk_batches -- --include-ignored` passed 9/0/0 before the production
controls. Native assertions cover fields, retained overlap extras, array order
and identity, and dictionary keys 2, 10, alpha separately from their values.
Both bare and shared raw/owned/borrowed entries accept root extras at depth126
and refuse127; union child extras accept124 and refuse125, with their additional
array/object nesting. Accepted retained child chains are read through their
native buffers down to leaf17. Root extras are discarded and only their
acceptance is claimed. Node accepts140. This is a CHOSEN shared bounded-capture
policy, independently recorded from UTF8 and non-JSON restrictions.

`check-batch-controls.py --exclusive-worktree` compiled the actual shared
Array::next_element_seed draining-all-before-None mutant. Five selected targets
went 5/0/0 -> 0/5/0 exit101 -> 5/0/0 restored. Each failure was a named
`BATCH-CARDINALITY Type.field bare raw native length` after successful decoding.
Keeping that mutant and removing both native-length and comparison-count guards
still produced 0/5/0 at `native collection identity`. The guards are redundant
FOR THESE MUTANTS, not proven load-bearing; they remain in the gate. Mutant
failures stop on the first bare-raw assertion; the restored baseline, rather
than each failing mutant, exercises all six entry paths.

The same permanent runner compiles independent symmetric public/private serde
name swaps for each envelope's accountIndex/apiKeyIndex and the response's
successIndices/failedIndices. Each exits101: respectively
`BatchTxReq "all-four" bare raw.accountIndex native identity`,
`BatchCreateOrdersParams "both-orders" bare raw.accountIndex native identity`,
and `BATCH-CARDINALITY BatchTxResponse.successIndices bare raw native length`.
All watched source bytes were restored; the shared numeric file has no diff.

The adversarial review reproduced a checker false green before commit: removing
constants imports from the in-memory source left 21 diagnostics while every
admission/content assertion passed. Clean source has zero diagnostics. The
oracle now refuses nonempty getPreEmitDiagnostics before checker use, and the
permanent missing-import control fails `batch source program diagnostics must
be empty: 21`. This repaired the instrument, not production code.

Independent sub-agent implementation review returned MERGE after its own
compiling projection control: BatchCreateOrder retained a typed market-order
projection instead of the original object. The unchanged corpus failed 0/1/0,
exit101, at `BatchCreateOrdersParams "both-orders" bare raw retained own key
count`, with seven native keys versus twelve source keys. Full selected baseline
9/0/0 and byte-identical restoration 9/0/0 surround that control. The reviewer
also independently re-ran the missing-import probe and confirmed diagnostic
refusal. No historical fixture bytes were rewritten.

Slice gates use jobs1: strict host and separate wasm clippy target
`live_sdk_batches`; parity7341 rows/3393 public items; extractor zero updates,
zero added/orphaned; layers78 passed; tomllib359 blocks/359 distinct, no blank
reasons. Extractor uses this locally provisioned dependency tree, not a new
clean-install proof. No Cargo.lock, package script, workflow, layer container or
shared production source changes belong to the slice. This is a scoped slice
review, not a current-integration merge-up, workspace-build or live-CI claim.

## Older checker diagnostics audit

Census: three older scripts use createProgram. The protocol-types unresolved
TxType alias mutant already failed its closed-union assertion. The transaction
oracle accepted a selective removal of OrderType/TimeInForce/OrderSide imports
while retaining TxTypeValue: six unresolved-name diagnostics, yet byte-identical
output. The constant-alias oracle accepted a broken export-star path with one
diagnostic and byte-identical output. These are two reproduced instrument gaps,
not findings about production Rust behavior or grounds to relabel its rows.

Collecting diagnostics early changes the legacy alias checker's union emission
order. The repair checks diagnostics after existing evaluation but before any
stdout capture is published, preserving historical bytes without accepting an
uncompilable source program. Permanent source controls retain the exact two
mutations and fail on named diagnostic refusals through their existing Rust
child-test path. No source fixture is regenerated and no manifest count changes.

Repair verification: transaction-record selection3/0/0 and constant-alias
selection3/0/0, with the two new child controls asserting exit101 and their
six/one-diagnostic names. Strict host and separate wasm Clippy cover both test
targets. Independent sub-agent review reproduced byte-identical old/new output
and the two source injections without using the permanent env controls. Clean
capture SHA256s remain transactions
2fd28498a76572ce17333873be3ea9f114fbdd50b33f600ee72679aad3db614c and aliases
e01c6e058d63b74452804733386f6173b415ee1e77331bec522b03b5e697473f.

## SDK order API record slice

`ApiOrder` and `AccountOrdersResponse` adopt the existing SDK numeric OrderStatus
wrapper with explicit output eligibility. The runtime OrderApi methods remain
pending. The selected declaration oracle uses byte-exact interface/import text;
its zero diagnostics do not assert that the Axios/client closure compiles.

The scoped `live_sdk_order_api` baseline is 8 passed / 0 failed / 0 ignored.
It reads all native fields before any wire assertion, crosses six numeric entry
paths, and independently pins all eight boolean combinations. A live SHA256
assertion over the complete corpus body protects encoded inputs and metadata;
only actual runtime header metadata is excluded. Same-length price replacement,
array reversal and a surrogate encoded-input substitution all fail by name.
This is a running content assertion, not a digest of an archived capture.

An external exact selected-source mirror reproduces content hash
`e0117b9257f0a13eebe4814e37a12847dd425d1e64e31436309b8bb414881bdb`.
Both mirror and ambient missing-compiler controls exit 1 with MODULE_NOT_FOUND.
Capture metadata records Node 26.7.0, arm64 and darwin; no other runtime claim is
made. Numeric lexemes include overflow and negative underflow but cannot contain
NaN; their finite values, infinities and signed zeros remain bit-exact.

Unknown-extra depth is independently priced: root 126 accepts / 127 refuses;
orders[0].extra 124 accepts / 125 refuses across six paths. Node accepts 140.
String and unknown-extra corpora record post-JSON.parse UTF16 units and positive
scalar/astral/BOM/NEL cases. Lone-unit refusals are raw-only witnesses because a
Rust Value cannot carry those materialized units. Optional-null and malformed
shape refusals are scoped to the paths actually tested, not generalized from the
six-path numeric result.

The production shared Array consuming-empty mutant compiles and changes the
selected collection target from 1/0/0 to 0/1/0 at `ORDER-API orders native length
bare raw` (0 versus 1). Keeping that mutant while removing both count guards
still gives 0/1/0 at `ORDER-API orders native collection identity`. The count
guards are redundant for this mutant, not proven load-bearing; they are retained.
Each failing target reaches the first nonempty bare-raw case, so the mutant run
is not claimed to prove every path covered by the six-path baseline.

Symmetric public/private serde-name swaps for createdAt/updatedAt and
postOnly/reduceOnly each compile and fail 101 at the respective
`ApiOrder.createdAt native identity` and `ApiOrder.postOnly native identity`.
Restored collection baseline is 1/0/0; shared numeric source restores exactly to
HEAD. The permanent runner subsequently skips writes of already-restored bytes;
that does not change mutant content or assertions and avoids needless rebuilds.

Selected-target strict host clippy and separate wasm32-unknown-unknown clippy
both exit 0 with jobs 1. The extractor writer updates only this slice's manifest,
with 0 added / 0 orphaned symbols. This slice does not repeat or relabel the
historical full-crate result attached to the previous consolidation SHA.

## Fresh-agent order-book delivery

Measured SHA: b7fd9f6406c122d9f7f377c57d1150baff6bed80.
Actual parent: fe620c0f16fef85d7556cb253e66a5ffedf6cb80 (surrogate-control
repair), whose parent is inherited b03cca651537130e4ca470bae2871c60749fe638.
Integration ancestry base is 3c5044dc1a5cf1f05b7fd34eea0a3b70c8bf747b;
this branch was not refreshed to integration 2bc83d78d4. The inherited order-API
commit and this three-record slice received fresh read-only sub-agent reviews,
with no outstanding production finding in their stated record floors.

Rows moved in this slice, measured with Python 3.14 tomllib against b03cca6515:
0 pending->ported, 3 pending->divergent. SDK triple at measured SHA is
58 ported / 72 divergent / 259 pending. At requested integration 2bc83d78d4,
tomllib measured SDK 58/67/264, venue 132/18/186 and Poseidon 0/2/20 across
seven Poseidon manifests. The inherited order-API slice accounts for the other
two SDK pending->divergent rows relative to that integration snapshot.

Measured gates at the SHA above, with tracked source restored exactly:
- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p
  safe-cex-venue-lighter --all-targets --jobs 2`: exit 0. An initial attempt
  exited 101 because the next slice's untracked test was present before its
  module export; it was moved out of Cargo's test directory. A prematurely
  launched retry waited on the mutation runner's build lock and was terminated
  at exit 143; final serial retry above exited 0.
- Scoped cargo test, `live_sdk_order_api` plus `live_sdk_order_books`, jobs 2,
  include-ignored: exit 0, respectively 8/0/0 and 7/0/0. Source controls include
  actual-import schema/property guards and same-length surrogate substitution.
- Extractor check through the specified Node/Yarn command: exit 0; output text
  says `Would update 0 manifests; 0 added, 0 orphaned symbols.` Writer previously
  exited 0 with `Updated 0 manifests`; scalar annotations were already canonical.
- `check-order-books-controls.py --exclusive-worktree`: exit 0. Baseline
  collection target exit 0 (1/0/0); compiled consuming-array mutant exit 101
  (0/1/0), named `BOOKS collection native length`; compiled symmetric public and
  private bestBid/bestAsk rename mutant exit 101 (0/1/0), named `BOOKS
  OrderBooksResponse bare raw native field identity`; restored target exit 0
  (1/0/0). Shared numeric source and model restored byte-exactly. Mutant runs
  stop at their first failing bare-raw case, not all six paths.

Enumeration floor: three new response trees, ten numeric leaves x nine lexemes,
17 string leaves x nine inputs, six collection axes x three modes, nine unknown
locations x nine strings/seven depths. Native projection precedes wire checks;
all base book/detail/bid arrays now have distinct reversal-sensitive content.
Review found and repaired imported-level type/method/optional drift survivors
and a palindromic collection-only fixture limitation before this measured SHA.

Not run: workspace compilation, host clippy, wasm clippy/check, full monorepo
suites, full SDK semantic compilation, HTTP/transaction runtime, live exchange
or CI. No PR opened or merged. Unlinked next-slice source files were present;
they are excluded from this SHA's claims.

## TransactionClient pure helpers

Measured SHA f6abb9e551a088da28dcf4068e76a75b675d76ad; actual parent
6a8cac00dc (order-book evidence only). The integration ancestry remains
3c5044dc1a; no merge-down or integration/CI claim is made.
Python 3.14 tomllib comparison against parent measured 9 pending->ported and
1 pending->divergent. SDK triple is now 67 ported / 73 divergent / 249 pending;
transaction-client.ts has 9 ported / 1 divergent / 41 pending of 51 rows.

At that SHA:
- Scoped cargo check for safe-cex-venue-lighter, all-targets, jobs2: exit0.
- Scoped live_transaction_client_helpers cargo test, jobs2/include-ignored:
  exit0, 1/0/0. Native strings, UTF16 error units, numbers and booleans are read
  directly; no wire roundtrip is used. All input columns/counts are independently
  pinned: 13 UTF16 cases, 3 byte vectors, 11 uint32 and 7 int64 inputs, 7 order
  types, 105 type/post-only/TIF combinations and 13 numeric classifiers.
- Compiled production uint32 mutation to_be_bytes in place of to_le_bytes:
  exit101, 0/1/0, `toUint32FieldHex native success`, for source value1
  (`0000000100000000` versus `0100000000000000`). Exact bytes restored; rerun
  exit0, 1/0/0. This mutation stops at the first mismatching uint32 value and
  does not individually prove all ten functions' discrimination.
- Specified extractor check exit0; output `Would update 0 manifests; 0 added,
  0 orphaned symbols.` Writer earlier exited0 and updated1 manifest.
- Fresh read-only sub-agent review of the SHA: no outstanding findings in the
  ten-helper floor. Independent input pins and declared CHOSEN always-available
  native UTF8 encoder close the two review scope gaps. Both actual TS encoder
  branches and the no-encoder error are captured; native encoding always exists.

Native UTF16 preserves lone units in prefix stripping and dynamic range-error
names. UTF8 encoding replaces unpaired units as TextEncoder/Buffer do; uint32
-0 equals +0, while classifiers retain ordinary JS comparisons for NaN/infinity.
Enumeration stops at the fixed inputs above and these ten pure methods.
Nonce locking/cache, parameter records, transaction preimages, signing,
submission and the TransactionClient class remain pending.

Not run: workspace compilation, host clippy, wasm clippy/check, full SDK semantic
compilation, monorepo suites, network/venue or CI. The oracle transpiles and
executes actual source helpers; it is not a full SDK typecheck. Poseidon record
files were unlinked and excluded from this SHA's claims.

## TransactionClient native parameters and unsigned builders

Moved **18 rows pending -> ported, 0 -> divergent**, measured with Python 3.14
`tomllib` against `0e5021eab6` over the lane's 26 SDK, 13 venue and 7 Poseidon
manifests. Only transaction-client.ts changes status: seven parameter/literal
union types, eight unsigned builders/validators, and three public getters.
Its triple is now **27 ported / 1 divergent / 23 pending** of 51 rows. SDK is
**85/73/231**, venue **132/18/186**, Poseidon **0/4/18**, ordered
ported/divergent/pending. This is the manifest enumeration floor, not a claim
that every runtime expression has been inventoried.

Tested payload SHA: **97dd0a1a8ffba05b558912ba75173a1ba500dbd0**.
Actual parent and fresh branch base: **0e5021eab698a61809bdab56bd3eebe44c24cb0c**.
Branch: `feat/rust-lighter-transaction-builders`. No pre-merge feature work is
an additional ancestor; the base is the requested integration merge. Read-only
sub-agent review confirmed committed source/type/oracle/test/manifest blobs
match reviewed bytes, with no outstanding finding in the 18-row floor. No
blanket review of every integration ancestor is implied.

Measured at this payload SHA, with exact source restoration after mutants:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter
  --all-targets --jobs 2`: **exit 0**.
- Scoped `live_transaction_builders` cargo test, jobs2/include-ignored:
  **exit 0, 4 passed / 0 failed / 0 ignored**. Node compiler path was the
  required `/Users/sam/cs/tealstreet-next/node_modules/typescript`.
- Compiled production approval preimage swap, perps maker versus spot taker:
  selected native-result test **exit 101, 0/1/0**, named
  `approval distinct fields decoded native words`. Words 8/9 are 43/17 instead
  of independently expected 17/43. Test stops at builder case index362 (first
  approval), before returned-hex equality; it does not prove each method's
  discrimination individually. Exact source restored; selected target
  **exit 0, 1/0/0**. The new production code has no panic/expect path.
- Permanent source validation-order child control asserts **exit 101** at
  `transaction builder live corpus content`. A sub-agent first found that
  moving duplicate-ID checking before same-market checking survived the
  original single-fault corpus byte-identically. Three composite cases, one
  per grouping, now change their errors and digest; independent reviewer
  injection and the permanent hook publish identical changed results. The
  original 81 cases stay unchanged; this proof stops at the added common-guard
  precedence cases and does not claim every possible error precedence.
- Specified Node/Yarn `parity:extract:check`: **exit 0**, with output text
  `Would update 0 manifests; 0 added, 0 orphaned symbols.` Writer after the last
  manifest edit exited0, `Updated 0 manifests; 0 added, 0 orphaned symbols.`
  The first writer in this slice updated1 manifest. No hand-written TOML array
  replaces the writer's canonical output.

An exploratory pre-commit test compile exited101 on an E0716 temporary borrow
in the new test; the binding was fixed before the tested payload. This result
is not relabeled as a passing gate. Final baseline, mutation/restoration and
scoped check above ran serially, one Cargo process at a time.

Native parameter fields and Clone are projected directly from decoded test-only
Node tags; source bigint interfaces have no JSON.stringify codec. Bigints retain
full magnitude, numeric fields retain binary64 bits, margin literal0 retains
negative zero, and L1 signature UTF16 retains unpaired units. Closed literal
unions stay closed. The getter clock panics if read; no-read tests include four
actual source contexts with distinct fields, large signed bigint, -0 and
nonfinite numbers. Class and constructor rows remain pending rather than mapping
a fabricated full TransactionClient to the explicit pure-method context.

Builder metadata and decoded LE native preimage words precede returned-string
comparisons. The independent fixed floor is **410 builder cases**:
311 create, 9 attributes, 4 cancel, 38 leverage, 29 approval and 19 transfer;
**84 group validations**, **one grouped-order info case**, **ten native record
fixtures** and **four getter contexts**. Create-order default expiry reads the
clock lazily at the source call sites; error class/message and clock count are
compared with actual source calls. L2 attributes keep source conversion,
zero-filter, ordering and padding. Transfer keeps minimum-width radix16 text,
including fractional/nonfinite and oversized/negative inputs; no extra range
validation is inferred from its comments. Rust transfer tests visit 15 special
numeric inputs and four large/signed bigint inputs. The reviewer additionally
compared a separate radix16 model to Node across 5,000 deterministic bit-pattern
iterations, finite subset only; that is not an additional Rust execution claim.

Live corpus digest is
`a54b6bbb656b67eb70b312ad28c820ed42e31d81fa339e2cd4dee1283ffd2ba9`.
Actual capture runtime was Node v26.7.0, arm64, darwin. The historical shared92
signing fixture and its zero-fee swap survivor remain unchanged and separately
named in the manifest; this distinct-fee gate does not rewrite archived proof.

Not run: workspace compilation, host clippy, wasm check/clippy, full SDK semantic
compilation, full monorepo suites, real cryptographic signing, live network or
CI. The source oracle transpiles/executes actual methods and checks parameter
AST shapes; it is not a full SDK typecheck. Class lifecycle, nonce allocation,
grouped crypto aggregation, signing, signed transaction wrappers, auth and HTTP
submission remain pending. No PR opened or merged. The evidence-only followup
commit does not relabel these exits as measurements of a different SHA.

## Signing composition and native HTTP transaction types — 2026-10-13

Measured payload SHA: `c4055a52e8c0542791ff5c2315ec33003fd43b82`.
Actual parent: `7fcfce7950c79407b19ec6f8987e37ec6eed9661` (merged integration head; no additional unreviewed lane ancestor).
This evidence commit does not relabel the measured gates as covering a different SHA.

Rows moved, independently measured with Python 3.11 tomllib over all lighter-sdk manifests against that parent: **15 pending rows: 14 ported, 1 divergent**. SDK census before: 85 ported / 73 divergent / 231 pending; after: 99 / 74 / 216. Ten native HTTP transaction types and four synchronous transaction composition functions are ported. Auth-token composition is CHOSEN divergent: synchronous Result versus source Promise rejection/microtask scheduling. Transaction-client remains 18 pending rows; its class, nonce scheduler and public submission wrappers are not implemented by this slice.

Measured gates (one Cargo invocation at a time, jobs 2):

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit 0**, output `Finished dev profile`, zero compiler errors. This is the scoped crate check, not workspace compilation.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`: **exit 0**, output `Updated 2 manifests; 0 added, 0 orphaned symbols.` Writer ran after manifest edits, before the payload commit.
- Same launcher `parity:extract:check`: **exit 0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols.` Output was read, not inferred from the Yarn exit.
- With `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`, `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_transaction_signing --test live_transaction_builders -- --include-ignored --nocapture`: **exit 0**, signing 2 passed and builders 4 passed. These are the restored, committed payload tests.
- Compiled negative control at `transaction_signing.rs::sign_transaction`, replacing `schnorr_sign(data, private_key)` with `schnorr_sign(&tx_hash, private_key)`: filtered `signing_native_results_and_calls` **exit 101**, failed named assertion `signTransaction native crypto call arguments/order/receiver`. The mocked signature alone is constant; the native call argument assertion discriminates the substitution. Source restored in try/finally; the full six tests above then passed. `git diff --exit-code c4055a52e8 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests`: **exit 0** after restoration. One compiled mutation attempted and caught; no survivor observed in this floor.

Native proof floor stops after 44 HTTP type records (ten symbols) and 52 composition cases (five functions), not an exhaustive domain or exception-side-effect enumeration. Independent method totals: 10 regular signing, 15 attributed signing, 2 signed create-order, 19 grouped construction, 6 auth. Type totals and grouped lengths are pinned independently: first 22 corpus rows represent absent optionals/empty group arrays, second 22 represent present optionals/four children. Tests directly project decoded native fields before callback comparisons; no decode/re-encode gate or native Serialize codec is claimed. UTF16 lone units, full BigInt, binary64 signed zero/nonfinite fields, optional values and child ordering are witnessed. Returned errors retain prefixes; opaque thrown carriers retain pointer identity and native UTF16 string, null marker, symbol marker and dynamic error payloads. The actual JS-to-Rust exception adapter is not implemented or tested.

Sub-agent review of the committed payload and exact parent closed with no outstanding findings. Reviewer independently ran the Node source capture (exit 0, 44 type / 52 composition), and source-only controls: original-data to hash changes traces in 9 cases, binding extracted fp5 changes receiver traces in 11. Both source-only probes exited 0; differing traces discriminate the changes. These are separate from the compiled control above. Review found and prompted repair of a native-derived grouped count expectation and insufficient concrete host payload witnesses; both are repaired in the measured SHA.

Not run: workspace builds, full monorepo suites, host clippy, wasm clippy, CI, network/HTTP tests, real Poseidon/Schnorr arithmetic, WASM loader lifecycle, full semantic TypeScript SDK check. Deterministic crypto callbacks establish composition only. Merge-down must be verified before push; unchanged crate source/tests retain these measured exits under the standing merge-down rule, and any changed gate inputs require the affected gate again.


Merge verification measured at `39bfdba65809f738a395d00b17b0d807d842f32a`: parents `643d72bcebdfbc16ca28b43c7b670bfd809b6bf3` (evidence tip) and `f436fc816390a8b4556ab7a2fe0ae85072337036` (fetched integration). Merge was clean. Diff from payload SHA across crate src/tests/scripts, packages/lighter-sdk and all lighter-sdk manifests: **exit 0**, byte-unchanged. Native tests and compiled control remain measurements of `c4055a52e8`, retained under the standing merge-down rule, not rerun claims. Shared dependencies changed, so the exact scoped Cargo check above was rerun at the merged SHA: **exit 0**, `Finished dev profile` in 1m 02s. Extractor check rerun at this same SHA: **exit 0**, `Would update 0 manifests; 0 added, 0 orphaned symbols.` A final fetch/merge and byte comparison are required immediately before push; any later unchanged merge does not relabel these gate measurements.

## Native TransactionClient / FIFO nonce promises — 2026-10-14

**19 pending rows moved: 4 ported, 15 CHOSEN divergent.** Additionally the existing auth-token divergent row is corrected to ported. Python3.11 tomllib census across all lighter-sdk manifests against actual slice base `5a488d641d7499ed6a84bd3d766ce86fdc4b85a3` and again against fetched integration `1181108d2dfeee7dd6cb1d21a3a39922e1e13b9e`: before 99 ported / 74 divergent / 216 pending; after 104 / 88 / 197. Transaction-client is 35 ported / 16 divergent / 0 pending (51 rows). Census is distinct from proof of full JavaScript behavior.

New ported rows: constructor, getNextNonce, resetNonce, SendTxBatchResponse. Native class and its fourteen public get/submit wrappers have real implementations but are **CHOSEN** divergent for owned Rust input records/vectors versus TypeScript's mutable caller aliases retained across awaits. Submit wrappers inherit that observable difference; it is not hidden behind a ported delegate label. Native promises keep eager invocation, FIFO microtasks, finally-release ordering, detached execution, readonly primitive values and shared mutable DTO/array identities. Auth-token's former synchronous body is now adopted through an eager settled/rejected native promise with deferred observers, and encoder availability is read after first readiness and message construction.

Measured SHAs and actual ancestry:

- Initial implementation `9dd62fb842f85bbfae1634586ee2d234a158be74`, actual parent `5a488d641d7499ed6a84bd3d766ce86fdc4b85a3`.
- Batch-schema guard repair `929a7816e84d26690bf6c4dd67d853d7d0594778`.
- First clean merge-down `c63442bee3920a69352636f2b13705860695603b`, parents `929a7816e84d26690bf6c4dd67d853d7d0594778` and integration `1181108d2dfeee7dd6cb1d21a3a39922e1e13b9e`. Crate src/tests/scripts, packages/lighter-sdk and lighter-sdk manifests diff from 929: **exit0**, byte-unchanged.
- **Final measured gate SHA `aef073a87ed747baa979e1469a78f9c04fb5135e`**, actual parent `c63442bee3920a69352636f2b13705860695603b`. This adds direct typed assertions before tagged projections and repairs stale inherited manifest prose. Evidence documentation does not relabel these measurements as covering its own later SHA. No separate unreviewed lane branch was used as an ancestor; integration was merged down, never merged into.

Gates at the final measured SHA (one Cargo invocation at a time, jobs2):

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, `Finished dev profile`, zero compiler errors (14.15s). Scoped crate check, not workspace compilation.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols.` (12.97s). Output inspected. Writer ran after each manifest edit: initial writer exit0 `Updated 2 manifests`; final metadata-only writer exit0 `Updated 0 manifests`. No manifest arrays were hand-spelled.
- With `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`, `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_transaction_client --test live_transaction_signing -- --include-ignored --nocapture`: **restored exit0**, client3 passed + signing2 passed. All completed before this report.
- Compiled negative control at `transaction_client.rs::TransactionClient::get_next_nonce`: cached increment1→2. Filtered `nonce_fifo_release_reset_and_detach` **exit101**, named assertion `fifo-cache nonce FIFO cached native value`: **actual decoded Option<BigInt> Some(44), independent source expectation Some(42)**. Negative enumeration stopped on the first FIFO-cache scenario. Production restored via try/finally; the full five tests above then passed. Restored source diff from the gate SHA: **exit0**. An earlier control at9dd also failed101/restored0; it was repeated because the final cache assertion now compares native BigInts directly rather than tagged JSON strings.
- `SAFE_CEX_TYPESCRIPT_PATH=... node sdk-rs/safe-cex-venue-lighter/scripts/transaction-client-schema-controls.cjs`: **exit0**. Actual-source baseline child0; inherited-required-field substitution child **exit1** at `batch response no inherited schema`; property→method substitution child **exit1** at `batch response property fields`; restored child0 with unchanged corpus hash. These controls modify source reads only in isolated children, never production files.

Earlier completed gates are separate measurements: check/extractor0 at9dd; restored client3/signing2 tests0 at9dd; client3/extractor0 plus schema controls0 (children1/1) at929; post-merge scoped check0 (1m07s), extractor0 (`Would update 0`), client3/signing2 tests0 atc634. The final SHA-bound gates above supersede the affected earlier input sets, not their historical evidence.

Proof floor stops after **41 public-method cases, 7 nonce scenarios, 3 batch DTO records**, plus the listed object/hash-array identity and ownership probes. Source microtask flush enumeration stops at100 turns; native queue driver has a500-task bound and drains until empty. Neither is a claim of exhaustive asynchronous behavior. Independent public method totals: getCreateOrder2, submitCreateOrder2, getCreateOrderTransactions4, submitCreateOrders4, getGrouped3, submitGrouped2, getCancel3, submitCancel2, getTransfer2, submitTransfer3, getLeverage2, submitLeverage2, getApprove3, submitApprove2, auth5. Native DTO lengths are independently pinned0/3/2; ordinary get-batch result length0/2; returned API batch hashes2; nonce observer counts1/2/3 per fixed scenario. None compare an artifact's length to itself.

Native checks precede tagged projections: actual cached Option<BigInt>, auth UTF16 units, optional batch values/number bits/union variants/hash arrays and host exception identity/payload. Tagged projections read decoded native fields explicitly; no production Serialize, native JSON codec or decode/re-encode gate is claimed. Probes cover cache FIFO before caller builds, concurrent fetch, rejection/finally release, failed forced refresh retaining cache, reset during fetch/repopulation, reset before queued continuations, dropped observers retaining work, explicit approval nonce bypass, empty batches, validation after nonce consumption, ready checks disabling encoders, HTTP rejection, and native DTO/hash-array alias identity. Deterministic crypto callbacks prove orchestration only.

**Survivors/findings:** reviewer measured initial batch-schema inherited-required-field and method-shaped txHashes mutations surviving with Node0 and byte-identical corpus. Call site: `transaction-client-oracle.cjs`, SendTxBatchResponse declaration census. Parse/heritage/PropertySignature guards now reject both by name; repaired controls above close those findings. Source caller mutation after `getCreateOrderTransaction(params)` changes price to99 before resumed build; native owned input keeps2401. That difference remains deliberately visible in `native_batch_type_identity_and_owned_input_finding`, and motivates the CHOSEN class/get/submit rows. Exactly one direct caller-mutation probe is measured; transitive submit/batch alias differences are classified from the call sites, not an exhaustive mutation enumeration. No compiled-control survivor observed in the stated FIFO floor.

Not run or claimed: workspace builds, full monorepo suites, host clippy, wasm clippy, CI reruns, network/HTTP integration, real Poseidon/Schnorr arithmetic, WASM lifecycle, full semantic TypeScript SDK build, JS-to-Rust promise/exception interoperability, thenable assimilation, unhandled-rejection reporting, dynamically overridden callback factories throwing synchronously, dynamic property getters/prototypes, or complete callback-side-effect enumeration. Promise queue semantics depend on the host contract to enqueue FIFO and never execute inline. Host HTTP/crypto implementations are separately tracked dependencies.

Sub-agent review closed at9dd/929 and final follow-upaef with no functional findings in the declared host scope. Independent source schema mutations failed by name after repair. Stale inherited pending prose was corrected; direct native assertions were reviewed. Reviewer ran Node probes only, never Cargo; compiled-control and Cargo exits above are parent-agent measurements.

Post-evidence merge verification measured at `3304bcd26a14a10d8e1dbd1f7de6ab9186b84ea5`: parents `14a6bd2920cb22c34949eff216942ccbcdff0a25` and integration `3e4e053b47fbfa44475712b68fdf53c23f5071cd`. Diff fromaef across crate src/tests/scripts, packages/lighter-sdk and SDK manifests: **exit0**, byte-unchanged. Native tests/controls remain measurements ofaef and are retained under the standing merge-down rule, not rerun claims. Shared websocket code and Cargo.lock changed, so scoped check was rerun here: **exit0** (`Finished dev profile`,20.87s), and extractor check here: **exit0**, `Would update 0 manifests; 0 added, 0 orphaned symbols.` Final fetch/merge/preservation comparison must occur immediately before push; later unchanged merges do not relabel these measured SHAs.

## SDK validation and HTTP model primitives — 2026-10-14

Measured payload SHA: `23595173ac81c85d4b5a2eef4a910a7b5b4f0258`.
Actual parent: `2c04dafa36dd53b183f10b974fcd0a4adf2ada1a`, the assigned merged integration head; no additional unreviewed lane ancestor. This evidence commit records measurements of that payload, without relabeling them as measurements of the documentation commit.

Independent Python 3.11 tomllib comparison of all lighter-sdk manifests against the actual parent measured **26 pending rows moved: 24 ported, two FORCED divergent**. Before:104 ported/88 divergent/197 pending; after:128/90/171. Thirteen validation functions, nine native value-level HTTP declarations and two bigint safe-integer helpers are ported. ValidationError class and constructor are FORCED divergent for JavaScript Error prototype/reflection/engine stack and replacement of unpaired UTF16 units in native Display; decoded native name/message units remain exact. Lighter venue manifests were untouched by this slice.

Measured gates, one Cargo invocation at a time, jobs2:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, output `Finished dev profile` (31.39s), zero compile errors. Scoped crate compilation only.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`: **exit0**, writer output `Updated 3 manifests; 0 added, 0 orphaned symbols.` This was before the payload commit. An earlier pre-edit writer also exited0; two writer invocations overlapped unintentionally, and the subsequent committed extractor check below verifies their final result.
- Same launcher `parity:extract:check`: **exit0**, output read: `Would update 0 manifests; 0 added, 0 orphaned symbols.` (36.92s).
- With `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`, `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_sdk_primitives --jobs 2 -- --include-ignored --nocapture`: restored committed payload **exit0**, four passed, zero failed. Earlier uncommitted attempts exposed a BigInt shift type annotation, an invalid raw lone-surrogate JSON object key, and the stale header collector; these failed101 before repair and are not SHA-bound final gate measurements.
- Compiled control1 at `sdk_validation.rs::validate_amount`: replace upper-bound `>` with `>=`. Filtered `scalar_validator_native_results` **exit101**, named failure `validateAmount native validation result kind`. It stopped on scalar iteration116 (zero-based), the accepted maximum amount; one guard mutation attempted.
- Compiled control2 at `live_sdk_primitives.rs`, `GetApiKeysResponse` decoded-child collector: reverse `.iter()` order. Filtered `http_models_native_fields` **exit101**, named failure `native API key index bits`, first child of model iteration15 (zero-based), variant1. One child-order mutation attempted.
- Both controls restored in try/finally; the complete four-test invocation above then passed0. `git diff --exit-code 23595173ac -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests`: **exit0**, proving restoration to the measured payload.

Independent expectation floors:310 scalar cases (five bigint validators24 each; three integer-number validators25 each; three string validators27 each; deadline34), ending iteration309, deadline58/minFuture60/clock-1500ms. Getter scenarios44, ending iteration43, reduceOnly first-read host throw; independent outcomes3 success/32 validation/9 opaque throw. Error constructor field witnesses3, ending iteration2, unpaired message units. Safe-integer helper cases9, ending iteration8, negative2^200. HTTP schema/record witnesses27 (nine declarations x three independently authored variants), ending iteration26, transfer-fee variant2. Array counts0/4 and order[2,1,2,0] preserve duplicate children and now discriminate reversal. These are bounded proof floors, not exhaustive runtime input or mutation enumerations.

Assertions inspect decoded native result brands, exact UTF16 fields, builtin exception kinds, f64 bits, full BigInt values, optional fields, ordered children and opaque exception/payload identity. No native encode/re-encode comparison or wire codec assertion exists; the source-only corpus digest pins oracle inputs/results, not Rust output. Native payload slots include undefined, NaN, huge BigInt, unpaired strings, Symbol and Function identity. Source helpers are evaluated from actual transpiled TS; Axios imports are stubbed without constructing the HTTP client. Nine HTTP declarations are checked by actual-source AST fields, property kind, heritage and generic signature.

**Survivors/findings:** initial `sdk-primitives-oracle.cjs` API-key fixture[2,1,2] was palindromic; reviewer reversed it in memory, Node exited0 and the capture digest stayed identical. This is a real test finding at `GetApiKeysResponse.apiKeys`, closed by asymmetric four-child fixtures and compiled control2. Reviewer's first floor mutation accidentally changed only the JSDoc example and survived unchanged; the corrected runtime `const now` floor-to-trunc mutation changed exactly scalar iterations308/309. Independent reviewer Node probes exited0 because they capture decisions rather than asserting Rust parity; changed capture digests expose the controls. No survivor is omitted. Header raw lone-surrogate key was a carrier failure, repaired by UTF16 entry arrays without replacing native key values.

Sub-agent review of code, oracle, tests and all three committed manifests closed without blocking findings. Reviewer independently checked26 targets/provenance notes and24ported/twoFORCED counts at23595173ac; ran Node/read-only probes only, no Cargo/extractor. Parent-agent measured the Cargo/extractor exits above.

Not run or claimed: workspace compilation, full monorepo suites, host or wasm clippy, CI reruns, network/HTTP transport, native JSON decoding, JS object alias/reflection/property enumeration, JS Error stack/prototype interoperability, real cryptography, browser document.all/HtmlDda oracle, full semantic TS SDK build, dynamic arguments outside the declared numeric minFuture/bigint-helper domain, or exhaustive validation input enumeration. HtmlDda has a native marker by inspection, without browser proof. Pending client methods/interceptors/hex-base64 conversion retain pending status and absent native targets.

Post-evidence merge verification measured at `ea1ffe00e28373cea37d9261e4ac8cf38c83e78b`: actual parents `5773af0993c76b1e0c85ae05fffbfaf4e3137467` and integration `1f6e115b0cb3aec1c42860a0f97cf71bd84376d8`. Diff from23595173ac across crate src/tests/scripts, packages/lighter-sdk and lighter-sdk manifests: **exit0**, byte-unchanged. Shared numeric dependencies/Cargo.lock changed; scoped all-targets check rerun here **exit0** (`Finished dev profile`,38.25s), the four live_sdk_primitives oracle tests rerun here **exit0**, four passed; extractor check here **exit0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols.` (21.85s). Negative controls remain measurements of23595173ac, not new mutation-run claims. Final fetch/merge and preservation comparison occur before push; unchanged later merges retain these measured SHAs explicitly.

Final prepush preservation measured at `eb1c5b377423def4a690b40cfde46004ff8db242`, parents `4f63eb53094232e024e27369898cb745ef8a463e 3a4f19c35ab9a8a795bef393a969448fc2cad908`: diff against ea1ffe00e2 across the slice src/tests/scripts, TS sources, SDK manifests, Cargo.lock and checked dependency sources **exit0**. Cargo/tests/extractor were measured at ea1ffe00e2 and retained through this unchanged merge; no fresh execution at the final documentation SHA is claimed. The initial base remains2c04dafa36; all lane payload work was sub-agent reviewed. Only user-owned untracked brief files remain.

## Account SDK values and asynchronous method bodies — 2026-10-14

Measured payload SHA: `e6ebe6fa6d442c42508eb550bb88636d629aa769`.
Actual parent: `ebcb51ffa55881e2e5a745c0d419028b0632da7d`, assigned merged integration head; no additional unreviewed lane ancestor. This evidence commit records measurements of that payload, without relabeling them as measurements of the documentation commit.

Independent Python3.11 tomllib comparison over every lighter-sdk manifest against the actual parent measured **22 pending rows moved:21 ported, one CHOSEN divergent**. Before:128 ported/90 divergent/171 pending; after:149/91/149. Thirteen native value-level account declarations, constructor shared-client retention and seven eager method bodies are ported. AccountApi class is CHOSEN divergent: native explicit client/runtime/ECMAScript host context versus the JavaScript mutable/reflected class and prototype surface. Lighter venue manifests were untouched.

Measured gates, sequential Cargo/jobs2:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, `Finished dev profile` (45.98s), zero compiler errors. Scoped crate check only.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`: **exit0**, read output `Updated 1 manifests; 0 added, 0 orphaned symbols.` Writer ran after manifest edits, before the payload commit (14.49s).
- Same launcher `parity:extract:check`: **exit0**, read output `Would update 0 manifests; 0 added, 0 orphaned symbols.` (32.03s).
- With `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`, `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_account_api --jobs 2 -- --include-ignored --nocapture`: restored committed payload **exit0**, two passed, zero failed.
- Compiled control1 at `sdk_account_api.rs::request`: bypass required async-return resolution and settle the returned object directly. Filtered `account_methods_native_results_and_reads` **exit101**, named failure `getAccount iteration19 native returned object identity`. One bypass attempted, stopping on iteration19, thenResolve: native A versus source assimilated B.
- Source-carrier control2 at `account-api-oracle.cjs`, replace both `addr\ud800👩` occurrences with `addr\ud801👩`: identical UTF16 length/encoded-surrogate substitution, changed units. Filtered native test **exit101**, named failure `independent account source corpus pin`, after the179 native scenario assertions/census checks. One substitution attempted; native input/expected outputs co-vary, and the independently authored pin discriminates them.
- Both controls restored in try/finally; the complete two-test invocation above then passed0. `git diff --exit-code e6ebe6fa6d -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, proving exact restoration. Earlier uncommitted runtime test compilation failed101 for an unreachable match arm; it was fixed before these payload measurements.

Independent floors:179 runtime scenarios (five methods23 each; liquidations/PnL32 each), outcomes49 success/49 generated Error/81 original opaque throws; ends iteration178, getPnl detached case. Thirty-nine native declaration records (13 named declarations x three variants), ending iteration38, GetPnlOptions variant2. Nested rateLimits/PnL summary fields use distinct literals and number bits. Ordered arrays have independently pinned lengths0/4 and asymmetric duplicate children[2,1,2,0]; metadata count4 covers full BigInt, undefined presence, Symbol/Function identity and unpaired UTF16 keys. These are bounded enumerations, not exhaustive inputs or host resolution proof.

Native assertions precede the source-only corpus pin: result variants, shared object/client and throw identity, exact UTF16 fields/URLs/parameters/error messages, binary64 bits, generic metadata, optional fields, child ordering and getter/call/continuation traces. No native Serialize/JSON codec, encode-reencode assertion or wire comparison is used. Source AST checks exact13 schemas including inline objects, member kind, optional markers, heritage and generic signatures; actual TS class methods execute unchanged through transpilation with a mocked get dependency. The native get callable retains its receiver at the host boundary; source mock asserts the original client receiver. Options run before get-callee lookup, included options read twice even when the second value is undefined, successful response data re-reads for return, and synchronous dependency errors become eager promise rejections. Detached observers do not cancel work. Error fallback short circuits preserve original exceptions.

**Findings/survivors:** initial native request directly settled object data, omitting async return thenable assimilation. Reviewer executed a structurally valid source data.then getter/call and throwing getter: source fulfilled the adopted value or rejected the original token. Fixed call site is `sdk_account_api.rs::request`, requiring `AccountSemantics::resolve_return` at the actual return point;28 witnesses cover four return-resolution scenarios across seven methods, and compiled control1 catches omission. This dependency contract requires ECMAScript Get(then), call scheduling, recursive adoption and original exception preservation; it is supplied by the host, not a standalone Rust JS engine implementation. Its complete implementation/enumeration is not proved by this slice.

The initial runtime carrier also lacked an independent input pin. Reviewer measured source `[a]`->`[b]` same-length substitution surviving Node0 with identical census35/35/53 while changing both native inputs and expected results. No initial compiled Rust pass was measured or claimed for that probe. The final fullbody pin changes for the same substitution, independently verified by reviewer Node0; control2 proves the repaired native gate fails for an encoded-surrogate substitution. Both findings are retained here rather than silently dropped. No survivor observed in the two stated final controls.

Sub-agent review of body/schema/test/manifest and22target/provenance checks closed at e6ebe6fa6d with no remaining findings under the explicit host contracts. Reviewer ran read-only/Node probes, **exit0**, no Cargo/extractor. Parent measured the gate/control exits above.

Not run or claimed: workspace compilation, full monorepo suites, host clippy, wasm clippy, CI reruns, network/HTTP transport, native JSON decoding, JS record aliases/reflection/property enumeration, browser HtmlDda, full semantic TS SDK build, standalone JavaScript Promise Resolve/getter/coercion engine, exhaustive thenables or primitive receiver/prototype/callback monkeypatch scenarios, or generated Error engine stack/prototype interoperability. Property/coercion/return-resolution and FIFO enqueue are required ambient host contracts; only the stated representative operations are oracle-proven here.

Post-evidence merge verification measured at `c19a90d7b159f666a75c5e55827a4fd244ac5117`: parents `56f6493fae233c2a89895acd16d7eeec8cff47cc` and integration `95e6665431234b770e820f5f06dab0e3ad386d76`. Slice preservation against e6ebe6fa6d across crate src/tests/scripts, TS SDK sources and SDK manifests **exit0**, byte-unchanged. Shared crypto/store/transport/exchange sources and Cargo.lock changed, so scoped all-targets check rerun here **exit0** (`Finished dev profile`,30.78s); two live_account_api tests rerun here **exit0**; extractor check here **exit0**, read output `Would update 0 manifests; 0 added, 0 orphaned symbols.` (23.25s). Controls remain measurements of e6ebe6fa6d. Final unchanged merge preservation carries these measured SHAs without claiming fresh executions at documentation commits.

Final prepush merge preservation measured at `9da6d30caa8b35ba77a495fe1f35821a6ecb9fef`, parents `376fddceb4fe7364e96344523367b2f2d7dba1cb` and integration `1273115ab915ad59ed9b478c48ca6356d8ac03f0`: diff from c19a90d7b1 across slice src/tests/scripts, TS SDK sources and SDK manifests **exit0**; separate diff across Cargo.lock and checked types/num/crypto/candle-cache/store/transport/exchange source dependencies **exit0**. Gates measured at c19a90d7b1 and controls at e6ebe6fa6d are retained through this byte-unchanged merge. No fresh gate execution at the final documentation SHA is claimed. Actual initial parent remains ebcb51ffa5; payload sub-agent review is closed. Only user-owned untracked brief files remain.

## Transaction history SDK values and methods — 2026-10-14

Measured payload SHA: `eb91ea6609f7f47dfde903fc53274669211ce4a9`.
Actual parent: `de7f0c7698e85e937aa2c056f26c1570b063bea8`, assigned merged integration head; no additional unreviewed lane ancestor. This evidence commit records measurements of that payload, without relabeling them as measurements of the documentation commit.

Independent Python3.11 tomllib comparison over all lighter-sdk manifests against the actual parent measured **21 pending rows moved:20 ported, one CHOSEN divergent**. Before:149 ported/91 divergent/149 pending; after:169/92/128. Two native string enums, eleven native value-level declarations, constructor client-handle retention and six method bodies are ported. TransactionApi class is CHOSEN divergent for explicit native client/getter/query/FIFO/Promise Resolve contexts versus JS ambient mutable/reflected class/prototype surface. Venue manifests were untouched. Parent SDK pending census was149, not the approximately123 in the assignment.

Measured gates, sequential Cargo/jobs2:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, read output `Finished dev profile` (13.82s), zero compiler errors. Scoped crate check only.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`: **exit0**, read output `Updated 1 manifests; 0 added, 0 orphaned symbols.` (10.72s), after manifest edits and before the payload commit.
- Same launcher `parity:extract:check`: **exit0**, read output `Would update 0 manifests; 0 added, 0 orphaned symbols.` (13.27s).
- With `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`, `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_transaction_api --test live_account_api --jobs 2 -- --include-ignored --nocapture`: restored committed payload **exit0**, transaction2/account2 passed, zero failed. Account regression tests are necessary because the existing shared helper gained same-continuation projection; no broader suite was run.
- Compiled control1 at `sdk_transaction_api.rs::direct_params`: omit initial keys whose values are undefined. Filtered `transaction_methods_native_results_and_reads` **exit101**, named failure `native required initial query keys`. One omission attempted; stopped iteration0, getAccountTransactions with absent opts, whose source still constructs three own undefined keys.
- Compiled control2 at `sdk_transaction_api.rs::get_transaction_from_l1_hash`: change each hash unitD800 toD801, preserving UTF16 length but changing encoded units. Same filtered test **exit101**, named failure `getTransactionFromL1Hash iteration115 native URL`. One substitution attempted; native result assertions precede the direct captured native URL field comparison, with no encode/reencode gate.
- Both controls restored in try/finally; the complete four-test invocation above then passed0. `git diff --exit-code eb91ea6609 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, exact restoration to the measured payload.

Independent floors:192 runtime scenarios, six method counts38/31/37/26/34/26; outcomes66 success/47 generated Error or TypeError/79 opaque throws, ending iteration191, getTransactionFromL1Hash containerThenThrow. Composition is138 common scenarios (six methods23 each) plus15 types-query,14 deposit-filter,3 transfer-option,16 numeric-identifier/block and6 detail-property scenarios. Forty-five native declaration records (Transaction5, ten other types4 each), ending iteration44, GetTransferHistoryOptions variant3. Native enum inventories are status3/type5, ending Failed/Deposit; native deposit status4 ends Failed. Ordered child arrays use independently pinned lengths0/5 or0/4, asymmetric duplicates[2,1,2,0,4] or[2,1,2,0]. These are bounded enumerations, not exhaustive input or host operation proofs.

Native fields/variants/identity and call arguments are asserted before the independently authored source-only corpus pin. Full binary64 bits, raw UTF16 strings, optional effective values and closed enum fields are checked directly; no native JSON codec, Serializer, encode/reencode assertion or wire comparison is used. Actual source AST checks eleven interfaces including inline filter and exact property/optional/heritage/generic shapes. Actual runtime source checks the3/5 named enum maps. All six TS method bodies execute unchanged through transpilation against a mocked get dependency with original-client receiver assertions.

Query contracts preserve source ordering: initial keys exist even when undefined; types are read separately for truthiness, length and receiver-bound join with comma; filter/status/tokenId objects are re-read at every source access. NaN/negativezero/unbounded identifiers are unvalidated and retain native values. Number/UTF16 URL formatting is source-compatible over the declared primitive inputs. Shared `AccountApi::request_projected` executes projection inside the original await continuation, before host Promise Resolve; account calls pass an identity projection, with no intermediate promise or queue step. Detail methods read second data.transaction, never resolve/adopt the container; source oracle verifies all60 detail scenarios have no container.then getter calls while child thenable operations are observed. Original getter/callback/coercion exceptions and detached work are retained.

**Survivors/findings:** no new implementation or final compiled-control survivor found in the stated floors. Independent reviewer source-only D800->D801 hash substitution exitedNode0, kept192/45 census and changed the fullbody digest; the source capture is not itself a pass/fail native gate. Its independently pinned digest protects co-varying source inputs/expected outputs. Control2 separately proves the native URL assertion discriminates encoded-surrogate substitution. No unmeasured Rust pass/fail is inferred from the reviewer's Node result.

Sub-agent review of code, shared helper behavior, oracle/native assertions, manifest and21 targets/provenance entries closed at eb91ea6609 without remaining findings under the explicit host contracts. Reviewer measured read-only/Node **exit0**, no Cargo/extractor; parent measured the gate/control exits above.

Not run or claimed: workspace compilation, full monorepo suites, host clippy, wasm clippy, CI reruns, network/HTTP transport, native JSON decoding/encoding, JS object aliases/reflection/property enumeration, browser HtmlDda, full semantic TS SDK build, standalone ECMAScript property/coercion/abstract comparison/array-join/Promise engine, exhaustive prototype or thenable/callback scenarios, generated Error engine stack/prototype interoperability. Required TransactionQuerySemantics and AccountSemantics ambient host contracts implement those operations; query witness comparison covers Number/BigInt/Symbol, not the complete abstract-relational domain. Source enum mapping is value-level, not mutable enum-object interoperability.

Post-evidence merge verification measured at `531ee75487d169d9214369e7543e4fed803e9cdb`: parents `925b70b10a0be176c5eb114552104adc90a3a6ba` and integration `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444`. Slice diff from eb91ea6609 across crate src/tests/scripts, TS SDK sources and SDK manifests **exit0**, byte-unchanged. Cargo.lock changed; scoped all-targets check rerun here **exit0** (`Finished dev profile`,32.00s); extractor check here **exit0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols.` (12.40s). Four oracle tests and controls remain measurements of eb91ea6609, retained through unchanged source/test/script inputs; they were not rerun at this merge SHA.

Final prepush preservation measured at `f5457696c424b79d1cb066228a934d498e1eac5f`, parents `ef0df80ad1f90c8049cfe2ef5a3945bbfe007cda` and integration `4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a`: diff against531ee75487 across slice src/tests/scripts, TS SDK sources and SDK manifests **exit0**; separate diff across Cargo.lock and checked types/num/crypto/candle-cache/store/transport/exchange source dependencies **exit0**. Scoped check/extractor measured at531ee75487 and tests/controls measured at eb91ea6609 are retained through this byte-unchanged merge; no fresh execution at the final documentation SHA is claimed. Actual initial parent remainsde7f0c7698. Payload sub-agent review closed; only user-owned untracked brief files remain.

## Ordinary order-query SDK slice — 2026-10-14

Measured payload SHA: `fe37306316c6ed3ab4131ff5a31b211a4d8984d1`.
Actual parent: `ac9c9470293c896c3aadbc039a5b0ab70679d890`, assigned merged integration head; no additional unreviewed lane ancestor. This evidence commit records measurements of that payload, not fresh executions at the documentation commit.

Independent Python3.11 tomllib comparison over all lighter-sdk manifests against that parent measured **15 pending rows moved, all ported; zero moved to divergent**. Before:169 ported/92 divergent/128 pending; after:184/92/113. Seven native value-level declarations, constructor shared-client retention and seven ordinary method bodies are ported. Partial OrderApi class and getOrderBookOrders remain pending with empty rs targets and explicit notes. That missing method needs request-config spread/precedence and optional success/raw code200/array/Date.now/error handling through its own ABI. Existing five wire DTO divergences and venue manifests retain their statuses; stale prose about all runtime methods pending was updated without relabeling those prior proofs. Parent SDK pending census was128 rather than the approximately102 in the assignment.

Measured gates, sequential Cargo/jobs2:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, read output `Finished dev profile` (17.20s), zero compiler errors. Scoped crate check only.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`: **exit0**, read output `Updated 1 manifests; 0 added, 0 orphaned symbols.` (12.89s), after manifest edits and before payload commit.
- Same launcher `parity:extract:check`: **exit0**, read output `Would update 0 manifests; 0 added, 0 orphaned symbols.` (33.56s).
- With `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`, `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_order_queries --jobs 2 -- --include-ignored --nocapture`: restored committed payload **exit0**, two passed, zero failed.
- Source inherited-binding control: isolated Node fs read interception changes actual `Trade` import to `OrderBook as Trade`. **exit1**, named failure `ApiTrade Trade import original binding`. Production TS files were never edited. Baseline/restored oracle **exit0** with byte-identical body hash; reviewer independently measured the same fail/restore sequence.
- Compiled control1 at `sdk_order_queries.rs::default_limit`: replace truthy limit selection with only-undefined selection. Filtered `order_methods_native_results_and_reads` **exit101**, named failure `getAccountInactiveOrders iteration161 native UTF16`. One selector mutation attempted; stopped first full-options case161, negativezero limit source'50' versus native'0'.
- Compiled control2 at `sdk_order_queries.rs::truthy_raw`: map eachD800 unit toD801 in forwarded native strings, preserving UTF16 length and changing encoded units. Same filtered native test **exit101**, named failure `getAccountInactiveOrders iteration161 native UTF16`, at its raw cursor parameter. One substitution attempted; direct decoded native string assertion, no encode/reencode comparison.
- Native controls restored in try/finally; complete two-test invocation above then passed0. `git diff --exit-code fe37306316 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, exact restoration. Earlier uncommitted Node schema run failed1 because expected inline optional flags were not normalized; repaired before the measured payload.

Independent floors:251 runtime cases (seven methods23 base each;22 option/custom-stringifier cases;6 tuple cases;30 limit cases;32 market-number cases). Method counts31/50/31/31/41/44/23, outcomes113 success/52 generated Error or TypeError/86 original opaque throws. Ends iteration250, getRecentTrades numericMarket Number.MAX_VALUE. Native declaration records28, seven declarations4 variants each, ending iteration27, GetTradesOptions variant3. Closed native side2/inactive-sort3/trades-sort2/order2 mappings end Sell/Price/Price/Desc. Child arrays have independent lengths0/4 and asymmetric duplicates[2,1,2,0]; native tuple length2 is fixed and each slot has distinct expected bits. These are bounded enumerations, not exhaustive host/input proofs.

Decoded native results/identities, UTF16 fields/URLs/parameters, binary64 bits, full BigInt, optional fields, tuple/child order and getter/call/continuation traces precede the independently authored source-only corpus pin. No native JSON codec, Serializer, encode/reencode or wire comparison is used. Actual source AST guards seven declaration schemas including inline market stats and the actual imported Trade local/original/module binding; imported Trade has exactly eight compatible fields, all redeclared by ApiTrade. Derived/base heritage/generic/property/optional/union shapes are guarded. Actual TS method bodies execute unchanged against mocked receiver-bound get dependencies.

Methods retain source truthy limit||50 without range clamps, separate market/askFilter reads, separate tuple guard/first-container0/second-container1 reads, and raw second cursor/sort values including undefined/empty. Required OrderQuerySemantics does Get(toString) and receiver-bound no-argument call, retaining native returns and original exceptions. Witnesses include custom toString returning number17.25, number50 prototype override returning the original object, getter failures, invalid second values and tuple index/coercion throws. Recent-market template formatting occurs after get-callee retrieval; parameter construction occurs before it. Established getter/coercion/FIFO/Promise Resolve host boundaries retain eager detached work and second data return/thenable behavior. No existing account/transaction helper implementation changed in this slice.

**Survivor/finding retained:** initial oracle checked common.ts Trade and `ApiTrade extends Trade` but not the actual import binding. Reviewer aliased OrderBook as Trade: Node0 and byte-identical capture survived despite different inherited fields. Call site: `order-queries-oracle.cjs`, inherited ApiTrade source guard. The new binding assertion closes the finding; parent and reviewer controls above fail1 by name, then baseline passes0. No unmeasured initial Rust pass is inferred from that source-only survivor. No survivor observed in the stated final controls.

Sub-agent review of bodies, schemas, native assertions, manifest and15 target/provenance entries closed at fe37306316 without remaining findings under explicit host contracts. Reviewer measured read-only/Node **exit0** plus the named import mutation **exit1**, no Cargo/extractor; parent measured native gates/controls above.

Not run or claimed: workspace compilation, full monorepo suites, host clippy, wasm clippy, CI reruns, network/HTTP transport, native JSON codec or adoption of existing wire DTOs, complete OrderApi class, raw getOrderBookOrders, JS aliases/reflection/property enumeration, browser HtmlDda, full semantic TS SDK build, standalone property/coercion/toString/Promise engine, exhaustive prototype/thenable/callback behavior or generated Error engine stack/prototype interoperability. Required host semantics supply those operations; only stated representative behavior is proved.

Post-evidence merge verification measured at `1b3d30455a9fdbe5050b5274008bfb5c3e7c7dba`: parents `60d5099dd7c20566a7f782b6884140fa8ffb4f36` and integration `ab593ef7504191a1cfd2bb094ef066aedd4c5c25`. Diff from fe37306316 across slice src/tests/scripts, TS SDK sources, SDK manifests, Cargo.lock and checked types/num/crypto/candle-cache/store/transport/exchange source dependencies **exit0**, byte-unchanged. Scoped Cargo/test/control results remain measurements of fe37306316 and were not rerun at this merge. Extractor check rerun here **exit0**, read output `Would update 0 manifests; 0 added, 0 orphaned symbols.` (30.68s).

## Candlestick, funding-rate and withdrawal-delay SDK slice — 2026-10-14

Native payload measured at **46b17797168b63e1c93ec3bbeeb906a158de2a53**, actual parent **c4c1ec196d794b431bed97b2ede5eff17a41c6f5**. Fresh per-slice branch `feat/rust-lighter-sdk-candlestick-api`; no additional unreviewed lane ancestor introduced. Python3.11 tomllib census against that parent: **18 pending rows moved: 15 ported, 3 CHOSEN divergent**. SDK184p/92d/113pending -> **199p/95d/95pending**. The eight DTOs, three constructors and four methods are ported at the declared native host/value boundary. Three class objects choose explicit runtime/property/Promise Resolve/Error hosts and native reflection/prototype surfaces. LighterApiError class itself stays pending: MarketApiErrors is a required constructor contract, not a fabricated implementation or target.

Measured commands and exits at payload SHA:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, zero errors, Finished dev profile (55.18s).
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, output read: `Would update 0 manifests; 0 added, 0 orphaned symbols.` (18.48s). The later metadata correction adds adapted layers, sdk-rs-relative targets and source provenance; its separate writer/check are recorded below.
- Compiled deliberate control at `sdk_market_api.rs::CandlestickApi::config`: replace resolution code unit D800 with D801, preserving length and changing encoded bytes. `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_market_api market_methods_native_results_and_reads -- --ignored --nocapture`: **exit101**, named failure `getCandlesticks iteration0 native UTF16`, native units[49,104,55297,55357,56425] versus independently captured source units[49,104,55296,55357,56425]. One compiled mutation attempted, stopped at first case0. No survivor observed in this control.
- Mutation restored in try/finally. Same environment with `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_market_api --test live_account_api --test live_transaction_api --test live_order_queries -- --include-ignored --nocapture`: **exit0**, eight passed, zero failed. This also checks the shared request_handled extraction against all three existing ordinary-query callers.
- `git diff --exit-code 46b17797168b63e1c93ec3bbeeb906a158de2a53 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, exact native input restoration.

Independent bounded floors: **120 runtime cases**, four methods30 each, stopped after iteration119 (`getWithdrawalDelay/codeFalse`); **32 DTO cases**, eight schemas4 each, stopped after iteration31 (`WithdrawalDelayResponse/variant3`). Child arrays use independent0/4 lengths and asymmetric ordered duplicates[2,1,2,0]. Runtime queue ceiling20 prevents a hung fixture. DTO fields assert authored UTF16 markers (including lone D800), full binary64 bits (negativezero/NaN/infinities/large integers/subnormal), effective optional fields, distinct nested values and ordered duplicate children. Method assertions inspect native returned object identity, opaque exception identity/payload, API error name/message/raw code, UTF16 URL/query keys/values and getter/call/continuation order before the independent authored source corpus pin. No serialization or native re-encoding assertion is used. Exact AST guards pin source DTO fields, property/optional/heritage/generic shape and actual LighterApiError import local/original/module binding. Actual TypeScript method and Error constructor bodies execute against mocked HTTP dependencies.

Failure paths preserve source short-circuit guards, two separate response.error accesses (including second-null/changed-object/throw), message and code fallback truthiness, code evaluation before constructor message coercion, and returned data reread before required ECMAScript Promise Resolve. Constructor and HTTP host exceptions keep identity. Returned thenables include resolve/reject/getter-throw/noncallable representatives. Constructors retain the original client without get access; work remains eager after caller/observer handles are dropped. Required MarketApiErrors must implement the actual Error constructor semantics; fixture only witnesses the stated cases and does not supply an entire JS Error/Promise engine.

Sub-agent review found no concrete source body/schema issue under the explicit contracts. Reviewer Node baseline **exit0**, exact authored hash; source-only same-length resolution mutation capture **exit0** with changed hash (capture emitter is not an assertion gate; no native pass inferred). Reviewer verified code getter precedes constructor/coercion in actual TS trace. No reviewer Cargo/extractor claimed. Final metadata review follows normalization.

Not run/claimed: workspace compilation, full monorepo suites, host clippy, wasm clippy, CI, network HTTP, native JSON codec, full semantic SDK compilation, JS reflection/property enumeration/alias mutation surface, browser HtmlDda, independent production ECMAScript property/coercion/Promise/Error engine, exhaustive prototype/thenable inputs, pending LighterApiError class, referral request method or remaining SDK/venue rows. Preliminary uncommitted check and two new tests also exited0, but the payload-bound results above carry the delivery claims.

Final metadata commit **05604049d641ecc5d1f39e21623d4eee7dc31c35** separates target/provenance normalization from native payload46b177. Writer ran after metadata edits, **exit0**, output `Updated 3 manifests; 0 added, 0 orphaned symbols.`; normalized manifest output committed here. Sub-agent independently checked18 bindings/inventory/layers/statuses/provenance **exit0** and exact reviewed metadata equality to05604049 **exit0**, no remaining finding.

Post-merge gates measured at **bfa2b8298c3092f4492458bb1862a8f4ecffa671**, actual parents **05604049d641ecc5d1f39e21623d4eee7dc31c35** and integration **65e086a3c9039a79e4c7e9f354aba31d8f83009e**. Source/tests/scripts/TS SDK diff from46b177 **exit0**; SDK metadata diff from05604049 **exit0**. Build dependency diff **exit1** because integration changed Cargo.lock and transport source; therefore scoped Cargo and eight query tests rerun sequentially here: check command above **exit0**, Finished dev profile40.31s; same four scoped test-target invocation above **exit0**, eight passed/zero failed. Mutation discrimination remains measured at46b177, whose source/tests are byte-unchanged. Extractor check **exit0** atbfa2b829, output read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (10.77s). Python3.11 census remeasured against actualc4c1ec base:18 moved(15p/3d), SDK199p/95d/95pending.

## Private websocket warning and rate-state slice — 2026-10-14

**8 pending->ported, 0->divergent**, measured with Python3.11 tomllib against actual slice base **f0cf136efbc17b93fe52369e71342e620dd925ff**. Venue132p/18d/186pending -> **140p/18d/178pending**. Branch `feat/rust-lighter-private-warnings`. Final reviewed payload **5094a1477890fb87332e8457c6bb1987ef8ae76c**, actual commit parent **cbabc76bc7ceefa734a334423b4b7571cdf65406** (the initial implementation in this slice). No unrelated unreviewed lane ancestor introduced; current bodies and eight target/classification/provenance mappings received sub-agent review. Whole LighterPrivateWebsocket stays pending.

Measured at5094a147:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, zero errors, Finished dev profile3m10s.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, output read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (27.17s). Writer ran after each manifest edit: first **exit0**, `Updated 1 manifests`; final **exit0**, `Updated 0 manifests`; both0added/0orphaned. rust_files generated programmatically and writer normalized.
- Warning-family compiled control at `ws_private_warnings.rs::get_readonly_fallback_warning_key`: substituteDC00->DC01 in account-index units, preserving UTF16 length/changing encoded bytes. `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_private_warnings private_warning_native_results_and_effects -- --ignored --nocapture`: **exit101**, named assertion `getReadonlyFallbackWarningKey iteration195 native UTF16`; decoded native last unit56321 versus source56320.
- Rate-state-family compiled control at `PrivateWarningState::note_private_stream_rate_limit_error`: increment2 instead of1. Same command filtered to `private_rate_state_native_transitions`: **exit101**, named assertion `notePrivateStreamRateLimitError iteration324 native count bits`; native2.0 versus source1.0. One mutation per new family, both killed; no survivor observed.
- Both restored in try/finally. Same environment and cargo test prefix with `--lib --test live_private_warnings --test live_ws_state -- --include-ignored --nocapture`: **exit0**, two new native tests plus four existing websocket-helper tests passed; library target contains0tests, not an extra proof gate.
- `git diff --exit-code 5094a1477890fb87332e8457c6bb1987ef8ae76c -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, exact restoration.

Independent floor428cases:324warning cases ending iteration323 (stream-warning/route callback changes readonly);104rate-state cases ending iteration427 (clear/large initial counter). Warning method counts114route/28georestriction/57stream-message/3guidance/4key/117emission plus1shared-class-map witness. Rate-state96note/8clear. Native UTF16 strings, boolean results, exception identity/payload, full binary64 first-seen/count/map values, shared cooldown map contents, effects/forceToast/order and read/call traces precede the authored source-only corpus pin. No serialize/reencode/wire comparison. Actual selected arrow bodies, cooldown/map statics and georestriction constant execute unchanged; AST guards pin actual clock/route/severity imports and the readonly warning-field initializer/no reassignment invariant.

Review finding corrected before final gates: initial owned fallback-state snapshot could miss route-host reentrancy. Source-compatible fallible live owner getters now run after route callback; five fixtures cover two getter throws and three callback state changes. Source readonly warning text is initialized once and never assigned; reflective override/interception of that field is outside the declared body contract. Parent logging/emitter operations are required receiver-bound hosts, shared cooldown map is class-wide across instances, and state is updated before log/emit exceptions. Initial423-case native tests passed0 without the five new witnesses, so no claim they proved reentrancy. Earlier cbabc76 scoped check/extractor also exited0; those measurements do not cover corrected inputs. Preliminary source oracle failed1 on a cross-realm fixture prototype comparison; fixed before first payload by inspecting forceToast and its sole own key. Corrected uncommitted two native tests and standalone source oracle exited0; final delivery claims use5094a147 above. Sub-agent read-only target/provenance/status review exited0 and closed both live-state and readonly-message scope concerns; no sub-agent Cargo/Node control measurements claimed.

Not run/claimed: workspace build, full monorepo or full venue integration suite, host clippy, wasm clippy, CI, network/websocket transport, whole websocket owner/lifecycle, transport resolver implementation, arbitrary mutation of source readonly fields, reflection/prototype/setter interception, exhaustive host inputs or production runtime binding. The scoped test targets and declared host/value boundaries are the proof scope.

Merge-down verification at **e0acf951dfd1e4cc96c995b6c84cb8bc88f65722**, actual parents **0c944d3b901a55fcbb7a2d21c919974cd30ee065** and integration **5755c1bbc8ab6088443ec0d9e1ab95f3895aee10**: diff from5094a147 over crate src/tests/scripts plus venue TS/LogSeverity source **exit0**; venue manifests **exit0**; checked shared dependency src/Cargo.toml and workspace/crate Cargo.toml **exit0**. Whole lockfile diff **exit1**: adds safe-cex-candle-cache only to safe-cex-venue-apexomni. Independent tomllib package-record comparison and recursive Lighter dependency-graph traversal **exit0**, changed package set exactlyapexomni, changed packages within Lighter graph empty. Scoped Cargo/tests/controls therefore remain measurements at5094a147 and were not rerun here. Extractor check rerun here **exit0**, output read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (54.19s). Own payload and18prior divergent rows preserved; this slice moves8pending->ported only.

## Public websocket timer cleanup slice — 2026-10-14

**8 pending->ported, 0->divergent**, independent Python3.11 tomllib comparison with fresh slice base **200ef7e5dd8ead4ef9998682f302d272ca8a0dae**: venue140p/18d/178pending -> **148p/18d/170pending**. Branch `feat/rust-lighter-orderbook-state`. Measured final payload **4c42d0800299a4001f0a9cda4a9e4357247bc9f2**, actual parent **f413d25a6bc31f966e6098ecbf28b818a376f8bf** (this slice's implementation; parent200ef7e5dd). No unrelated unreviewed lane work introduced. Full public websocket class and scheduling/lifecycle stay pending; eight real native function targets mapped separately. New contracts are local to this venue; no shared type/trait consumer change or workspace-sweep claim.

Commands/exits measured at4c42d080:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, zero errors, Finished dev profile29.12s.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, output read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (10.69s). Writer after manifest edits: first **exit0**, `Updated 1 manifests`; final **exit0**, `Updated 0 manifests`; both0added/0orphaned. rust_files generated programmatically, writer normalized.
- Record-family compiled control at `ws_public_cleanup.rs::clear_topic_timeout`: substituteD800->D801 in the topic passed to deletion, preserving UTF16 length/changing encoded bytes. With `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`, cargo test prefix `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_public_cleanup public_cleanup_native_state_and_timer_arguments -- --ignored --nocapture`: **exit101**, named assertion `clearOhlcvNoDataTimeout iteration7 current native record keys`; native retains original topic key while source has no own key after deletion.
- Pending-array-family compiled control at `clear_pending_subscribe_timeouts`: filter falsey callback values before clearTimeout. Same filtered command **exit101**, named assertion `clearPendingSubscribeTimeouts iteration146 native timer-call count`, native3 versus independently captured source4; source passes the zero timer argument too. One final control per new family, both killed; no survivor in these controls.
- Both restored in try/finally. Same environment with cargo test prefix and `--lib --test live_public_cleanup --test live_private_warnings --test live_ws_state -- --include-ignored --nocapture`: **exit0**, seven tests passed (one new cleanup, two private warnings, four existing websocket helpers). Library target0tests is not an extra proof gate.
- `git diff --exit-code 4c42d0800299a4001f0a9cda4a9e4357247bc9f2 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, exact restoration.

Independent bounded floor159cases, ends iteration158 (`clearPendingSubscribeTimeouts/secondOwner`): four per-topic functions20each, bulk initial/stale21each, bulk REST23, pending-array14. The eleven authored primitive/object tag kinds per per-topic method have an independent producer assertion. Native current/original record identities, UTF16 own keys, values/enumerability, sparse-array indices/length/aliases, object/function timer identities, raw binary64/UTF16 timer arguments, exception identity/payload or generated error name/message, timer-operation kind/count/order and property/callee/iterator traces precede the authored source-only corpus pin. No native serialization or decode-reencode comparison. Actual selected TS arrow bodies execute unchanged; AST guards require exactly one member per target and the source ordinary record/array initializers.

Bodies preserve falsey entries without per-topic deletion, reacquire current records after timer cancellation, always delete REST in-flight topic independently, retain cooldown in per-topic stop, replace three bulk REST records in order with partial state on setter failure, and preserve old aliases on fresh {} or [] assignments. Required array host keeps receiver-bound actual forEach, synchronous callbacks, holes/live reads and unfiltered timer arguments. Required values host captures the actual Object.values callee and Object receiver before owner argument evaluation, defers noncallable failure until Call, and supplies actual for-of/GetIterator/live yields/IteratorClose with original body-throw precedence. Source fixtures observe callee/record replacement, lookup/call/iterator/next/closing failures and appended iterator values. These are explicit language/timer host boundaries, not an independent production ECMAScript implementation or whole websocket binding.

Review findings resolved: initial Vec-only bulk loop did not represent callee-before-argument evaluation or custom iterator closing. Added bound values/for-of host and36source cases before final payload. Sub-agent read-only review checked all8bindings/provenance/statuses **exit0**, closed body/contract findings; no reviewer Cargo/Node/mutation matrix claimed. Initial123-case and corrected159-case uncommitted native test each exited0; neither alone proved unenumerated behavior.

Producer finding at f413d25a: its scoped check/extractor exited0; first record mutation exited101 at iteration0, but proof runner exited1 because it expected iteration7. This exposed `run(..., value=a)` silently converting the intended explicitundefined case toA. try/finally restored source, measured exact diff0. Corrected only producer/default handling and native pin at4c42d080, with independent literal tag-kind floor; final controls above fail at intended iteration7/146. This was an enumeration/runner expectation finding, not a surviving mutation, and old gate measurements do not cover the corrected fixtures.

Not run/claimed: workspace build, full monorepo or full venue integration suite, host clippy, wasm clippy, CI, network/websocket or real platform timers, complete owner/lifecycle, standalone ECMAScript record/array/iterator engine, exhaustive prototype/callback/global override inputs or runtime integration of these required host contracts. Scoped tests and enumerated native values are the proof floor.

Merge-down verification at **155cd7d9ae5c4d47a6da7a321ca483209be09dca**, actual parents **8860a6b5ffb634cddcf107bcec09933f7ddc788e** and integration **b588da1cd0ab56a565af6cc3c55e7afeb1dbc43b**: own src/tests/scripts plus venue TS/LogSeverity inputs diff from4c42d080 **exit0**; venue manifests **exit0**. Build-source diff **exit1** because shared exchange source changed; therefore scoped Cargo check rerun here **exit0**, Finished43.92s, and same seven-test scoped invocation rerun **exit0**, seven passed/zero failed (library0tests). Independent lock-package/recursive dependency comparison **exit0**: only bingx lock record changed, none within Lighter graph. Mutation discrimination remains measured at4c42d080 with exact unchanged own inputs. Extractor check at155cd7d9 **exit0**, output read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (10.69s).

Final pre-push merge verification at **ae90702d8f63c84dee619cf8d18dbb298e2e6dbd**, parents **bccd97fe2b17722add8ffd5d0078d01bbaceb22a** and integration **845cd310a963e53fc50dd9180a152e034262b6b4**. Diff from155cd7d9 across own src/tests/scripts, venue source/manifests, LogSeverity, workspace/venue Cargo files and complete checked shared dependency directories **exit0**. Cargo/tests remain the155cd7d9 measurements; controls remain4c42d080 measurements, byte-unchanged. Extractor rerun atae90702 **exit0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (11.11s). This additional merge introduced no Lighter gate-input change.

## Exchange cache and current-state slice — 2026-10-14

**8 pending->ported, 0->divergent**, independent Python3.11 tomllib comparison over the venue manifest directory with fresh base **c50aa8fb097cb5a40a600c1f615846474e6f0821**: venue148p/18d/170pending -> **156p/18d/162pending**. Branch `feat/rust-lighter-exchange-state`. Gate payload **19c963eb6a4d460e08602ab7536fa03a786cf810**, actual parent **c50aa8fb097cb5a40a600c1f615846474e6f0821**. No unrelated unreviewed lane work as an ancestor; previous own slices were merged by the coordinator. Full LighterExchange owner, HTTP fetch lifecycle and remaining account/order methods stay pending. Eight real adapted targets live in `src/exchange_state.rs`; all new host traits are venue-local, no shared type/trait change.

Commands and exits measured against19c963eb:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, zero errors, Finished dev profile40.77s.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, output read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (12.62s). Writer after editing the manifest: **exit0**, output `Updated 1 manifests; 0 added, 0 orphaned symbols.` (11.69s); array edited programmatically then normalized by the writer.
- Compiled hint-family control at `read_placed_order_hint` deletion call: substituteD800->D801 in the forwarded UTF16 key, preserving length/changing encoded bytes. Environment `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`, test prefix `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_exchange_state`, filter `hints_native_cache_and_identity -- --ignored --nocapture`: **exit101**, named `readPlacedOrderHint iteration14 expired native map count`, native1 versus source0. The wrong-key deletion retains the expired native cache entry.
- Compiled snapshot/cooldown-family control at `invalidate_account_snapshot_cache`: increment2 rather than1. Same prefix/environment, filter `snapshots_native_version_and_cooldown -- --ignored --nocapture`: **exit101**, named `invalidateAccountSnapshotCache iteration33 version/2 native version bits`, native4.0 versus source3.0.
- Compiled selection-family control at `get_current_leverage_for_symbol`: force nullish/exposure branch to fallback. Same prefix/environment, filter `positions_native_selection_and_clamping -- --ignored --nocapture`: **exit101**, named `getCurrentLeverageForSymbol iteration70 exposed native number bits`, native3.0 versus source7.0.
- Each mutation restored in try/finally. Same environment with cargo test prefix, `--lib --test live_exchange_state -- --include-ignored --nocapture`: **exit0**, three new native tests passed, zero failed. Library target0tests is not an extra proof gate.
- `git diff --exit-code 19c963eb6a4d460e08602ab7536fa03a786cf810 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, exact restoration. One compiled control per new family, three attempted, no survivors observed.

Bounded authored floor **82 cases**, stopped at iteration81 (`getCurrentLeverageForSymbol/minLookupThrow`). Per-method expectations are independent literals: hint remember13/read14, snapshot remember6/invalidate8/tier6/reconciliation13, margin10/leverage12. Native primitive kinds, full binary64 bits, UTF16 keys/strings, original object/exception identities, fresh spread fields plus an extra entry carrying the source symbol value (key identity gap corrected below), current/original map aliases/insertion order, recent snapshot fields/value identity, captured call arguments and live read/call traces precede the authored source-only full corpus hash in each family. There is no native serialization, roundtrip or native re-encoding assertion.

Actual selected TS arrow members execute unchanged; AST guards require exactly one member each and actual3000ms constant. Required fallible hosts capture receiver/callee before arguments (including delayed noncallability), CopyDataProperties, actual BigInt.toString, Map operations, subtraction/comparison/coercion, live owner fields, Array.find including holes/live values, Boolean/Number/Math and separately tracked helper calls. Fixture helper results are deliberately supplied dependencies: these tests prove the selected composition bodies, not tier extraction, margin conversion/configuration or tier snapping policy anew. Numeric cache-version/timestamp owner fields follow the source typed-owner contract. Cases witness cap2000 with eviction/empty-oldest break, strict10-minute expiry, changed current map after wall clock, version NaN/signed-zero/large-number behavior, partial state on throws, default/strict cooldown with timestamp reread, margin precedence, finite/exposure selection, unconditional fallback find, live leverage changes and nested clamp lookup order. The fixture's primitive language operations are a bounded test host, not a production ECMAScript engine.

Static reviewer identified a missing fixed corpus pin: count-only carrier input changes could preserve both generated inputs and expectations. Added full source-body+rows SHA256 pin after native assertions before19c963eb final gates. No measured survivor of the three compiled controls. Pre-payload development also corrected an oracle count arithmetic error (authored sum82, initially92) and the integration-test orphan-rule implementation via a local Host wrapper; neither preliminary attempt is a final payload gate.

Not run/claimed: workspace compilation, full monorepo or full venue integration suite, older venue test targets in this slice, host clippy, wasm clippy, CI, network/HTTP/websocket transport, complete exchange owner/runtime binding, independent helper-policy reproof, exhaustive coercion/prototype/global-override inputs or production implementation of the required language hosts. Scoped all-target compilation and the three enumerated native tests are the measured floor.

Review correction and final payload gates at **2ff2d8255ae98e0ee1a07f0a54724378fc34fe5e**, actual parent **36905e4d3b0b94e6c66494db1f4ddb441147da39**: the earlier hint fixture used the string `Symbol(proof)` as a stand-in for a symbol key. Its19c963eb native tests proved the extra value, not a native symbol key identity. This is a retained static proof finding, not a surviving compiled body mutation. Corrected Obj stores opaque symbol keys separately from string keys. Source and native input now also contain a real string key of the same description with L value, while the actual symbol key has A value; native assertions independently require4string keys/1symbol key, original symbol pointer identity and both different value identities. Source capture requires actual original Symbol identity. New fixed full corpus pin is86ba68a9ddd6d1e93a429360080c90d07d3021c2c2cf0d76c65d21402c376fbe. The82-case floor/stopping iteration81 is unchanged.

Fresh measured commands at2ff2d825 (same exact commands/environment/filters as above):

- Scoped all-target Cargo check **exit0**, zero errors, Finished28.90s.
- Extractor check **exit0**, output read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (13.17s).
- Hint surrogate control **exit101**, same named iteration14 native map-count assertion (1vs0).
- Version increment control **exit101**, same named iteration33 native version-bits assertion (4.0vs3.0).
- Exposure candidate control **exit101**, same named iteration70 native number-bits assertion (3.0vs7.0).
- Restored `--lib --test live_exchange_state -- --include-ignored --nocapture` **exit0**, three native tests passed/zero failed; library0tests not a gate.
- Exact src/tests/scripts diff against2ff2d825 **exit0** after try/finally restoration. No survivor in these three repeated controls. These are the final corrected fixture measurements; earlier19c963eb numbers remain earlier measurements only.

Sub-agent read-only review checked eight mappings/statuses/provenance and body order, raised both corpus-pin and symbol-key gaps, and closed both after correction; no remaining concrete finding within the declared host/82-case floor. Reviewer ran no Cargo/Node/mutation gate. No claim of exhaustive parity beyond the declared bodies/host contracts. Not-run list above applies unchanged.

First merge-down at **36905e4d3b0b94e6c66494db1f4ddb441147da39**, actual parents **61938b8ee6807bf8ebdb7700a7e269d43dbe8db9** and integration **77746a5b498ce3d3532424345f072425181f33ca**: own src/tests/scripts, venue TS/manifests diff against19c963eb **exit0**. Workspace/venue Cargo files and lockfile plus shared types/num/store/transport/crypto/candle-cache/exchange directories exact diff **exit0**. Extractor check at36905e4d **exit0**, output read0updates/0added/0orphaned (14.07s). The subsequent symbol-fixture correction changed tests/oracle, so fresh gates were run at2ff2d825 rather than treating the earlier gates as covering that repair.

Final pre-push merge at **e29cd77f6dfa62ed770f6d96be9a9fc659310b91**, actual parents **7393d6122865e6732e2871b680d682d007853399** and integration **3a957718fe445a833bb10a464e16e7cc6c585276**. Exact diff against2ff2d825 over own src/tests/scripts, venue TS/manifests, workspace/venue Cargo files and lockfile, and complete shared types/num/store/transport/crypto/candle-cache/exchange directories **exit0**. Cargo/tests/controls remain measurements at2ff2d825, preserved byte-for-byte; no rerun of those gates at this merge claimed. Fresh extractor check here **exit0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (10.48s). Independent tomllib census againstc50aa8fb here **exit0**, still exactly8pending->ported/0->divergent and156p/18d/162pending. Committed2ff2d825 repair inspection by the sub-agent **exit0**, no remaining concrete static finding, no reviewer runtime gates. The following evidence-only commit changes no gate input.

## SDK numeric-validation utility slice — 2026-10-14

**9 pending->ported, 0->divergent**, independent Python3.11 tomllib comparison over `parity/manifest/lighter-sdk` against fresh base **6a15df74a80721009ec4b07ea1c4236c1d3242e7**: SDK199p/95d/95pending -> **208p/95d/86pending**. Branch `feat/rust-lighter-sdk-numeric-utils`. Measured payload **918ea868817f5bff8b26b0e02221e71358a08dc5**, actual parent **6a15df74a80721009ec4b07ea1c4236c1d3242e7**. No unrelated unreviewed lane work as an ancestor; coordinator merged prior own slice. Nine real direct native targets in `src/sdk_numeric_utils.rs`, from one TS utils file. Eight other utility rows, including formatting, retry/sleep and RetryOptions, stay pending. No shared type/trait change, no workspace-sweep claim.

Measured commands/exits at918ea868:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, zero errors, Finished dev profile1m22s.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (9.89s). Writer after editing the manifest: **exit0**, read `Updated 1 manifests; 0 added, 0 orphaned symbols.` (12.14s). rust_files generated programmatically, writer normalized.
- All controls use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript` with prefix `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_numeric_utils numeric_utils_native_values_and_errors -- --ignored --nocapture`.
- Compiled UTF16-validation family control at `hex_to_big_int` invalid-input error construction: substituteD800->D801, preserving length/changing encoded bytes. **exit101**, named `hexToBigInt iteration16 native error UTF16`, native message ends55297 versus source55296.
- Compiled BigInt range/index family control at `is_valid_nonce`: replace inclusive upper bound with exclusive. **exit101**, named `isValidNonce iteration141 native boolean value`, nativefalse versus sourcetrue at4294967295n.
- Compiled number-conversion/percentage family control at `percentage_of`: round rather than floor the percentage product. **exit101**, named `percentageOf iteration88 native BigInt value`, native30n versus source20n.
- Each mutation restored in try/finally. Same environment with cargo test prefix and `--lib --test live_numeric_utils -- --include-ignored --nocapture`: **exit0**, one new native test passed, zero failed; library0tests is not an extra proof gate.
- `git diff --exit-code 918ea868817f5bff8b26b0e02221e71358a08dc5 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, exact restoration. Three controls attempted, one per new family; no survivor observed.

Independent bounded floor **160 cases**, stopped at iteration159 (`isValidApiKeyIndex/1e100`). Literal method floors: hex20/address13/clamp16/range16/number21/percentage43/account8/api-key15/nonce8. All decoded native BigInt results, booleans, error names/full UTF16 messages and source constant binary64 values are asserted before the source-only full corpus pin. No native serialization, native re-encoding or roundtrip comparison. Actual selected TS declarations execute unchanged. Source constants execute from their actual TS declarations; guards verify both original/local import names and non-type-only bindings. Actual import declaration and full types barrel are included in the fixed full corpus pin52d00e8799f3afe182e9f0c2958b5902fdb59cd8a2dcbdd38787dca9f963e997.

Native bodies preserve lowercase-only prefix and exact ASCII regex decisions, UTF16 input/error units, arbitrary BigInt precision, inclusive/reversed bound behavior, integer-before-safe-integer rejection and JS numeric error text, undefined-only decimals default, negative-guard ordering, two binary64 power evaluations/conversions, floor, and BigInt division toward zero. Percentage precision deliberately retains binary64 rounding before BigInt conversion; it does not substitute exact BigInt powers. Shared math_pow uses V8's platform-pow semantics and inherits host libm for general finite powers; these tests prove the measured host floor, not exhaustive cross-platform pow equivalence. Domains are the source typed primitive parameters and ordinary unmodified ECMAScript intrinsics. Reflective global/prototype replacement and out-of-type argument coercion are outside this adapted unit contract.

Sub-agent static review found original-binding and types-barrel provenance gaps, repaired before the measured payload. Both were closed; nine target/status/domain mappings checked, no remaining concrete static finding in this floor. Reviewer measured read-only metadata inspection exit0, no Cargo/Node/extractor gates. A suggested terminal-newline regex mismatch was withdrawn after actual source capture showed rejection without multiline mode; native newline cases pass. Pre-payload producer syntax correction and independent42-unit valid-address assertion (initial fixture44) were fixed before final gates; neither is a final payload gate. No compiled mutation survivor.

Not run/claimed: workspace compilation, full monorepo or full venue integration suite, older venue test targets in this slice, host clippy, wasm clippy, CI, network/HTTP/websocket transport, complete SDK or exchange runtime binding, reflective global/prototype overrides, out-of-type coercions, exhaustive numeric/UTF16 inputs, or cross-platform libm equivalence. Scoped all-target compilation and the enumerated native test are the proof floor.

Final pre-push merge at **cd6801598a2de38dec5d63508b38bc5e3badeeae**, actual parents **d6ea3d056ec048a3f84d5ab878a168850a64dc40** and integration **2a24bb3c48b25e781167071443dd603b0d0d2b8e**. Exact diff from918ea868 over own src/tests/scripts, SDK utils/types barrel/constants source and SDK manifests **exit0**. Workspace/venue Cargo files and complete shared types/num/store/transport/crypto/candle-cache/exchange directories exact diff **exit0**. Whole Cargo.lock quiet diff **exit1** (changed); independent tomllib package-record comparison and recursive resolution of both complete Lighter lock graphs **exit0**: only safe-cex-venue-okx record changed, changed records within Lighter graph empty, before/after graphs equal. Scoped Cargo/native tests/controls therefore remain918ea868 measurements, preserved byte-for-byte; no rerun at this merge claimed. Extractor check atcd680159 **exit0**, read0updates/0added/0orphaned (12.58s). Independent tomllib post-merge census against6a15df74 **exit0**: exactly9SDKpending->ported/0->divergent, SDK208p/95d/86pending; venue156p/18d/162pending with zero venue rows moved this slice. Following evidence-only commit changes no gate input.

## SDK field-element and UTF16 display encoding slice — 2026-10-14

**8 pending->ported, 0->divergent**, independent Python3.11 tomllib comparison over SDK manifests against fresh base **e6e3fe88995fee8ca4e89c0b4de4e87a6f101e0c**: SDK208p/95d/86pending -> **216p/95d/78pending**. Branch `feat/rust-lighter-sdk-field-encoding`. Measured payload **17a68362b5d71751bfa5daa716fb8d1d1bc22e86**, actual parent **e6e3fe88995fee8ca4e89c0b4de4e87a6f101e0c**. No unrelated unreviewed lane work as ancestor; coordinator merged prior own slice. All six field-elements functions plus truncate/formatAddress have real native targets in `src/sdk_field_encoding.rs`; other SDK rows stay pending. No shared type/trait change or workspace-sweep claim.

Measured commands/exits at17a68362:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, zero errors, Finished dev profile1m08s.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (16.61s). Writer after manifest changes: **exit0**, `Updated 2 manifests; 0 added, 0 orphaned symbols.` (25.27s). rust_files generated programmatically then writer normalized.
- Controls use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript` with `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_field_encoding field_encoding_native_strings_tuples_and_errors -- --ignored --nocapture`.
- Compiled numeric-field family control at `split_uint64`: swap returned high/low fields. **exit101**, named `splitUint64 iteration32 native tuple field0 UTF16`, native zero field versus source low-word1 field. The asymmetric tuple fixture distinguishes the swap directly; no encoder/decoder roundtrip.
- Compiled hex/concatenation family control at `concatenate_field_elements`: substituteD800->D801 in the returned code-unit sequence, preserving length/changing encoded bytes. **exit101**, named `concatenateFieldElements iteration85 native UTF16 result`, native55297 versus source55296 at the same position.
- Compiled display family control at `truncate`: add1 to the available prefix length. **exit101**, named `truncate iteration108 native UTF16 result`, native three dots versus source two for fractional max2.5.
- Each control restored in try/finally. Same environment with cargo test prefix and `--lib --test live_field_encoding -- --include-ignored --nocapture`: **exit0**, one new native test passed, zero failed; library0tests is not an extra proof gate.
- Exact `git diff --exit-code 17a68362b5d71751bfa5daa716fb8d1d1bc22e86 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, restoration complete. Three controls attempted, one per new family, no survivors observed.

Independent bounded floor **150 cases**, stopped at iteration149 (`formatAddress/emoji short input`). Literal method floors: uint32 18, uint64 12, int64 12, split12, hex26, concatenation12, truncate39, address19. Returned UTF16 strings, each ordered tuple field (independent2-element expectation), and error name/full UTF16 message are asserted natively before the source-only full body/input pincea7cb8fce8e296edeb7b5a1504d5233f98d395395600de1c1a0f9ca161445fd. These encoders return strings/tuples: those are the native function values under test. No native JSON serialization, re-encoding or paired decode/encode comparison.

Actual selected TS declarations execute unchanged. Native uint32 retains source range-only checks: fractions truncate, NaN/-0 become zero, unlike the stricter TransactionClient helper. BigInt unsigned/signed bounds, little-endian two's complement, exact range errors and low/high ordering match source. Hex conversion preserves the actual body, including lowercase-only prefix removal, odd leading zero, two-unit parseInt sign/whitespace/prefix handling, partial digits and NaN text, byte-count padding and input pair order. In particular source `hexToFieldElement('0x1234')` returns1234000000000000; its comment claims reversal but its body does not. This port mirrors the body without changing TS. Concatenation checks only UTF16 length16 before join, not hex syntax; its adapter contract is dense plain string-value arrays, excluding getter/accessor/proxy interception. Display operations retain UTF16 units and source ToIntegerOrInfinity/relative slice behavior for signed zero, fractions, NaN and infinities; endChars0 copies the full suffix. All functions assume ordinary unmodified intrinsics and source primitive parameter domains.

Sub-agent static review found no body mismatch in the declared floor and requested explicit plain-array scope, added before payload. Committed eight targets, TS provenance comments, adapted/ported notes and actual parent checked by read-only inspection **exit0**, no remaining static finding. Initial reviewer metadata checker assumed no doc line between provenance comment/function, corrected its checker; no production fix was needed. Reviewer ran no Cargo/Node/extractor gate or census. Initial uncommitted native test passed0; final claims above use the committed payload measurements.

Not run/claimed: workspace compilation, full monorepo or full venue integration suite, older venue test targets in this slice, host clippy, wasm clippy, CI, network/HTTP/websocket transport, complete SDK/exchange runtime binding, sparse/accessor/proxy array compatibility, reflective global/prototype overrides, out-of-type coercions, resource-exhaustion behavior, or exhaustive field/UTF16 inputs. Scoped all-target compilation and the enumerated native test are the measured floor.

Final pre-push merge at **cfeb3505ad1ee5d930ce49e6ccc9ae34f14a38c7**, actual parents **8468565656892fa522eeca1b2806925c0c286fe9** and integration **e0e1ceb8da277e79fa4bf29cbef4e94fd1dba0f0**. Exact diff from17a68362 over own src/tests/scripts, SDK field-elements/utils source and SDK manifests **exit0**. Workspace/venue Cargo files and lockfile plus complete shared types/num/store/transport/crypto/candle-cache/exchange directories exact diff **exit0**. Cargo/native tests/controls remain measured at17a68362 and are preserved byte-for-byte; no rerun of those gates at the merge claimed. Fresh extractor check atcfeb3505 **exit0**, read0updates/0added/0orphaned (26.32s). Independent tomllib post-merge census againste6e3fe88 **exit0**: exactly8SDKpending->ported/0->divergent, SDK216p/95d/78pending; venue156p/18d/162pending with zero venue rows moved. Following evidence-only commit changes no gate input.

## SDK generic HTTP, interceptor registration and config access — 2026-10-14

**9 pending->ported, 0->divergent**, independent Python3.11 tomllib SDK manifest comparison with fresh base **97b6f919d26396b52e4b3514f98fcfa34b4dcc6c**: SDK216p/95d/78pending -> **225p/95d/69pending**. Branch `feat/rust-lighter-sdk-http-access`. Measured payload **65963c2cfbf54afdf9d1edd6fdf0484bcaf7040e**, actual parent **97b6f919d26396b52e4b3514f98fcfa34b4dcc6c**. No unrelated unreviewed lane work as ancestor; prior own slice merged by coordinator. Nine real targets in `src/sdk_http_access.rs`. LighterApiClient class, constructor, setupDefaultInterceptors, error conversion/submission and concrete transport/runtime bindings remain pending. New host traits are venue-local; no shared type/trait change or workspace-sweep claim.

Measured commands/exits at65963c2c:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, zero errors, Finished44.99s.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (11.50s). Writer after manifest changes **exit0**, read `Updated 1 manifests; 0 added, 0 orphaned symbols.` (11.57s). rust_files generated programmatically, writer normalized.
- Controls use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript` and `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_http_access http_access_native_identity_state_and_order -- --ignored --nocapture`.
- Compiled HTTP-family control at `get` forwarding: substituteD800->D801 in the path, preserving length/changing encoded bytes. **exit101**, named `get iteration0 argument0 native UTF16`, native55297 versus source55296 at the same path position.
- Compiled registration-family control at `add_interceptor`: write the RHS registry instead of captured LHS, retaining a borrow of LHS so the original getter still executes and compiler lints remain satisfied. **exit101**, named `addRequestInterceptor iteration88 J0.request native object identity`; native retains X where source assignment installs Y after RHS getter changes current owner toJ1.
- Compiled config-access-family control at `get_config`: return the captured config object instead of CopyDataProperties result. **exit101**, named `getConfig iteration138 result native object identity`; native original C differs from source fresh D.
- Each restored in try/finally. Same environment with cargo test prefix and `--lib --test live_http_access -- --include-ignored --nocapture`: **exit0**, one new native test passed, zero failed; library0tests not an extra proof gate.
- Exact `git diff --exit-code 65963c2cfbf54afdf9d1edd6fdf0484bcaf7040e -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, restoration complete. Three distinct family controls reached native assertions and were killed; no runtime survivor observed.

Retained control-design finding: the first registration mutation replaced LHS with RHS without retaining a use of LHS. Its cargo invocation **exit101** at compiler `-D unused-variables` before tests, so it was not a discriminating native-test gate. Proof runner **exit1**, try/finally restored exact source (measured diff0). Adjusted only temporary mutation text to retain `let _ = &lhs`, preserving the LHS getter/record capture while changing the assignment target; retried registration and config controls as above. HTTP control was not rerun after its already-measured native failure. Four negative invocations total: three native assertion failures, one compiler-blocked attempt. No lint relaxation or source/test gate weakening. All measured against65963c2c.

Independent bounded floor **145 cases**, stopped at iteration144 (`getConfig/undefinedConfig`). Literal method floors:20each GET/POST/PUT/DELETE,17each request/response/error registration,7each config/base-URL access. Actual selected TS methods execute unchanged. Native primitive kinds/full binary64/BigInt/UTF16 values, opaque function/symbol/object and exception identities, original/current registry refs, array contents, config copy own string/symbol keys (distinct same-description string→A and actual Symbol→B), fresh result versus original config, nested alias identity, call verb/receiver/argument positions and read/call/await/return-adoption traces precede the source-only full body/input pin753432cbb560febc15d8b8f2ad47274aa918b8a9c9a815b1d0a56d4f57ff01e6. No native serialization, re-encoding or roundtrip comparison.

HTTP bodies call eagerly, reject synchronous lookup/call failures through the returned promise, await the normalized actual Axios result, read response.data once, and apply source async-return resolution including thenable adoption/getter rejection. Call arguments retain original identity and explicit undefined positions; no response success guard/error rewriting added. Registration captures LHS before independent RHS owner read, evaluates falsey fresh-array fallback, assigns captured LHS, then reacquires current owner/array and actual push callee/receiver. Config getter is a shallow fresh ordinary copy via required full CopyDataProperties host (own enumerable keys/symbols/getters, nullish skip); base URL is a direct property read without coercion. Owner/property/callee/language/Axios/Await/PromiseResolve operations are required host boundaries, not an independently implemented JS engine or actual transport. Fixture implements only enumerated host inputs and one copy per fixture; no repeated-copy allocation/exhaustive host-policy proof claimed. Caller handle is dropped before queue drain while completion remains observed; no claim of observer-unsubscription coverage.

Sub-agent static review closed body/contract and committed nine-target/provenance/status/parent checks; read-only metadata inspection **exit0**, no remaining concrete finding in this floor. Reviewer ran no Cargo/Node/extractor/census. Pre-payload fixture build initially failed E0505 moving a borrowed runtime into a queued callback; fixed by cloning the runtime handle before the closure. Initial corrected uncommitted native test passed0; final claims use65963c2c measurements above.

Not run/claimed: workspace compilation, full monorepo or full venue integration suite, older venue test targets in this slice, host clippy, wasm clippy, CI, network/HTTP/websocket transport, full client construction/default interceptor processing/error conversion, concrete production binding of required hosts, exhaustive Promise/thenable/proxy/coercion/copy policy, repeated-copy allocation, observer unsubscription or resource exhaustion. Scoped all-target compilation and the enumerated native test are the measured floor.

Final pre-push merge at **914ccd40efbaefbaa3c64bf965f5c781166abe3a**, actual parents **8adb522e7f2a29b05cd98685529219dc4ae76067** and integration **5d104b35e2487e910db00b1770cad99228c94341**. Exact diff from65963c2c over own src/tests/scripts, SDK api-client TS source and SDK manifests **exit0**. Workspace/venue Cargo files and lockfile plus complete shared types/num/store/transport/crypto/candle-cache/exchange directories exact diff **exit0**. Cargo/native tests/controls remain65963c2c measurements, preserved byte-for-byte; no rerun of those gates at the merge claimed. Fresh extractor check at914ccd40 **exit0**, read0updates/0added/0orphaned (11.45s). Independent tomllib post-merge census against97b6f919 **exit0**: exactly9SDKpending->ported/0->divergent, SDK225p/95d/69pending; venue156p/18d/162pending with zero venue rows moved. Following evidence-only commit changes no gate input.

## SDK key validation, normalization and native errors — 2026-10-14

**8 rows off pending: 6 pending->ported, 2->divergent (FORCED)**. Independent Python3.11 tomllib comparison against fresh base **1370d916758b58dffd432069b5a1c4bf4e11ab2d**, exit0: SDK225p/95d/69pending -> **231p/97d/61pending**; venue unchanged156p/18d/162pending. Branch `feat/rust-lighter-sdk-key-validation`. Initial production/test/oracle payload **bdb51bf702536b2aa2713f097f6485e12c955db4**, actual parent1370d916758b58dffd432069b5a1c4bf4e11ab2d. Corrected manifest commit and final native control baseline **d401c278a9cda0c93268cbef702da38249147291**, actual parentbdb51bf702536b2aa2713f097f6485e12c955db4. Fresh base contains coordinator-merged prior slices; no unrelated unreviewed lane work as ancestor. Six real ported targets (KeyPair, two validators, two normalizers, secureCompareKeys) and two real native error targets exist in `src/sdk_key_management.rs`. Four async/crypto functions in this source file remain pending. No shared type/trait change.

Measured commands/exits and their exact source baselines:

- Atbdb51bf702: `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2` **exit0**, zero errors, Finished dev20.39s. Exact committed diff bdb->d401 over own src/tests/scripts, workspace/venue Cargo files and lockfile, and complete shared types/num/store/transport/crypto/candle-cache/exchange directories **exit0**; this check is preserved, not claimed rerun atd401.
- Extractor check `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: atbdb **exit0**, read0updates/0added/0orphaned (33.87s); at corrected d401 **exit0**, read0updates/0added/0orphaned (27.98s). Writer after first symbol edits **exit0**, read0updates (31.66s); after programmatic rust_files derivation from actual rs targets **exit0**, read1updated (36.60s); after status correction **exit0**, read1updated (16.84s); all0added/0orphaned. Writer normalized arrays, no hand-spelled array.
- Final controls below all ran with HEAD held atd401, `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`, `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_key_management key_management_native_values_errors_and_cause_identity -- --ignored --nocapture`.
- Compiled validation family control at is_valid_private_key: change length80 guard to79: **exit101**, named `isValidPrivateKey iteration2 native boolean` (35.19s). Native assertion failed, not compiler rejection.
- Compiled normalization family control at normalize byte conversion: flip low hex nibble, preserving output length: **exit101**, named `normalizePrivateKey iteration52 native UTF16 result` (22.03s). Native assertion failed, not compiler rejection.
- Compiled comparison family control at secure_compare_keys: replace accumulated OR with last XOR, losing earlier mismatches: **exit101**, named `secureCompareKeys iteration102 native comparison` (17.29s). Native assertion failed, not compiler rejection.
- Compiled error-constructor family control at KeyManagerError::new: substituteD800->D801, preserving UTF16 length/changing encoded bytes: **exit101**, named `KeyManagerError.constructor iteration117 native error UTF16` (20.26s). Native assertion failed, not compiler rejection.
- Each mutation restored in try/finally. Same environment/prefix with `--lib --test live_key_management -- --include-ignored --nocapture`: **exit0**, one new native test passed/zero failed (43.22s); library0tests is not an extra gate.
- `git diff --exit-code d401c278a9cda0c93268cbef702da38249147291 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit0**, exact restoration. Final four controls attempted/four killed, no survivor.

Retained attribution finding: an initial four-control runner captured bdb as its fixed source baseline, but the status-only d401 commit occurred while its sequence was running. That sequence produced four named native101 failures, restored test0 and exact source/test/oracle diff againstbdb0; per-invocation HEAD was not recorded, so those numbers are not advertised as all executed atbdb or d401. Repeated all four controls and restored test with HEAD held atd401 as reported above. Eight negative native invocations total across the two sequences, no observed survivor; no compiler-blocked control. Two pre-payload development tests also passed0 but are not final committed-payload gates. Cause marker initially used the wrong native Symbol family; corrected before payload to an Object and direct Rc identity.

Independent bounded floor **121 source cases**, stopped iteration120 (`KeyManagerError.constructor/emoji message, object cause`). Independent method floors: private/public validators15each, private/public normalizers34each, compare15, constructor8. Actual selected TS declarations execute unchanged, including original KeyPair interface and error class in the fixed source-only full corpus pin. Native booleans, full UTF16 outputs and error messages/names/undefined causes, error object cause identity and independent distinct KeyPair native fields are asserted before that source-only pin. The type-field assertions witness representation, not execution of an erased TS interface. No native serialization/re-encoding or roundtrip comparison.

Bodies preserve source UTF16 length80 then exact ASCII hex decision; lowercase-only 0x stripping, original string letter case, ordinary Buffer/Uint8Array lowercase byte hex, normalization validation and exact private/public errors; unequal-length compare return then full per-unit XOR/OR loop. No machine-code constant-time claim. Adapter domain: typed primitive UTF16 strings and owned ordinary unmodified Buffer/Uint8Array byte values; no overridden Buffer.toString, typed-array iterators, constructors/prototypes or global intrinsics. Unsupported-input case is represented explicitly. No resource-exhaustion equivalence claim.

Sub-agent review found a status overclaim for KeyManagerError class/constructor: native field construction does not reproduce Error super/prototype/instanceof/stack semantics. Reclassified both rows to per-row canonical FORCED divergence atd401; body/function and representation mappings remain6ported. Review also verified object-family cause identity, real targets/provenance, authored121 floor, native-before-pin order and actual parents. Final committed metadata/exact bdb->d401 src/tests/scripts review **exit0**, no remaining concrete static finding in the declared domain. Reviewer ran no Cargo/Node/extractor/runtime gate. Class and constructor gaps remain documented divergences, not parity proof.

Not run/claimed: workspace compilation, full monorepo or full venue integration suite, older venue test targets, host clippy, wasm clippy, CI, network/HTTP/websocket transport, WASM initialization/key generation/derivation, whole SDK runtime binding, JS Error prototype/instanceof/stack bridge, exotic Buffer/Uint8Array/global overrides, out-of-type coercions, resource exhaustion, exhaustive inputs, or machine-code timing properties. Scoped all-target compilation and the enumerated native test are the measured floor.

Final pre-push merge at **160743f017caf870a09f1c2e75aa3fad9fb8a3a7**, actual parents **19a97ac5b0df9eb4dbf5c295c5126c62226c749e** and integration **c0567b4ed712109f72436698411e8b35ed782b33**. Exact diff from d401 over own src/tests/scripts, SDK key-manager source and SDK manifests **exit 0**. Workspace/venue Cargo files and lockfile plus complete shared types/num/store/transport/crypto/candle-cache/exchange directories exact diff **exit 0**. Cargo check remains measured at bdb; final native controls/restored test remain measured at d401, preserved byte-for-byte; no Cargo/test rerun at this merge claimed. Fresh extractor check at 160743f017 **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (22.22s). Independent post-merge tomllib census against 1370d91675 **exit 0**: exactly 6 SDK pending->ported, 2->divergent (FORCED), SDK 231p/97d/61pending; venue unchanged 156p/18d/162pending. Following evidence-only commit changes no gate input.

## Venue private stream recovery and timer lifecycle — 2026-10-14

**8 venue pending->ported, 0->divergent**. Independent Python3.11 tomllib comparison against fresh integration base **376188bcf26e9690ad4ff63383514480a3ddb142**, exit 0: venue 156p/18d/162pending -> **164p/18d/154pending**. SDK unchanged 231p/97d/61pending. Branch `feat/rust-lighter-private-recovery`. Production/test/oracle payload **3b2635c2121550b5149fdad519b711eaa56ce7c2**, actual parent 376188bcf26e9690ad4ff63383514480a3ddb142. Clarified manifest wording and final native proof baseline **80f9a3dbd8e4f7e70507fb17d6819cb08b617558**, actual parent 3b2635c2121550b5149fdad519b711eaa56ce7c2. Fresh branch contains coordinator-merged prior slices, no unrelated unreviewed lane work as ancestor. Eight actual body targets in `src/ws_private_recovery.rs`; full LighterPrivateWebsocket class construction/transport/event/auth binding remains pending. Host traits are venue-local, no shared trait/type change.

Measured commands/exits:

- At 3b2635c212: `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2` **exit 0**, zero errors, Finished dev 42.77s. Exact committed 3b->80 diff over own src/tests/scripts, workspace/venue Cargo files and lockfile, and complete shared types/num/store/transport/crypto/candle-cache/exchange directories **exit 0**; check preserved, not claimed rerun at 80.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: at 3b **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (11.46s); at 80 **exit 0**, same green output (12.28s). Writer after programmatic manifest/rs-derived rust_files update **exit 0**, read 1 updated/0added/0orphaned (12.62s); after cooldown-note clarification **exit 0**, read 0updates/0added/0orphaned (12.96s). No hand-spelled rust_files array.
- All four final controls ran with HEAD held at 80f9a3dbd8; runner records/checks actual HEAD before every control and restoration. Environment `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`; `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_private_recovery private_recovery_native_state_timers_callbacks_and_errors -- --ignored --nocapture`.
- Compiled cleanup family control at shared clear helper (three cleanup rows and timer portions of disposal): pass the truthiness-test handle instead of the handle re-read after callee lookup, retaining the second read to discriminate native identity rather than trace alone: **exit 101**, named `stopDiagnosticsLoop iteration15 call0 argument0 native object identity` (26.45s). Native assertion reached; no compiler rejection.
- Compiled scheduling family control at fallback timer scheduling: change grace delay 20000 to 20001: **exit 101**, named `scheduleStreamRecoveryReconnectFallback iteration85 call1 argument0 native number bits` (4.43s). Native assertion reached; no compiler rejection.
- Compiled recovery family control at maybe_recover_private_stream: make stale comparison >= instead of source >: **exit 101**, named `maybeRecoverPrivateStream iteration119 state lastStreamRecoveryAttemptAt native number bits` (4.6s). Native assertion reached; no compiler rejection.
- Compiled activity family control at note_account_stream_activity: substitute D800->D801 in channel text, preserving UTF16 length/changing encoded bytes: **exit 101**, named `noteAccountStreamActivity iteration149 call3 argument0 native UTF16` (6.23s). Native assertion reached; no compiler rejection.
- Every mutation restored in try/finally. Same environment/prefix with `--lib --test live_private_recovery -- --include-ignored --nocapture`: **exit 0**, one new native test passed, zero failed (7.78s). Library 0tests is not an extra gate.
- Exact `git diff --exit-code 80f9a3dbd8e4f7e70507fb17d6819cb08b617558 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit 0**. Four negative invocations attempted/four native kills/no observed survivor; no compiler-blocked attempt. Initial uncommitted 207-case development test passed 0, not a final payload gate.

Independent bounded floor **207 source cases**, stopped iteration 206 (`dispose`, actual selected cleanup helpers composed with base host). Independent method floors: stop17, recovery-clear17, retry-clear17, retry-schedule34, fallback-schedule25, recovery38, activity33, dispose26. All native owner numeric/boolean values, finite/infinite/signed-zero number bits and explicit NaN checks, timer object identity, opaque original thrown identity, captured callees/arguments, deferred delays/callback outcomes, exact UTF16 messages and live getter/write/call traces are asserted before the full source-only corpus pin. NaN payload-bit equivalence is not claimed. Actual selected eight TS arrow members and five readonly static constant declarations execute unchanged; import guards check actual original/local names and both import/type-specifier flags. Full actual logging severity source and types reexport source are pinned alongside selected bodies/inputs. No native JSON serialization, re-encoding or roundtrip comparison.

Body details: timer cleanup tests truthiness then captures global clear callee before repeated handle read, writing undefined only after success; disposal invokes diagnostic/recovery/retry helpers in order, clears positions then balance timers, delegates actual super.dispose and deliberately leaves orders timer to the base host. Retry computes virtual-clock cooldown delay with source Math.max/Math.round, clears prior timer, logs then schedules; callback clears handle before live flags, evaluates actual current isWsRateLimited method then reschedules or calls auth. Fallback callback reads fresh clock/owner fields, repeated timestamp getters, exact zero-count OR strict age >120000, then logs/reconnects. Recovery skips only elapsed <180000 cooldown (equality eligible), uses >180000 no-stream and >120000 stale boundaries, updates attempt timestamp before readonly/rate-limit exits and preserves log/auth/fallback call order. Activity resets only positive failure/streak counters, adds binary64 1, records virtual time, performs ordered cleanup/rate-state/stats calls, rereads count for first-message log and preserves channel lone surrogates/empty-string fallback.

Adapter scope: source typed numeric/boolean owner fields and string channel/topic; ordinary unmodified Math intrinsics and readonly constants. Timer/callee/receiver/owner getter-setter/clock/super/auth operations are required fallible live hosts retaining original thrown identity and actual timer identity. Special `boolean("isWsRateLimited()")` means actual live owner method Get+Call with owner receiver/clock/throws, never a cached flag. Authentication return is discarded without await/adoption/catch. Production host must supply timer queue, cancellation and callback-exception reporting. Fixture explicitly fires the first scheduled callback after supplied state changes, stubs external reconnect/auth/reschedule/base calls, and composes real selected cleanup helpers in designated cases. It proves body decisions/calls, not physical cancellation, recursive timer scheduling, real transport or whole-runtime binding.

Retained producer finding: two initial uncommitted oracle attempts **exit 1** because contextified VM global getter throws were masked as ReferenceError (`clearInterval is not defined`), violating original-token assertion. Repaired producer with DONT_CONTEXTIFY genuine VM global rather than weakening identity checks; standalone producer **exit 0**, followed by the development native test 0 and final native gates above. No production mutation was involved or left behind.

Sub-agent static review found no concrete body mismatch in this domain, requested explicit live isWsRateLimited Get+Call contract and unambiguous cooldown wording; both corrected. Verified all eight real adapted/ported mappings, provenance, actual parents, full class still pending and committed 3b->80 source/test/oracle preservation. Final read-only metadata checker **exit 0**; its first phrase matcher **exit 1** expected wording without the word 'is', corrected checker to accept actual correct prose, no production fix needed. Reviewer ran no Cargo/Node/extractor/runtime gates.

Not run/claimed: workspace compilation, full monorepo/full venue suites, older venue targets, host clippy, wasm clippy, CI, real timer/network/HTTP/websocket operations, full owner/base/auth/runtime binding, physical cancellation, recursive reschedule execution, exhaustive proxy/noncallable/coercion/timer-host policy, reflective Math/static overrides, out-of-type owner fields, NaN payload bits, resource exhaustion or exhaustive state enumeration. Scoped all-target compilation and the enumerated native body test are the proof floor.

Final pre-push merge at **1c41fd2ef4deba49e1a996c52a38d63d01028c21**, actual parents **49ad138dd059bd8362edb36095e0a8f8c885e03a** and integration **0e116b60e90e37e08f3995645493fc20176c0b86**. Exact diff from 80f9a3dbd8 over own src/tests/scripts, actual private-websocket/logging-severity/types TS source and venue manifests **exit 0**. Workspace/venue Cargo files and lockfile plus complete shared types/num/store/transport/crypto/candle-cache/exchange directories exact diff **exit 0**. Scoped Cargo check remains the 3b2635c212 measurement; final native controls/restored test remain the 80f9a3dbd8 measurements, preserved byte-for-byte. No Cargo/test rerun at this merge claimed. Fresh extractor check at 1c41fd2ef4 **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (13.80s). Independent post-merge tomllib census against 376188bcf2 **exit 0**: exactly 8 venue pending->ported/0->divergent, venue 164p/18d/154pending; SDK unchanged 231p/97d/61pending. Following evidence-only commit changes no gate input.

## Venue public ticker readiness and startup — 2026-10-14

**8 venue pending->ported, 0->divergent**. Independent Python3.11 tomllib comparison against fresh integration base **acf29148c3f26a07b72f93d1d964b49e81016ec6**, exit 0: venue 164p/18d/154pending -> **172p/18d/146pending**. SDK unchanged 231p/97d/61pending. Branch `feat/rust-lighter-public-readiness`. Measured production/test/oracle/manifest payload **1b98a5c170f9f1e80910e7a1864e1bde5b539e46**, actual parent acf29148c3f26a07b72f93d1d964b49e81016ec6. Fresh branch contains coordinator-merged prior slices, no unrelated unreviewed lane work as ancestor. Eight real body targets in `src/ws_public_readiness.rs`; full LighterPublicWebsocket class/constructor and concrete owner/REST/transport/timer/emitter bindings remain pending. New host traits are venue-local; no shared trait/type change.

Measured at 1b98a5c170:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit 0**, zero errors, Finished dev 2m08s. One Cargo invocation at a time.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (26.52s). Writer read 1 updated/0added/0orphaned (11.71s), then sequential writer after all edits **exit 0**, 0updates/0added/0orphaned (13.14s); rs-derived rust_files programmatically handles both existing scalar and legacy list targets, no hand-spelled array.
- All five controls use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript` and `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_public_readiness public_readiness_native_lifecycle_callbacks_clocks_and_identity -- --ignored --nocapture`. Runner records/checks actual fixed HEAD before every invocation; no commit during the sequence.
- Compiled cleanup family control at clear_ticker_rest_fallback_watchdog: pass initial truthiness-test handle instead of re-read handle, retaining second getter for native value discrimination: **exit 101**, named `clearTickerRestFallbackWatchdog iteration40 call0 argument0 native object identity` (22.6s). Native assertion reached, not compiler rejection.
- Compiled watchdog family control at watchdog callback: retain second getter but select reason using cached first received-message flag, ignoring false->true live getter sequence: **exit 101**, named `armTickerRestFallbackWatchdog iteration73 call3 argument0 native UTF16` (14.72s). Native assertion reached, not compiler rejection.
- Compiled fallback family control at start_ticker_rest_fallback warning: substitute D800->D801, preserving UTF16 length/changing encoded bytes: **exit 101**, named `startTickerRestFallback iteration120 call1 argument0 native UTF16` (17.11s). Native assertion reached, not compiler rejection.
- Compiled startup family control at on_open: retain both handler getters but pass captured old handler to on after off changes handler H0->H1: **exit 101**, named `onOpen iteration202 call8 argument1 native function identity` (77.02s). Native assertion reached, not compiler rejection.
- Compiled clock family control at ping: retain virtual-clock lookup/call but use performance sample in payload timestamp: **exit 101**, named `ping iteration220 call0 argument0 payload.timestamp native number bits` (19.75s). Native assertion reached, not compiler rejection.
- Every control restored in try/finally. Same environment/prefix with `--lib --test live_public_readiness -- --include-ignored --nocapture`: **exit 0**, one new native test passed/zero failed (29.35s); library 0tests is not an additional proof gate.
- Exact `git diff --exit-code 1b98a5c170f9f1e80910e7a1864e1bde5b539e46 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit 0**, restoration complete. Five negative invocations/five named native kills/no observed survivor; no compiler-blocked control.

Independent bounded floor **221 source cases**, stopped iteration 220 (`ping`, repeated calls with distinct performance/virtual clocks and fresh payloads). Literal independent method floors: hydration sync13, prime12, watchdog clear17, watchdog arm36, fallback stop42, fallback start51, onOpen34, ping16. Native owner flags/values, finite/infinite/signed-zero number bits and explicit NaN checks, opaque timer/object/function/throw identities, actual receiver/callee/argument positions, deferred callback outcomes/delays, full UTF16 warning/reason text, ping payload type/timestamp fields and distinct repeated-call payload references are asserted before full source-only corpus pin. Expected counts come from authored constants or independent TypeScript execution, never the native artifact itself. No native serialization, re-encoding or roundtrip comparison; NaN payload-bit equivalence is not claimed.

Actual selected eight TS arrow members and two readonly static declarations execute unchanged. Import guards check original/local names, module and both clause/specifier type-only flags. Full actual logging severity and types-reexport source are pinned alongside bodies/inputs. Source-side clock and owner/parent/emitter/global receivers are checked; native mock captures source owner/parent/emitter/global receiver identities, while the two clocks are distinct required operations. Discarded-fetch cases return an object whose then getter throws on observation; source and native avoid the getter and retain eager fetch calls. Fixture executes selected cleanup/prime/stop helpers in designated composed cases, fires only the first scheduled callback, and stubs external subscription/fetch/send/reconnect/emit operations.

Bodies preserve hydrated clear-before-stop/optional reason start; proxy-connected AND primed suppression (nonproxy repeat still fetches); primed flag set before discarded fetch; truthy timeout/interval guards and post-callee handle rereads; undefined-only delay/reason/log defaults; watchdog field cleared before live disposal/message/hydration guards and start callee captured before second message flag read; warning flag set before exact UTF16 log, eager discarded fetch before interval; interval callback stops only on disposed or connected+message+hydrated chain, otherwise fetches. onOpen retains seven startup calls in source order, independent current emitter/handler acquisition for off/on, prior-ever-connected reconnect emit then flag assignment. Ping assigns performance sample to pingAt, captures send callee before virtual clock, and passes a fresh source ping literal through the required object host.

Adapter domain: typed boolean flags, Number delays/clocks and UTF16 string reasons, ordinary readonly constants. Fallible live owner/property/callee/receiver/emitter/timer/clock/object operations are required hosts retaining original thrown identity. Fetch returns are discarded without Await/PromiseResolve. Ping object host requires fresh ordinary literal with own ordered type/timestamp data properties and original intrinsic prototype; native fixture witnesses fields/freshness, not JS prototype/descriptors. Literal allocation-before-timestamp resource observability remains outside this adapter. Timer host must supply real queue/cancellation/exception reporting; no physical cancellation/repeated-interval/runtime-binding proof is claimed.

Pre-payload findings retained: first standalone producer **exit 1** for missing parent-receiver carrier; corrected Parent tag. Second producer **exit 1** for authored arithmetic floors (watchdog three binary flags have 8 combinations, interval four flags have 16); independent loop formulas/literal totals corrected, then four additional cases authored for proxy repeat/discarded returns/empty watchdog reason. Standalone producer **exit 0**, development native 221-case test **exit 0**, neither advertised as committed final gate. Initial manifest helper assumed rs always string and threw AttributeError before writing on a legacy list target; its individual exit was not separately captured, so no exit number claimed. Fixed programmatic scalar/list flattening and ran the writer sequentially after all manifest edits. No production mutation was involved in these development findings.

Sub-agent review found no concrete body mismatch in this domain and verified all eight real adapted/ported targets, TS provenance/host-scope notes, full class/constructor still pending, exact parent and committed payload. Read-only committed metadata checker **exit 0**; no reviewer Cargo/Node/extractor/native runtime gate claimed. Prototype/descriptors/concrete runtime bindings remain required contracts, not independently verified parity.

Not run/claimed: workspace compilation, full monorepo/full venue suites, older venue targets, host clippy, wasm clippy, CI, real timer/network/HTTP/websocket operations, full owner/REST/emitter/transport/runtime binding, physical cancellation or repeated interval execution, native JS object prototype/descriptor bridge, exhaustive proxy/noncallable/coercion/timer policy, reflective static overrides, out-of-type arguments, NaN payload bits, resource exhaustion or exhaustive state enumeration. Scoped all-target compilation and the enumerated native body test are the measured floor.

Workflow finding retained: initial manifest repair overlapped the first writer; the final sequential writer ran after all manifest edits, returned 0updates/0added/0orphaned, and the committed-payload check was green. That final stable result is the manifest gate, not an inference from the overlapped invocation.

Final pre-push merge at **b16dabc676440111d23dbacdfb77fb3029b45caa**, actual parents **3226c915778e6fbcab0bc41025ad04d83a7407ef** and integration **74fc4846d408b2b29e843728c4233fe900abb371**. Exact diff from 1b98a5c170 over own src/tests/scripts, actual public-websocket/logging-severity/types TS source and venue manifests **exit 0**. Workspace/venue Cargo files and lockfile plus complete shared types/num/store/transport/crypto/candle-cache/exchange directories exact diff **exit 0**. Scoped Cargo check, final native controls/restored test remain measurements at 1b98a5c170, preserved byte-for-byte; no Cargo/test rerun at this merge claimed. Fresh extractor check at b16dabc676 **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (23.95s). Independent post-merge tomllib census against acf29148c3 **exit 0**: exactly 8 venue pending->ported/0->divergent, venue 172p/18d/146pending; SDK unchanged 231p/97d/61pending. Following evidence-only commit changes no gate input.

## Venue public stream watchdogs, recovery and disposal — 2026-10-14

**8 venue pending->ported, 0->divergent**. Recursive Python3.11 tomllib comparison against fresh integration base **7495cc9eec52fd1c6444775455859a45e20ad247**, exit 0: venue 172p/18d/146pending -> **180p/18d/138pending**. SDK unchanged 231p/97d/61pending. Branch `feat/rust-lighter-stream-watchdogs`. Measured payload **49b2411edd571c809397f8211c3c13914844a424**, actual parent **7495cc9eec52fd1c6444775455859a45e20ad247**. Fresh branch contains coordinator-merged prior slices, no unrelated unreviewed lane work as ancestor. Eight real adapted body targets in `src/ws_public_watchdogs.rs`; full public websocket class/constructor and concrete imported/owner/store/timer/superclass bindings remain pending. New host traits are venue-local; no shared trait/type change.

Measured at 49b2411edd571c809397f8211c3c13914844a424:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit 0**, zero errors, Finished dev 1m07s. One Cargo invocation at a time.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (50.69s). Pre-commit writer **exit 0**, read `Updated 1 manifests; 0 added, 0 orphaned symbols.` (54.78s), after all manifest edits; rust_files derived programmatically from all scalar/list rs targets.
- All controls use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript` and `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_stream_watchdogs stream_watchdogs_native_records_callbacks_recovery_and_disposal -- --ignored --nocapture`. Runner verifies actual fixed HEAD before every invocation, restores production bytes in try/finally, and requires exit 101 plus the named assertion and `test result: FAILED`. No commit during the sequence.
- Compiled timer control at arm: retain old handle instead of the returned setTimeout handle: **exit 101**, named `armOrderBookStaleTimeout iteration0 current orderBookStaleTimeouts entry value native object identity` (75.12s). Native assertion reached, not compiler rejection.
- Compiled prefix control at is_order_book_topic: substitute semicolon for colon in the exact book prefix: **exit 101**, named `isOrderBookTopic iteration25 result native boolean` (237.54s). Native assertion reached, not compiler rejection.
- Compiled enumeration control at arm_existing_order_book_timeouts: pass the original array instead of its first string element while retaining empty/sparse Undefined handling: **exit 101**, named `armExistingOrderBookTimeouts iteration37 call4 argument1 native kind mismatch` (47.17s). Native assertion reached, not compiler rejection.
- Compiled literal control at attempt_ohlcv_recovery: duplicate nowMs with the same value; behavior/value getters remain unchanged but actual native field schema differs: **exit 101**, named `attemptOhlcvRecovery iteration70 literal0 native key order/count` (33.81s). Native assertion reached, not compiler rejection.
- Compiled encoding control at recovery_log: D800->D801 encoded-surrogate substitution in reason, preserving UTF16 length and changing bytes: **exit 101**, named `attemptOhlcvRecovery iteration86 call4 argument0 native UTF16` (31.92s). Native assertion reached, not compiler rejection.
- Compiled market control at maybe_recover_ohlcv_stream: retain market.id acquisition but substitute interval resolution as builder id: **exit 101**, named `maybeRecoverOhlcvStream iteration128 call1 argument0 native UTF16` (28.52s). Native assertion reached, not compiler rejection.
- Compiled disposal control at dispose: omit fresh candle timeout record replacement after successful cleanup: **exit 101**, named `dispose iteration151 current ohlcvNoDataTimeouts native record alias` (27.31s). Native assertion reached, not compiler rejection.
- Restored same environment/scoped prefix with `--lib --test live_stream_watchdogs -- --include-ignored --nocapture`: **exit 0**, one new native test passed/zero failed (46.89s); library 0tests is not an additional proof gate.
- Exact `git diff --exit-code 49b2411edd571c809397f8211c3c13914844a424 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit 0**. Seven negative invocations/seven named native kills/no observed survivor or compiler-blocked control in this sequence.

Independent bounded floor **173 source cases**, stopped iteration **172** (`dispose`, record replacement during clearTimeout). Authored method floors: book arm12, candle arm12, book prefix12, existing book17, existing candle17, recovery55, explicit market recovery26, disposal22. Native owner flags/Number bits (NaN checked separately), timer/object/function/opaque thrown identity, current and old record aliases and direct values/keys/enumerability, captured callee/receiver/argument positions, callback completion/delays, generated interval error UTF16, exact log UTF16 and getter/iterator/call/write ordering are asserted before full source-only corpus pin. Expected counts derive from fixed authored constants or independent TypeScript execution, never the native artifact. No native JSON serialization, re-encoding or roundtrip assertion. NaN payload-bit equivalence is not claimed.

Actual eight TS members and four readonly static declarations execute unchanged; original/local/module/type-only import guards pin clock, severity and three OHLCV helpers plus interval resolver. Actual helper bodies/INTERVAL literal execute independently in source; full severity and types reexport files are pinned. Genuine VM global preserves original getter throws. Timed assignments capture their LHS record before timeout callee acquisition/call; callbacks delete CURRENT record before flags, exposing owner replacement at get/call setTimeout. Enumeration observes own enumerable keys and live values, numeric-key order, hidden entries, empty/sparse arrays, inherited constructor handler and IteratorClose throw precedence. Source entry destructuring is actual JS; custom inner entry iterator effects are a required host contract beyond this floor.

Recovery constructs ordinary parameter literals after captured helper/Boolean arguments; fixed native four-/three-field key schemas assert count/order/extras/duplicates from the constructed Literal.fields, before argument-value assertions. Promotion writes both route flags before logging; timestamp assignment and reconnect follow successful logging. Explicit market lookup captures markets before options symbol, builder before market id, and resolver before options interval. Disposal replaces three records with fresh native alias identities in source order only after preceding operations succeed, and calls actual super receiver through required host.

Adapter scope: typed UTF16 topics/options/reasons, boolean flags and Number clocks/delays with ordinary readonly constants. Live record/property/callee/receiver/import/object/iterator/timer/clock/super operations are required hosts preserving native identities, getter/proxy effects and original throws. Object.keys/entries retain ordinary string-key/entry-pair result domains; reflective replacement intrinsics returning other types are unproved. Literal prototype/descriptors/resource behavior, full custom inner entry iterator effects and concrete runtime bridge are required contracts, not independently demonstrated by this native fixture. External cleanup/arming/reconnect methods are recorded calls; no live network/timer binding or composed full websocket execution claimed.

Pre-payload findings retained: first development native build **exit 101** compiler E0605 at fixture record Rc trait-object conversion, repaired with owned Rc::clone and all alias-map lookup; compiler rejection was not a control. Subsequent native development 173-case tests, including sparse-array and final schema repair, **exit 0**; standalone source producer **exit 0**. Read-only sub-agent review found missing native exact literal key count/order; repaired by recording actual constructed fields and fixed authored schemas. Compiled duplicate-field control above proves repaired assertion discriminates. Reviewer closed that gap and found no remaining concrete mismatch in this eight-body floor; final committed read-only metadata check **exit 0**, no reviewer Cargo/Node/runtime gate claimed. Initial census using only top-level SDK files returned 4pending; that nonrecursive result was not the full SDK census. Corrected recursive census **exit 0** gives the SDK totals above.

Not run/claimed: workspace compilation, full monorepo/full venue suites, older venue test targets, host clippy, wasm clippy, CI, real timer/network/HTTP/websocket operations, physical timer cancellation or repeated timer execution, full owner/store/import/superclass/runtime binding, native JS literal prototype/descriptors, custom inner-entry iterator mutations, exhaustive proxy/noncallable/coercion/timer policy, reflective intrinsic/static overrides, out-of-type arguments, NaN payload bits, resource exhaustion or exhaustive state enumeration. Scoped all-target compilation and bounded native body test are the measured floor.

Final pre-push merge **a0129e554526b949547c2498d246d891e1e63f58**, actual parents **0e944ef745e5cbda6c6f9b023a96481a2f1ddcc1** and integration **22ee2d3fdd6e60b617e95d3179571adf0111590b**. Exact diff from measured payload 49b2411edd571c809397f8211c3c13914844a424 over own src/tests/scripts, venue manifests, actual public-websocket/OHLCV-helper/interval/logging-severity/types TS files **exit 0**. Workspace/venue Cargo manifests, seven complete shared crate directories (types/num/store/transport/crypto/candle-cache/exchange), Cargo config/toolchain paths exact diff **exit 0**. Whole Cargo.lock diff **exit 1**: only safe-cex-venue-bitget adds its safe-cex-crypto dependency. Independent before/after lockfile traversal resolves every dependency unambiguously; all **481** reachable Lighter package records, including versions/sources/checksums/dependencies, match exactly (**exit 0**). These are graph census plus record-equality claims; no new compilation result inferred for Bitget. Scoped Cargo/native controls/restored test remain measurements at 49b2411edd, with their relevant inputs preserved; no Cargo/test rerun at merge claimed. Fresh extractor check at a0129e554526b949547c2498d246d891e1e63f58 **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (38.55s). Recursive post-merge tomllib census **exit 0**: exactly8 venue pending->ported/0->divergent, venue180p/18d/138pending; SDK unchanged231p/97d/61pending. Following evidence-only commit changes no gate input.

## Venue public routing, topic hydration and book fallback startup — 2026-10-14

**8 venue pending->ported, 0->divergent**. Recursive Python3.11 tomllib comparison against fresh branch base **2ba1109d294708d532774387792ec3a216df8c09**, exit 0: venue 180p/18d/138pending -> **188p/18d/130pending**, SDK unchanged 231p/97d/61pending. Branch `feat/rust-lighter-public-subscriptions`. Initial eight-body commit **83e74797090c0fc5a12c196b1a5db759fda95aef** has actual parent 2ba1109d29; correction payload **6e406034fc17da05422df36b55c208f55995d95b** has actual parent 83e7479709. Those two own unmerged commits form this slice and received sub-agent static review; no unrelated unreviewed lane ancestor. Eight real adapted functions in `src/ws_public_subscriptions.rs`; full websocket class/constructor, subscription batching, actual imported/transport/store/socket/timer/JSON bridge remain pending. New required-host traits venue-local; no shared trait/type change by this slice.

Final measured merge **a221659baaf00a48b90c061275b387b6b7c9e3f2**, actual parents **6e406034fc17da05422df36b55c208f55995d95b** and integration **1ef197eb733830953e0ea1cffb40de13ee24269d**. Exact diff from corrected payload 6e406034fc over own src/tests/scripts, venue manifests and actual public-websocket/transport-types/logging-severity/types TS files **exit 0**. Integration added crypto source modules that are Lighter dependencies, so earlier scoped results were not carried as final compilation/native gates; rebuilt and ran complete control/restored sequence below at this merge.

Measured at a221659baaf00a48b90c061275b387b6b7c9e3f2:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit 0**, zero errors, Finished dev 2m07s. One Cargo invocation at a time.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (49.01s). Before initial commit, writer **exit 0**, 1updated/0added/0orphaned (17.31s); after correction note edit, sequential writer **exit 0**, 0updated/0added/0orphaned (26.29s). rust_files derived programmatically from all scalar/list rs targets, no hand-spelled array.
- Controls use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript` and `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_public_subscriptions public_subscriptions_native_routes_topics_aliases_callbacks -- --ignored --nocapture`. Runner verifies fixed actual HEAD before every call; requires101 plus named assertion and `test result: FAILED`; no commit during sequence; each production mutation restored in try/finally.
- Compiled route control at is_ws_proxy_route_enabled: negate final public host truthiness: **exit 101**, named `isWsProxyRouteEnabled iteration30 result native boolean` (60.6s). Native assertion reached, not compiler rejection.
- Compiled connect control at connect_and_subscribe URL: encoded surrogate D800->D801 substitution preserving UTF16 length/changing bytes: **exit 101**, named `connectAndSubscribe iteration46 call4 argument0 native UTF16` (33.5s). Native assertion reached, not compiler rejection.
- Compiled missing_host control at connect_and_subscribe includes: swallow required member-get error and return Undefined instead of propagating catchable error: **exit 101**, named `connectAndSubscribe iteration91 native completion mismatch` (43.4s). Native assertion reached, not compiler rejection.
- Compiled market_payload control at on_markets_changed: duplicate channel field with the same value, leaving field lookup/value semantics unchanged: **exit 101**, named `onMarketsChanged iteration97 literal0 native key order/count` (34.44s). Native assertion reached, not compiler rejection.
- Compiled send control at send_payload: retain payload parameter use but pass owner object to serializer instead of original payload: **exit 101**, named `sendPayload iteration112 call0 argument0 native object identity` (33.04s). Native assertion reached, not compiler rejection.
- Compiled sync control at sync_bbo_ticker_topics: retain live getter event but delete from cached pre-enumeration topics record after getter replaces owner record: **exit 101**, named `syncBboTickerTopics iteration131 Topics0 native missing key` (35.72s). Native assertion reached, not compiler rejection.
- Compiled tickers control at handle_tickers_event: pass entire ticker array in data field instead of current ticker while retaining handler capture first: **exit 101**, named `handleTickersEvent iteration136 literal0 field0 data native object identity` (36.86s). Native assertion reached, not compiler rejection.
- Compiled cooldown control at is_order_book_rest_fallback_cooling_down: compare clock+1 instead of sampled clock: **exit 101**, named `isOrderBookRestFallbackCoolingDown iteration163 result native boolean` (33.63s). Native assertion reached, not compiler rejection.
- Compiled book_interval control at start_order_book_rest_fallback: interval delay 2001 instead of source 2000: **exit 101**, named `startOrderBookRestFallback iteration176 call3 argument0 native number bits` (20.87s). Native assertion reached, not compiler rejection.
- Restored same environment/scoped prefix with `--lib --test live_public_subscriptions -- --include-ignored --nocapture`: **exit 0**, one new native test passed/zero failed (58.77s). Library 0 tests is not an additional proof gate.
- Exact `git diff --exit-code a221659baaf00a48b90c061275b387b6b7c9e3f2 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit 0**. Nine negative invocations/nine named native kills/no observed survivor or compiler-blocked control in final sequence.

Independent bounded floor **209 source cases**, stopped iteration **208** (book interval, owner record replacement during setInterval call with first callback fired). Fixed authored method floors: routing46, connect47, markets-changed19, send5, topic-sync15, ticker-batch14, cooldown30, book-fallback33. Actual eight TypeScript members/two readonly static declarations execute unchanged, plus full severity/types-reexport and transport type file pins. Original/local/module/type-only import guards bind routing registry/nanoid/clock/severity. Required external method/getter/iterator/literal/timer/socket/serializer operations are instrumented fixtures, not a concrete full JS runtime bridge.

Native flags/Number bits (explicit NaN check), opaque object/function/original thrown identity, topic/cooldown/interval/owner record values and aliases, own field count/enumerability/UTF16 order, distinct socket receivers and handler identity, actual callee/argument positions, full UTF16 URLs/logs/payload values, timer delays/callback outcomes, iterator receiver identities and getter/iterator/write order assert directly against independent TypeScript execution. Fixed native subscribe literal schema[type,channel] and ticker literal schema[data] assert actual count/order/extras/duplicates and values/fresh identity. Every native assertion for every case and authored counts executes before deferred serializer-return forwarding checks, then full source-only hash last. No native serialization/re-encoding/roundtrip assertion. All counts come from fixed authored expectations or independent TypeScript execution, never native artifacts.

Source JSON.stringify required host returns fixed `wire/` plus D800 sentinel; only original payload handoff and returned-value forwarding are proved, not JSON codec equivalence. Compiled send control changes native serializer argument identity while the sentinel output stays constant; direct native assertion still kills it. Routing helper remains a captured actual import boundary; two composed cases execute actual selected eligibility body. String() in source invokes actual builtin on market ids; includes property/get/call is independently instrumented with primitive String receiver. Generic string-property intrinsic replacements outside ordinary domain, actual routing resolution/nanoid/JS object/JSON runtime binding, and native JS prototypes/descriptors are unproved.

Bodies retain typed boolean/nullish route short circuits and repeated parent evaluation; eligibility/direct/readonly route choices, endpoint includes before nanoid, ordered flags/clock/socket assignments, three independent current-socket/method/handler captures, and proxy fallback priming. Market topic loops capture String before id Get, preserve falsey id suppression, and source record/prefix evaluation order; Object.keys captures before owner record lookup, deletes through current record after live prefix, and preserves source for-of/IteratorClose error precedence. Ticker batch requires Array.isArray then per-item fresh data literal through captured handler. Cooldown samples Number clock before record/topic lookup and source falsey fallback, with full Abstract Relational Comparison required host (Number/inherited-constructor-function fixture floor; broader Number/BigInt/coercion behavior remains required contract). Book fallback retains log-before-eager-discarded-fetch, assignment old record before interval lookup/call, original book/callback references and live callback disposal/optional-handler guards. Discarded fetch returns have a throwing then getter; body never observes/adopts them.

Review finding corrected: ResolvedWs.host is optional in actual transport/types.ts. Initial body extracted string too early, causing Rust panic if a true first eligibility lookup was followed by missing second routing host. Correction keeps native AccountValue until actual fallible includes Get/Call; initial-missing-host and lost-host-on-second-resolve source fixtures assert exact TypeError name/full UTF16 message, and native checks decoded generated name/message. Compiled error-suppression control kills named native completion assertion. Full native JS Error prototype/stack/resource identity bridge is required runtime contract, not fixture proof.

Development findings retained: first source producer **exit 1**, import-path expectation wrongly used transport/resolveTransportRouting, corrected to actual transport/registry. Next producer **exit 0** nevertheless emitted unexpected generated ReferenceErrors because skeleton class name Subject did not define LighterPublicWebsocket static references; found by inspecting outputs, corrected actual class name and explicit reject-unexpected-generated assertion. Final source producer **exit 0** with only two authored exact includes TypeErrors permitted. Initial native development build **exit 101**, -Ddead-code on unused event helper, not a mutation gate; removed helper and unified all traces/effect hooks as full UTF16 units. Development native 207 test **exit 0**; corrected209-case native development test **exit 0**, neither advertised as committed final native gate. Initial commit 83 scoped check **exit 0**, Finished dev 1m13s, extractor **exit 0**22.59s, but review found missing-host semantic gap afterward. Corrected 6e scoped check **exit 0**, Finished dev 2m08s, extractor **exit 0**30.34s; these are measurements at those SHAs, superseded for final compilation/native claims by merged-head gates above. Review also requested full relational-comparison contract and all native checks before forwarding; both repaired. Final committed read-only metadata checker at 6e **exit 0**, no reviewer Cargo/Node/native runtime outcome claimed.

Not run/claimed: workspace compilation, full monorepo/full venue suites, older venue test targets, host clippy, wasm clippy, CI, real network/HTTP/websocket/timer operations, physical timer cancellation/repeated interval execution, full owner/store/routing/nanoid/socket/JSON runtime bridge, native JS literal/Error prototypes/descriptors/stacks, exhaustive coercion/proxy/noncallable/timer policy, reflective static/intrinsic overrides, out-of-type inputs, NaN payload bits, resource exhaustion or exhaustive enumeration. Scoped all-target compilation and bounded body test are measured floor. Following evidence-only commit changes no gate input.

Final pre-push merge **f44c9cd22f18b5f0f28eec73525d844f25165388**, actual parents **76f16a6054223a7d09f2a6345f478cb4f42d760c** and integration **5c3c55b7b5631e9d04b3d31bc98b213b5888f18d**. Exact diff from measured merge a221659baaf00a48b90c061275b387b6b7c9e3f2 over own src/tests/scripts, venue manifests and actual public-websocket/transport-types/logging-severity/types TS files **exit 0**. Workspace/venue Cargo manifests, complete seven shared dependency crate directories and Cargo config/toolchain paths exact diff **exit 0**. Whole Cargo.lock diff **exit 1**, only Phemex adds safe-cex-crypto. Independent before/after lock traversal resolves every dependency unambiguously; all **481** reachable Lighter package records (versions/sources/checksums/dependencies) match, **exit 0**. No Phemex compilation claim. Cargo/native controls/restored test remain measurements at a221659baa with relevant inputs preserved; no Cargo/test rerun at final merge claimed. Fresh extractor check at f44c9cd22f18b5f0f28eec73525d844f25165388 **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (29.10s). Recursive final census **exit 0**: exactly 8 venue pending->ported/0->divergent, venue 188p/18d/130pending; SDK unchanged 231p/97d/61pending. Following evidence-only commit changes no gate input.

## Venue exchange signer and snapshot error recovery — 2026-10-14

**8 venue pending->ported, 0->divergent**. Independent recursive Python3.11 tomllib comparison against fresh integration parent **d5d564a0ddc29cc84e1699b30da0726346492599**, exit 0: venue 188p/18d/130pending -> **196p/18d/122pending**; SDK unchanged 231p/97d/61pending. Branch `feat/rust-lighter-exchange-error-recovery`. Actual measured production/test/oracle/manifest payload **98d48b0375a2280513eeb03caae7c47e0c459db2**, actual parent **d5d564a0ddc29cc84e1699b30da0726346492599**. Fresh branch contains coordinator-merged prior slices, no unrelated unreviewed lane ancestor. Eight real adapted targets in `src/exchange_error_recovery.rs`; full LighterExchange class/constructor and concrete clock/owner/tier/wasm/property/coercion/instanceof/regex/JSON bridge remain pending. New host traits venue-local; no shared trait/type change.

Measured at 98d48b0375a2280513eeb03caae7c47e0c459db2:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit 0**, zero errors, Finished dev 46.70s. One Cargo invocation at a time.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (28.35s). Pre-payload writer after all manifest edits **exit 0**, 1updated/0added/0orphaned (32.89s); rust_files programmatically derived from all scalar/list rs targets, no hand-spelled array.
- Controls use `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript` and `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_exchange_error_recovery exchange_error_recovery_native_values_getters_cooldowns_and_throws -- --ignored --nocapture`. Runner checks fixed actual HEAD before every invocation; requires exit 101 plus named native assertion and `test result: FAILED`; no commit during sequence; every production mutation restored in try/finally.
- Compiled guidance control at guidance standard branch returns UNKNOWN, retaining STANDARD use to avoid dead-code rejection: **exit 101**, named `getAccountRateLimitGuidanceMessage iteration0 result native UTF16` (38.42s). Native assertion reached, not compiler rejection.
- Compiled normalize control at normalizer retains both message Gets but returns cached first truthiness-test value after second getter changes it: **exit 101**, named `normalizeSignerErrorMessage iteration23 result native UTF16` (45.1s). Native assertion reached, not compiler rejection.
- Compiled retry control at short-circuit retry comparison changes strict before to inclusive equality: **exit 101**, named `shouldShortCircuitSignerRetry iteration30 result native boolean` (19.42s). Native assertion reached, not compiler rejection.
- Compiled retry_message control at retry reason uses encoded surrogate D800->D801 substitution, preserving UTF16 length/changing bytes: **exit 101**, named `getSignerRetryBlockMessage iteration58 result native UTF16` (22.86s). Native assertion reached, not compiler rejection.
- Compiled warning control at warning timestamp stores clock-1 instead of exact sampled clock: **exit 101**, named `warnSignerUnavailable iteration99 state lastSignerWarningAt native number bits` (29.72s). Native assertion reached, not compiler rejection.
- Compiled extract control at JSON fallback returns original serializer error instead of catching it and invoking String: **exit 101**, named `extractOrderErrorMessage iteration129 native completion mismatch` (31.53s). Native assertion reached, not compiler rejection.
- Compiled transient control at retain original classifier call effects then force false for ERR_NETWORK code: **exit 101**, named `isTransientLighterRestTransportError iteration156 result native boolean` (115.25s). Native assertion reached, not compiler rejection.
- Compiled snapshot control at snapshot nontransient Error passes owner object to emitter instead of original error, preserving original strings: **exit 101**, named `handlePrivateSnapshotFetchError iteration187 call1 argument0 native object identity` (21.67s). Native assertion reached, not compiler rejection.
- Restored same environment/scoped prefix with `--lib --test live_exchange_error_recovery -- --include-ignored --nocapture`: **exit 0**, one new native test passed/zero failed (95.21s). Library 0 tests is not an additional proof gate.
- Exact `git diff --exit-code 98d48b0375a2280513eeb03caae7c47e0c459db2 -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests sdk-rs/safe-cex-venue-lighter/scripts`: **exit 0**. Accepted controls: eight named native kills at the same fixed SHA, first six retained across runner restart and last two measured after repair; no observed survivor. Earlier compiler-blocked and earlier-than-intended count failures are separately retained below.

Independent bounded floor **221 source cases**, stopped iteration **220** (`handlePrivateSnapshotFetchError`, log-call original throw with timestamp already assigned). Fixed authored method floors: guidance8, normalization21, retry29, retry-message40, warning12, extraction34, transient-classifier40, snapshot-handler37. Native decoded scalar returns, BigInts, opaque symbols/functions/objects/original and alternate throws, Number bits (NaN checked separately), mutable property/state values, exact UTF16 messages/reasons, actual callee/receiver/argument positions and getter/call/write order compare against independently executed TS before source-only corpus hash. Expected counts come from fixed authored constants or independent source execution, never native artifacts. No native serialization/re-encoding/roundtrip comparison.

Actual eight TS arrow members, existing WASM issue helper, actual three guidance strings/transient cooldown/static signer-warning cooldown and full severity/types reexport source are pinned. Import guards verify original/local/module/type-only binding for actual poseidon isWasmReady, virtual clock and severity. Genuine VM global preserves getter throws. Composed WASM issue fixture executes actual helper and native required lower-case member boundary; composed extraction/transient fixtures execute actual selected methods. Source String conversion uses builtin on representative primitives/ordinary error/record/function fixtures; native fixture renders those independently, preserving native input identities/UTF16 and observed coercion gets. Full arbitrary JS coercion/Unicode case folding/instanceof/regex policies remain required hosts, not an independently implemented native JS engine claim.

Body behavior: cached-tier exact standard/plus/premium selection; original-string extraction fast path, actual Error instanceof then two independent message Gets; ordered independently live nullish response/data/error/message paths (false and zero retained), String candidate, JSON lookup and call both inside try, then String fallback on any caught original error. JSON undefined return is preserved as native Undefined; downstream classification includes an authored exact generated TypeError at undefined.toLowerCase. Retry captures clock then helper before reason getter, readiness only for wasm issue; retry hint captures Math.max before until getter and ceil only when remaining positive; conditional and template reason Gets remain separate. Warning timestamp stores exact clock before potentially throwing log. Transient classifier captures String before code lookup, actual uppercase/extraction/lowercase callees then strict code/message cases and captured regex host only when needed. Snapshot emitter captured before instanceof/classification/extraction; actual original Error/string passed through. Transient snapshot warning performs two independent last-warning Gets, stores timestamp before log callee/extraction/template, retaining partial state and original errors.

Adapter domain: ordinary Number-valued owner state/clock/Math returns; typed UTF16 signer warning/retry reasons with optional messages. Unknown error values remain native and full fallible property/callee/receiver/instanceof/template/regex/JSON operations are required hosts. Reflective numeric intrinsic overrides or non-Number numeric fields are outside this adapter. Native mocked case folding covers authored ASCII/unpaired-surrogate inputs; concrete JS Unicode/regex/prototype/coercion/runtime bridge is unproved. Error prototype/descriptors/stack and primitive-to-object boxing are required bridge policies, not class-port claims.

JSON.stringify host returns a fixed wire/D800 sentinel or authored Undefined, and throws in lookup/call cases. Sentinel return forwarding assertions run only after all 221 native assertions and authored counts, then hash last. No JSON codec equivalence claimed. Native serializer argument identities, caught-error precedence, fallback String arguments and non-serializer outputs are directly asserted before forwarding.

Findings retained: first producer **exit 1**, fixture run(method) parameter shadowed capture function named method, yielding generated TypeError `method is not a function`; generated-exception guard caught it. Diagnostic rerun **exit 1** named that cause; renamed capture helper, producer **exit 0**. Initial native development test **exit 101**, named `shouldShortCircuitSignerRetry iteration57 independent source call count` (native composed helper bypassed required lower-case member operation); corrected mock composition and authored Math.ceil arity to 1, restored221-case development test **exit 0**, not committed final gate. A read-only temporary control-pattern validation had Python SyntaxError **exit 1** (else1 typo), no mutation/Cargo invocation; corrected validation **exit 0**.

First guidance mutation at 98d48b0375a2280513eeb03caae7c47e0c459db2 **Cargo exit 101**, 17.5s, compiler `constant STANDARD is never used` under -Ddead-code; intended native assertion was NOT reached, runner **exit 1**. This is a compiler-blocked finding, not a passing discrimination gate or native survivor. Production restored in finally and exact source/test/oracle diff **exit 0**. Repaired control explicitly retains STANDARD use while returning wrong UNKNOWN guidance; restarted at the same fixed SHA. Later transient control (ERR_NETWORK_BROKEN) was also caught: Cargo exit 101, named `isTransientLighterRestTransportError iteration158 independent source call count`, native 5 versus independently executed source 4 calls (20.4s), before intended iteration159 boolean assertion. Runner exit 1 because its intended label was not reached; this caught mutation is not a survivor. Production restored and exact diff exit 0. Retained the six already completed native controls without repeating them; repaired classifier control preserves call effects and corrupts returned boolean, resumed final two controls at the same fixed SHA. All eight accepted controls reached named native assertions as recorded above; no committed changes between proof invocations; each starts from restored source/test/oracle bytes.

Sub-agent review found no concrete body mismatch in this domain, requested explicit ordinary Number-valued state/clock/Math return note (added), verified native assertion ordering/opaque identities/real targets/class+constructor pending and committed actual parent. Final committed read-only checker **exit 0**, no reviewer Cargo/Node/extractor/runtime result claimed.

Not run/claimed: workspace compilation, full monorepo/full venue suites, older venue test targets, host clippy, wasm clippy, CI, real WASM/signer/network/HTTP/socket/timer operations, concrete owner/tier/clock/instanceof/coercion/regex/Unicode/JSON runtime bridge, native JS Error prototypes/descriptors/stacks or boxing, exhaustive coercion/proxy/noncallable policy, reflective intrinsic/static overrides, out-of-type numeric arguments, NaN payload bits, resource exhaustion or exhaustive state enumeration. Scoped all-target compilation and bounded native body test are measured floor. Following evidence-only commit changes no gate input.

Final pre-push merge **651e91717f3a0a85c816ad72915eb2d9740e6b1c**, actual parents **402f897ed737dac4fa86b7752e8efa993b24796a** and integration **e8fea3f53ada0bda88ee35193548bbbdb9f4063d**. Exact diff from measured payload 98d48b0375 over own src/tests/scripts, venue manifests, actual exchange/logging-severity/types TS files **exit 0**. Workspace/venue Cargo manifests, complete seven shared dependency crate directories and Cargo config/toolchain paths exact diff **exit 0**. Whole Cargo.lock diff **exit 1**, only WOO adds futures-util. Independent before/after lock traversal resolves every dependency unambiguously; all **481** reachable Lighter records (versions/sources/checksums/dependencies) match, **exit 0**. No WOO compilation claim. Cargo/control/restored native exits remain measurements at 98d48b0375 with relevant inputs preserved; no Cargo/test rerun at this merge claimed. Fresh extractor check at 651e91717f3a0a85c816ad72915eb2d9740e6b1c **exit 0**, read `Would update 0 manifests; 0 added, 0 orphaned symbols.` (38.57s). Recursive final census **exit 0**: exactly 8 venue pending->ported/0->divergent, venue 196p/18d/122pending; SDK unchanged 231p/97d/61pending. Following evidence-only commit changes no gate input.

## Exchange account access/preflight — eight pending rows (2026-10-14)

Payload/gate SHA `1c6c04d82b98f07b57e0f44000bf71fb55ca2d8c`; actual fresh parent `039d308cd99cfd09e22205fb1083c3df5a306548`. Branch `feat/rust-lighter-account-lookup` is fresh from the user-assigned integration commit; no carried-over unreviewed lane work is an ancestor. **Eight pending->ported, zero divergent** in lighter.exchange.ts only. tomllib row identity/status comparison: this file44 ported/1 divergent/56 pending ->52/1/48, eight exact transitions, zero other row transitions in this file. Full venue census is a separate claim; not inferred from this file.

The user repartitioned the lane while a private-websocket slice was in proof. That prior local branch is parked at `7f5f41c754` (payload `d4131f94e460d0efad56b01b49950012ba7a3068`), not pushed/submitted and not an ancestor of this slice; its temporary mutation was restored and its incomplete controls recorded on that branch. No private/public/builder-approval/signer/API file or transferred SDK code/manifests are ported by this slice.

Mapped real bodies: extractAccountFromResponse, fetchParsedAccountLookup, ensureAccountIndex, getValidatedApiKeyIndex, getPrivateWsAccountIndex, getPrivateWsApiKeyIndex, getAccount, warmupSignerRuntime. New venue-local host captures actual Get/Set, callee/receiver, ordinary import/endpoint table bindings, full Error Construct and thrown projection. Both await and internal async-return resolution are required bindings; returned thenables are not silently settled as plain objects. Source class, HTTP, JS Get/coercion/Array/Promise/recursive-adoption engine and wasm/signer integration remain separate contracts. No shared trait/type changed.

Measured commands at the gate SHA:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, Finished dev2m08s (including observed package-cache lock wait). This compiles this venue/all-targets and its dependencies, not workspace compilation.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, actual output Would update0 manifests;0 added;0 orphaned, Done20.59s. Writer preparation before commit exited0, Updated0 manifests, Done14.24s; rust_files derived from actual rs targets, not manually enumerated.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_account_access --jobs 2 -- --include-ignored --nocapture`: restored **exit0**, one named target/test;190 native cases, enumeration stopped189. No other target's execution is claimed. Restore exact src/tests/scripts diff to gate SHA exited0. Cargo commands ran sequentially.

Independent authored/source floors: extract57, parsed lookup27, ensure14, validated key24, websocket account19, websocket key16, getAccount22, warmup11; total190, last iteration189. The live producer executes actual eight TS members, original parseAccountIndex member and two actual utils helpers, checks full import originals/module/type-only flags and actual endpoint table AST. Intrinsic/helper engine traces are at required delegated host boundaries. VM fake API promises use the source realm; proxy fulfillment Get(then) is preserved before native continuation. Native assertions cover decoded scalars, binary64 bits/NaN, large native BigInt values, UTF16/lone surrogates, opaque function/Symbol/object/original throw identities, receiver/property/argument order, live changed fields, constructor arguments and actual native constructed Error fields, exact ordered literal keys/values and decoded returned native record fields. Six inherited VM Object.prototype.then cases cover getter throw/queued resolve/reject in getAccount and lookup. Microtask drain is bounded100 (exhaustion would fail at99); no exhaustive queue/recursive-adoption claim. **All native assertions precede the source-only corpus pin; there is no native serialization, roundtrip or wire equality.** Source hash `25272e0d2d77d8e4db95fe424bb5150c92bed4c15c49387f418777aa068fe46c`.

Compiled native mutations, each at the gate SHA with try/finally restoration:

- record-surrogate: **Cargo101**, 11.16s, named native assertion `iteration0 extractAccountFromResponse literal:referrer nativeUtf16`.
- lookup-error: **Cargo101**, 8.05s, named native assertion `iteration67 fetchParsedAccountLookup Error arg0 nativeUtf16`.
- cache-index: **Cargo101**, 7.34s, named native assertion `iteration81 ensureAccountIndex set:accountIndex nativeBigInt`.
- validated-index: **Cargo101**, 13.72s, named native assertion `iteration101 getValidatedApiKeyIndex initialResult nativeNumberBits`.
- ws-key-string: **Cargo101**, 5.89s, named native assertion `iteration135 getPrivateWsApiKeyIndex initialResult nativeUtf16`.
- account-schema: **Cargo101**, 4.55s, named native assertion `iteration154 getAccount nativeLiteralOwnKeySchema`.
- warmup-completion: **Cargo101**, 5.5s, named native assertion `iteration179 warmupSignerRuntime asyncResult nativeKindMismatch`.
- return-then-adoption: **Cargo101**, 4.77s, named native assertion `iteration173 getAccount asyncResult nativeCompletionMismatch`.

The surrogate control replaces D800 with D801 in the native referrer, preserving length while changing bytes. The schema control duplicates userId instead of affiliateId; the async-return control bypasses required resolve_return and reaches native completion failure at the inherited-then getter case. All eight accepted controls failed their intended named native assertions on their first runs; no compiler failure is counted and no mutation survivor was observed. Restored target exit0, 4.56s, source restored exactly.

Retained development/review findings (not final gates): initial producer exited1 because VM function declarations were nonconfigurable globals; moved unchanged helper bodies into a lexical export wrapper with import-bound host globals, then producer0. Dev1 native test exited101 at missing Array constructor identity (script had tagged a function as a generic callee); source and native function identities repaired. Review found opaque Awaited marker fulfillment and missing async-return adoption; both repaired before final gates. Dev2 native test exited101 at iteration57 nativeOperationOrder because cross-realm mock promise resolution produced two observable payload Get(then) calls; use source-realm promises and retain its resolution lookup, then dev3 native190 passed0. Read-only sub-agent review closed these boundaries and direct record/Error-field assertions; reviewer ran no runtime commands. The controls above independently prove native discrimination after repair.

Not run: workspace check/full monorepo build or suites, full venue/older native targets, host clippy, wasm clippy, CI, real HTTP/network/timers, actual wasm/signers, or full exchange/JS engine/recursive thenable integration. Ordinary parser Number-or-undefined and unmodified ENDPOINTS domains are explicit in module/row notes.

Merged-head gates: `c6a096540cbe9b4445dcb40d3834dc058b4a0690`, actual parents `908ee7d01e304f5d49f7db589fa061e06ff497b7` and integration `970c8a3a41e3ff7e0da31cbf524f7d6b0e064849`. Earlier merge `908ee7d01e304f5d49f7db589fa061e06ff497b7` had parents evidence `61bd16fff7d96ed63dd23de4d6c8c05688e5f1ad` and integration `053b8969251168bffc4b028b6766c4fef077e048`. Exact payload-to-merged-head diff of whole venue src/tests/scripts, assigned exchange manifest and all three actual TS oracle inputs exited0. Shared exchange/transport source and workspace Cargo metadata did change, so **scoped all-targets Cargo was rerun at c6a096: exit0**, Finished dev1m59s, and **live_account_access native target rerun: exit0**,190 cases/stop189, Finished test13.53s (test0.26s). **Fresh extractor at c6a096: exit0**, Would update0 manifests, Done18.27s. Native negative controls remain measurements at their explicitly named payload SHA, not rerun at this merged SHA; their exact source/test/oracle inputs survive byte-for-byte, and the positive native gate was rerun against changed shared dependencies. Recursive Lighter Cargo.lock records were independently compared before/after:481 equals481, entire records equal, exit0; workspace membership added a separate lighter-sdk crate. No compilation claim for that new crate.

At c6a096, tomllib census exited0: eight exact pending->ported in assigned exchange file,44/1/56 ->52/1/48, stop after all101 rows in that file; no completeness inference for other files. Diff against actual integration parent has exactly six owned paths: exchange manifest, new account-access src/test/oracle, venue lib export and this evidence. Transferred venue/SDK work is upstream-only; no transferred path is part of this slice's implementation diff.

Final merge preservation: `c538aec198f83352518f6aac1892767dbb9be093`, actual parents evidence `09ff6bb382cf48a3ab277ecedeba985a466bc7ad` and integration `61d416782d630e0058f93a515e81fafc9be87187` (fresh fetch and merge-down). Full venue src/tests/scripts, assigned manifest and three actual TS inputs, workspace/venue Cargo manifests, complete seven shared dependency directories, Cargo configs/toolchains, package.json and extractor implementation were byte-identical to c6a096 (scoped exact diff exit0). Cargo.lock changed only outside the independently compared Lighter dependency records:481 before/after, full recursive records equal, exit0. Thus the c6a096 scoped Cargo0/native1900 exits stand by measured input preservation; no claim of a new Cargo execution at c538aec. Updated global manifest/exemption inputs required a **fresh extractor at c538aec: exit0**, actual Would update0 manifests, Done20.21s. Diff to this actual integration parent contains exactly the six owned paths listed above; no transferred venue/SDK path. The following evidence-only commit changes no gate inputs; final source/input diff is checked before push. Final exchanged-file census remains eight pending->ported, zero divergent,52/1/48, all101 rows of the assigned file considered; other-file census not claimed.

## Eight exchange reconciliation/lifecycle bodies — snapshot-reconciliation slice

Payload/gate SHA: `4cd43bffd8a5b686391a68bbb4fc9938137dae9b`. Actual fresh parent: `98a847bb171d9a895a95f1e4e287c9126fc2c66e`. Branch `feat/rust-lighter-snapshot-reconciliation`; no unreviewed lane implementation ancestor. Only lighter.exchange.ts rows are ported: fetchAndSetPrivateSnapshot, fetchAndSetPrivateSnapshotHelper, tick, fetchBalance, fetchAndSetBalance, #syncLeverageFromExchange, getPrivateWsAuthToken, dispose. No transferred lighter-sdk or other venue file is part of this implementation slice. Native read-only review closed bodies and committed metadata, with no remaining concrete static finding; reviewer did not run Cargo/Node/gates.

Independent Python3.11 tomllib census at payload SHA: **8 pending->ported, 0 divergent**. Assigned exchange file:52 ported/1 divergent/48 pending ->60/1/40, all101 rows of this file compared to actual parent; enumeration ends at this file, no other-file completeness claim. All8 rs targets exist. rust_files derived programmatically from actual targets; writer preparation exited0, Updated0 manifests,0 added/0 orphaned, Done72.77s.

Measured payload commands (Cargo sequential, never concurrent):

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, Finished dev1m33s. Scope is this venue/all-targets/dependencies, not workspace compilation.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, actual output Would update0 manifests;0 added/0 orphaned, Done162.19s. Output read, not inferred from shim exit.
- Native test command for each control and restored pass: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_reconciliation --jobs 2 -- --include-ignored --nocapture`.

Independent authored/source floors: wrapper4, helper42, tick18, fetchBalance19, fetchAndSetBalance19, syncLeverage17, auth31, dispose20; **170**, last iteration **169**. The oracle executes actual eight TS member bodies and original invalidation constant; AST import binding/module/type checks, actual LogSeverity source and type re-export are pinned. Actual source-realm Promise.all/GetIterator/two next/IteratorClose, primitive/symbol error projection, inherited then lookup/adoption, ordinary property getter/descriptor and live owner changes are preserved at required host boundaries. This is a method-body adapter, not construction of the complete exchange or implementations of delegated imports, HTTP, timers, signer, websocket or JavaScript engine.

Direct native assertions precede source-only pin: decoded binary64 bits/NaN, booleans, null/undefined, UTF16/lone-surrogate values, opaque object/function/Symbol and original throw identities, raw versus stored balance identity, receiver/property/argument/order, exact native record field schemas/values, native callbacks and synchronous/awaited failures. Raw/stored inherited-then fixtures separately assert getter throw/queued resolve/reject; source dependency fulfillment consumes first raw Get(then), return resolution consumes next/raw or stored Get and adoption. No decoded re-encoding, native wire comparison or roundtrip. Source hash `f75769cd103770362574ae201ca32813669d91a3eca4b0763efbd0dc1d5e660a`. Microtask drain100 is a bounded floor, exhaustion fails99; source stage floor12, no exhaustive/recursive thenable-engine claim.

**Spread fixture limitation:** own keys are ASCII (numeric key7 plus flags), includes hidden/nonenumerable descriptor and lone-surrogate value; Symbol or lone-surrogate property-key behavior is not proven. Full CopyDataProperties/UTF16/Symbol/ordinary own-order host contract is required and row notes disclose the gap. Clock/update fields assume the source ordinary Number-or-undefined domain; deliberate first Number/second Undefined live timestamp read is covered, reflective non-number fields are unproved. Superclass dispose lookup/owner receiver, full JS coercion/Get/Set/iterator/await/adoption engine remain required live host bindings. No fallback silent map or fake rs target was introduced.

Retained development/review findings (not final gates): initial dev Cargo101 was fixture compiler E0605 (`&Rc<Object>` clone cast), repaired before native execution; development161 and then170 native runs exited0 but were uncommitted development measurements. Review identified plain raw/stored balance fixtures did not prove async-return adoption: six inherited-then throw/resolve/reject cases added; Symbol-message ToString throws and numeric noncallable signer.dispose calls added. These are represented in the final170 floor; the spread-key proof limitation remains disclosed, not declared closed.

Not run: workspace check/full monorepo builds or suites; full venue unit tests/older native targets; host clippy; wasm clippy; CI; real HTTP/network/timers/websocket/signers; full LighterExchange construction, superclass or recursive JavaScript engine integration. No shared trait/type was changed (new host is venue-local).

Compiled native controls at the payload SHA, with try/finally exact restoration after each run:

- wrapper-completion: **Cargo101**, 87.13s; `iteration0 fetchAndSetPrivateSnapshot nativeResult nativeKindMismatch`. Enumeration stopped at the named failing iteration.
- snapshot-pair: **Cargo101**, 118.25s; `iteration4 fetchAndSetPrivateSnapshotHelper Store.addOrUpdatePositionsFull arg0 nativeObjectIdentity`. Enumeration stopped at the named failing iteration.
- tick-interval: **Cargo101**, 17.44s; `iteration46 tick loop arg1 nativeNumberBits`. Enumeration stopped at the named failing iteration.
- balance-disposed: **Cargo101**, 28.99s; `iteration65 fetchBalance getReceiver nativeObjectIdentity`. Enumeration stopped at the named failing iteration.
- set-balance: **Cargo101**, 71.78s; `iteration77 fetchAndSetBalance literal:balance nativeKindMismatch`. Enumeration stopped at the named failing iteration.
- leverage-surrogate: **Cargo101**, 34.63s; `iteration104 syncLeverageFromExchange Owner.log arg0 nativeUtf16`. Enumeration stopped at the named failing iteration.
- auth-default: **Cargo101**, 38.93s; `iteration118 getPrivateWsAuthToken Signer.getAuthToken arg0 nativeNumberBits`. Enumeration stopped at the named failing iteration.
- dispose-completion: **Cargo101**, 44.51s; `iteration150 dispose nativeResult nativeKindMismatch`. Enumeration stopped at the named failing iteration.
- return-adoption: **Cargo101**, 122.58s; `iteration64 fetchBalance independentSourceEventCount`. Enumeration stopped at the named failing iteration.

All nine accepted controls reached named native assertions; none is a compiler exit. Initial set-balance attempt Cargo101/2.53s was **rejected** because -D warnings detected unused balance, not a native assertion. Control repaired to consume original balance while substituting undefined and rerun; native failure at77 then observed. No mutation survived. The leverage control changes D800 to D801, preserving UTF16 length. The return-adoption bypass control stops at64 on the independently source-recorded required Get(then) event count; it does not reach96–101 inherited-then completion cases, so no negative-control claim for those later cases. Their positive native outcomes remain separately asserted.

Restored live_reconciliation target: **exit0**,36.57s,170 native cases/stop169. Full venue src/tests/scripts exact restoration diff to payload SHA: **exit0**. Cargo commands were sequential.

Merged-head gates: `7e9f2264f8c367406d27dc4fb64990c235c40bb7`; actual parents evidence `4dddb5085503855261ffe1e809c6e8d90b67f410` and integration `198f871b9b004f6384d910a8c3af5909bd706b48`. Fresh fetch/merge-down exit0. Exact payload-to-merged diff of reconciliation src/test/oracle, assigned manifest and all three actual TS inputs exited0. All seven shared dependency directories and workspace/venue Cargo manifests, Cargo.lock and rust-toolchain were byte-identical, diff0; independent recursive Lighter lock records481 before/after equal0. Upstream builder-approval module/test/oracle were added to the same venue and lib.rs retains both exports; therefore positive gates were actually rerun, not inferred: **scoped Cargo all-targets exit0**, Finished dev1m17s; **live_reconciliation exit0**, Finished test11.44s/test0.33s,170 cases/stop169; **extractor exit0**, Would update0 manifests,0 added/0 orphaned, Done23.97s. Negative controls remain measured at payload SHA, not claimed rerun at merged head; exact owned source/test/oracle preservation is measured. Census at merged SHA remains eight pending->ported, zero divergent,60/1/40, stop after all101 assigned exchange rows. Diff against actual integration parent has exactly six owned paths: exchange manifest, reconciliation src/test/oracle, venue lib export and this evidence. Other venue/SDK paths are upstream-only, not lane implementation changes.

Final merge preservation: `1ef397b1374fef01c02bf33f90a0562725913a0e`, actual parents evidence `4f025e651be5e610e63b5d37b3e9cfa1a05da019` and integration `00cafd8f46790c3e283decca55b6618bf8ea4d97`. Fresh fetch/merge-down exit0. Scoped exact diff to measured merged gate SHA7e9f2264f8 exited0 over full venue src/tests/scripts, assigned manifest, all three actual TS inputs, complete seven shared dependency directories, workspace/venue Cargo manifests/Cargo.lock/config/toolchain, package.json/yarn.lock and extractor source. Independent lock closure481 records unchanged, exit0. Therefore Cargo0/native1700 measured at7e9f2264f8 stand by measured byte-preservation; **no new Cargo execution at1ef397 is claimed**. Global manifests changed upstream, so fresh extractor actually ran at1ef397: **exit0**, Would update0 manifests;0 added/0 orphaned, Done45.12s. Final tomllib census at1ef397 exited0:8 pending->ported/0 divergent,60/1/40,101 assigned rows considered. Diff to actual integration parent contains only six owned paths, no transferred implementation path. The following evidence-only commit changes no gate inputs; exact scoped preservation and clean tracked status are checked before push. No unreviewed lane implementation was used as an ancestor.

## Eight exchange trading preflight/margin bodies — trading-preflight slice

Payload/gate SHA: `6fde3fab1e1240aadf8cb9d080fa7bf070d7ec4e`. Actual fresh parent: `a8a1322918ee9e6f036f876a7dab93bcb9ee16ee`. Branch `feat/rust-lighter-trading-preflight`; no unreviewed lane implementation ancestor. Eight actual method bodies only in lighter.exchange.ts: #ensurePartnerAttributionTierEligible, #ensurePartnerAttributionApproved, #refreshMarginModeStateAfterInvalidOrder, #assertReduceOnlyLimitHasPosition, #resolveConditionalExecutionPrice, setLeverage, changeMarginType, cancelSymbolOrders. Other venue/SDK files are outside this implementation slice. Class and constructor remain pending. Read-only native sub-agent review closed source bodies, oracle/native test and committed metadata; reviewer ran no Cargo/Node/gates. Metadata Python check ultimately0 after initial unavailable-tomllib inspection1; no runtime implication.

Independent Python3.11 tomllib census at payload SHA: **8 pending->ported, 0 divergent**. Assigned exchange file:60 ported/1 divergent/40 pending ->68/1/32; all101 assigned rows compared to actual parent, stop at this file. No census-completeness claim for other files. All8 rs targets are actual exported pub fn bodies with matching provenance. rust_files derived programmatically from actual row targets; writer preparation before commit **exit0**, actual Updated0 manifests;0 added/0 orphaned, Done14.23s.

Measured payload commands (ONE Cargo at a time):

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, Finished dev1m38s. This compiles the venue/all-targets and its dependencies, not the workspace.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **exit0**, actual Would update0 manifests;0 added/0 orphaned, Done10.15s. Output read, not inferred from shim exit.
- Native control/restored command: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_trading_preflight --jobs 2 -- --include-ignored --nocapture`.

Independent authored/source floors: tier20, approval29, margin refresh19, reduce-only18, conditional12, leverage27, margin change16, symbol cancel7; **148**, last iteration **147**. Source producer executes actual eight arrow fields on their real constructed Subject owner with observable dependency getters, not rebinding arrow this by apply. Original TS bodies are unchanged. AST binding/module/non-type imports verified for10 names; actual OrderType/OrderSide/LogSeverity enum definitions and two original tier-message constants are used. Full types/severity/account-tier source and original members/imports included in source-only pin `24f557fdfa81d803b399bf3545635c8e3e6fe411427ca7013c19aa6449eec7ed`.

Fixed authored success rows3/23/49/68/98/125/141, one signed update at98 and125, one cancel at141, and adjusted101 at86 are asserted independently in the producer. All68 authored throwAt cases must reach their named injection exactly once (asserted, not assumed); captured output census51ok/28generated/69thrown is a census of this148-case floor, not proof of all possible branches. Source stage enumeration is bounded12; exhaustion fails. Native FIFO drain100, exhaustion fails99. No exhaustive iteration/recursive queue claim.

**Native assertions run before the source-only pin**: primitive/Numberbits/NaN/BigInt/UTF16/lone-surrogate values, opaque object/function/Symbol/original throw identities, property and dynamic UTF16-key reads, captured callee/receiver and argument counts/order, exact native literal own field count/ordered keys/values, original Error name/message and callback/async completions. Every source Array.some/filter callback is executed as the actual native closure and its decoded native boolean/error is asserted, including strict expiry>now, flat-mode inversion and symbol match. Required imported/JS operations may consume their recorded internal events only at greater captured Call depth; callback invoke/result cannot be skipped. No native serialization, re-encoding, roundtrip or wire equality.

**Host scope and gaps:** this is actual method-body composition at required live JS/import/store/cache/signer boundaries. The fixture models delegated Get/Math/Array/String/import results from independent actual TS execution; it does not implement/prove those complete engine/import dependencies. Arithmetic uses the source typed Number domain. Index fixture keys are Strings, including lone-surrogate UTF16 keys; Symbol/other ToPropertyKey domains and object template coercion remain required full host contracts. Template Symbol failure is explicitly covered; non-string error getter/coercion domains are not exhaustively explored. All seven async bodies return primitives in this floor; resolve_return is required in production, but recursive thenable/async-return object adoption is not proven by this fixture. Full exchange construction, JS array/prototype/coercion/Promise engine and actual signer/network integration remain separate. New host is venue-local; no shared type/trait changed.

Retained development/review findings (not final gates): review found reduce_final captured message-builder after opts getters; fixed before payload commit. Initial oracle applied arrow fields from a separate Subject to a mock owner and captured generated errors instead of exercising source bodies; repaired by binding actual Subject dependency getters and adding independent authored success/update/cancel/101 assertions. Mock Number/Math/Array static-call receivers were repaired alongside this ownership fix. One development native target run exited0,148/stop147, Finished test27.15s/test0.39s, before the committed throw-site reached guard; this uncommitted measurement is not used as a payload gate. Native mutation controls below operate on the committed inputs, including the reached guard.

Not run: workspace/full monorepo check/build/suites; full venue unit tests or older native targets; host clippy; wasm clippy; CI; real HTTP/network/timer/websocket/signers; full exchange/JS engine/recursive Promise integration. Only the named new native target is executed. None of the other venue files or transferred lighter-sdk files/manifests is ported by this slice.

Compiled native controls at the payload SHA, try/finally exact restoration per control:

- tier-message: **Cargo101**, 8.06s; `iteration6 ensurePartnerAttributionTierEligible Owner.emitAndLogError arg0 nativeUtf16`. Enumeration stopped at its named failing iteration.
- approval-expiry: **Cargo101**, 4.82s; `iteration27 ensurePartnerAttributionApproved callbackResult nativeBool`. Enumeration stopped at its named failing iteration.
- refresh-inversion: **Cargo101**, 5.11s; `iteration49 refreshMarginModeStateAfterInvalidOrder Cache.set arg1 nativeNumberBits`. Enumeration stopped at its named failing iteration.
- reduce-callee-order: **Cargo101**, 5.29s; `iteration72 assertReduceOnlyLimitHasPosition nativeOperationOrder`. Enumeration stopped at its named failing iteration.
- conditional-side: **Cargo101**, 4.62s; `iteration86 resolveConditionalExecutionPrice adjust arg0 nativeNumberBits`. Enumeration stopped at its named failing iteration.
- leverage-payload: **Cargo101**, 5.37s; `iteration98 setLeverage literal:leverage nativeNumberBits`. Enumeration stopped at its named failing iteration.
- margin-surrogate: **Cargo101**, 6.71s; `iteration125 changeMarginType Owner.log arg0 nativeUtf16`. Enumeration stopped at its named failing iteration.
- cancel-filter: **Cargo101**, 6.99s; `iteration141 cancelSymbolOrders callbackResult nativeBool`. Enumeration stopped at its named failing iteration.

All eight controls compiled and failed their intended named native assertions on first runtime attempts; no compiler failure is counted and no survivor was observed. Reduce-callee-order deliberately reintroduces the review defect; nativeOperationOrder fails at72 before wrong message emission. Approval uses strict expiry boundary; cancel filter native callback result fails on first wrong symbol decision. Margin-surrogate replaces D800 with D801 in native symbol interpolation, preserving length while changing bytes.

Restored target: **exit0**,5.43s,148 native cases/stop147 (Finished test2.57s/test0.40s). Full venue src/tests/scripts exact restoration diff to payload SHA: **exit0**. Cargo commands ran sequentially.

Merged-head gates: `32bc4ae8bee261e9601c7eba6e550601393d394d`; actual parents evidence `aaf1f79a6289d1cba26235bb6d8134a21d4cfc50` and integration `28bdf6ff0e3a16f050b20c449cca4128d55e7ac3`. Fresh fetch/merge-down exit0. Exact payload-to-merged diff of owned trading src/test/oracle, assigned manifest and all four actual TS inputs exited0. Upstream shared websocket source changed and signer-runtime module/test/oracle were added to this venue (lib retains both exports), so positive gates actually reran: **scoped Cargo all-targets exit0**, Finished dev1m10s; **live_trading_preflight exit0**, Finished test14.43s/test0.41s,148 cases/stop147; **extractor exit0**, Would update0 manifests;0 added/0 orphaned, Done13.51s. Negative controls remain measurements at payload SHA, not rerun at merged head; exact owned inputs survive. Independent recursive lock records481 equal481/full records equal0. Tomllib census at merged SHA remains8 pending->ported,0 divergent,68/1/32,101 assigned rows examined. Diff against actual integration parent contains exactly six owned paths: exchange manifest, new trading src/test/oracle, venue lib export and this evidence. Other venue/SDK files are upstream-only, not implementation by this lane.

Final merge preservation: `547105f8dd09b2394da31d912e619797fcd3aaf3`; actual parents evidence `d9163c1bae2c25d5d1a4cf7518e04688c720df9c` and integration `4d4b43c2bfc2ef7649b3a24cb658f9003525bec5`. Fresh fetch/merge-down exit0. Exact scoped diff to measured merged gate SHA32bc4ae8be exited0 over full venue src/tests/scripts, assigned manifest, all four actual TS inputs, complete seven shared dependency directories, workspace/venue Cargo manifests/Cargo.lock/config/toolchain, package.json/yarn.lock and extractor source. Recursive lock481 records equal0. Cargo0/native1480 exits measured at32bc4ae8be stand by measured byte preservation; **no new Cargo execution at547105 is claimed**. Global manifests changed, so extractor actually reran at547105: **exit0**, Would update0 manifests;0 added/0 orphaned, Done11.96s. Final tomllib census exit0,8 pending->ported/0 divergent,68/1/32,101 assigned exchange rows. Exactly six owned paths differ from actual integration parent, no transferred implementation file. The following evidence-only commit changes no gate inputs; exact input preservation and tracked-clean status are checked before push. No unreviewed lane implementation ancestor was introduced.


## Eight market/position/balance read bodies — market-position-reads slice

Payload/gate SHA: `1a6d69f29d0692a89cf85168977c71787d3a51d7`. Actual fresh parent: `c54b80192e833f40b00bdebe2de289336cfb6b86`. Branch `feat/rust-lighter-market-position-reads`; no unreviewed lane implementation ancestor. Eight lighter.exchange.ts methods: #resolvePositionLeverage, #resolvePositionLiquidationPrice, mapPosition, fetchRawBalance, fetchMarkets, #fetchLighterMarketRows, fetchMarketsHelper, fetchOrders. Only the exchange manifest and new venue-local module/export/oracle/test change. Class/constructor remain pending; other venue/SDK implementation is outside this slice. Read-only native sub-agent reviewed committed bytes and metadata, static checker exit0, no remaining concrete issue; reviewer ran no runtime gates.

Independent Python3.11 tomllib comparison of all101 assigned exchange-file rows against actual parent: **8 pending->ported, 0->divergent**, 68 ported/1 divergent/32 pending -> **76/1/24**. Enumeration stops at this file. Real Rust targets verified; rust_files derived from rs targets before running the reconciler. Writer preparation before payload commit: exit0, Updated0 manifests;0 added/0 orphaned, Done12.66s. No manifest regeneration or annotation replacement. Shared declarations are cited at [MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#divergence-taxonomy), not duplicated here.

Measured at payload SHA (Cargo sequential):

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, Finished dev1m38s. Scoped venue/all-targets and dependencies only.
- `node "$YARN_JS" parity:extract:check`: **exit0**, actual output Would update0 manifests;0 added/0 orphaned, Done36.18s; output inspected.
- `SAFE_CEX_TYPESCRIPT_PATH="$TYPESCRIPT_INSTALL" cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_market_reads --jobs 2 -- --include-ignored --nocapture`: baseline **exit0**, Finished test0.96s/test0.57s; restored **exit0**,4.58s (Finished test2.31s/test0.48s). `actual_market_reads_native_floor`: **156 native cases, stop155**. Environment variable points to the installed TypeScript package; no machine-specific path is required in source or these instructions.
- Exact restored src/tests/scripts diff to payload SHA: **exit0**.

Authored floors: leverage22 (0–21), liquidation25 (22–46), position22 (47–68), balance29 (69–94 and150–152), markets9 (95–103), rows15 (104–118), market-helper22 (119–137 and153–155), orders12 (138–149). Fixed total156 is independent of captured length. Sixty-six authored throwAt injections assert the injection was reached exactly once; repeated ordinary getter reads are permitted. Captured completion census82ok/71original-thrown/3generated is only a census of this floor. Producer bounds stages12; native drain bounds100 (exhaustion99); iterator fixture bounds10 (exhaustion9).

Producer executes unchanged eight actual TS bodies on a real constructed Subject owner (including arrow this), original parseMarginMode/marginModeToMarginType members and four original local helpers in a separate helper realm. AST originals/module/type-only status checked for eight imports. Source pin covers members/imports/helpers/constants and full types, LogSeverity, lighter.types, get-key, safe-math, createPositionId sources: `26e403a838023663785bc131e4d4438a1103b67bc814727e8754348a6a37f9b5`. Original endpoint/enum modules compile separately so dependency getters are observed. Native assertions check decoded Number bits/NaN, UTF16/lone-surrogate strings, BigInt, function/Symbol/object/original-throw identities, actual callback outputs, constructor schemas and field values, property/callee/receiver/argument order and deferred completions **before** the source-only pin. No native serialization, roundtrip or wire comparison.

Source-realm actual arrays/iteration/spread and real Promise/finally run in the producer. Native callbacks actually execute. Memoized native providers and finally callbacks preserve join identity, source pending-provider leftovers and cache setter order. Return-adoption cases150–155 use actual inherited Object.prototype.then getter throw/queued resolve/reject on fresh returned balance and market records. Native records are deep-checked before that getter/queued Call result. Required resolve_return handles these returns; this is not a full recursive JS engine implementation.

Compiled native controls at the payload SHA, each try/finally restored exactly; enumeration ends at each named failure:

- leverage-number: **Cargo101**, 23.17s; `iteration0 resolvePositionLeverage nativeResult nativeNumberBits`.
- liquidation-formula: **Cargo101**, 5.36s; `iteration22 resolvePositionLiquidationPrice Number.isFinite arg0 nativeNumberBits`.
- position-notional: **Cargo101**, 8.91s; `iteration47 mapPosition literal:notional nativeNumberBits`.
- balance-total: **Cargo101**, 10.48s; `iteration69 fetchRawBalance literal:total nativeNumberBits`.
- markets-predicate: **Cargo101**, 9.63s; `iteration95 fetchMarkets callbackResult nativeBool`.
- market-rows: **Cargo101**, 7.91s; `iteration104 fetchLighterMarketRows nativeResult nativeKindMismatch`.
- market-surrogate: **Cargo101**, 6.28s; `iteration119 fetchMarketsHelper parseLighterMarketSymbol arg0 nativeUtf16`.
- order-finally-clock: **Cargo101**, 6.85s; `iteration138 fetchOrders setValue nativeNumberBits`.
- async-return-adoption: **Cargo101**, 4.98s; `iteration138 fetchOrders nativeDeferredCompletion`.

The liquidation control was caught at the Number.isFinite argument before the initially planned final-result assertion. The harness first rejected that label mismatch (driver exit1), then inspected and retained the actual named native Number assertion; no production/test weakening or repeated mutation run. All nine controls compiled and produced actual native assertion failures; no compiler failure counted and no survivor observed. The surrogate substitutes D800→D801 in a native market symbol, preserving length. Async-return substitution settles the pending Promise object as an ordinary value: failure is at138, **before** inherited-then cases150–155; the latter have positive witnesses only, not a separately reached negative control.

Host/proof limits: actual bodies are composed against required complete live JS Get/Set/Call/import/coercion/Array/iterator/spread/Promise/finally/return-adoption hosts; full exchange/store/HTTP/signers and engine construction remain separate. Numeric operators use source typed Number/ordinary numeric builtin domains. number_power requires full ECMAScript Number exponentiation; fixture proves only base10/exponents2 and3, no Math.pow lookup. IteratorClose contract is required; fixture return is absent, not a callable-return/full iterator proof. Spread fixture has ASCII own keys only; lone-surrogate values are covered, Symbol/non-ASCII spread keys are not. Dynamic keys include Number/Null/Undefined/String and lone-surrogate strings; Symbol/object ToPropertyKey proof remains outside this fixture.

At default opts case73 an independently authored fresh empty object schema has0 own fields; source default allocation never escapes. At spread failures135/136 the producer cannot observe four earlier unescaping native allocations. Four independent expected schemas (precision, amount limits, leverage limits, parent limits; distinct child identities) assert their actual native fields, while source thrown completion stays authoritative. These are **authored expectations, not observed source allocation events**, and no fake returned root is inserted. Full realm prototype behavior/recursive assimilation remains a required host contract, not an accepted runtime omission hidden by the test.

Retained development/review findings (uncommitted, not payload gates): nullish helper chains initially erased terminal Null; fixed before commit and witnessed null fraction/margin/id paths. Initial source preparation errors exposed export/endpoint compilation scope and owner cache omissions; fixed without dropping cases. Source seen-read count incorrectly assumed one read on fallback; replaced with actual injection-count1 guard. Native dev1–5 exited101 at missing default-object schema, missing null-Get TypeError or missing unescaping allocation expectations; dev6 passed156/stop155. The final exact TypeError, empty schema and four partial expectations are disclosed above. All controls and restored gates ran after these repairs.

Not run: workspace/full monorepo build/check/suites; full venue or older native targets; host clippy; wasm clippy/build; CI; real HTTP/network/timers/sockets/signers; full exchange/JS engine/recursive thenable integration. Scoped all-targets compilation does not claim those tests executed. No shared type or trait was changed.


Merged-head measurements: `aa37e14d738c99a3c7b859444f4e3c1853e6a551`, actual parents evidence `24f1f8e2814f8e36c85acac4b0480d84f7dc3e0e` and integration `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`. Fresh fetch/merge-down exit0 after retaining both module export sets in the lib conflict. Whole venue source/test/script inputs changed upstream, as did shared dependencies, so payload Cargo exits were not relabeled as merged gates. Actual reruns at aa37: **scoped all-targets Cargo check0**, Finished dev52.91s; **live_market_reads0**,156/stop155, test0.62s; **extractor check0**, Would update0 manifests;0 added/0 orphaned, Done14.90s. Exact payload-to-merged diff of the three owned new inputs, assigned exchange manifest and all seven actual TS inputs exited0. Native negative controls remain measurements at payload1a6, not newly executed at aa37. Recursive Cargo.lock closure481 before/after, full records equal, exit0. Named live Lighter CI gate-step count1→1; all-targets/include-ignored root script retained, so this target participates in existing mandatory CI wiring. Census at aa37 remains8 pending->ported/0 divergent,76/1/24 across101 assigned rows. Actual integration-parent diff contains exactly six owned paths and no transferred implementation. Machine-path scan of this new module/test/oracle/evidence section and PR text exited0. No CI run is claimed.


Adversarial review repair and new measurements: isolated read-only review initially returned **DO NOT MERGE** for map/reduce callback indexing: a replaced captured Array method may call its callback with omitted arguments. This was a static finding, not a mutation survivor. Commit `74aa1d525c` binds missing formals to Undefined; actual-source map()/reduce() witnesses brought the target to158/stop157, positive exit0 at that commit (Finished test59.39s/test0.71s). Commit `fa9e5d3b371d0bcb597f19006494834fa4a3dbb3` also replaces the numeric-only accumulator addition with required full JS addition, evaluated AFTER numeric RHS/position.contracts Get. Actual reduce(0), reduce(undefined,position) and reduce('prefix/',position) witnesses produce exact TypeError, native NaN and native UTF16 prefix/20. Positive target at fa9 **exit0**,161/stop160, Finished test6.23s/test0.55s. The test-only panic catcher makes a reintroduced bounds panic fail a named completion assertion; no production panic recovery is introduced. Full JS addition/coercion is a required live operation; this fixture covers Number, Undefined and String accumulators with a fixed numeric20 string-conversion witness only. Object ToPrimitive/nonnumeric position fields remain outside the fixture's stated typed domains, not claimed complete.

Final repaired metadata/gate SHA: `d0c5b0ca65f5fb565c72aaff580bf7aaf6fc0160` (same production/test/oracle bytes as fa9, two row notes updated). Writer after metadata edit **exit0**, Updated0 manifests;0 added/0 orphaned, Done14.40s. Scoped all-targets check at d0 **exit0**, Finished dev58.35s. Extractor check at d0 **exit0**, Would update0 manifests;0 added/0 orphaned, Done18.35s. Three new compiled controls at d0:

- map-callback-bounds: **Cargo101**, 19.89s; `iteration156 fetchMarketsHelper nativeCallbackMustReturnJsCompletion`; stopped at its named iteration.
- reduce-callback-bounds: **Cargo101**, 15.72s; `iteration157 fetchRawBalance nativeCallbackMustReturnJsCompletion`; stopped at its named iteration.
- reduce-default-zero: **Cargo101**, 16.26s; `iteration159 fetchRawBalance callbackResult nativeNaN`; stopped at its named iteration.

Restored161/stop160 target at d0 **exit0**,14.62s; exact src/tests/scripts restoration diff0. All three controls caught; no survivor/compiler failure counted. Nine earlier controls remain explicitly measured at payload1a6, not retroactively relabeled as d0 executions. Source floors now: leverage22, liquidation25, position22, balance33, markets9, rows15, market-helper23, orders12; **161 total, stop160**. New cases156–158 assert exact source TypeErrors;159 asserts NaN,160 independently asserts prefix/20. Earlier150–155 indices remain unchanged.

Final source-control/test commit: `85545c1787466bc3acc4c410ecd4a2310d0f5d30`. New permanent `market_source_mutation_must_fail_by_name` runs an actual-source in-memory totalEquity-upnl arithmetic mutation, requiring child exit1 AND the independent `authored balance980` assertion. Initial full named native target at855 **exit0**,2tests passed,161/stop160, Finished test28.47s/test0.62s. The parent's no-op countercontrol at855 **Cargo101**,7.41s, named `sourceMutationMustFailNamed` (child falsely succeeds0). Restored full target at855 **exit0**,1.01s,2tests/161/stop160, Finished test0.29s/test0.67s; exact source/test/script diff0. This is a source-gate discrimination proof, separate from the twelve native body controls above.

At855 a selected-source mirror with **no node_modules in any ancestor** ran against external copied installed TypeScript and lodash packages: oracle baseline0/161/stop160; missing compiler1/MODULE_NOT_FOUND; in-memory missing lodash module1/MODULE_NOT_FOUND; exact restoration0/161/stop160. Actual source arithmetic child1 reached authored balance980. These are local isolated-resolution measurements, **not** a fresh CI dependency install or CI execution. Existing isolated CI selector already includes lodash and TypeScript; existing mandatory all-targets/include-ignored venue step includes this target. TypeScript source files themselves are unchanged. New source-control/script/test text carries no laptop path.

On restart, independent tomllib census at integration ac0f5fcc found the six requested SDK type rows already divergent with real targets (L2TxInfo, SignedTx, TxReceipt, BatchTxReq, BatchTxResponse, BatchCreateOrdersParams); zero new movement is attributed to them, and their transferred implementation/manifests are untouched. This exchange slice remains8 pending->ported,0 divergent;76/1/24 across101 owned exchange rows. New source/control execution does not imply full venue/workspace/wasm/clippy/network coverage; the explicit non-runs above still apply.


Delivery gate SHA: `276379552e8a7734877bfbff4bcc9f67a40ac2d6`. Fresh fetch/merge-down reported Already up to date against integration ac0f5fcc; no merge was fabricated. Actual **scoped all-targets cargo check0**, Finished dev2m43s; actual **extractor check0**, Would update0 manifests;0 added/0 orphaned, Done15.66s. Exact diff855→276 over full venue src/tests/scripts, assigned manifest, all seven TS inputs, seven shared dependency directories, workspace Cargo manifests/lock/config/toolchain, package/lock and extractor source exited0. Thus final native2tests0/161/stop160 and source no-op101/restored0 remain explicitly measured at855 and are preserved by exact inputs, not claimed rerun at276. Scoped rustfmt check of the two new Rust files0; no workspace formatting claim. Named workflow step count92→92 and live Lighter step1→1 compared fresh parent c54 and delivery276. Final census0:8 pending->ported,0 divergent;76/1/24 in101 exchange rows, and full venue manifest census270/19/47 in336 rows. The latter is a census only, not proof of all venue behavior. Sub-agent final static verdict MERGE after the reported callback fixes; reviewer ran no runtime gates. Its additional Python recheck could not start due a transient temp-file disk error, not a source failure; the displayed committed diff and all agent runtime gates above completed. The following evidence-only commit changes no gate inputs; exact scoped diff and clean tracked status are checked before push. No unreviewed lane implementation ancestor was introduced.

## Ordered L1 and configured-account lookups — two rows

Payload `c729589cc12bf1b79380e975bed39c5181d4aa34`, actual direct parent `0ab98b70fcabc7b99ea4974641391a288e934d7a`. Fresh branch base `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`; initial body commit `61f858cfa8`. Final fixture payload/gate SHA `1758b12216de98e4f77cf632a196c3eb5c31ff6a`, direct parent c729589cc1. This branch starts at integration and does not carry the separate unmerged market-position-read PR. All own implementation ancestors received sub-agent review; no unreviewed external lane ancestor was added.

Independent Python tomllib comparison examined all 101 assigned exchange rows, stopped at index100: **2 pending->ported, 0 pending->divergent**, triple `(ported, divergent, pending)` **(68,1,32)->(70,1,30)**. Targets are `fetch_account_from_l1_address_lookup` and `fetch_account_info` in `src/exchange_account_lookup.rs`. Venue census: `(264,19,53)` across336 rows. SDK census only: `(269,110,10)` across389 rows; no SDK or transferred socket/signer/API/builder-approval implementation was changed. The six SDK types named by the restart brief already have mapped divergent rows; this slice claims no movement for them.

Bodies retain captured callee/receiver order, four actual response spreads, optional/nullish distinctions, native BigInt/UTF16, first-account parsing, configured-index/L1/legacy candidate records, actual filter callback/uniq calls, live iterators across await, original thrown identities, last truthy error message and IteratorClose completion precedence. Synchronous lookup throws use an eager trampoline; the independent stress floor is2048 primary attempts (last2047),2049 next calls (last2048), then one fallback. Required host operations retain full JS/Promise/import/iterator semantics; complete exchange construction, HTTP wiring and a full JS engine are not supplied or proven by these fixtures. Canonical boundary/taxonomy declarations remain at [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Commands use `cargo` on PATH, `$TYPESCRIPT_INSTALL` for the real installed TypeScript package, and `$YARN_JS` for the actual Yarn JS entry point. Only one Cargo command ran at a time, jobs2. Gate commands:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`.
- `SAFE_CEX_TYPESCRIPT_PATH="$TYPESCRIPT_INSTALL" cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_account_lookup --jobs 2 -- --include-ignored --nocapture`.
- `node "$YARN_JS" parity:extract:check` (read the output text).

Writer/reconciliation before c729589cc1: **exit0**, Updated1 manifests,0 added/0 orphaned,44.13s; after trampoline note **exit0**, Updated0,38.81s. Annotations were retained; rust_files was derived from actual targets. At c729589cc1, all-targets scoped Cargo **exit0**, Finished dev3m26s; extractor **exit0**, Would update0 manifests,0 added/0 orphaned,27.56s. No workspace compilation claim.

Actual source AST members/imports and original utility/endpoint source produce content pin `b70231ed27105158dadd81f0011be113873a3d73649e46b60ea5ab2038851663`. Source oracle/native assertions examine decoded native fields, BigInt, Number bits, UTF16, object/function/symbol identity, Array/Error kind, callback receivers/arguments, provider phases and pending providers before checking the source pin. Fresh native DTOs/arrays contain the actual Rust fields; source snapshots are independent expectations. Error name/message are diagnostic projections, not an assertion of Error own-key schema. Original constructed Error handles are now checked. Unescaping allocation counts and a complete runtime are not claimed.

At c729589cc1 three deliberate compiled body mutations each failed **exit101** with named assertions: referrer D800->D801 substitution preserving length (`iteration1 fetchAccountFromL1AddressLookup nativeResult:referrer nativeUtf16`,138.24s); configured lookup mode index->l1_address (`iteration50 fetchAccountInfo Array.push arg0:by nativeUtf16`,23.76s); discarded IteratorClose error (`iteration87 fetchAccountInfo nativeResult nativeCompletionMismatch`,26.28s). Restored target **exit0**,21.85s,2 tests,102 native cases/stop101. Exact restored src/tests/scripts diff to c729589cc1 **exit0**. No survivor was dropped.

Sub-agent static review found a fixture gap: matching numeric fields did not check the native Array discriminant. Fixed at1758b12216 by asserting native Array/Error flags directly. An additional fixture gap found locally was diagnostic-only projection of constructed Error handles; fixed at1758b12216 by retaining and asserting the original handle, plus one real Error provider rejection/fallback case. Reviewer then returned MERGE, no remaining concrete finding; reviewer ran read-only git inspection0 and no Cargo/Node/mutations. Runtime measurements below are root-agent measurements, not reviewer executions.

At1758b12216, compiled fixture adapter mutations: original Error replaced with diagnostics **exit101**,12.32s,`nativeOriginalErrorIdentityRequired`; native fresh Array flag reversed **exit101**,10.74s,`nativeArrayKind`. Restored target **exit0**,12.08s,2 tests,**103 cases/stop102** (53 L1/50 info, independent authored counts). No surviving mutation or compiler error was counted as a successful control. Permanent source-control test actually changes the first spread to the other array and requires child exit1 plus named `authored first spread account11`; replacing this control with a no-op failed parent **exit101**,0.91s,`sourceLookupMutationMustFailNamed`. Restored target **exit0**,2.89s,2 tests103/stop102; exact restored src/tests/scripts diff to1758b12216 **exit0**.

Source/fixture stage floors: at most20 source provider stages, exhaustion index19; native runtime queue bound100, exhaustion99. Covered iterator return absent plus throwing return getter, not callable-return or full-engine integration. The 103 cases are an authored floor, not an exhaustive census.

Separate isolated module-resolution measurement at c729589cc1: copied actual installed TypeScript/lodash into an isolated source mirror with no ancestor node_modules; baseline **exit0**,102/stop101; remove compiler **exit1 MODULE_NOT_FOUND**; remove real lodash/uniq package **exit1 MODULE_NOT_FOUND**; restore **exit0**,102/stop101; in-memory source-spread mutation **exit1**,named first-spread guard. Original mirrored oracle restored byte-equal. This is a local resolution proof with copied installed packages, not a CI dependency installation/run, and was not relabeled as a103-case measurement after fixture changes.

Development exits (not accepted controls): initial --lib check61f858cfa8 **exit101** unused import, fixed0ab98b70fc --lib **exit0**5.45s. Uncommitted test builds failed101 on HostThrownValue accessor misuse (fixed to value().downcast_ref()), raw lone-surrogate JSON metadata (fixed with UTF16 unit metadata), and an overbroad nested then-event consumer (fixed to match same object and then key). A later uncommitted Error-diagnostic assertion build failed101 on nonliteral Rust format arguments; fixed. Positive uncommitted103 run **exit0** (Finished test1m06s/test1.92s) is not substituted for committed gates above. Production surrogate values were never replaced by encoded surrogates.

Not run: workspace/monorepo builds or suites; full venue suite; host or wasm clippy; wasm build; live exchange/network/signer integration; CI install or CI rerun. Scoped Cargo, this named target, extractor and named mutations are the only runtime gate claims.

Final committed-payload gates at1758b12216: scoped all-targets Cargo **exit0**, Finished dev2m22s; extractor **exit0**, Would update0 manifests,0 added/0 orphaned, Done13.16s. Native restored103/stop102 exits0 at the same SHA are recorded above. Fresh fetch and merge-down from integrationac0f5fccf24207b26c279e67da2a88f2c72a7aae **exit0**, Already up to date. Full tracked diff from measured payload contains only this evidence file; scoped src/tests/scripts/manifest diff **exit0**. No new implementation/shared/build/extractor/TypeScript inputs intervene; subsequent evidence-only commit does not claim new executions. Final push preservation and row census are checked before delivery.

## Live account snapshot and limits cache bodies — two rows

Fresh base `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`; body commit `79cbcf8e0d5e54e82e7c390d61473fbc62fc7f0f` directly on that base. Fixture/manifest payload `0e3645467bbfb1655aecb0aba7117ad9b8712b02`, actual direct parent79cbcf8e0d. Separate unmerged market-read and ordered-lookup PRs are not ancestors. No unreviewed external implementation ancestor was added.

Independent tomllib comparison examines101 exchange rows (last100): **2 pending->ported,0 divergent**, `(ported,divergent,pending)` **(68,1,32)->(70,1,30)**. Venue census `(264,19,53)` across336 rows is a branch census, not an aggregate claim covering the separate pending PRs. Targets `fetch_account_snapshot` and `fetch_account_limits` are real bodies in `src/exchange_account_cache.rs`. Only the assigned exchange manifest, new module/export, named native test/oracles and this evidence differ from integration. No SDK or transferred venue implementation is changed.

Bodies retain live recent/in-flight version guards, owner rereads, original account/payload identity, eager nested async functions, captured actual finally callable, snapshot conditional and limits unconditional cleanup, nested native request records, exact catch boundaries, retry/cache timestamp expressions, warning flag and logging evaluation order. Required Promise realm wrapping, adoption, arithmetic/coercion, JS/import/clock hosts remain explicit; this is not full exchange/HTTP/engine construction. Canonical declarations remain at [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

The real source member ASTs, import modules/original names/non-type bindings, constant declarations, endpoint source, actual LogSeverity enum and verified types bridge are pinned. Native assertions precede the source pin: returned account/old-limit object identity, decoded DTO own fields and UTF16, Array/Error kind, original Error handle and diagnostic projections, distinct captured callee/receiver, each actual state read/set and final nine authored owner fields. State reads return actual persistent native state, not oracle values. Native finally fixture invokes the actual registered closure asynchronously and forwards/replaces completion. Source/native provider records here are nonthenable; implicit Promise engine then reads are outside the trace. Tagged orphan source Promises receive intrinsic rejection observers so deliberately throwing finally getters/calls do not shut down the process; root completion is still compared. Neither policy proves full Promise-engine internals or unhandled-rejection process behavior.

Authored floor **72 cases/stop71**,39 snapshot/33 limits; nine source/native owner fields independently fixed. Snapshot appended ages999,1000,NaN,-1 are iterations68–71; the first68 floor had no exact TTL equality proof. Limits includes now==nextFetchAt. Source provider bound10/exhaustion9; native queue100/exhaustion99. These are floors, not exhaustive enumeration.

Manifest reconciliation/writer **exit0**, Updated1 manifests,0 added/0 orphaned,36.49s. rust_files derives from actual target rows; annotations retained. Extractor at0e3645467b **exit0**, Would update0 manifests,0 added/0 orphaned, Done16.44s. Commands: `node "$YARN_JS" parity:extract`, then `node "$YARN_JS" parity:extract:check`; output text checked, not shim status alone.

Scoped native command: `SAFE_CEX_TYPESCRIPT_PATH="$TYPESCRIPT_INSTALL" cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_account_cache --jobs 2 -- --include-ignored --nocapture`. All Cargo commands ran sequentially. Positive development72 run **exit0**,2 tests (Finished test1m26s/test0.51s); committed restored/control measurements follow below. Initial library-only development check **exit0**,21.10s, is not an all-targets/workspace claim.

Sub-agent adversarial production review found no concrete body mismatch. Fixture review flagged unmeasured TTL equality; added exact cases and a +1ms TTL control. Final static verdict at0e3645467b: MERGE pending runtime gates; reviewer ran read-only git inspection0, no Cargo/Node/mutations. Native runtime results are implementing-agent measurements.

Development failures, not accepted controls: unreachable statement **101** after an intentional test-host panic (removed); null-index case47 **101** because the test host consumed a member event before the required TypeError (fixed to nullish Get throw); permanent source-control **101** because its original global search matched two source sites (fixed to bound actual fetchAccountSnapshot and require one site there). Permanent child now changes only that body's200 status check to201 and must fail exit1 at `authored first cache success` before any pin comparison.

Not run: workspace/monorepo builds or suites; full venue suite; clippy host/wasm; wasm build; live network/signer/clock/exchange integration; isolated dependency installation/resolution, CI installation/run or rerun. No claims for transferred SDK/venue files or complete JS engine behavior.

Compiled body controls at0e3645467b: unconditional snapshot clear **exit101**,91.54s,`iteration16 fetchAccountSnapshot unexpected native set`; TTL+1ms **exit101**,19.79s,`iteration69 fetchAccountSnapshot nativePropertyKey`; header authorization->Authorization **exit101**,104.65s,`iteration35 fetchAccountLimits Client.get arg1:headers nativeOwnKeySchema`; message D800->D801 preserving UTF16 length **exit101**,19.41s,`iteration43 fetchAccountLimits Owner.log arg0 nativeUtf16`. The TTL harness initially expected the later nativeDeferredCompletion assertion and stopped on label mismatch; source restored automatically. Its actual earlier named property-read failure was verified from the retained log before resuming the remaining controls, without relabeling a new execution or dropping a survivor. All four actual compiled controls failed assertions, not compilation. Restored target **exit0**,19.55s,2 tests72/stop71; full src/tests/scripts exact restoration diff0. No survivor was observed among these four controls; no completeness claim beyond them.

Permanent source-control no-op countercheck at0e3645467b **exit101**,0.63s,`sourceCacheMutationMustFailNamed`; restored complete target **exit0**,0.80s,2 tests72/stop71; exact restored src/tests/scripts diff0. Final required scoped all-targets command `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2` at0e3645467b **exit0**, Finished dev1m19s. Extractor exit0 at that payload is recorded above. Fresh integration fetch remainsac0f5fccf2; merge-down and final full tracked input preservation are checked before push. The evidence-only delivery commit changes no measured gate inputs; no new executions at the delivery SHA are implied.

## Four candle request and live polling bodies — OHLCV slice

Payload/control SHA: `2b5d3d94a1c10d4bef91db6062d0054534e2f3fa`. Fresh branch base: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`; actual payload direct parent: `c64e95f26fd7ad83b25539118d07783f3eb7f557`. Branch `feat/rust-lighter-ohlcv-bodies`; no prior unreviewed lane implementation ancestor. Production body commit `ee34e1ad1d31dc383c91d2c982e049d11a3a4d6c`; explicit poll self-cycle repair `c64e95f26fd7ad83b25539118d07783f3eb7f557`. The four mapped functions implement fetchOHLCV, #fetchLatestOhlcvWithSharedFallback, listenOHLCV and listenOrderBook. Full exchange construction remains pending.

Python3.11 tomllib comparison against actual integration parent examined all101 exchange-file rows: **4 pending->ported, 0 divergent**, `(ported, divergent, pending)` **68/1/32 ->72/1/28**. Venue census at `b302449d3a9d329ba323c73018f64866ca6f987f`:266/19/51; SDK read-only census269/110/10. These are censuses, not proof for transferred files. No transferred implementation or manifest changed. rust_files was derived from actual row targets; extractor writer **exit0**, actual Updated1 manifests;0 added/0 orphaned, Done14.70s. Payload extractor check **exit0**, Would update0;0 added/0 orphaned, Done19.26s.

Canonical references: [string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [typed outputs](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [alias ownership](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [numeric ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [surrogate observability](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

The source oracle compiles the actual four TypeScript members, getOhlcvFallbackKey helper, constants, interval table/resolver and LogSeverity definition; it AST-verifies and pins the import declarations and shared-types re-export. It executes real dayjs and lodash/orderBy. Source-only pin `ab33922dfb763645289b9c016b61e9160c5cb9634dad08cc552cd873559cd147`. Authored floor **69 =30 fetch +18 shared fallback +16 listener +5 book**, stopped at iteration68. Source completion floor10 stops9; native queued-task floor100 stops99; exhaustion fails. No exhaustive enumeration claim.

Native assertions precede the source pin: actual callback candle fields (six independently named keys), Number bits/NaN class, UTF16 and Symbol fields/keys, captured callee/receiver/arguments/order, original throw handles, fresh object/function identity bijection, actual persistent Map entries, owner state, provider completions, timers, emitted candles and listener deduplication. Actual Rust closures are invoked; required callback events cannot be skipped. Literal cases independently assert two sorted candles/timestamp1, empty arrays, cooldown equality, synchronous-fetch cleanup-before-outer-set, stale120000 equality versus120001, distinct equal-length surrogate fingerprints, cancellation-before-provider and fingerprint retention after a synchronous user callback throws. No native re-encoding, roundtrip or wire equality is used.

Compiled mutations at payload SHA, exact try/finally restoration after each (same scoped live_ohlcv target, jobs2, no concurrent Cargo):

- Timestamp1000->1001: **Cargo101**,17.20s; iteration0 `callbackResult` timestamp `nativeNumberBits`.
- Shared request limit3->4: **Cargo101**,16.33s; iteration30 `Owner.fetchOHLCV` request limit `nativeNumberBits`.
- Fingerprint D800->D801 substitution, preserving length: **Cargo101**,22.49s; iteration55 `nativeOperationOrder` detects the suppressed second emission.
- Book symbol/callback swap: **Cargo101**,17.11s; iteration63 `Socket.listenOrderBook arg0 nativeKindMismatch`.
- Reuse raw empty array instead of fresh array: **Cargo101**,51.23s; iteration6 `nativeResult nativeObjectBijection`.

All five compiled and failed named native assertions; no survivor observed in this floor. Restored target **exit0**,27.21s,2 tests/69 cases/stop68. A separate permanent source child mutates actual TS timestamp1000->1001 in memory and requires child **exit1** plus `authored first candle timestamp1`. Substituting a no-op made the parent **Cargo101**,1.97s, `sourceOhlcvMutationMustFailNamed`; restored **Cargo0**,0.83s,2 tests/69 cases/stop68. Exact src/tests/scripts restoration to payload SHA **exit0**.

Isolated-source execution at payload SHA: no node_modules in any ancestor; compiler, dayjs and lodash copied into a separate installed-package directory. Baseline/restored **Node0**,69/stop68/same source pin; missing compiler and missing lodash import each **Node1/MODULE_NOT_FOUND**; in-memory source timestamp child **Node1/named assertion**; exact source restoration0. Existing CI selectors already include all three packages. This is a local dependency-resolution proof, not a new frozen install or CI execution.

Retained development findings: initial library check101 exposed an unused copied helper; removed, restored library check0. Native fixture failures101 exposed callback-loop event skipping and invoking an absent WS callback after failed subscription; fixed. A console fixture getter recursively tagged itself; repaired and added independent empty-result guards. These uncommitted development runs are not payload gates. The initial self-binding native run passed69/2tests0 before payload commit; final controls/restoration above cover the committed fixture. Root audit found the explicit strong lexical poll self-cycle; required self-aware fresh-callable binding now supplies actual callee identity on invocation. Sub-agent static review found no remaining body/fixture/metadata blocker and cleared synchronous callbackThrow case68; reviewer ran no Cargo/Node/mutations.

Scope and unproved behavior: production requires complete host Get/Call/construct/coercion, RegExp, iterator binding, CopyDataProperties, Promise adoption and scheduling. The fixture exercises ordinary split arrays with absent iterator.return, numeric arithmetic, ordinary enumerable spread records with UTF16/Symbol keys, and nonthenable providers; arbitrary object/Symbol numeric coercion, throwing/custom iterator close, accessors/prototypes/Proxy enumeration and recursive thenable engine semantics are not established. Async poll rejection promises are observed to prevent shutdown but their rejection outcomes are **not independently asserted**; those paths remain an accepted proof gap. Removing the explicit Rust self-cycle does not establish full GC/drop/cancellation behavior. Actual HTTP, websocket/timer realm construction and full exchange integration remain separate work.

Merge-down/gate SHA: `b302449d3a9d329ba323c73018f64866ca6f987f`; actual parents `2b5d3d94a1c10d4bef91db6062d0054534e2f3fa` and integration `1a43f3a8f25b7e5fe1c238794e79bad11e4c4788`. Fresh fetch/merge exit0. Exact scoped diff payload->merged SHA **exit0** over entire sdk-rs, all Lighter TS, shared types/LogSeverity and yarn.lock. Upstream changed12 CI/documentation/package-script paths only. No new unreviewed lane implementation ancestor introduced. Native/control exits above were measured at payload SHA, not rerun as new merged-head tests; bytes are preserved. At merged SHA, actually measured:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, Finished dev2m19s. Scoped venue compilation only.
- `node "$YARN_JS" parity:extract:check`: **exit0**, actual Would update0 manifests;0 added/0 orphaned, Done18.00s. Output inspected.
- Payload tests/controls used `SAFE_CEX_TYPESCRIPT_PATH="$TYPESCRIPT_INSTALL" cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_ohlcv --jobs 2 -- --include-ignored --nocapture`; exact native exits and iteration stops are above.

Not run: workspace or full monorepo builds/suites, full venue unit suite or older native targets, host clippy, wasm clippy, CI or mirror runner tests, actual network/socket/timer realm, full exchange/JS/recursive-Promise/GC integration. Only live_ohlcv was executed. Final evidence-only commit changes no gate inputs; delivery verifies exact preservation and tracked-clean status before push. Local read-only sub-agent reviews closed production self-binding, source/native fixture, four mappings and evidence; reviewer ran no runtime gates.

## Four order error/update/cancel bodies — order-actions slice

Body/control payload: `668c9f4ab84637577f69fe958c6283cbd0ca6cd1`; actual parent `96d2182e4aa10f8a891719f028f1a4dcd4375605`; production `d5beb0cdf6719f1c32ffca13ee26abd2a58f6e11`. Fresh branch base/integration `1a43f3a8f25b7e5fe1c238794e79bad11e4c4788`, branch `feat/rust-lighter-order-actions`; no previous unreviewed lane implementation ancestor. Parameter-binding fix96d precedes the payload. Gate SHA **`48f3c3b6240b2040f02066ad6c8efa85d49d6ae6`** adds exactly five test lines asserting13 actual native callback Promise bindings at60; no assertion removed. Exact src/scripts/manifest diff668c->48f **exit0**. Controls below were measured at668c, not rerun at48f.

Python3.11 tomllib examined all101 assigned rows against actual integration base: **4 pending->ported, 0 divergent**, `(ported, divergent, pending)` **68/1/32 ->72/1/28**. Four actual rs bodies: #normalizePlaceOrderErrorMessage, #placeOrdersSequentially, updateOrder, cancelOrders in exchange_order_actions.rs. Class/constructor and other pending methods stay pending. No transferred venue or SDK file/manifest changed. rust_files derived from actual targets; writer **exit0**, actual Updated1;0 added/0 orphaned, Done24.84s.

Canonical references: [strings](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [typed outputs](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [alias ownership](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [numeric ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [surrogate observability](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Oracle compiles actual four members, OrderType/OrderSide definitions, two cancellation helpers and sleep source; tier constants compile separately. Nine imports are AST-verified/pinned, not executed as module declarations. Real lodash/chunk and p-iteration execute from installed dependencies. Source-only pin `970ed6cefc266594d111ec9e02b02fbe822d0de0e74785ab44ace795646b8e9f`. Authored **70 =27 normalize +9 sequence +22 update +12 cancel**, stop69. Source provider floor40 stops39; native queue/argument-spread/external-event floors100 stop99; exhaustion fails or leaves events and fails. No exhaustive enumeration claim.

Native assertions precede the source pin: primitive/Numberbits/NaN/UTF16/BigInt/Symbol values, original throws, getter/member/callee/receiver/argument order, actual map/find/push results, fresh fields/alias bijection, copied UTF16/Symbol keys, cancellation Map entries, iterator next/close and root completions. Actual Rust callbacks execute. Each source async callback's actual native Promise is found, required completed, and its decoded fulfillment/rejection compared. The native13 count is independently fixed from two lots plus eleven requests. Source ready events settle delegated import promises, without replacing native callback outcome assertions. No native re-encoding, roundtrip or wire equality.

Independent witnesses include two sequential placements; close-on-rejection once; original throw surviving a throwing iterator.return; invalid protective39/41 versus valid38/40/42/43/45/46; trigger/slippage/override fields; null/undefined parameter TypeErrors49/50; two cancels58; eleven cancels60; one1000ms second-lot delay; two-lot/eleven-request callback count13; blacklist-before-store-before-network order. Source helpers' zero-ID acceptance is retained.

Compiled controls at668c, jobs2, sequential Cargo, exact try/finally restoration:

- Deviation100->101: **Cargo101**,28.30s; iteration5 `Number.isFinite arg0 nativeNumberBits`.
- Omit outer IteratorClose: **Cargo101**,32.78s; iteration30 `placeOrdersSequentially independentSourceEventCount`,3 required close events remain.
- Symmetric new-order price/amount swap: **Cargo101**,29.01s; iteration36 `Owner.placeOrder` native price `nativeNumberBits`.
- Wrong cancellation market55: **Cargo101**,25.48s; iteration58 `Signer.cancelOrder` native marketId `nativeNumberBits`.
- Template D800->D801 substitution, preserving length: **Cargo101**,50.97s; iteration5 `nativeRootResult nativeUtf16`.

All five compiled and failed named native assertions; no survivor observed. Restored668c target **exit0**,25.13s,2 tests/70/stop69; exact src/tests/scripts restoration diff0. Source child mutates actual TS deviation100->101 in memory, requires **Node1** plus `authored accidental deviation10`. At48f, no-op substitution made parent **Cargo101**,16.55s, `sourceOrderActionsMutationMustFailNamed`; restored **Cargo0**,1.55s,2 tests/70/stop69 including new native13 assertion. Exact src/tests/scripts restoration to48f **exit0**.

Isolated-source proof at668c: no node_modules in any ancestor; separate installed TypeScript/lodash/p-iteration copies. Baseline/restored **Node0**,70/stop69/same pin; missing compiler and missing lodash import each **Node1/MODULE_NOT_FOUND**; TS child **Node1/named assertion**; exact mirror restoration0. CI selectors already include these packages. Local resolution proof only; no frozen install or CI execution.

Development/review findings: library-only development check0 before binding fix, not final all-targets proof. Source guards found guessed protective enum spellings bypassed intended invalid branches; fixed to actual values and required intended Error outcomes before accepting native results. Initial helper declarations blocked instrumented import redefinition; helpers compile separately now. Native development101 at58 and diagnostic101 at58 found Proxy Get wrapping a member getter. Binding now validates both observations, callee identity and original throw without removing oracle events. Restored uncommitted target0,2 tests/70 preceded payload. Review found missing explicit native13 count; added separately and measured at48f. Reviewer performed static inspection only, no runtime gates.

Limits/accepted proof gaps: numeric operators cover typed Number inputs; spread uses ordinary enumerable descriptors/own fields, one Symbol and an ordinary Reflect.get Proxy wrapper. Arbitrary descriptors/prototypes/post-get Proxy effects, exotic coercion/iterator close and full language/Promise/async-callable metadata require real host bindings and are not established. Covered callable/throwing iterator.return cases are a floor. Per-step fulfilled-callback pending timing, complete p-iteration/microtask semantics, recursive thenable adoption and GC/drop/cancellation behavior are unproved. Actual sleep's internal timer is explicitly host-only; its import Promise arrives at a source ready event. Full exchange/signer/HTTP/store construction and real network/timers remain separate work.

Final gates at48f (fresh fetch/merge-down exit0, Already up to date with1a43):

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**, Finished dev4m48s. Scoped venue compilation, not workspace.
- `node "$YARN_JS" parity:extract:check`: **exit0**, actual Would update0 manifests;0 added/0 orphaned, Done38.23s; output inspected.
- Tests/controls used `SAFE_CEX_TYPESCRIPT_PATH="$TYPESCRIPT_INSTALL" cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_order_actions --jobs 2 -- --include-ignored --nocapture`. Their separate SHA-bound exits are above; no new control execution at48f is implied except the named source counter/restoration.

Not run: workspace/full monorepo builds or suites, full venue unit suite or older native targets, host clippy, wasm clippy, CI/mirror runner tests, actual signer/network/socket/timer construction, full exchange/JS/recursive-Promise/GC integration. Only live_order_actions was executed. Local read-only sub-agent reviews closed body order, parameter binding, Proxy getter binding, native13 count, manifest targets and evidence; reviewer ran no runtime gates. The delivery evidence-only commit changes no gate inputs; exact scoped preservation and tracked-clean status are checked before push.

## Account tier, signer initialization, validation and startup — four rows

Fresh parent: 1a43f3a8f25b7e5fe1c238794e79bad11e4c4788. Production body:
6dd376e3e743724a6fc4834d38b42e61a62b8fe2; fixture:
77b9d920f49e7708f3ac0c80845a4e3f213e3b2c; manifest payload:
df6a6b3a3cd423c250268028169741fcb945cb3f. Four pending -> ported,
zero -> divergent. Independent Python tomllib exchange-file triple
(ported, divergent, pending): (68, 1, 32) -> (72, 1, 28), 101 rows.
Venue directory census at this branch: (266, 19, 51), 336 rows; other files
remain outside this assignment. Census does not prove runtime integration.

The live oracle compiles the four maintained method bodies and their actual
retry static field; eight imported bindings are AST-verified, the endpoint
and severity declarations are compiled, and the severity re-export is checked.
Source/import pin: 0fb904d487ed2e6529c2620d232ed0085c585766e3ebd19380d7e90bc64e933f.
Native value, identity, owner state, DTO, callback result and operation checks
precede this pin. Authored floor: 86 cases, stopped at iteration85:
18 tier, 24 signer, 17 validation, 27 startup. Provider loop stops at39;
native callback/delegated-ready loop stops at99 with residual-event checks.
Cases include synchronous IIFE-cleanup-before-Promise-assignment residue,
WASM-race retry and setter throws, startup-warning throws, disposed branches,
concurrent market/signing work, repeated external-path await identity, and
both normal and throwing snapshot IteratorClose. No wire roundtrip gate.

At payload df6a6b3a3cd423c250268028169741fcb945cb3f, five compiled Rust
mutations all exited101 in actual_runtime_initialization_native_floor:

| Mutation | First named native assertion | Seconds |
| --- | --- | --- |
| authorization -> Authorization | iteration0 ApiClient.post arg2 nativeOwnKeySchema | 180.02 |
| retry deadline +1 after the actual clock read | iteration25 nativeSetValue nativeNumberBits | 29.78 |
| validation 255 -> 254 | iteration44 nativeRootResult error nativeUtf16 | 26.71 |
| startup ticker bid0 -> bid1 | iteration56 nativeCallbackResult bid nativeNumberBits | 27.95 |
| template D800 -> D801, preserving UTF16 length | iteration25 Error arg0 nativeUtf16 | 31.15 |

All five were restored; the scoped source/test/script/manifest diff from the
payload exited0. Removing the source mutation exited101 at
sourceRuntimeInitializationMutationMustFailNamed (129.79s); restoration ran
both live_runtime_initialization tests at exit0 (2.47s), 86 cases. The permanent
source child changes the actual retry expression +1 in memory and must exit1
at authored retry10000plus30000. No control survived.

Before acceptance, development runs exited101 at iteration0 nativeOperationOrder
(Store.loaded Proxy plus accessor reads) and iteration19 unexpected native get
(an overbroad harness repair consumed repeated owner reads). The reviewed repair
validates both observations only for Store.loaded; owner reads remain independent.
Ordinary source-created loaded records copy actual native fields and Symbol
identities without inventing Proxy observations. The corrected baseline passed
both tests, stopped at85, exit0. These were harness fixes, not hidden controls.

Isolated source check at payload: copied TypeScript compiler, the two oracle
scripts and four declared source inputs outside ancestor module directories.
Baseline/restored Node0 (86 cases, 0.75s each); missing compiler Node1 (0.10s);
retry source child Node1 at its named assertion (0.86s). No other external
package is required by this oracle; no frozen install or CI was exercised.

Merge-down: 32db01dc3ab988ad9937993b1db1e5ae019c07ea has parents
payload and integration 0445a753756f197e4363222eab9e787868e4f700. Scoped diff
for venue src/tests/scripts, this manifest, and the pinned TypeScript inputs
exited0; shared crypto/exchange dependencies changed, so merged compile and
native gates are recorded separately below. No other unmerged lane branch is
an ancestor of this slice.

Required owner/import/realm, signer and network construction, full thenable
adoption, full engine callback timing, callable metadata and GC/drop behavior
remain separate bindings or unproved domains. The witness uses authored
nonthenable provider records; delegated imports and JS primitives are bounded
hosts. See [shared UTF16 boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[closed typed outputs](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[owner aliasing](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[numeric ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[divergence taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[forced surrogate conflict](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
This slice did not run workspace builds/suites, full venue tests, host/wasm
Clippy, wasm builds, CI, network, actual signer construction or GC/drop proofs.

At merge SHA 32db01dc3ab988ad9937993b1db1e5ae019c07ea:
- Scoped cargo check --manifest-path sdk-rs/Cargo.toml -p
  safe-cex-venue-lighter --all-targets --jobs 2: exit0, dev profile2m42s.
- node $YARN_JS parity:extract:check: exit0; output explicitly says
  Would update0 manifests;0 added;0 orphaned (48.05s).
- With SAFE_CEX_TYPESCRIPT_PATH=$TYPESCRIPT_INSTALL, scoped cargo test
  --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test
  live_runtime_initialization --jobs 2 -- --include-ignored --nocapture:
  exit0, both tests passed,86 cases stopped85 (test execution1.63s).

## Position snapshot body — one row

Fresh integration parent: 0445a753756f197e4363222eab9e787868e4f700.
Body 7e53f6c9df0d8e7c654abc0218d2cae746594f4d; native/source fixture
0b342758f21e3ad179e52f32dccabcf7c1129c24; manifest/helper-binding payload
4cb6f3d477676fc19743f741be0e2fb338214a49. One pending -> ported,
zero divergent. Python tomllib exchange-file triple (ported, divergent,
pending): (68, 1, 32) -> (69, 1, 31), 101 rows. This is a symbol port,
not complete venue/class construction. No other unmerged lane branch ancestor.

The source oracle compiles the maintained fetchPositions body, actual
PositionSide/PositionMode declarations and createPositionId declaration.
Four main imports and the helper's two enum imports are AST-verified;
real lodash uniqBy is called. Pin of body/imports/enum declarations/helper
source/lodash version: 4e0579950bc66d88ada83ba2f72f7c6e58ecb023ce0cfe842a651b90d34586e0.
This does not pin all lodash implementation bytes. Native DTO fields, array
and reducer results, bound dependency callee/owner/full arguments, cache
updates, UTF16 units, identity, errors and operation order are asserted before
the pin. Floor50, stopped at iteration49; provider loop bound4 stops3,
queue/spread iterator bounds100 stop99. Authored native row17 expects two
placeholders, eleven fields and binary64 leverage10. Rows24/25 each expect
one native leverage RHS invocation before the nullish assignment throws.

Sub-agent review initially proposed an early nullish-base/key-coercion check.
That finding was disproved by the actual maintained source: the first source
fixture exited1 at authored nullish assignment base before leverage RHS,
actual call count1 versus expected0. Independent Node probes also observed
key expression, RHS, then assignment failure/key conversion. The reviewer
retracted both that finding and approval of the early-check draft after
checking the [computed-reference specification](https://tc39.es/ecma262/multipage/ecmascript-language-expressions.html#sec-evaluate-property-access-with-expression-key)
and [PutValue](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html#sec-putvalue).
The committed body captures the raw key before the RHS and assigns afterward;
this was a disproved review proposal, not a repaired TypeScript defect.

At payload 4cb6f3d477676fc19743f741be0e2fb338214a49, three compiled controls
in actual_positions_native_floor each exited101, then restored exactly:

| Mutation | Named assertion | Seconds |
| --- | --- | --- |
| fallback leverage10 ->11 | iteration17 Array.push leverage nativeNumberBits | 33.21 |
| placeholder symbol D800 -> D801, equal UTF16 length | iteration0 Array.push symbol nativeUtf16 | 25.15 |
| premature nullish PutValue before RHS | iteration24 nativeOperationOrder | 21.74 |

Source/test/script/manifest restoration diff exits0. No control survived.
The permanent source child changes the actual fallback10 to11 and exits1
at authored placeholder fallback leverage10. Removing that mutation exited101
at sourcePositionsMutationMustFailNamed (26.45s). Restored scoped cargo test
--manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_positions
--jobs 2 -- --include-ignored --nocapture exited0: both tests,50 cases stopped49
(0.95s); SAFE_CEX_TYPESCRIPT_PATH=$TYPESCRIPT_INSTALL was set.

Isolated source check at the payload copied the two scripts, three source
inputs, TypeScript and lodash outside ancestor modules: baseline Node0
(50 cases,0.36s), missing compiler Node1 (0.06s), missing lodash Node1 (0.26s),
source child Node1 by name (0.29s), restored Node0 (50 cases,0.34s).
These were copied dependencies, not a frozen install or CI gate.

At payload after fresh fetch/merge-down returned Already up to date:
- Scoped cargo check --manifest-path sdk-rs/Cargo.toml -p
  safe-cex-venue-lighter --all-targets --jobs 2: exit0, dev1m35s.
- node $YARN_JS parity:extract:check: exit0; output Would update0 manifests,
  zero added/orphaned (42.38s). Writer previously reconciled one manifest,
  zero added/orphaned, exit0 (46.64s).

The mapPosition dependency is authored at the host boundary. Native for-in
witnesses cover own string-key accumulator insertion order, not inherited or
integer-key ordering, deletion, Symbols or key-coercion effects. Providers
are nonthenable. Full primitive/realm/import/owner construction, thenable
adoption, engine timing, callable metadata, GC/drop and network remain separate
bindings or unproved domains. See [UTF16 boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[closed outputs](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[owner aliasing](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[numeric ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[divergence taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[forced surrogate conflict](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
This slice did not run workspace builds/suites, full venue tests, host/wasm
Clippy, wasm builds, CI, network, actual signer construction or GC/drop proofs.

At merge SHA 32db01dc3ab988ad9937993b1db1e5ae019c07ea:
- Scoped cargo check --manifest-path sdk-rs/Cargo.toml -p
  safe-cex-venue-lighter --all-targets --jobs 2: exit0, dev profile2m42s.
- node $YARN_JS parity:extract:check: exit0; output explicitly says
  Would update0 manifests;0 added;0 orphaned (48.05s).
- With SAFE_CEX_TYPESCRIPT_PATH=$TYPESCRIPT_INSTALL, scoped cargo test
  --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test
  live_runtime_initialization --jobs 2 -- --include-ignored --nocapture:
  exit0, both tests passed,86 cases stopped85 (test execution1.63s).
Did not run workspace/full venue suites or builds, host/wasm Clippy, wasm,
CI, network, full dependency-byte closure or GC/drop proofs.

Final merge-down 65803a05c5aed4e3fc3ae1e4e491da102717a056 has parents
44a1c813bba87b7a00c17c24e08973a74aa2186e and integration
d19d2874e0265c85602b9240d618612c65984c02. The incoming change affects only
bybit source/tests/scripts/evidence/manifest. A complete sdk-rs and pinned
TypeScript input diff from payload exits0 after excluding those bybit paths
and this evidence file; all Lighter inputs and shared dependencies are unchanged.
At that merge SHA, parity:extract:check reran at exit0 with Would update0,
zero added/orphaned (13.36s). Cargo/native gates remain the measured payload
exits above; they were not rerun after this unrelated merge.

## Active-order snapshot body: one pending row

Source payload: 169f9f6464c78e613857dd2da5d4a159c6fd8ccf. Actual branch parent: integration f853d56fd227c2106dba609141b1fdd44bcab125. Production32f3f8eebe, oracle606458d49a and native fixture40933a599b are this slice's own ancestors; no other lane implementation branch was added. Assigned manifest census by independent tomllib:101 rows, (ported, divergent, pending)=(69,1,31), exactly1 pending->ported and0 divergent, LighterExchange#fetchOrdersOnce. This is the single complex active-order body and its source-local helpers, not the full exchange. Counting eight explicitly enumerated lane payloads (this payload plus the seven previously pushed/reviewed slices) gives26 disjoint pending->ported; enumeration stopped after those eight. Six assigned rows remain untouched by that branch union. Those union numbers are branch observations, not integration delivery claims.

The Rust body preserves undefined-only account-index fallback, warning flags and synchronous catch boundaries, eager six-candidate parsing before candidate iteration, order-shape membership tests, primary255 lookup and market-ID fallback, actual nested async callback Promise adoption, invalid-auth classification/deduplication, strict cancellation expiry and live owner Map access, alias precedence and mapOrder call order. Host methods are required realm/import/iterator primitives with no default replacement result. TypeScript remains the maintained reference.

Oracle executes the actual selected TS body, verifies its three runtime import bindings, compiles actual ENDPOINTS and LogSeverity declarations and checks the shared reexport, and loads real lodash/uniq. Content pin28b9188a8ba66fe6627a20762156fc3ade7b6e9408d32e7d7bbf59cf0db43c54 covers body/import declarations, three complete auxiliary source files and lodash VERSION; it does not pin all lodash/compiler implementation bytes. Native assertions decode actual AccountValue variants, field schemas, UTF16 units, Number bits, alias identity, flags, Map contents, iterator close and request arguments before checking that pin. Actual Rust async callback outcomes are observed separately through a native-only Promise registry and checked before aggregate allSettled input is provided; two callback outcomes are independently required at12/13/14. No decode/reencode comparison is used.

Independently authored floor60, stop59; provider-drain floor30, stop29; native queued/cursor bound100, stop99 (individual mutations stop on their named first failure). Explicit cases cover candidate getter eagerness, every direct ID alias, changing nested order getter, source array and outer Map return/throw closure, original throw precedence, missing index/auth, invalid-auth warnings, fallback rejection and empty market lists. Source independently requires strict30000/30001 cancellation behavior and five closes at32/one abrupt outer close at34/35.

Development findings retained: real lodash returns a different realm Array; initially its callbacks were not observed and the authored two-callback count failed (Node1). Import-returned arrays now receive observer methods in place, preserving identity and real method delegation. An early source mutation ran far enough to hit an unrelated injection-reached assertion; moved the independent255 assertion immediately after first authored case, so it now fails at its intended target. Initial library-only development check101 hit the literal escaped-newline export typo (compiler failure, not a discriminating gate), repaired before production commit and before the native fixture existed. Another diagnostic-only Rc cast failed compilation101 and was corrected. A native baseline then failed iteration0 identity bijection because oracle setup/final snapshots called observed Map.entries and realm Array.map; replaced only harness snapshots with intrinsic Map.entries and direct indexing. No native identity assertion was weakened. Corrected development target while HEAD606458d49a plus uncommitted oracle/native-fixture repair (later committed40933a599b): both tests0,60/stop59, Finished test0.73s/execution0.97s. These are development measurements, not payload gates below.

Static adversarial sub-agent review found no remaining blocker after snapshot repair; checked native callback receipts precede aggregate input, cancellation iterator close/throw precedence and after_orders catch scope. Reviewer ran no runtime commands. Bounded gaps: fixture has own-property membership and authored dense cancellation pairs/typed arithmetic; full inherited/proxy/coercion/regex/instanceof engines, malformed pair/iterator result exotica, callable/prototype metadata, recursive thenables and complete Promise job timing, GC/drop and network/exchange construction remain unproved. Imported uniq and authored mapOrder dependency results do not prove their full implementations. See [shared string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [typed output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [snapshot alias boundary](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) [numeric ingress boundary](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [forced surrogate conflict](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict) and [divergence taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).

At payload169f9f6464c78e613857dd2da5d4a159c6fd8ccf, isolated copy of two scripts, four actual TS source files, TypeScript and lodash: baseline Node0,60/stop59,0.32s; missing compiler Node1,0.05s; missing lodash Node1,0.25s; source child actual255->254 Node1 named authored all-markets sentinel255,0.31s; restored Node0,60/stop59,0.32s. No inherited NODE_PATH/NODE_OPTIONS or other source checkout was needed. Dependency-failure controls are not semantic parity proof. Writer at40933a599b plus manifest diff: exit0, Updated0 manifests;0 added/0 orphaned,16.04s (annotation/rust_files were reconciled from target paths, never regenerated).

Compiled controls at payload169f9f6464c78e613857dd2da5d4a159c6fd8ccf; each exact source restored in finally before the next sequential Cargo:

- sentinel255->254: Cargo101,127.31s; iteration0 ApiClient.get params.market_id nativeNumberBits (Finished test1m47s/execution1.39s).
- fallback outer callback swallows await rejection: Cargo101,25.83s; iteration13 nativeAsyncCallbackOutcome nativeCompletionMismatch, before aggregate input (Finished test9.90s/execution1.50s).
- strict cancellation cutoff30000->30001: Cargo101,33.05s; iteration0 nativeOperationOrder at missing delete call (Finished test13.38s/execution2.15s).
- warning template D800->D801: Cargo101,31.94s; iteration1 Owner.log arg0 nativeUtf16 (Finished test14.76s/execution1.70s). UTF16 length retained; decoded units differ.

All four controls compiled and failed the named native assertion; no survivor. Exact entire venue src/tests/scripts diff to payload after restoration exit0. Permanent source mutation no-op control: Cargo101,32.58s, sourceOrderSnapshotMutationMustFailNamed (actual child0 instead of required1), Finished test9.78s/execution0.61s. Restored positive target: Cargo0,3.11s;2 passed,60 native cases/stop59, Finished test1.37s/execution1.62s. Full venue src/tests/scripts restoration diff at that payload exit0 again. Development compilation failures above are not these controls.

Positive payload gates: scoped Cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs2 exit0,129.29s (Finished dev2m08s). Extractor via node $YARN_JS parity:extract:check exit0,20.32s; output Would update0 manifests;0 added/0 orphaned, Done20.13s. These are measured at169f9f6464c78e613857dd2da5d4a159c6fd8ccf after exact restoration. One Cargo at a time throughout. Not run: workspace/full monorepo checks/build/suites, other native targets or complete venue unit tests, host clippy, wasm clippy, CI, actual HTTP/websocket/timer/signer integration or full JS/exchange runtime.

Delivery merge-down: fresh fetch/merge exit0, Already up to date, integration f853d56fd227c2106dba609141b1fdd44bcab125; HEAD remains measured payload169f9f6464c78e613857dd2da5d4a159c6fd8ccf. Exact entire venue src/tests/scripts and owned manifest diff to payload exit0; whitespace check exit0. The following evidence-only commit changes no gate input; no Cargo/extractor rerun at that evidence commit is claimed. Full scoped-input preservation and clean tracked status checked before push. No unreviewed implementation ancestor introduced.

## Batch placement body: one pending row

Payload c78c382c13d54b6ab63b6a6903d91693716526b7; fresh branch parent integration f853d56fd227c2106dba609141b1fdd44bcab125. Own production26d7623a1f, oracle26ceae5901 and native fixture00a3c557fa precede this annotation. No other implementation branch ancestor introduced. Independent tomllib census101 assigned exchange rows: (ported, divergent, pending)=(69,1,31), exactly1 pending->ported/0 divergent, LighterExchange.placeOrders. This is the batched body only; placeOrder, mapOrder, historical fills and runtime class remain separate. Neither transferred SDK nor socket/signer/API/builder-approval files/manifests is changed.

Rust body implements sequential fallback, ordered precision/minimum/builder imports, tier/approval awaits, source BigInt candidate arithmetic and zero fallback, per-order preparation, cached first TIF vs live second TIF, own request/hint field order, signer/hint timing outside the try, submission and post-success throttle/map catch scope, cleanup, optional margin refresh, normalize/throttle/emit/throw, and required return adoption. Live JS/import/operator/iterator primitives have no default result; imported minimum helper's ordinary always-record return domain follows its actual implementation. Numeric/coercion/iterator engine implementations themselves are required bindings, not claimed complete here.

Oracle compiles actual selected TS body; verifies eight runtime import bindings; compiles actual complete lighter.utils, actual rounding cache initialization/declarations, builder-fee source plus actual Robinhood predicate, and actual OrderSide/OrderTimeInForce enum declarations. Pin4a0b84e1e89aa1d0b3913a3c6465c64c1de1402c07d11b043205b4c6cc167c1c covers body/import declarations and five complete auxiliary source inputs, not TypeScript implementation bytes. Import-returned Minimums/BuilderConfig records retain original handle/values with in-place observer getters; descriptor/prototype fidelity is not proven. Owner dependency stubs (sequential placement, attribution, signer, minimum-reason/error helpers) are authored inputs, not proofs of those implementations.

Native decoded AccountValue variants, BigInts, UTF16, Number bits, exact own schemas, graph identities and actual callback results are asserted before source pin. Array.map output is built from actual Rust callback results. The bounded iterator host checks actual native prepared-array row identity before executing each Rust loop body. Host call entered markers distinguish pre-entry injected throws from work that mutates then throws. Native hint Map updates/delete and submitted requests are tracked independently from source final results. Authored floors58, stop57; provider bound30, stop29; native queue/iterator bound100, stop99. Enumeration is a floor, not a complete input census.

Independent witnesses0/14/15 require one batch of two requests,12 own request fields, native scaled prices1235/1335 and amounts1250/1750, exact candidate IDs, cached-vs-live TIF and post-only flag. Source wrap case14 produces IDs1,1 and one overwritten hint; deliberately preserved and explicitly asserted as source behavior, not silently corrected. Additional floors4/9/12 require no hints/submission on oversize fallback, two deletes on batch failure and second throttle after success-throttle failure. Abrupt remember/cleanup iterator closure is independently required at56/57. Proposed fresh sourceOrder-data-field getter injection is statically unobservable for this authored fresh literal; it was omitted before execution and is an unexercised proposed control, not a mutation survivor or a passing gate.

Development findings: initial library-only check101 reported BigInt-from-str and alternative-pattern type errors; corrected to actual parsed num_bigint values and separate String/BigInt comparisons, then library-only check0 (Finished dev4.14s). No all-targets claim for that command. First source attempt Node1 caught the independently authored expected-ID arithmetic typo (expected123456321 but source is123456322); corrected the independent formula assertion. Development native target while HEAD26d7623a1f plus uncommitted oracle/native fixture (later26ceae/00a3c5) passed2 tests,58/stop57, Finished test2m07s/execution1.04s; not used as the payload gate. Source child actual base multiplier1000n->1001n Node1 reaches named authored base1000 plus random321 plus first1. No source TS file was edited.

Adversarial static sub-agent reviews found no concrete production/body or native-fixture blocker under actual helper domains. Checked evaluation/callee order, field order, BigInt wrapping, hint timing, catch boundaries, real callback/iterator identity and entered-work semantics; reviewer ran no runtime commands. Full JS coercion and exotic objects, descriptor/prototype metadata, inherited thenables/recursive adoption and complete engine job timing, malformed iterator results, GC/drop, complete class construction and actual signer/network integration remain unproved. See [shared string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [typed output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [snapshot alias boundary](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [numeric ingress boundary](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [divergence taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [forced surrogate conflict](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

At payloadc78c382c13d54b6ab63b6a6903d91693716526b7, isolated two scripts + six actual source files + TypeScript only, no inherited NODE_PATH/NODE_OPTIONS: baseline Node0,58/stop57,0.42s; missing compiler Node1,0.06s; source child Node1/named,0.31s; restored Node0,58/stop57,0.45s. No other external package required by this selected oracle. Writer at00a3c557fa plus annotation diff exit0, Updated0 manifests;0 added/0 orphaned,20.00s. Annotations/rust_files reconciled, never regenerated.

Discriminating compiled controls at payloadc78c382c13d54b6ab63b6a6903d91693716526b7, exact restoration after each sequential Cargo:

- zero candidate index+1->index+2: Cargo101,100.87s; iteration14 BigInt arg0 nativeNumberBits (Finished test1m30s/execution0.75s).
- cached TIF replacing second Get: Cargo101,23.50s; iteration0 nativeOperationOrder, expected Get before constant (Finished test6.08s/execution1.08s).
- wrong cleanup key: Cargo101,23.62s; iteration9 Hints.delete arg0 nativeUtf16 (Finished test8.96s/execution0.77s).
- length-preserving D800->D801 normalized-message substitution: Cargo101,21.82s; iteration9 Owner.emitAndLogError arg0 nativeUtf16 (Finished test8.55s/execution0.88s).
- value-only cached TIF, retaining the second actual Get: Cargo101,24.79s; iteration15 Array.push request.postOnly nativeBool (Finished test5.85s/execution0.75s). This independently reaches the changed-getter value witness beyond getter order.
- fixture host IteratorClose bypass, setting cursor done before the still-called close routine: Cargo101,27.28s; iteration42 independentSourceEventCount, missing Get(return) (Finished test5.98s/execution0.90s). This is a required host/fixture primitive control, not a production-body mutation.

All six compiled controls failed named native assertions, stopped on the stated iteration, no survivor. Earlier host-close control draft removed the only close call and failed compilation101,7.24s under dead-code warnings; no test ran, so that attempt is NOT counted as a discriminating gate. It was replaced by the compilable bypass above. Full venue src/tests/scripts diff after all restorations to payload exit0. No production/test mutation remains.

Source-control no-op at payload: Cargo101,13.48s; sourceBatchPlacementMutationMustFailNamed (child0 instead of required1), Finished test3.12s/execution0.31s. Restored target Cargo0,1.30s;2 tests,58 native cases/stop57, Finished test0.34s/execution0.91s. Each native assertion precedes the content pin. One Cargo at a time. No full suites, older targets, workspace builds, host/wasm clippy, CI, actual HTTP/websocket/timer/signing or complete exchange/JS runtime integration run.

Positive payload gates atc78c382c13d54b6ab63b6a6903d91693716526b7 after exact restoration: scoped Cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs2 exit0,79.03s, Finished dev1m18s; extractor via node $YARN_JS parity:extract:check exit0,13.13s, Would update0 manifests;0 added/0 orphaned, Done13.01s. Full venue src/tests/scripts restoration diff exit0. Fresh fetch/merge-down exit0, Already up to date at integrationf853d56fd227c2106dba609141b1fdd44bcab125; HEAD remains measured payload. Owned manifest/input preservation diff exit0 and whitespace check exit0. The following evidence-only commit changes no gate input; no new Cargo/extractor execution at that commit is claimed. Complete scoped-input preservation and tracked-clean status checked before push; no unreviewed implementation ancestor introduced.

## Historical execution lookup body

Fresh branch: feat/rust-lighter-historical-fills-body, actual starting parent f853d56fd227c2106dba609141b1fdd44bcab125. Constructor/class and transferred venue/SDK files are outside this implementation slice. Source-body and native fixture were reviewed by a read-only native sub-agent; no reviewer runtime execution is claimed.

Payload/control SHA: 9006e77d4a095681f484cad422b39b2c6a3b9304. Diagnostic-only fixture SHA: 624c93c6345c4b398ad9bcaa06e0160b872cdd7e adds the authored case label to the iterator alias assertion. The source producer and production Rust body are unchanged between these SHAs. All controls restored exactly before merge-down (scoped git diff exit0).

Independent Python3.11 tomllib expectation compares all101 assigned rows with the actual integration parent: 1 pending->ported, 0 divergent; exchange triple (ported, divergent, pending) changes68/1/32 ->69/1/31. Enumeration stops at this one source file. The row maps to the actual exported fetch_historical_fills body. rust_files was derived from row targets; reconciliation writer exit0, actual Updated0 manifests,0 added/0 orphaned, Done21.59s. No regeneration.

Canonical references: [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Authored source/native floor90, stopped at iteration89. The source root must complete within30 provider resolutions (loop stops at29); residual unused providers after root completion are not independently rejected. Native FIFO bound100/stop99 fails on exhaustion. Actual method body plus4 AST-verified runtime imports and3 full auxiliary source inputs are pinned by b85838e0e041d0cb41bb24a53b2972fbcc274d940942011560f2eeeae66a9387. Native decoded fields, Number bits, UTF16 including lone surrogates, absent IDs, original errors, callbacks, fresh object identity and Map/iterator aliases are asserted BEFORE the pin. No re-encoding or roundtrip assertion.

Independent authored witnesses include: first primary request limit100 while final clamp200; four extraction writes into two deduplicated executions; eight execution own fields; decoded IDs18/17 with prices4.25/1.25 and quantities3.5/2.5; raw-empty fallback versus nonempty-invalid primary suppression; strict timestamp unit discontinuities and negative Date.parse; last duplicate asset LAST/D801; ID-less rows retained with an order filter; two nested IteratorClose calls, including throwing return with original body-error precedence; stable two-element tie IDs17/18. Native Map overwrite/values and array reductions/filters/spreads use actual Rust-produced values. Native sorting derives order from actual Rust comparator results, not the source final array. Source metadata Map snapshots bypass observed iteration with captured Map.entries and direct pair reads.

Compiled controls at9006e77d4a, one Cargo at a time, exact try/finally restoration:

- clamp200->199: Cargo101,100.21s, iteration0 Math.min arg1 nativeNumberBits.
- nanosecond divisor1e9->1e6: Cargo101,135.40s, iteration42 Math.floor arg0 nativeNumberBits.
- descending comparator reversed: Cargo101,27.02s, iteration0 nativeCallbackResult nativeNumberBits.
- warning D800->D801 substitution preserving UTF16 length: Cargo101,26.98s, iteration7 Owner.log arg0 nativeUtf16.
- host primitive keeps the first Map value instead of overwriting: Cargo101,31.24s, native iterator value alias; original diagnostic omitted the iteration. After adding the case label, this control reran at624c93c634: Cargo101,92.88s, explicitly iteration0 native iterator value alias. No survivor; no compiler failure counted as a control.

At624c93c634 the source child was made a no-op: Cargo101,110.87s, sourceHistoricalFillsMutationMustFailNamed (child exit0 rejected). Restoration: Cargo0,2.92s, both named target tests pass,90/stop89. Isolated source run at9006e77d4a copies only2 scripts and4 TS files, with explicit TypeScript installation and no NODE_PATH/NODE_OPTIONS: baseline0/.41s floor90; missing TypeScript1/.05s; child1/.29s at authored maxLimit200; restored0/.43s. No other package installation is needed by this producer.

Retained development findings: a library-only development check failed101 for a moved Rc capture, repaired before the production commit; its later library check0 is not all-targets evidence. A source-only development injection initially did not reach RegExp.test with numeric timestamps; the authored string timestamp now reaches that named fault. The first native development run failed101 because the fixture overescaped slash in the RegExp literal metadata; correction matches actual source .source, and the restored90-case development target passed0 before payload commit. These are development receipts, not final merged-head gates.

Required live boundaries retain full JS coercion/import/property/call/iterator/Promise behavior; this fixture covers bounded dense arrays, typed numeric operands, nonthenable provider records and two distinct executions. Larger sorts, exotic coercion/regex/date/prototype domains, cross-module table-proxy identity, thenable/adoption schedules, GC and full engine/class integration are unproved. Temporary unused endpoint-object allocations/schema are not directly asserted; selected request fields and separate finite/min calls are asserted. Imported implementations and actual HTTP/signer/socket effects are separate claims. No descriptor/reflection completeness is inferred.

Not run: workspace/full-monorepo build/check/suites, complete venue unit tests or older native targets, host clippy, wasm clippy/build, CI, real network/timer/websocket/signer operations, full exchange construction or full JS engine integration. Only the named new native target is executed.

Merged gate SHA: 3c4296f0df9b11497b0933068c7427ba0f94ad8e, actual parents624c93c6345c4b398ad9bcaa06e0160b872cdd7e and integrationfada0ab29c9534bf4b51ec6e9c9385cf490b5b4c. Fresh fetch/merge-down0; no unrelated unreviewed lane implementation ancestor. Exact scoped diff from624c93c634 over full venue src/tests/scripts, assigned manifest and4 TS inputs0. The integration changes are CI recovery metadata/loader tests plus Bybit test repair; no inherited Lighter method port. Controls remain measured at their explicitly named earlier SHAs; they were not rerun at this merge.

Actually measured sequentially at3c4296f0df:

- cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2: exit0,74.24s, Finished dev1m14s.
- SAFE_CEX_TYPESCRIPT_PATH=$TYPESCRIPT_INSTALL cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_historical_fills --jobs 2 -- --include-ignored --nocapture: exit0,9.43s, Finished test.87s/execution2.23s; actual_historical_fills_native_floor and historical_fills_source_mutation_must_fail_by_name both pass,90/stop89.
- node $YARN_JS parity:extract:check: exit0,18.71s; actual Would update0 manifests;0 added/0 orphaned, Done18.58s. Output read independently of exit.

Independent merged-head tomllib census0:1 pending->ported,0 divergent;69/1/31,101 assigned rows, stop one file. The following evidence-only commit changes no compilation/oracle/extractor inputs; exact preservation is checked before push. Positive exits are measurements at3c4296f0df, not a claim of a later Cargo execution.

## Derived constructor remainder

Branch feat/rust-lighter-constructor-body starts freshly at actual integration parentfada0ab29c9534bf4b51ec6e9c9385cf490b5b4c. Productione04f2ea2b95edb1db9646c5433d0be566e57f1bd; fixture d45f2e2cdae45e735b12dd8ef8991a054e3d5abb; payload/controlf87f856fea6ad64c1d5de609bb2bf8022fee6e50. No implementation ancestor from another unmerged lane slice. Source-only runtime-binding pin c703122e4614a8b663b1e2a098d687c176996b16e3e398c582ecb0869fbc3a3e includes actual constructor body and7 AST-verified non-type imports. Imported implementations are required boundaries, not included in this pin or executed by this oracle.

Canonical references: [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Actual Rust construct_exchange_body executes the constructor remainder after the required super_and_instance_elements boundary. That boundary must preserve full base construction, derived instance field/arrow initialization, newTarget, reentrancy, descriptors and original failures. This slice does not implement/prove that boundary or reuse Rust BaseExchange::new as an exact substitute. The class row remains pending; no complete native exchange/class/GC claim. The source Subject contains the actual constructor body, with synthetic base/import constructor probes; actual base and derived field initialization are not executed by this fixture.

Independent source/native floor28, stop27: five successful variants,21 named fault sites, second live-client-read fault, and reentrant client mutation followed by Candle constructor failure. Fresh native config own schema, UTF16/Symbol original exchange name, original option/store/super-owner handles, six independent native result identities, two actual live Owner.client reads, original Token throws and partial assignments are asserted BEFORE the source pin. Independent28-entry expected own-count table checks every case, including retained state after reentrant constructor failure. Source metadata snapshots inspect the raw backing target so observation adds no getter events. No re-encoding/wire/roundtrip assertions.

Native fixture tracks named lookup/call/construct order and actual shared native Owner fields. It does not independently assert captured callable replacement, actual this receiver or newTarget identity; those remain required realm/import-call contracts. Cross-instance allocation, five Map freshness, all126 instance fields/92 arrows, complete base subscriptions, module/prototype/reflection/lifetime behavior and class composition are unproved. No inventory completeness claim beyond the actual source class inspected by the reviewer. Synchronous constructor result has no async/adoption claim; any unexpected async use in this fixture fails.

Retained development receipts, not final gates: the first native-target attempt failed compiler101 because the fixture moved an Rc while a field borrow was live; fixed by cloning. Next native attempt compiled but failed101 at case6 originalThrow native fresh literal: a contextified VM replaced the injected global-getter Token with an unrelated error. DONT_CONTEXTIFY mode and an independent original-Token assertion for every source fault repaired the fixture; original native identity checks were retained. Corrected development target exit0, both tests,28/stop27, Finished test.62s/execution.53s. No production change was required by these fixture findings.

Reconciliation writer before payload: exit0, actual Updated1 manifest;0 added/0 orphaned, Done26.01s. rust_files derived programmatically from row targets, never regenerated. Only LighterExchange.constructor changes pending->ported; class and transferred files stay pending/untouched by this implementation.

At payloadf87f856fea, isolated source producer copies only2 scripts and1 TS file, with explicit TypeScript installation and NODE_PATH/NODE_OPTIONS removed: baseline0/.34s,28 cases; missing TypeScript1/.06s; child1/.28s at authored six constructor result identities; restored0/.31s. Full imported SDK/socket/base implementations are not loaded or claimed.

Not run: workspace/full monorepo build/check/suites, complete venue unit tests or older native targets, host clippy, wasm clippy/build, CI, real network/timer/websocket/signer operations, full base/class construction or JS engine integration. Only the new named constructor-body target is executed. No human review gate or CI rerun.

Compiled controls atf87f856fea, sequential Cargo and exact try/finally restoration:

- Keep the second live client Get, but pass the first cached client to CandlestickApi: Cargo101,99.79s, case4 CandlestickApi arg0 nativeObjectIdentity. This fails on value identity, not on removal of the lookup.
- Config exchangeName D800->D801 preserving UTF16 length: Cargo101,26.33s, case0 resolveLighterDeploymentProfile arg0 nativeUtf16.
- Source child made a no-op: Cargo101,20.68s, sourceConstructorBodyMutationMustFailNamed rejects child exit0.
- Restored target: Cargo0,.65s, both tests,28/stop27, Finished test.27s/execution.33s. Exact venue src/tests/scripts restoration diff to payload0. No compiler failure counted as a control; no survivor.

Merged-head gate SHA7a06ae330741069ba060705ea5b5cccc7ce14122; actual parentsf87f856fea6ad64c1d5de609bb2bf8022fee6e50 and integration333fd419fdbf70f081039610f22da9ee653bc96f. Fresh fetch/merge-down0. Exact full venue src/tests/scripts, assigned manifest and original TS file diff to payload0. Shared production/core and Cargo configuration changed upstream, so positive gates actually reran here rather than inferring them from the payload.

Measured sequentially at7a06ae3307:

- cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2: exit0,115.11s, Finished dev1m55s. Venue and dependencies, not workspace compilation.
- SAFE_CEX_TYPESCRIPT_PATH=$TYPESCRIPT_INSTALL cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_constructor_body --jobs 2 -- --include-ignored --nocapture: exit0,36.17s, Finished test23.05s/execution.31s; actual_constructor_body_native_floor and constructor_body_source_mutation_must_fail_by_name pass,28/stop27.
- node $YARN_JS parity:extract:check: exit0,21.28s, actual Would update0 manifests;0 added/0 orphaned, Done21.16s. Output text read.

Independent merged-head Python3.11 tomllib census0:1 pending->ported,0 divergent, exchange68/1/32 ->69/1/31;101 assigned rows examined, stop one file. Controls are measurements atf87f856fea and were not rerun at the merge. The following evidence-only commit changes no gate inputs; exact entire SDK except this evidence plus TS/package/yarn input preservation is checked before push. No later Cargo execution is implied.

## Single placement body, lot splitting and protective siblings

Payload/control SHA: `255e47511261b25a8fb68f31e01576e99ede759d`. Fresh branch `feat/rust-lighter-single-placement-body` started at integration `333fd419fdbf70f081039610f22da9ee653bc96f`; merged integration `d4e20845a85cc4e2711c99648ef7547f8aa806df` into `e439685633dbf623cf277ffe726db6d74f7928a3`. No other unmerged lane implementation ancestor. Only the `LighterExchange.placeOrder` row is moved by this slice; source reference remains maintained and unchanged. Canonical declarations: [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Independent Python3.11 tomllib census compared all101 assigned exchange rows against the actual integration parent; stopped at row100. **1 pending->ported, 0 divergent**, `(68,1,32)` -> `(69,1,31)` in ported/divergent/pending order. Venue-directory census at this merge is `(263,19,54)`; that census includes other lanes' upstream work and is not the change attributed here. Writer preparation at payload exited0, actual Updated1 manifest;0 added/0 orphaned, Done23.36s. rust_files derived programmatically from actual row targets; manifest reconciled, not regenerated.

Actual source arrow body and11 runtime binding AST checks execute with six actual auxiliary source inputs. Source-only pin: `2b6edc50a84e9daaee4cca942ff1e26cb553a3298a715c04d2915905a73b6b20`. Authored floor **91**, stopped at **iteration90**:48 initial,30 dependency faults,9 recovery faults,4 sibling faults. Source provider bound30 requires root completion; it does not separately reject unused residual providers after completion. Native FIFO bound100 fails at99. No exhaustive engine/queue enumeration claim.

Native checks assert actual decoded scalar kinds/bits/BigInts/UTF16, original opaque/function/Symbol/error identities, live callee/receiver/event order, fresh record schemas, hint retention/deletion, actual lot arrays, actual mapped sibling arrays and iterator elements/close outcomes. Native JSON assertions inspect actual parameter own fields and each actual Rust replacer callback input/output before the bounded native scalar encoder runs. The actual native encoded JSON and log comparisons are deferred across the **entire91-case corpus**, after every native assertion and the independent floor check. Cases0/18 independently require two wire comparisons; case18 asserts the raw native lone D800 field before wire. This avoids a wire assertion preempting a later decoded native assertion. Wire values are not used as decoded-value oracles.

Source findings preserved: amount25 splits into scaled lots6667/6667/6667/5000, totaling25001; the tiny-tail amount21 path gives6667/6667/7667. At `placeOrder`'s per-lot `lotOrderId`, the source increments past281474976710655 to656 and657 without another modulus; native BigInt values are independently asserted. These are source behavior findings, not fixes or Rust divergences.

Required live language/import/owner/Promise/iterator operations have no fallback business result. The child `owner.placeOrder` property is a required live boundary with a mocked provider in this fixture; complete recursive exchange composition is **not proven**. Bounded dense arrays/primitive operators/scalar JSON and actual helper execution are exercised, not a complete coercion/prototype/reflection/GC/thenable engine. The JSON encoder is driven by the actual Rust callback over actual native fields, not a source-fixed serialized result. Full exchange construction, real signer/network/timer and exotic host domains remain unproved. No shared trait/type changed; the new host is venue-local.

Retained development findings: initial rustfmt parse exit1 and moved-value compiler corrections were development, not negative gates. A development native run failed101 at iteration4 `nativeOperationOrder`: Rust argument evaluation read opts.side before the conditional price method was captured. The initial static sub-agent pass missed this; the native assertion caught it and `c635677fa9ed412869819af9067bdf66d6dfb703` fixed production to capture the callee first. Later fixture work recognizes both actual source Error realms and asserts the raw surrogate before deferred wire. Development restored runs passed91/stop90; those are not substituted for the merged-head gates below.

At the payload SHA, sequential compiled controls (all runtime101, no compiler failures counted), each restored exactly in try/finally:

- conditional-capture:101,102.48s; iteration4 `placeOrder nativeOperationOrder`.
- trailing-lot-value:101,21.74s; iteration11 `placeOrder adjust arg0 nativeNumberBits`.
- iterator-close-host:101,24.43s; iteration25 `placeOrder independentSourceEventCount`.
- encoded-surrogate-host:101,19.61s; iteration18 `placeOrder bounded JSON wire encoded wire after ALL native values`, after the explicit91-native-completed marker. Only native JSON quoting substitutes D800->D801, preserving encoded length while changing bytes.
- source-noop:101,17.02s; `sourceSinglePlacementMutationMustFailNamed` rejects a successful mutated-source child.
- restored:0,1.88s; **2 passed**,91/stop90, test execution1.17s.

Each failing enumeration stopped at its named assertion. No mutation survived; no control was dropped. Full owned production/test/oracle exact restoration diff exited0 before merge; the four owned body/test/scripts are byte-identical at the merged gate SHA.

Isolated producer at payload copied only two scripts and seven actual TS inputs, with explicit TypeScript installation and no NODE_PATH/NODE_OPTIONS: baseline0/.56s,91; missing-TypeScript1/.06s; source-child1/.40s with named `authored base100 plus random56`; restored0/.58s,91. These are dependency/source discrimination checks, not additional Rust gates.

Actual merged-head positive gates at `e439685633dbf623cf277ffe726db6d74f7928a3`, sequential, not inferred from payload controls:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**,172.58s; Finished dev2m52s. This is the venue/all-targets check, not workspace compilation.
- `SAFE_CEX_TYPESCRIPT_PATH=$TYPESCRIPT_INSTALL cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_single_placement --jobs 2 -- --include-ignored --nocapture`: **exit0**,43.01s; Finished test29.04s/test execution2.15s;2 passed,91/stop90. Explicit all-native-completed marker precedes deferred wire checks.
- `node "$YARN_JS" parity:extract:check`: **exit0**,13.02s; actual Would update0 manifests;0 added/0 orphaned, Done12.87s. Output read.

Native sub-agent static review returned MERGE after reviewing immutable payload and merge. No further blocker found; native-before-wire ordering, original hint/recovery/close catch boundaries and one adapted mapping checked. Reviewer ran no Cargo/Node gates and explicitly retained the first-pass missed callee-order defect. Incoming `http_transactions.rs` changed upstream, so merged positive gates above were actually rerun. Negative controls remain measured at payload, not claimed rerun at merged SHA.

Not run: workspace/full monorepo checks/builds/suites; full venue unit tests or older native targets; host clippy; wasm clippy; CI; real network/HTTP/websocket/signer/timer integration; full exchange constructor/recursive child/JS engine integration. Only the named native target ran. The following evidence-only commit changes no gate inputs; exact byte preservation is checked before push.

## Live order mapping, loose fields and original info identity

Control/runtime payload SHA: `6398edac642e8ded4b41bd50d2f5841e9a0ef045`. Production body `274d26cfb47cb91dc5cf170fa4c4a140de5f62c3`; parse-ID Rust result type annotation fixed in `615c3abfc7a1b86e7d39721a0b52660e87825cc7`. Fixture/source `2eb2e90d8f3cbcf543257f5975af61e07c9a04ab`, captured-callable proof strengthened in `6c7a6fb385`. Manifest-only followup `124adc43fd66d6ae2678272a7a5ae7feb334e2d7` sets layer=adapted and retains the prior inherited-table finding; runtime/control inputs unchanged. Fresh branch `feat/rust-lighter-order-mapping-body` started at actual integration `d4e20845a85cc4e2711c99648ef7547f8aa806df`. No other unmerged lane implementation ancestor.

Canonical declarations: [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Only `LighterExchange.mapOrder` pending->ported, **1 row**, zero new divergence. Actual Rust body covers nested-order merge, direct/normalized loose-field search, finite loose numbers/booleans, numeric/string type/TIF maps, market selection, signed-int64 ID candidates, sticky hints/existing order recovery, side/quantity/notional/price/reduce-only decisions, inference and the14-field order/two-field extra record. `info` retains the original input or fresh merged object by native identity. No-ID still constructs/discards the order and reads the clock before returning a fresh empty array.

Source-only pin `13a41bc1893751bc1af64ff0363287e2ead0ae2e15b52c5c3d96862d084a761c` covers the actual method,10 runtime import binding AST checks, full types/lighter.types/get-key source and actual installed Lodash version4.18.1. Producer executes actual get-key with actual camelCase/memoize/snakeCase dependencies. Allocation-only AST instrumentation observes ordinary literal/rest-array/regex creation; actual source for-of drives a traced array iterator with close hooks. RegExp objects retain their actual brand. Data snapshots bypass observer getters/iteration. Source/main VM uses DONT_CONTEXTIFY. Business statements/TS source remain maintained and unchanged.

Independent authored floor **100**, stopped at **iteration99**:54 initial cases,26 dependency/iterator faults,19 targeted reviewer witnesses and one callable-replacement case. The exact authored spec count100 is asserted independently before executing cases. Named injected fault cases must reach their injection exactly once. There is no asynchronous provider or queue enumeration in this synchronous body. No exhaustive engine/array enumeration claim.

Native assertions run before the source-only pin; there is **no wire, re-encoding or roundtrip comparison**. Actual native variants, Number bits/NaN, BigInts, UTF16 string/property keys, original error/function/object identities, source argument/field counts and ordered key schema are checked. Each actual native IteratorStepValue is checked against independently observed source done/value before body execution. Native original-throw versus nonthrow IteratorClose precedence is implemented, not a source-fixed return. Both outer and inner early-return loops close. Destructuring consumes two actual native pair values then closes its pair iterator. Native Object.entries uses actual native field keys and getters, and native find executes the actual Rust callback with actual native array elements/index/receiver. Fresh literal/copy/array values are built from actual native inputs. Selected fixed expectations include signed-int64 max/fallback IDs, amount/remaining/price,14+2 schemas, original versus fresh info, NaN boolean true, protective type/TIF and no-ID completion effects.

Callable review finding: initial wrappers used fixed names and native Call stored only name/receiver, proving operation/order but not callable replacement identity. Fixed before payload: distinct source callable IDs are recorded on actual calls; native Call retains the corresponding Function identity and asserts it before arguments. Case99 replaces String during v(symbol), after the outer String capture but before invocation. Source independently asserts the original captured identity and original BTC/lone-D800 result after the live provider changes; native host tracks that provider change independently and snapshots its version at capture. Callable reflection/arity/newTarget and a complete engine remain unproved.

Scope: required full language/import/live-owner bindings do not implement missing exchange business bodies. Fixture covers dense arrays, primitive Number/BigInt operations, ASCII casing with retained UTF16 units and the four actual regex patterns/JS whitespace used here. Arbitrary Unicode case expansion, Proxy/descriptors/symbol-spread/exotic coercion/prototypes and complete class/engine composition are not proved. Actual v and owner hint/inference are required dependency boundaries; this target does not substitute a complete native implementation of those imported/sibling bodies. Source raw typeByNumericCode/tifByNumericCode inherit truthy Object.prototype members that suppress defaults; the pre-existing scripts/table-inheritance-oracle.cjs finding is retained, **not closed by this100-case fixture**. No semantic fix to that source behavior is claimed.

Development receipts, not final gates: initial library-only compilation101 needed the swallowed Result's explicit Rust type; fixed615c. Initial source/native fixtures hit a RegExp-proxy brand mismatch, raw JSON lone-surrogate key decoding, missing initial owner.store state and an incorrect authored empty-case index52; repaired respectively with actual RegExp brand, keyUnits transport/native UTF16 keys, explicit actual initial owner field, and independently enumerated index50. One native development compile101 failed copying an incremental object file; subsequent runs explicitly use CARGO_INCREMENTAL=0. Native99-case target passed0 (Finished test28.30s/execution1.91s), then strengthened99-case target passed0 (Finished test2m40s/execution2.32s). First callable-replacement development run failed101 at99 `String actualNativeReturn nativeUtf16` because the native v host omitted its authored provider transition; transition fixed before payload. These fixture findings are not production defects or negative gate claims.

At payload, sequential `CARGO_INCREMENTAL=0 SAFE_CEX_TYPESCRIPT_PATH=$TYPESCRIPT_INSTALL cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_order_mapping --jobs 2 -- --include-ignored --nocapture` baseline **exit0**,308.61s;100 cases/stop99,2 passed. Compiled control results and final restored run follow; none is inferred from this baseline.

Isolated producer at payload copies only two scripts and four actual TS inputs, with explicit TypeScript/Lodash installations and NODE_PATH/NODE_OPTIONS absent: baseline0/.47s,100; missing-TypeScript1/.05s; missing-Lodash1/.32s; actual source child1/.28s with named `authored open remaining recovers amount10`; restored0/.43s,100. These are dependency/source discrimination checks, not Rust gates.

Not run: workspace/full monorepo check/build/suites; full venue unit tests or older native targets; host clippy; wasm clippy; CI; real HTTP/network/signing/timers/websocket; full exchange class, recursively composed helpers or complete JS engine. No shared trait/type changed; new host is venue-local. The legacy table_inheritance target was read, **not rerun** by this slice.

At control payload6398edac, compiled runtime controls (all Cargo101, stopped at the named failing iteration):

- remaining:101,30.19s; `iteration0 mapOrder Number.isFinite arg0 nativeNumberBits`.
- id-boundary:101,20.63s; `iteration17 mapOrder nativeOperationOrder`.
- undefined-match:101,25.55s; `iteration80 mapOrder nativeOperationOrder`.
- id-return-close:101,21.64s; `iteration85 mapOrder unexpected native member`.
- callee-cache:101,21.07s; `iteration0 mapOrder actual captured native callee nativeFunctionIdentity`.
- native-surrogate:101,13.28s; `iteration0 mapOrder get Input [115, 121, 109, 98, 111, 108] actualNativeValue nativeUtf16`.
- source-noop:101,19.26s; `sourceMappingMutationMustFailNamed`.

Initial callee-live-version attempt101/20.14s is **rejected as a gate**: it held a RefCell borrow across reentrant native v work and failed on `RefCell already borrowed`, not a named decoded-value assertion. The corrected control copies/releases the version before Call; its actual runtime measurement is at manifest-followup124adc43 (runtime inputs byte-identical to6398).

- callee-live-version-repaired: exit101,30.62s, SHA `124adc43fd66d6ae2678272a7a5ae7feb334e2d7`. `iteration99 mapOrder String actualNativeReturn nativeUtf16`; compiled, no borrow/compiler failure.

- repaired-restored: exit0,14.17s, SHA `124adc43fd66d6ae2678272a7a5ae7feb334e2d7`. 2 passed,100/stop99; execution2.26s.

Original restored run at6398: exit0,30.41s,2 passed/100 cases, execution3.55s. Full owned src/test/scripts restoration diff to6398 exited0. Native-surrogate replaces decoded D800->D801, retaining UTF16 length; this target has no wire assertion and makes no encoded-wire control claim. No accepted mutation survived, no control was dropped. Cargo executions were sequential; CARGO_INCREMENTAL=0 was explicit.

Actual merged positive gate SHA: `6876a6011319a748ed35793c32a9467f01ec4293`, actual parents manifest-followup124adc43 and integration `694e07f7e049a5a7047c41e994a0fc854dc78a59`. Integration advanced during fetch;694e is the parent actually merged/built, not the earlier fetched ac2bd2f1. Fresh initial parent remains d4e20845. Read-only sub-agent review closed the callable finding and found the empty layer/removed inherited-table finding in metadata; both metadata issues repaired124adc43 through the writer. No other unmerged lane implementation ancestor.

Independent Python3.11 tomllib at merged gate SHA compared every assigned exchange row against actual integration parent694e: **1 pending->ported,0 divergent**, `(68,1,32)` -> `(69,1,31)`,101 rows, stopped row100. Exactly six implementation/manifest paths differ from integration; GATE_EVIDENCE makes the seventh owned path. Other venue/SDK work is upstream-only. Writer preparation actual outputs read: first0/Updated0/Done25.26s; callable-followup0/Updated0/Done16.13s; metadata-followup0/Updated0/Done184.97s; all0 added/0 orphaned. Arrays derived programmatically, no regeneration.

Sequential merged-head gates, CARGO_INCREMENTAL=0 explicit:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`: **exit0**,135.42s; Finished dev2m15s. Venue/all-targets, not workspace compilation.
- `SAFE_CEX_TYPESCRIPT_PATH=$TYPESCRIPT_INSTALL cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_order_mapping --jobs 2 -- --include-ignored --nocapture`: **exit0**,9.10s; Finished test.56s/execution2.06s;2 passed,100/stop99.
- `node "$YARN_JS" parity:extract:check`: **exit0**,12.09s; actual Would update0 manifests;0 added/0 orphaned, Done11.97s. Text read, not inferred from shim status.

Negative controls remain measurements at6398 and the repaired control at124adc43, not claimed rerun at merged SHA. Owned body/test/scripts and actual TS inputs survive the merge exactly. The following evidence-only commit changes no gate inputs; exact scoped preservation is checked before push. No additional runtime, workspace, clippy, wasm, full suite, CI or legacy table-inheritance execution is claimed.

## Class composition checkpoints (class row still pending)

Fresh branch `feat/rust-lighter-class-composition` starts at actual integration parent `694e07f7e049a5a7047c41e994a0fc854dc78a59`. It composes thirteen locally reviewed, unmerged lane slices; those reviewed implementation ancestors move 31 rows, independently measured as `(68,1,32)` to `(99,1,1)` in the assigned exchange manifest. No new class row has moved at these checkpoints. The fetched integration `8fcf323edfc6747fcdf4427ce19267a7147d018b` still has `(68,1,32)`. This census is separate from callback/runtime proof.

At `cb25b0d17dd33f0be782d08f887ec7027569500d`, `CARGO_INCREMENTAL=0 cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2` measured exit **0**, Finished dev 3m25s. This checkpoint precedes the new live-helper/class modules and does not cover their compilation.

At immutable fixture payload `1f999c4948`, `CARGO_INCREMENTAL=0 SAFE_CEX_TYPESCRIPT_PATH=$TYPESCRIPT_INSTALL cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_class_composition --jobs 2 -- --include-ignored --nocapture` measured exit **0**, Finished test 3.01s, execution .43s, two tests passed. The authored helper floor is 82, stopped at iteration81; native decoded values precede the source pin and no wire assertions run. The structural census is 126 derived fields, 92 arrows, two prototype methods and three statics. This passing checkpoint has two subsequently identified fixture gaps: imported Grafana handles were incorrectly fresh and the registered constructor callback was discarded. It is not evidence discharging those gaps or full class composition.

Static review retracted an incorrect `++` allegation: the maintained invalidation body uses `+= 1`. A real review finding changed both liquidation parse sites from global `parseFloat` to `Number.parseFloat` in `4e9613ce59`. Initial development checks measured exit101 for a borrowed-argument compiler error and a fixture Debug formatting compiler error; a later development run measured exit101 for an unsupported fixture `some` intrinsic. None is counted as an adversarial runtime control. The earlier 61-case development test measured exit0 but was not measured against an immutable fixture commit and is not a SHA-bound gate claim.

Not run at these checkpoints: workspace/full monorepo builds or suites, complete venue tests, host clippy, wasm clippy/build, CI, real HTTP/signing/websocket operations, complete JavaScript engine or realm-GC integration. Existing source prototype-table findings remain open. Canonical declarations remain at [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).


## Delivered class composition: owned pending zero

Measured executable/manifest payload: `99c667b5e4651868a2676b4dfde65f9803e45f8a`. Actual merge checkpoint `b88290528261a9e6431943c49a19367468886f6c`, actual incoming integration parent `49a942e76143d6d1d44567edff59c12669d92627`, original fresh parent `694e07f7e049a5a7047c41e994a0fc854dc78a59`. The only executable-file changes after that merge are module declaration ordering and test-log formatting; the final Cargo gates below were repeated at the measured payload. Final evidence-only publication must preserve this executable/manifest tree.

Independent tomllib census of the assigned source manifest: **(101 ported, 0 divergent, 0 pending)**, 101 rows through row100. Relative to the actual integration parent, **32 rows move off pending**: 31 from the thirteen reviewed dependency slices below, plus one newly completed class row. Separately, the existing price-reference helper changes divergent to ported. This slice changes43 rs targets:41 already-ported helpers, the former divergent helper, and the class. Existing row identities, order and kinds remain. Whole venue census is (295,18,23); all23 remaining pending rows are outside the assigned exchange-file partition and were not edited.

The Rust class owns derived-field initialization and direct invocation of the maintained Rust method bodies. Native source witnesses exercise registered construction, detached lexical-owner calls, shared imported arrays, fresh instance functions/Maps, mutable class tiers, warning calls/state, UTF16 store keys and both original price Errors. The structural floor is126 fields through field125,92 own arrows through arrow91,94 callable bindings through binding93, two prototypes through prototype1 and three statics through static2. These are structural counts, separate from the85-case helper floor through iteration84 and three composed async cases through iteration2. Root completions occur at FIFO3,3,0; no full queue-drain claim. The async path calls actual Rust getAccount/fetchAccountInfo/fetchParsedAccountLookup/extractAccountFromResponse/parseAccountIndex bodies and independently expects provider counts[1,1,0]. Decoded native values, Error identity, aliases and field schemas precede the source pin; the new class target performs no wire assertions.

Required realm operations supply exact inherited super/newTarget and JavaScript callable, descriptor, import, operator, exception and Promise representation. Full engine/GC/descriptors and foreign prototype receiver integration remain unproved. Previous inherited prototype-table findings at mapOrder are retained, not closed by this corpus. Canonical declarations remain at [14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Final measured gates, `CARGO_INCREMENTAL=0`, one Cargo process at a time:

- `all-targets`: exit **0**, 120.06s, SHA `99c667b5e4651868a2676b4dfde65f9803e45f8a`.
- `native-focused`: exit **0**, 54.82s, SHA `99c667b5e4651868a2676b4dfde65f9803e45f8a`.

`all-targets` is `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2`. `native-focused` is `SAFE_CEX_TYPESCRIPT_PATH=$TYPESCRIPT_INSTALL cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --test live_class_composition --test live_constructor_body --test live_trading_preflight --jobs 2 -- --include-ignored --nocapture`: five tests across three binaries; class85/stop84 plus async3/stop2, constructor28/stop27, trading148/stop147.

At the merge checkpoint, `node $YARN_JS parity:extract:check` measured exit **0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols`, Done27.07s. The assigned manifest and source inputs are unchanged at the final payload; the extractor was also rerun at `8b4b7e21b5e6a5a9885b228be75d792cfed08f3c` with exit0, the same zero-update output and Done28.58s. That is the extractor SHA; the Cargo gate SHA is `99c667b5e4651868a2676b4dfde65f9803e45f8a`. Both manifest writers measured exit0 after edits: first output Updated1/zeroadded/orphaned, then Updated0/zeroadded/orphaned. Direct scoped rustfmt check on the six changed Rust files measured exit1 for inherited module ordering at the merge, then exit0 after module normalization. A workspace-edition2024 check subsequently measured exit1 for only the class test log-line layout; after formatting that line, the final six-file edition2024 check measured exit0. The earlier merge-checkpoint all-targets/native-focused gates also measured exit0 (305.99s/106.56s).

Adversarial controls below are attached to their actual payloads, not inferred as executions at the final SHA. Each listed negative result reached the named runtime assertion; restored complete targets passed.

| Payload | Control | Exit | Seconds |
| --- | --- | ---: | ---: |
| `26a001f0db127e27a2ce0da7bb28566d90a7ab1d` | baseline | 0 | 71.24 |
| `26a001f0db127e27a2ce0da7bb28566d90a7ab1d` | wrong-body | 101 | 35.56 |
| `26a001f0db127e27a2ce0da7bb28566d90a7ab1d` | wrong-parser-binding | 101 | 38.15 |
| `26a001f0db127e27a2ce0da7bb28566d90a7ab1d` | encoded-surrogate | 101 | 34.77 |
| `26a001f0db127e27a2ce0da7bb28566d90a7ab1d` | frozen-tiers | 101 | 46.77 |
| `26a001f0db127e27a2ce0da7bb28566d90a7ab1d` | registered-constructor | 101 | 41.44 |
| `26a001f0db127e27a2ce0da7bb28566d90a7ab1d` | producer-noop | 101 | 43.06 |
| `26a001f0db127e27a2ce0da7bb28566d90a7ab1d` | restored | 0 | 0.94 |
| `ded29e9ea119dff1c1a960bd574ff18355fdb26c` | baseline | 0 | 201.63 |
| `ded29e9ea119dff1c1a960bd574ff18355fdb26c` | wrong-api-receiver | 101 | 23.79 |
| `ded29e9ea119dff1c1a960bd574ff18355fdb26c` | surrogate-store-key | 101 | 27.62 |
| `ded29e9ea119dff1c1a960bd574ff18355fdb26c` | surrogate-error-message | 101 | 27.06 |
| `ded29e9ea119dff1c1a960bd574ff18355fdb26c` | recreated-generated-error | 101 | 25.71 |
| `ded29e9ea119dff1c1a960bd574ff18355fdb26c` | restored | 0 | 16.9 |

The six first-payload controls detect wrong class helper dispatch (iteration8/nativeKindMismatch), wrong Number.parseFloat capture, encoded-surrogate substitution in the OHLCV key (iteration20/nativeUtf16), frozen class tiers (iteration42/nativeNumberBits), wrong registered constructor callback (independent field count), and a no-op producer (sourceClassMutationMustFailNamed). Four final-fixture controls detect wrong API receiver, changed surrogate store key (iteration82/nativeResultUnexpectedError), changed surrogate Error bytes (iteration83/native error UTF16), and a matching Generated Error replacing the actual Thrown Error (iteration83/nativeThrownOriginalErrorRequired). No survivor. Initial wrong-receiver attempt at046795a1fb450bc8b36ffc71b6727037fd6bc956 measured cargo101 due unused client/compiler rejection7.12s, was rejected as a gate, restored, repaired and rerun at ded29; it is not in the ten valid controls.

Isolated tracked-source mirror at b88290528261a9e6431943c49a19367468886f6c, with no ancestor node_modules and explicit compiler/Lodash roots: producer exit0/.59s, missing compiler exit1/.06s, missing Lodash exit1/.36s, actual source-field mutation child exit0/.63s (requires named child exit1), no-op producer child exit1/.11s, restored producer exit0/.50s. The runtime source pin is e3b6e9ed37ceb1031fda29b1a27ee5f41a144d301eccdcaa1095f087fa436d02; it covers maintained class/constants/enums/import declarations/get-key/account-tier/utils/endpoint declarations. Fixture inputs are additionally bound by their commit. No package implementation hash claim beyond executing the installed imports.

Sub-agent reviews found/fixed Number.parseFloat binding and fixture gaps in imported alias identity, registered constructor dispatch, transport receiver identity and original Error discrimination. An incorrect source ++ allegation was retracted: source uses +=1. Final bounded static review at the merge checkpoint returned MERGE, ran no runtime gates, and preserved the explicit integration limits above. Formatting-only followups preserve all module declarations and test operations. Merge-down named workflow-step occurrences:93 before/93 after, none lost. No human review gate.

Reviewed unmerged dependency ancestors (all thirteen are included; no known unreviewed lane implementation ancestor):

- PR7849: `6dfe8de5c5a80a41261350986a08da88981ad077`.
- PR7868: `792619e64fc1032a56c7b936361a94547c867177`.
- PR7875: `8113e13a4e0eec80ca8bc038ab54f581ec678c99`.
- PR7886: `32b157cf1a2154af02c308df52c009d4381665c9`.
- PR7897: `da1286840e40994e2be8da6b8410b27f23efbc5c`.
- PR7904: `06d0e113de1651e42d75f78882d7d8586b1d6c8b`.
- PR7908: `f6336f69d2d9d2bab96de05a4099fdc136e9ccee`.
- PR7916: `bb121d7ec8ec72537f2216c3acb91b573a19a982`.
- PR7918: `9db4e8d8e95fbfd5499b38b091460b8c78995692`.
- PR7931: `071f6edb1318d0b0f060ebafe1a41385a0a76038`.
- PR7936: `7bf10d951ddd24f8b7bbd89235e1f3fa729dd9b9`.
- PR7949: `d8ab2382c562eb1c1326e1851b56b8144a7d3433`.
- PR7966: `cfb5b953894ce474fd4c0d4e1a81a009f461427f`.

Not run: workspace/full monorepo builds/checks/suites, complete venue tests or all older native targets, host clippy, wasm clippy/build, CI or CI reruns, real HTTP/network/timer/signer/websocket operations, complete engine/GC/descriptors/foreign-prototype-receiver integration. No merge into integration, history rewrite, stash or SDK/other-owner source/manifest edit.

Evidence-only publication `edbac6409012cb3fe605e670a399ef202933fc01` was verified against the measured Cargo payload: the only changed path was this evidence file (verification exit0). This attribution-only followup preserves the same executable and manifest payload.

## Sprint batch lint repair and separate delivery

Lint defect: aggregate delivery `e683a6305027e6ea0d182e7097348ecf55846cb1` measured host workspace clippy101 and canonical wasm-clippy wrapper1/Cargo101 at exchange_batch_placement.rs305. Both rejected the redundant Ok/result-question-mark wrapper. Commit `babb9f7581683b100f334c4ab44140d316e46827` returns the same SigningResult directly, preserving success/error identity and the surrounding batch_error catch. No diagnostic class is suppressed and no parity row is relabelled.

Toolchain environment finding: original-delivery wasm policy build wrapper1/Cargo101, exchange default/all-feature builds101, warning-probe wrapper1/Cargo101 and core build101 stopped because local rust-lld could not locate its own libLLVM.dylib. Audit0 and tooling setup0 were separate completed results. A per-process DYLD_FALLBACK_LIBRARY_PATH containing the selected toolchain lib directory made the linker-version probe pass0; no repository/global toolchain setting was changed. These failures are not Rust parity findings or successful wasm build claims.

Evidence correction: actual merge7fdd57054297835164fe7ce4a39c5dd7a88c00a5 has incoming integration parent `539ac8165b8e36ef009dc467123e69fe14e002e2`. The earlier60c59f01 attribution was stale because the shared remote-tracking ref advanced before merge. The actual parent was verified from the commit object. Against that parent, the isolated branch contains only the seven batch-owned paths and none of the other twelve unmerged slice implementations.

Sub-agent static review returned MERGE for the one-line fix and isolated seven-path diff; it ran no runtime gates. Pending movement remains exactly one row for this standalone batch slice. The aggregate class slice remains a separate delivery.

Sprint gates are cargo build --workspace --locked, host workspace clippy and the canonical wasm-clippy helper. Delivery receipts will be stored under sdk-rs/target/lighter-sprint-gates keyed by the exact measured handoff SHA. The local receipt is produced after the final commit and is not followed by another commit, avoiding attribution to a different SHA. Full workspace tests, formatting/extractor/report checks and separate wasm build/runtime steps are deferred under the sprint protocol. Historical receipts above retain their original SHA/scope.


## Sprint ordered stack consolidation

Consolidation starts at exact integration parent `539ac8165b8e36ef009dc467123e69fe14e002e2` on local branch merge/lighter-stack. Fourteen published PR heads were merged in ascending order, stopping at PR7982. These merge commits preserve each original head rather than squash or rewrite history.

| PR | Incoming head |
| --- | --- |
| 7849 | `6dfe8de5c5a80a41261350986a08da88981ad077` |
| 7868 | `792619e64fc1032a56c7b936361a94547c867177` |
| 7875 | `8113e13a4e0eec80ca8bc038ab54f581ec678c99` |
| 7886 | `32b157cf1a2154af02c308df52c009d4381665c9` |
| 7897 | `da1286840e40994e2be8da6b8410b27f23efbc5c` |
| 7904 | `06d0e113de1651e42d75f78882d7d8586b1d6c8b` |
| 7908 | `f6336f69d2d9d2bab96de05a4099fdc136e9ccee` |
| 7916 | `bb121d7ec8ec72537f2216c3acb91b573a19a982` |
| 7918 | `9db4e8d8e95fbfd5499b38b091460b8c78995692` |
| 7931 | `071f6edb1318d0b0f060ebafe1a41385a0a76038` |
| 7936 | `7bf10d951ddd24f8b7bbd89235e1f3fa729dd9b9` |
| 7949 | `d8ab2382c562eb1c1326e1851b56b8144a7d3433` |
| 7966 | `cfb5b953894ce474fd4c0d4e1a81a009f461427f` |
| 7982 | `e683a6305027e6ea0d182e7097348ecf55846cb1` |

The reviewed local batch lint repair is additionally retained from `36a4effac695dddb4eb4b71fdfd9c6d3f4f3e138`; its code change is the direct SigningResult return at the existing batch map call. Conflicts were confined to the exchange manifest, venue lib.rs exports and this evidence file. Row records were reconciled three ways with their existing annotations; missing module declarations were retained; evidence conflicts used union resolution so both sides remain.

Independent fixed expectations were101 row identities/order/kinds and the authored per-slice pending movements8,2,2,4,4,4,1,1,1,1,1,1,1,1. Final exchange manifest is (101 ported,0 divergent,0 pending):32 rows leave pending and one existing divergence becomes ported relative to the actual incoming integration parent. The complete final parsed manifest equals the reviewed PR7982 manifest. Canonical parity:extract writer after reconciliation reported Updated0 manifests,0 added,0 orphaned symbols, Done18.26s; every symbol annotation was checked unchanged. No manifest was regenerated.

The merge-blocker delivery gates are cargo build --workspace --locked and host workspace clippy, sequential with jobs2. They will run after this final commit. Their exact-SHA local receipts will be written to sdk-rs/target/lighter-stack-gates/<handoff-SHA>/gates.json, with no later commit changing the handoff SHA. No push or integration merge is authorized or performed. Other gate receipts above remain attached only to their original SHAs; no old result covers this consolidation by inference.

## Sprint batch merge into the consolidated lighter stack

The requested batch head `dcb88b8e64efc8da8ed831388c9adaec2609b77f` is merged into local merge/lighter-stack, whose pre-merge head is `3afaa00c802b0bc6b30fc2fc30e545d162127dd0`. The sole conflicted file is lighter.exchange.ts.toml. Seven rows retain both their class-dispatched live helper and the incoming standalone adapter/capability mappings, with notes identifying their separate ownership. No row is regenerated or relabelled; the exchange triple remains101 ported,0 divergent,0 pending. Both parents' evidence remains retained.

The requested workspace build and host workspace clippy will run sequentially after the merge commit, with exact-commit exits in sdk-rs/target/lighter-stack-gates/<handoff-SHA>/gates.json. No earlier gate result covers this merge. Canonical wasm clippy and workspace tests are not part of these two requested gates. No push is performed.
