# Asterdex gate evidence

Initial types/tables slice: 33 mapped manifest rows, 198 pending.

- Branch base: core `d854ac9ebd`, utils `3bbc52657f`, origin integration merged.
  Workflow named steps: 54 before, 55 after; no pre-existing name removed.
- Crate-only check, strict clippy all-targets and wasm build passed, each with
  `--jobs 2`. No workspace build or full repository suite ran.
- Live TypeScript table/type oracle: 33 tests passed after restoration.
  Changed `ASTERDEX_ENDPOINTS.REST.PRODUCTION` independently in Rust and TS:
  both runs exited 101 naming `table_asterdex_endpoints`; restored run passed.
- CI wiring/layer audit: 104 tests passed. Initial attempt failed because cargo
  was absent from PATH; reran with `/opt/homebrew/opt/rustup/bin` and passed.
  Permanent controls remove and mask the mandatory Asterdex oracle step.
- Sub-agent review: no mapped contract mismatch; requested arbitrary WebSocket
  payload witnesses, false/tuple-length boundaries and updated crate guidance.
  All three addressed; dropped-payload mutation and re-review recorded next.

Shared pending operations are not implemented locally: BigInt, Number(any), own-key
ordering and JSON.stringify await their shared numeric owner. Pending does not claim
parity or an accepted omission.
- Review fix control: removing WebSocket extra-field storage from both wire and
  public records exited 101, naming `contract_asterdexwebsocketmessage` round trip.
  Restored source; reviewer re-read the updated isolated checkout and returned MERGE.
- Additional serde metadata oracle checks the actual version/STP spellings and
  aliases, beyond testing the expected valid inputs.
- Added source inventory checks: 35 live tests pass; new unchecked table/type
  declarations cannot silently bypass the named contract tests.
- Formatting mutation failed at exit 1; invalid Rust constant failed strict
  clippy and wasm build at exit 101 with E0308. Restored gates passed.
- Missing Rust target mutation failed parity check, report and progress at exit 1,
  naming MISSING_CONTROL. Restored check/report/progress passed.
- Coverage initially found a formatting-only manifest drift (0 added/orphaned).
  Canonical serializer fixed its trailing newline. A temporary exported TS constant
  failed coverage by name; restored coverage has 0 updates, additions or orphans.

## Auth and PnL helper slice

All 45 types-file rows mapped; 186 family rows pending. The 12 newly mapped rows
cover placement options and pure helpers. Scoped fmt, clippy, 37 live tests, wasm
build and extractor coverage pass. One serial sub-agent review returned MERGE.
A plausible Number-for-parseFloat substitution fails at exit 101 naming
normalizeAsterDEXRealizedPnlDeltas; restored helper oracle passes.

After merging shared primitives, the global parity gate is blocked by shared
numeric exports without mappings/exemptions and the old safe-math round mapping.
These failures are reported to the orchestration owner; no venue mapping fails.
Merge named-step counts: 56 before and after, zero lost names.

## API pure helper slice

12 API rows mapped; family total 57 mapped / 174 pending. Read classification is
GET/HEAD or any method on a `/listenKey` suffix, passed into the shared onlyReads
mechanism by the later factory. Live tests exercise original TS functions, with
unselected I/O dependencies throwing if invoked. Private signing helpers retain
own-key and UTF-16 sorting, fallible String conversion, and prototype assignment.
Keeping `__proto__` instead of silently dropping it fails at exit 101 naming
buildV3SigningPayload; restored API oracle passes. Serial review returned MERGE.
Crate compiles with dependencies intact after response-bearing REST core merge;
workflow names 56 before, core parent 55, merged 56. Strict clippy passes.
V3 recoverable signing and EVM checksum validation remain routed shared blockers.

## Builder pure slice

17 additional rows mapped (9 approval, 8 fee): 74 mapped / 157 pending.
39 live tests, strict clippy and wasm build pass. Changing chainId 56 to 57
fails exit 101 naming buildAsterApproveBuilderTypedData; restored oracle passes.
Serial review found an unrepresented native Error message and incomplete taker
zero-rate fixtures. Both fixed; repeated review returned MERGE. Native messages
are passed separately before JSON erasure; maker/taker independently cover zero,
nonzero and the literal nonzero-suppressed string "0.0".

## Exchange and clock pure helpers

Seven more rows mapped: measured 81 ported / 150 pending. Live member extraction
checks five exchange decisions and two clock helpers; clippy and wasm pass.
Swallowing an open-interest Number error fails exit 101 by helper name. Review
also found a null-index assertion that masked unexpected errors; comparing the
whole tagged outcome fixes it, and an injected error on undefined input now
fails 101 by name. Restored tests pass. Private member manifest names use #;
all seven exact rows mapped and serial re-review returned MERGE.

## BigInt decimal and nonce slice

Six more rows mapped: measured 87 ported / 144 pending. Uses the registered shared
BigInt primitive. Decimal corpus includes 400-digit integers, 18/19-place fractions,
JS whitespace and invalid formats. Changing maker >= taker to > fails 101 naming
getAsterBuilderApprovalFeeRate; restored passes. Review caught thread-local nonce
state duplicating a signer nonce across native threads. A synchronized module map
and cross-thread wrapper regression fix it; serial re-review returned MERGE.
All 41 tests (including live oracles), clippy, wasm, extraction, global parity
check/report/progress pass. Integration now includes both math and BigInt glue
registrations; earlier global blockers are resolved. Merge checks retain 56 named
CI steps versus 55 on integration, with no names lost; dependencies intact and
layer audit green. Exemption union parses with no duplicate targets or blank reasons.

## Ticker/position mappers and algo limits

Four more rows mapped: measured 91 ported / 140 pending. Raw response extras are
retained in position/ticker info. The oracle tags nonfinite values and negative
zero before serialization, pins first-match realized PnL and first-filter errors.
Resetting preserved realized PnL fails exit 101 naming transformPosition; restored
passes. Tests, strict clippy, wasm, extraction and parity check pass. Serial review
returned MERGE with no findings.

At committed checkpoint 5188834689, all five oracle scripts also passed from a
tracked-source-only /private/tmp mirror with no node_modules in any ancestor.
Each missing-compiler control exited 1 with MODULE_NOT_FOUND; mirror deleted.
Runtime imports for selected functions are node:crypto and source-only helpers.
Mandatory Asterdex live tests execute in the TYPES job after root installation;
the native ordinary Cargo matrix discovers but ignores the live tests.

## Approval transport bridge

buildAsterApprovalRequest now uses the shared registry/private-proxy interfaces.
Live source traces prove early override bypass, header resolution and URL reads,
including header failures and empty-valued credentials. Treating empty-valued
headers as absent fails exit 101 naming buildAsterApprovalRequest; restored passes.
Serial review returned MERGE. Crate tests, clippy, wasm, extraction and parity pass.

## Pure account-event parsing

Four private-stream parser rows mapped without starting socket lifecycle work.
Measured family checkpoint: 96 ported / 135 pending. Live cases preserve lazy
clock reads, null errors, zero/negative values, all-or-nothing asset parsing and
nonfinite derived arithmetic. A positive-only guard fails exit 101 naming
parseFiniteNumber; restored passes. Serial review returned MERGE. All 43 tests,
strict clippy, wasm, extraction and global parity check pass.

## Account UPNL and algo capacity

The ordered-key UPNL extraction and capacity guard bring the measured count to
98 ported / 133 pending. Both pass live TypeScript fixtures and serial sub-agent
review (MERGE). Sorting currency keys changes the selected suffix and fails 101
naming getAccountUpdateUpnlDeltas; replacing strict capacity > with >= fails 101
naming assertAlgoOrderCapacity. Restored code passes all 43 scoped tests, clippy
and wasm. Shared Balance currency insertion order is routed before store wiring.

## Tick size and declared union mappings

101 ported / 130 pending after getTickSize, mapOrderType and mapTimeframe.
All legal union members and tick-size first-match/error/fallback cases match live
TypeScript. Presence instead of truthiness fails 101 naming getTickSize; restored
passes. Serial review MERGE; scoped oracle, strict clippy, wasm, extraction and
parity check passed. Open-string reverse mappings are not included in this slice.

## Auth readiness and endpoint selection

104 ported / 127 pending. canSign and both instance endpoint selectors use explicit
instance/runtime inputs without claiming the exchange lifecycle is implemented.
Live TS cases cover whitespace credentials, remote signing with empty keys, full
V3 auth and version overrides. Allowing remote signing to bypass an empty key
fails 101 naming canSign. Restored passes; serial review MERGE. Scoped oracle,
strict clippy, wasm, extraction and global parity check passed.

## Merge and oracle-control checkpoint

Merged integration35e1b03e7f: named CI steps56/62 ->63. Merged coref90f598708:
63/59 ->67, no missing parent names. Dependencies intact;250 unique exemptions,
zero blank reasons. Resolved additive CI/exemption/lockfile unions and relocated
transport prose/test helper; serial read-only review MERGE. Crate compilation,
all43 tests, global parity, layer audit and150 CI audit tests passed. Diff-check
flags incoming unified-patch context whitespace in crypto fixtures; left verbatim.

All seven live oracle scripts passed at aaa408bdfd in a tracked-source-only
/private/tmp mirror without ancestor node_modules; all seven missing-compiler
controls failed MODULE_NOT_FOUND, then mirror deleted. Permanent child endpoint
mutation control now fails by comparison name and mutated value, not merely child
exit. Replacing its mutation with a no-op fails101; restored all37 type tests pass.
Serial review MERGE. Family row count remains104/127.

## Inherited-member socket witnesses

104 ported / 127 pending, unchanged: witnesses alone do not implement Rust rows.
All twelve inherited names run through six live socket paths (72 cases). Public
batch errors are caught silently and stop later events; private errors are caught
and logged. Seven inherited combined-stream mapped types resolve to no handler; the five
underscore-prefixed names are truncated to an empty key before lookup and miss. Topic
allocation is suppressed after the subscribe send, and both order-status maps
forward the inherited value instead of Open. Pending rows name the shared
TableHit/ProtoMember blocker and ts-bug witnesses.

The new mandatory live test passes. Its in-memory null-prototype table mutation
fails on the named topic suppression assertion; replacing that mutation with a
no-op makes the Rust test fail at exit 101. Restoring the mutation passes.
Serial sub-agent review caught the underscore-truncation distinction; the fixed
oracle captures eventType and mappedType at the live lookup. Re-review: MERGE.

## Clock snapshot calculations

Four pure snapshot rows bring the count to108 ported /123 pending. Real Dayjs
matches7600 combinations: missing state, invalid dates, fractional inputs,
TimeClip limits, small offsets and the five-minute boundary. Replacing > with >=
fails101 naming needsSync; restored passes. Serial review caught possible ancestor
Dayjs fallback; absolute sibling package paths fix it, and re-review returned MERGE.
The missing-dependency control redirects the actual absolute Dayjs import and
requires MODULE_NOT_FOUND at that path. All nine live scripts passed in a tracked
source mirror without ancestor node_modules; all nine missing-compiler controls
failed. The corrected absolute-path clock oracle and Dayjs control were re-run in
a fresh mirror and passed; both mirrors were deleted.
Scoped47 tests, strict clippy, wasm compilation, extraction and global parity
passed. Layer/CI audit initially lacked cargo on PATH; corrected invocation passed
layer audit and155 CI tests. No workspace build or full workspace suite ran.

## Shared lookup and retry merges

Integration6eb262afc5 merged at44151f4754: CI68/66 ->71 with no missing parent
names,263 unique exemptions, dependencies intact, own crate compiled, layer audit
and169 CI tests passed; serial review MERGE. Retry260f3bcba4 merged at67c4fe2cdd:
CI71/59 ->71,265 unique nonblank exemptions after additive union, dependencies
intact, crate compilation and layer audit passed; serial review MERGE.

## Reverse order status

Shared TableHit preserves all twelve inherited values and the missing-only Open
fallback. Live source captures typeof before JSON erases function-valued fields;
__proto__ remains an object and serializes differently. Substituting Open for an
inherited hit fails101 naming reverseMapOrderStatus and constructor; restored
passes. The UTF-8-only divergence cites the canonical shared declaration, with
valid-JSON lone-surrogate refusal and TS acceptance pinned separately.
Review caught an oracle identity label derived from the input rather than the
actual returned function. Fixed by matching actual prototype member identity;
a permanent swapped-function child control requires Constructor versus ToString
comparison failure. Making that control a no-op fails101; restored passes.
Re-review MERGE. Scoped48 tests, strict clippy, wasm, extraction and global parity
passed. Measured108 ported /1 UTF-8-only divergent /122 pending.

## Ethereum merge and builder address/config wrappers

Owned preparation2ee2ce679e merged ate1d0342b68: CI71/59 ->71, dependencies
intact, crate check and layer audit passed; serial review MERGE. Integration
35b7ec85f9 merged atd18874fd3f: CI71/70 ->71;265 unique exemptions; compile,
layer audit,34 extractor tests and169 CI audit tests passed; serial review MERGE.
Ethereum82877523a3 merged at2fdd884217: CI71/70 ->71; crate compiled, dependencies
intact, layer/parity and175 CI tests passed; serial review MERGE. Two incoming
crypto mappings explicitly cover primitives only; their venue wrappers remain
unimplemented and are not claimed complete.

Address/config wrappers match1024 live ethers cases, including oversized ICAP
reference behavior. Moving address resolution before the zero-fee return fails101
naming resolveAsterBuilderFeeConfig; restored passes. Two valid-JSON lone-surrogate
witnesses retain TS behavior and Rust representation refusal. A tracked-source
mirror without ancestor node_modules passed the actual ethers oracle; missing
compiler and actual ethers path controls failed MODULE_NOT_FOUND; mirror deleted.

Serial review returned MERGE. All49 scoped tests (including mandatory live oracles),
strict Clippy, wasm compilation, extractor drift check and global parity passed.
Measured108 ported /5 divergent /118 pending; two of the divergent entries are
incoming primitive-only crypto coverage, not completed venue wrappers.

## Reverse type and filter extraction

Serial review returned MERGE for unknown-string passthrough, all twelve inherited
identities, first-match termination, falsy fallback and null-element errors.
The live oracle covers28 reverse types and1440 filter cases (including own shadows),
with double-escaped lone-surrogate acceptance/refusal witnesses. Both rows cite
only the shared UTF-8 boundary; the closed Order.type consumer remains pending.
Replacing inherited filter reads with missing/zero compiled and failed101 naming
extractFilterValue/constructor; restored passes. The permanent live TS own-only
mutation likewise must fail101 by helper name.
Replacing unknown order types with Limit compiled and failed101 naming
reverseMapOrderType/alien; restored before final gates.
All50 scoped tests, strict Clippy, wasm compilation, extractor drift and global
parity passed. The changed oracle passed a tracked-source-only no-ancestor-module
mirror and missing-compiler control; mirror deleted. Measured108 ported /7
divergent /116 pending (two incoming divergent rows remain primitive-only).

## Consolidated integration merge

Merged integration d8bcccd413 (contains590f1a4d7e core and later member coercion).
CI71/71 ->72 with zero missing parent occurrences. Exempt265/273 ->273, no
blank reasons or duplicates; shared number-string reason uses incoming equivalent
wording. Crate dependencies and ethers selector retained. A clean lock hunk lost
ten incoming Windows packages required by ring: offline scoped repair restored
exactly the parent package-identity union, with no new identities or upgrades.
An initial local conflict-removal script truncated file tails; reconstructed from
both parents before validation. Extractor validation then caught a lost json-parse
root; restored and mechanically verified all nine parent roots. Corrected tree
passed --locked scoped compile, layer audit,35 extractor tests and180 CI tests.
Serial merge-integrity review returned MERGE, independently checking tails, test
names, roots, lock union, exemptions and Aster-owned state. No workspace build ran.

## Wasm lint repair base

Integration9222ed2454 merged without textual conflicts: CI72/73 ->74, every
parent occurrence retained; exempt.toml parsed280 unique nonblank entries.
Own locked compilation and layer audit passed. HOST strict Clippy passed.
Separately, WASM strict Clippy passed with --target wasm32-unknown-unknown
--all-targets -- -D warnings (no --no-deps). CI wiring188 tests passed.
Clock lifecycle is uncommitted/stashed pending the routed shared interval seam;
its review exposed a fixed-grid oracle assumption, corrected by actual Node
getLibuvNow-before-callback measurement. No clock progress is counted yet.
Serial merge-integrity review returned MERGE; no parent losses found.

## Canonical snapshot declaration merge

Merged935fc8f578 at36645f2887. CI named steps74/74 ->75 retain both parent
occurrence multisets; real tomllib parse284 blocks /284 distinct exemption
targets, zero blank reasons. Own dependencies unchanged. Scoped locked compile,
layer audit and193 CI wiring tests pass. The initial CI invocation lacked the
rustup PATH and failed before tests; rerun with explicit PATH passed end to end.
Serial merge review returned MERGE, with no parent loss.

## Market mapper and metadata boundary audit

Live source895 market cases pin fractional/negative precision, underflow,
nonfinite/falsy leverage, first filter wins, unknown root/filter info properties,
and Unicode. Two market and four ticker/position double-escaped valid JSON lone
surrogate witnesses contrast TS preservation with Rust ingress refusal; scalar,
astral and whitespace strings remain in the positive corpus.
Two market and six ticker/position cases tag nonfinite info BEFORE serialization;
three alias witnesses record source-to-info and info-to-source mutation, while
Rust snapshots isolate both directions. These are separate FINDING and CHOSEN
causes, not a claim of forced loss or serialized convergence. Retroactively
reclassified ticker/position from ported to divergent.
Rust precision substitution with math_pow compiled and failed101 by
transformMarket name; restored. Permanent TS nullish-fallback mutation fails101
by helper name. Review found the nonfinite corpus originally asserted a hardcoded
precision and ignored live derived output. Replaced with complete tagged-output
comparison, normalizing only the declared info field loss; a permanent TS
nonfinite precision correction now fails101 at transformMarket nonfinite full
output. Five exchange tests pass. Tracked-source-only no-ancestor-node_modules
mirror ran895 market,2 nonfinite market,6 nonfinite metadata,3 alias and12 string
cases; missing compiler failed MODULE_NOT_FOUND. Mirror deleted afterward.

Follow-up review caught alias observations after JSON conversion with a detached
source string. Replaced those with direct typed source/output observations and
mutation of output.info while both objects remain alive. Final52 scoped tests,
HOST strict Clippy and separate WASM strict Clippy pass. Extractor reports zero
added/orphaned rows; global parity passes6984 rows/1408 public items. Counts:
106 ported /10 divergent /115 pending, including two incoming primitive-only
crypto rows that still do not claim venue wrapper completion.
Serial final review returned MERGE after both oracle findings were repaired.

## Account position mode extraction

Both declared boolean inputs match the actual TypeScript method. Inverting the
Rust condition compiled and failed101 naming getAccountPositionMode; restored.
A permanent independent TS inversion child control also requires101 by name.
All53 scoped tests pass, including all mandatory live oracles.
Host and wasm strict Clippy pass separately; extractor reports zero drift.
Parity initially rejected a missing exact-target rename note; corrected note and
rerun pass6984 rows/1409 public items. Isolated tracked-source oracle and missing
compiler control pass; mirror deleted. Serial review returned MERGE. Measured
107 ported /10 divergent /114 pending.

## Order mapper

377 normal live-source cases compare complete tagged results.28 refused outputs
cover all twelve prototype names in type/status plus four unknown type strings.
The witnesses record typeof/member-specific coercion before JSON; functions omit
only their field, __proto__ survives as {}, unknown strings survive unchanged,
and TS retains the whole record where Rust refuses it.16 numeric cases cover
orderId/time/updateTime/workingTime with NaN, both infinities and negative zero;
only the declared nonfinite info loss is normalized, never derived outputs.
Six string cases pin Unicode acceptance and valid-JSON lone-surrogate refusal.
A live-reference witness checks both mutation directions against typed Rust objects.
Order wire extras now survive in info through the same strict-map flattened
extras mechanism used by the other mappers. No shared enum was widened.
Independent Rust and TS zero-stop-price substitutions compile and fail101 naming
transformOrder. Restored before final checks. The changed oracle ran in a
tracked-source mirror without node_modules ancestors; missing compiler failed
MODULE_NOT_FOUND, mirror deleted. Serial review returned MERGE.
Final55 scoped tests pass; HOST and separate WASM strict Clippy pass. Extractor
zero drift and global parity6984 rows/1410 public items pass. Measured107 ported /
11 divergent /113 pending. No workspace build or full workspace suite ran.

## Merge-up preparation on canonical integration

Numeric Store ABI2b96611e8f merged at716b826e13 after serial MERGE review:
CI75/76 ->77, parsed299 exemption blocks /299 distinct, parent lock package
identities retained, own dependencies unchanged, locked compile/layer/195CI tests
passed. The only conflict joined the AsterDEX and WOO CI oracle assertions.
Then merged verified origin ancestor91db928805 for Axios header/body metadata.
CI77/76 ->77 preserves every parent occurrence. Real tomllib parse301 exemption
blocks /301 distinct targets, no blank reasons. No parent lock identity lost;
AsterDEX Cargo dependencies unchanged. On this merged tree:55 scoped tests with
all live tests included; HOST strict Clippy and separate WASM strict Clippy;
extractor zero drift; parity check, report and progress; layer audit;195CI wiring
tests all pass. No workspace build or workspace suite ran. Counts remain107
ported /11 divergent /113 pending. Clock interval WIP remains held and uncounted.
Final adversarial merged-state review returned MERGE; code/manifests preserve
reviewed1af9ddd237 exactly, with no CI/lock/selector/exception losses.

## Value-first order ingress witnesses

Actual JSON.parse of1e400/-1e400 across orderId/time/updateTime/workingTime
produces eight overflow cases. serde_json::Value then from_value preserves their
native infinities; the complete tagged mapper result compares live with only the
separate declared info-null loss normalized. A permanent direct typed from_str
substitution compiles and fails101 naming transformOrder Value-first ingress and
number out of range. All nine exchange oracle tests pass. No numeric shim or
production behavior changed; manifest counts remain107/11/113.
Serial review returned MERGE. Host strict Clippy passes. Tracked-source isolated
oracle reports all eight cases, missing compiler fails MODULE_NOT_FOUND, mirror
deleted. This follow-up changes only tests/docs; production merged-state wasm
coverage remains the separately recorded run at c4aac4c88d.

## V3 feature guard

16 live version/feature cases and two lone-surrogate witnesses compare the actual
TS guard plus BaseExchange emitter with shared Rust reject_preflight. The initial
emitter stub missed the event prefix; replacing it with the actual source exposed
and resolved that oracle gap. Rust version inversion and TS returning an Error
instead of throwing both compile then fail101 by assertV3Feature. An isolated
tracked-source mirror passed, missing compiler failed MODULE_NOT_FOUND, and was
deleted. Serial adversarial review returned MERGE. Runtime class wiring remains
pending. Own Cargo lock adds only exchange and test-only store dependencies.
Full scoped suite:58 tests pass, including ignored live tests. Host and separate
wasm strict Clippy pass; extractor has zero additions/orphans; layer audit passes.
Measured107 ported /12 divergent /112 pending.

## Signed-zero ingress amendment

The four order fields now each compare native f64 bits against live Node JSON.parse
of -0, separately from raw text and an already-constructed Value. All eight Rust
paths preserve8000000000000000; all nine exchange oracle tests pass, including
overflow. No production decoder was converted or reverted. This claim is scoped
to the current Aster order decoder and does not generalize to other serde shapes.

## Consolidation re-merge on6bcff79504

KuCoin/grvt integration moved after c4aac4c88d readiness. The additive conflict in
package scripts and CI assertions retains both venues unchanged; identical moved
shared assertions appear once. CI203 tests pass. Named steps77/78 become79, with
all parent occurrences retained. All85 root scripts retain parent values. A real
TOML parse reports303 exemption blocks and303 distinct targets, no blank reasons
or parent losses. Lock package identities from both parents survive. Own Cargo
dependencies and ethers selector remain; layer audit and locked own all-targets
compile pass. Prior scoped tests, separate host/wasm lints, extractor and parity
evidence stand; guard follow-up adds58-test/lint evidence above. No workspace
build or full suite ran. Counts107 ported /12 divergent /112 pending.
Adversarial merged-state review returned MERGE: parent scripts/tests, exemptions,
lock identities and selectors retained; Aster code/manifests unchanged by merge.

## Existing numeric decoder census

68 numeric scalar fields across22 shapes, representing18 TypeScript rows, now
compare native f64 bits with live Node JSON.parse for -0/-0.0 and both overflow
signs. Borrowed and owned parsed Values, raw typed JSON, and native Value(-0.0)
are distinct paths.62 fields preserve parsed-Value bits; six flattened placement
option fields lose integer-lexeme -0 but preserve -0.0 and native negative zero.
Those six reject raw negative-zero literals and parsed-Value overflow. All68
reject raw typed overflow, where Node accepts. Eighteen formerly ported type rows
are now FINDING: measured89 ported /30 divergent /112 pending. No production
decoder changed. A temporary Rust pricePrecision +0.0 mutation compiled and
failed101 naming AsterDEXMarketResponse.pricePrecision owned lexical -0; restored.

The flattened placement-options raw refusal also includes finite fractional1.25.
Each numeric field is isolated against an all-integer baseline required to decode
successfully first, preventing an unrelated fractional field from masking it.
The final39-test live-types suite and host strict Clippy pass. Extractor reports
zero additions/orphans. Parity initially caught nested target names lost from the
reclassification notes; exact target references were restored and parity passes.
Serial review returned MERGE. Isolated tracked-source oracle reports22 shapes and
68 fields; missing compiler fails MODULE_NOT_FOUND; mirror deleted. Production
code is unchanged, so no new wasm behavior is claimed beyond the prior separate
strict wasm run. The permanent erased-zero oracle control fails101 by field name.

## Consolidated base b8fc4dfbaa

The shared currency-ordering/core-interceptor merge retains own source and
manifests unchanged.60 scoped tests pass, host and wasm strict Clippy pass
separately,217 CI tests pass, extractor reports0 added/0 orphaned, and parity
passes7334 rows/2170 public Rust items. Layer audit passes. CI79/82 becomes82
with both occurrence multisets retained. Real TOML parse312 blocks/312 distinct,
no blank reasons or parent losses; all parent lock identities and own dependencies
remain. Counts89 ported /30 divergent /112 pending.
Serial merged-state adversarial review returned MERGE; own numeric FINDINGs
and parent scripts/gates/lock/exemptions retained.

## Balance currency-order repair consumed

Four live cases use actual TypeScript DefaultStore constructor, clone, update and
balance getter, then the actual UPNL helper. Rust uses production DefaultStore
through Store::update and Balance.currencies.keys. USDT-before-DT now selects
USDT for BTCUSDT; the former BTreeMap representation selects DT. Canonical numeric
keys are retained too. A permanent sorted-key substitution compiles then fails101
naming getAccountUpdateUpnlDeltas production store first margin asset. No private
store replica or local ordered map was added. Counts remain89/30/112; this clears
the store-key blocker for later socket wiring rather than claiming runtime done.
All three account oracle tests and host strict Clippy pass. Serial review
returned MERGE. Isolated tracked-source actual-store oracle passes all four cases;
missing compiler fails MODULE_NOT_FOUND, mirror deleted.

## State-version helpers

Four exchange state-version helpers now use explicit binary64 state inputs. Eleven
live seed values cover ±0, fractions, signed values, Number.MAX_SAFE_INTEGER,
2^53, large magnitude and infinities/NaN; eight sequential calls per seed assert
return identity and both independent counters before serialization. A TS mutation
that returns the old balance version compiles and fails101 by helper name.
Reconciliation/lifecycle remain pending.
Parity reports7334 rows/2174 public Rust items after the method-target note fix.
Layer audit passes. The complete isolated oracle returned11 state seeds, the
source postincrement mutation changed those rows, missing compiler failed
MODULE_NOT_FOUND, and the mirror was deleted.

## Shared numeric ingress opt-in for flattened order options

The actual `AsterDEXPlaceOrderOpts` flattened deserializer passes the shared
`js_from_str`, `js_from_value` and `js_from_value_ref` paths for six fields. Live
Node's f64-bit corpus covers -0, -0.0, 1, 1.25 and both overflow signs for each
field. The source-side signed-zero mutation fails the named `amount raw -0 bits`
assertion; a temporary compiled Rust mutation fails the shared comparator at
`$.amount`. Ordinary Serde decoding remains unchanged, so its raw-overflow and
flattened-rebuffer finding remains open. The other numeric record shapes have
not yet opted in. Exhaustive destructuring of both venue-owned AsterDEX options
and shared PlaceOrderOpts makes shared field additions a compile-time review
point.

The nested `AsterDEXAccountResponse` graph is the second opt-in shape. Its
feeTier, updateTime, asset updateTime and position updateTime fields preserve the
six f64 bit cases from raw, owned-Value and borrowed-Value paths; the source
negative-zero control and a temporary compiled decoder mutation both fail by
field path. This does not alter the ordinary Serde path or close its raw-overflow
finding. The open-interest, trade and user-trade type records also pass the
shared raw, owned-Value and borrowed-Value bit corpus for their six scalar
fields; source and compiled Rust mutations fail at `$.id`/named field paths.
Sixteen of the 68 audited fields now have shared ingress acceptance; the
remaining 52 are still measured by the existing census only.

`fetchStpMode` response validation is extracted to
`parse_stp_mode_response`. The actual TypeScript V3 method oracle pins the
`/fapi/v3/stpMode` request, all accepted strings, invalid/missing responses and
the emitted error. A source-side mode mutation fails by helper name; a compiled
Rust mutation changes `EXPIRE_TAKER` to `EXPIRE_MAKER` and fails the named
`fetchStpMode response` assertion with exit 101.

## Aster code-message EIP-712 signature

`generate_aster_dex_code_message_signature` builds the actual AsterSignTransaction
domain and Message struct, then delegates to the shared EIP-712 signer. The
isolated live ethers 6.17 oracle compares exact recoverable signature strings
for both chain IDs, empty and Unicode query strings. Its source mutation swaps
the chain ID and fails by helper name; a temporary compiled Rust chain-ID swap
also compiled and failed by the same named assertion (exit 101), then was
restored. Zero and malformed private keys are included as error witnesses:
ethers distinguishes Error from TypeError and its malformed-key TypeError
contains the supplied string; the Rust wrapper refuses through shared crypto
and keeps that key out of its error. Error identity remains a shared finding,
and redaction is retained for security.

## Bulk and per-symbol open-interest reads

`fetch_open_interest_map` now executes through the shared RestAdapter. Its live
TypeScript method oracle pins the endpoint and 2000ms timeout, successful bulk
short-circuit, empty bulk fallback gating, warning and sequential per-symbol
requests after bulk failure, falsey-symbol filtering, silent per-symbol failures,
duplicate updates, and partial results when a later `Number` coercion throws.
Empty response text remains a non-object as in Axios's silent JSON transform.
The source control removes the nonempty-map short-circuit and fails at the named
request assertion. A temporary compiled Rust mutation removing the same guard
compiled and failed at that assertion, then was restored. Store application and
polling/timer lifecycle remain separate rows. The valid escaped lone-surrogate
symbol witness captures `typeof` and Map size before serialization: TypeScript
retains the key, while the Rust UTF-8 response decode cannot represent it and
returns no entry. This is recorded as the shared UTF-8 boundary divergence, not
claimed as parity.

## Public combined-stream dispatch

`public_stream_dispatch` pins the actual `handleStreamData` routes for ticker,
trade, mark-price, kline, bare depth and partial depth streams, including the
source's strict partial-depth suffix and conditional symbol injection. Its live
whole-path prototype witness confirms inherited event-map values pass the guard
but resolve to no own handler, preserving the silent frame drop. A valid escaped
lone-surrogate stream is retained by TypeScript and reaches the ticker handler;
the Rust JSON UTF-8 ingress refuses it, with `typeof` captured before
serialization. The source own-key mutation exits 1 at
`handleStreamData inherited lookup preservation constructor`; a compiled Rust
mutation swapping the ticker and aggTrade dispatch values compiled and failed
at `AsterDEXPublicWebsocket#handleStreamData handler route "btc@ticker"`;
restored focused witness passed. Its generated stream cases have no identical
same-typed sibling fields. The escaped-surrogate source witness remains separate
from those UTF-8-representable dispatch cases. The private listen-key-expired
helper also asserts warning, refresh clear, watchdog clear and reconnect order;
swapping the two clear calls compiled and failed at its named ordered-effects
assertion, then restoration passed.

## Fixture identity and numeric-bit capture audit

Executed all 26 runnable AsterDEX oracle scripts and recursively scanned their
JSON outputs for sibling fields with equal values and equal JSON types. Seventeen
outputs contain such pairs; the scan is a candidate inventory, not by itself a
swap-blind verdict, because several are wrapper metadata or have other cases
that distinguish the fields. The record and builder pair audit examined 1,215
same-shape sibling pairs: 1,206 in 39 type schemas (27 object schemas and 12
enum-like schemas) and 9 in four builder schemas. Every pair has an accepted
case with distinct values. The oracle now generates path-distinct strings,
alternating booleans and distinct numeric values; position amount, entry price,
notional, unrealized profit and liquidation price are also differentiated in
the exchange corpus, as are historical-fill buyer/maker flags and builder
key/secret values.

All AsterDEX oracle artifacts are consumed only by tests in this crate; none
feeds a shared numeric corpus, another venue, or another crate. The local
artifacts were therefore repaired in place. Rust test consumers invoke Node
once per corpus and cache the resulting JSON for case iteration; the audit
found zero Node invocations inside per-case loops. No shared-feeder artifact
was regenerated, and no retroactive independence claim is made.

All 23 top-level type contract cases now assert each decoded native field,
tuple element, or enum variant directly before serialization; the four builder
contract schemas do the same. The field-name-based checks cover all 39 type
schemas in `live_types.rs`, including nested records. The tuple case now checks
all 12 decoded indices, and enum identity matching fails closed for an
unclassified accepted value. A compiling consumer mutation swapped distinct
same-typed native values in all 18 of the 23 cases that have sibling pairs;
each failed by its named field or tuple assertion. The other five have no
same-typed sibling pair (three enums and two single-field records), so the
control changed their enum variant or sole decoded value instead. The separate
PlaceOrder contract also failed under its field swap. Thus all 23 requested
cases changed verdict under a compiling identity mutation, and 18/23 did so
under literal sibling-field swaps. The restored full `live_types` target
passed 49/49, including nested records. A symmetric serde-name mutation
swapping `positionAmt` and `entryPrice` on both Serialize and Deserialize also
compiled and failed at `AsterDEXPositionResponse.positionAmt native field
identity`; before direct checking, the same mutation passed the re-encoded
roundtrip assertion. Other integration gates assert helper results, effects,
requests or dispatch outputs directly rather than relying only on re-encoded
bytes.

The exact-bit capture audit found NaN-capable payloads in three local oracles:
historical fills, OHLCV and balance. Their capture helpers canonicalize only
NaN to `7ff8000000000000`; finite payloads, infinities and signed zero stay
bit-exact. These are local, crate-only artifacts, so canonicalization was
applied at capture in place. The schema numeric corpus contains finite values,
infinities and signed zero but no NaN; the exchange oracle's only bit-exact
capture is signed-zero ingress, while its other non-finite outputs use semantic
markers rather than NaN payload bits. The clock oracle compares NaNs by
`is_nan` equivalence and retains bit-exact signed-zero checks. Each exact-bit
capture includes its `captureArchitecture` (`${platform}-${arch}`), at the
case or corpus level.

The numeric oracle's negative-zero mutation fails at
`AsterDEXPlaceOrderOpts.amount raw -0 bits`; its finite-payload mutation fails
at `AsterDEXPlaceOrderOpts.amount raw 1.25 bits`. The restored controls pass.
The scoped `live_types` target passed 48 of 49 tests before the expanded
generated-schema inventory was updated; the inventory assertion then passed
in isolation, and the `live_builder::builder_helpers_match_typescript` test
passed. The symmetric swap mutation failed by the named native-field assertion
and the restored baseline passed. No Yarn gate was run through the Volta shim.

Assertion-shape census: the crate owns 11 integration-test binaries and 110
integration `#[test]` cases at this count. The roundtrip-only identity
gates in the two targeted binaries are now converted to direct decoded-value
assertions; decode/encode equality remains as an additional structural check.

The public `handleKlineUpdate` corpus executes the extracted live TypeScript
method in one Node subprocess per corpus and compares its selected callback
stream, timestamp and five candle fields directly. Its all-fields case uses
distinct OHLCV values; the source close/open mutation and a temporary compiling
Rust close/open mutation both fail at `handleKlineUpdate field identity
"all-fields"`. The corpus header records architecture and Node major; only NaN
is canonicalized, while signed zero and finite values remain bit-exact. The
signed-zero capture mutation fails at `handleKlineUpdate field identity
"signed-zero-and-nan"`, and the finite capture mutation fails at the named
`"all-fields"` identity assertion. The corpus is crate-local and feeds no gate
outside this venue.

Precision audit, per field: `transform_market` maps `price_precision` to
`Market.precision.price` and `quantity_precision` to
`Market.precision.amount`; this is metadata mapping, not an order formatter.
The compiled price-input-to-amount-output mutation failed at
`transformMarket native price precision identity`; the compiled
quantity-input-to-price-output mutation failed at
`transformMarket native amount precision identity`. Each was restored and the
focused target passed. These two metadata-field controls are ALREADY COVERED
NOW; this report does not claim the earlier corpus was never blind. No fixture
values changed and the source artifact remains byte-identical.

Formatter census: `get_tick_size` reads only the `PRICE_FILTER.tickSize` field;
there is no implemented Rust order formatter or paired price/amount adjustment
site. The pending TypeScript `placeOrder` uses amount precision for split and
quantity adjustments and `getTickSize` for limit/stop prices. It is a valid
negative for currently implemented Rust formatters, not a swap-proof claim.
The pending order rows inventory those call sites and require fractional,
distinct-step cases, both sides, each order type, direct native step and
payload assertions, and per-field compiling swap controls before Rust targets
are added.

## Collection cardinality audit

Static census found five array-pair walks in the AsterDEX test sources: recursive
array equality in `live_types`, the Kline tuple comparison, recursive array
equality in `live_account_helpers` and `live_exchange_helpers`, and the
`fetchPublicTrades` source/output walk. The recursive comparators check lengths
before pairing; Kline pins tuple length; public trades pins source and output
totals and now independently asserts native output count equals TypeScript
output length. State-version comparisons now use fixed `11 x 8` loop bounds,
pin each source count, and assert 88 native steps and 264 comparisons rather
than deriving their loop bounds from the oracle.

For type records, all 49 `check_record` contracts pin source and accepted-case
totals, native comparison totals, and declared native field inventories. The
recursive direct-field assertion checks every nested array length before value
comparison. Fourteen collection fields across six schemas also have independent
source and native element totals: market subtype/filter/order/time-in-force,
account assets/positions, depth bids/asks, stream depth sides, and both account
update balance/position arrays.

The production decoder mutation consumed every element of the 12 `Vec` fields
across those six schemas and then returned an empty vector. The five containing
model targets failed at named direct assertions: `AsterDEXAccountResponse.assets
native array length`, `AsterDEXAccountUpdate.a.B native array length`,
`AsterDEXDepthResponse.bids native array length`,
`AsterDEXMarketResponse.underlyingSubType native array length`, and
`AsterDEXStreamDepth.b native array length`. A second consuming-empty control
removed both the recursive array-length guard and the generic semantic-equality
length guard; the independent fixed total still failed at
`AsterDEXAccountResponse.assets fixed native collection element count`. The
mutations were restored, and the focused 24-case `live_types` contract target
passed. The focused public-trades and fixed-bound state-version targets passed
3/3. This is a measured census and targeted mutation proof; it does not claim
every array walk in all 110 crate tests has been subjected to all six
cardinality mutations.

The public depth snapshot slice pins 15 source cases in one Node invocation,
including source callback counts, callback snapshot isolation, ticker presence,
malformed levels, and zero-quantity rejection. Rust helper matching has a fixed
15-case count and compares its returned book against four source callback
payloads. The fixed source inventory expects five callback payloads, and a
counter incremented inside the Rust comparison loop must reach that total. The
per-case source callback assertion is an independent source-cardinality guard;
the former `native_callbacks == 4` assertion was mislabeled and had the wrong
expected total. An oracle-output mutation empties the `fractional-levels`
callback list while bypassing the per-case guard; it exits 101 at
`handleDepthSnapshot fixed callback comparison count`, and the restored
baseline passes. The five callback events are counted on the TypeScript source
side only; Rust does not execute callback effects. The compiling source
mutation that permits zero quantity exits 101 at
`handleDepthSnapshot source callback count "zero-quantity"`; the restored
baseline passes. Rust currently implements only the pure depth-book helper, so
the manifest remains divergent for the missing Store ticker patch and
subscription callback effects. The expanded
`live_types` target pins and checks the 15 nested record and enum corpora; all
49 cases pass with direct decoded-value identity assertions.
