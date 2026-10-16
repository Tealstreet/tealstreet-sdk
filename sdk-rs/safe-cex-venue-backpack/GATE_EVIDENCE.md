# Backpack table slice evidence

Base: integration `a3d514bec82d6760d4e9e7a27186f992d175c36d`, merged into
`feat/rust-venue-backpack` at `66a21867ac1738b3f5dab02642017db0fe6692f1`.
Read all six venue TypeScript files end to end (3,562 lines) before implementation.

Scope: six exported tables in `backpack.types.ts`, not the record declarations,
REST adapter, signing composition, or WebSockets. The two pre-existing shared
crypto divergence rows were not changed. No shared feeder changed.

Python 3.14 `tomllib` parsed all six Backpack manifests:
- Before: total 147; ported 0, divergent 2, pending 145.
- After: total 147; ported 6, divergent 2, pending 139.

Witness: `actual_typescript_tables_match_per_key_and_order` executes the actual
TypeScript module through `constants_oracle.cjs`, using the required
`SAFE_CEX_TYPESCRIPT_PATH`. It compares six exports, including key order and
per-key values; the oracle asserts string types before JSON serialization.
There are no numeric values in this slice. No interface AST census is claimed.

Six deliberate Rust mutations were each compiled and tested: alter one value
in BASE_URLS, WSS_URLS, ENDPOINTS, INSTRUCTIONS; reverse each Grafana list. Every
mutation failed `source table <NAME>: per-key value and order` in the named
witness. Restoring the source passed 1 test. Removing the compiler environment
variable failed `SAFE_CEX_TYPESCRIPT_PATH is required`.

Gates (all Cargo invocations use `--jobs 1`):
- `cargo test -p safe-cex-venue-backpack`: 1 integration test passed.
- Host `cargo clippy -p safe-cex-venue-backpack --all-targets -- -D warnings`: passed.
- wasm `cargo clippy -p safe-cex-venue-backpack --target wasm32-unknown-unknown --all-targets -- -D warnings`: passed (compile/lint only).
- Layer audit: actual locked Cargo metadata for this crate + `layers.json`:
  venue registered; 0 runtime dependency edges; 1 registry dev dependency,
  `serde_json`. Exact dependency census asserted.
- Scoped parity invention/status: passed, 34 types-manifest rows and 6 public
  Rust items. The official parity-check binary indexed an isolated copy of this
  crate source plus the types manifest, with no exemptions; it did not index
  unrelated venues or claim to revalidate the existing shared crypto rows.
- Shared `parity/exempt.toml`: Python 3.14 tomllib measured BLOCKS 338,
  DISTINCT rs 338. No exemptions added or edited.
- Extractor drift: BLOCKED loading `smol-toml` from the existing dependency
  installation (`ERR_MODULE_NOT_FOUND`). No fallback extractor, dependency
  installation or shared-file workaround performed. This gate has not passed.

No integration merge, PR creation, live venue request, browser execution or
REST/WebSocket parity claim is made by this slice.


## Record declaration slice (after e7afc4acf8)

Scope: all 28 interfaces in `backpack.types.ts`, represented by 59 explicit
records and named inline unions. No REST/write/WebSocket method is claimed.
Runtime adapter acceptance of malformed/partial responses is a separate future
witness obligation; declaration shape checks do not establish that policy.

Python 3.14 tomllib measured all six Backpack manifests: total 147;
ported 7, divergent 29, pending 111. Of the 28 new mappings, BackpackTime is
ported and 27 cite CHOSEN `MANIFEST-SPEC.md#shared-utf8-string-boundary`.
The two original signing primitive divergences remain untouched.

`actual_typescript_records_match_decoded_fields_and_bits` runs the source schema
census and compiles typed fixtures against the actual TypeScript declarations,
including expected type errors, before executing the resulting module. Interfaces
are erased: this witnesses their declared shapes, not invented runtime validators.
The live oracle has 1,545 cases: 318 accepted and 1,227 refused, including 444
lone-surrogate cases. Each runs through raw, owned and borrowed numeric ingress
(4,635 route/case executions). The generic response also carries retained
JsNumberInput subsequently decoded as BackpackTime. Native fields and numeric
bits are asserted separately from output serialization. Wire comparison applies
the existing shared js_json_stringify projection to Serde output and compares
actual JavaScript JSON.stringify; no venue serializer is introduced. String
scalar/astral positives accompany typeof/code-unit captures for refused surrogates.

The local frozen schema has a permanent inherited-required-field control and
fails on unhandled AST types, missing heritage declarations and ambiguous inherited
field names. It is not a shared feeder and no existing shared fixture changed.
Source census and numeric output macros cover earlier table/type rows at this
slice boundary; the old signing primitive mappings have not been re-audited.

Adversarial sub-agent review found three witness gaps (serialization, nested
refusals, invalid literals). All three were addressed and re-reviewed with zero
new implementation findings. The following production mutations were actually
compiled/run, observed failing, restored and observed passing:

- Serialize Spot as PERP: `JSON.stringify wire BackpackMarket full`.
- Symmetrically rename p/P in the wire companion: `decoded native fields/bits BackpackWsTickerUpdate full`.
- Default a missing nested imfFunction.base: `typed/UTF-8 refusal BackpackMarket missing imfFunction.base`.
- Accept unknown marketType as Spot: `typed/UTF-8 refusal BackpackMarket literal refusal marketType`.
- Make required nullable subaccountId optional: `typed/UTF-8 refusal BackpackPosition missing subaccountId`.
- Normalize serverTime signed zero: accepted-case assertion `BackpackTime serverTime -0`.

Removing heritage traversal failed `inherited required field control`; adding a
required field to the actual source failed `complete source field census`.
Both source mutations were restored and the oracle passed again.

Host Clippy initially rejected 40 unnecessary-borrow/redundant-closure sites in
native probe code. Those were simplified; strict host Clippy then passed.
Scoped official parity-check passed 34 types-manifest rows /65 public Rust items,
using the isolated exact crate-source copy and no exemptions. Extractor drift
remains BLOCKED by missing smol-toml; this is not a passed extractor gate.
All commands used explicit Cargo or Node entrypoints; no Yarn shim was invoked.


Record-slice final scoped gates on integration a3d514bec8 merged state:
- Full scoped cargo test: 2 passed, 0 failed (table witness + record witness).
- Host and wasm Clippy separately: passed, --all-targets --jobs 1, warnings denied.
- Dependency/layer audit via locked Cargo metadata: exactly safe-cex-num and
  safe-cex-types workspace edges, both authorized venue grants with exact package
  paths; serde is the sole registry runtime dependency; serde_json the sole
  registry dev dependency. No shared layer policy change.
- Test harness process count: one Node subprocess for the complete 1,545-case
  record corpus, one for the six-table corpus. No per-case subprocesses.
- In accordance with the machine load policy, subsequent Cargo invocations are
  sequential; the full scoped suite is used at this slice's merge-up checkpoint.

## Pure API helper slice (after af0914c790)

Integration ancestor remains a3d514bec82d6760d4e9e7a27186f992d175c36d.
Python 3.14 tomllib: total 147; ported 8, divergent 33, pending 106.
Five new rows: resolveBackpackAuthWindow, isRecord, collectAuthParams,
isPublicEndpoint, getInstruction. createAPI remains pending.

The actual source module executes once for the entire helper corpus: 101 window,
30 classification, 900 collection, 172 public URL and 1,548 instruction cases
(2,751 total), plus alias, host-value and UTF-16 boundary controls. Captures retain
native typeof/f64 bits and object order before serialization. Ordinary Unicode
controls accompany escaped surrogate primitive/key/value and URL refusals.
Actual source surrogate collection output keys/value code units are asserted,
not merely input or output classification. Rust native object order is observed
directly; the witness does not sort it again. Snapshot isolation is observed by
mutating the original nested input in place and comparing source after-state.

Six compiled production mutations each failed the following named assertion,
then passed the scoped api_helpers target after restoring production source:
- Default window 60000 to 5000: resolveBackpackAuthWindow bits None.
- Arrays classified as records: isRecord typeof (empty array input).
- Reverse query/body spread: collectAuthParams native values/order case 621.
- Reverse native collected keys: collectAuthParams native values/order case 21.
- Substring changed to prefix: isPublicEndpoint substring https://example.invalid/api/v1/time.
- Instruction fallback changed: getInstruction priority GET (empty URL).

Sub-agent review found one oracle startup blocker and three witness gaps;
module metadata handling, direct key-order observation, original-graph mutation,
and exact surrogate output captures resolved them. Final review: 0 actionable
findings. Review did not run Cargo or edit files.

Slice handoff gates (sequential Cargo, all compile/test commands --jobs 1):
- Full scoped crate suite: 3 tests passed, 0 failed.
- Strict host Clippy --all-targets: passed.
- Strict wasm32-unknown-unknown Clippy --all-targets: passed (compile/lint only).
- Official parity-check on exact isolated crate-source copy and types/API
  manifests: passed, 40 rows / 70 public Rust items, no exemptions in scope.
- Locked metadata dependency/layer audit: 2 authorized workspace edges (num,
  types), serde runtime and serde_json dev at this historical slice; exact
  package paths checked. The later signing slice moved serde_json to runtime.
- Shared exempt census: 338 blocks /338 distinct rs; no exemption edits.
- Extractor loader retried: BLOCKED with ERR_MODULE_NOT_FOUND, missing installed
  smol-toml/index.js. No environment workaround; extractor is not a passed gate.
- One Node invocation per corpus; no Yarn shim, per-case subprocess, concurrent
  Cargo invocation, shared feeder edit, integration merge-up or PR creation.

## Earlier-row mutation census (after 965064ade6)

Closed the earlier record-slice evidence gap: every one of the 28 record rows
now has an individually compiled production mutation. For each row, rename its
first Serde wire field to mutationControl, run only --test types --jobs 1,
require a runtime failure at that row's full-case assertion, restore the exact
original production bytes, and require the complete types target to pass.
One Node subprocess returns the entire corpus on each run. This establishes
per-row execution sensitivity, not a separate mutation for every field.

The witness now orders reusable Signature/Error/Time record cases before their
consumers so the diagnostic identifies the mutated row. Stable sorting retains
all 1,545 cases and all three ingress routes; no filters were added.
Final production types.rs bytes were compared to git HEAD and are identical.

Measured results: 28 distinct rows, 28 named runtime failures, 28 restored passes.
Exact failure assertions (the missing-field diagnostics are named accepted-case
assertions in actual_typescript_records_match_decoded_fields_and_bits):

- BackpackAsset full route 0: missing field `mutationControl`; restored PASS.
- BackpackMarket full route 0: missing field `mutationControl`; restored PASS.
- BackpackTicker full route 0: missing field `mutationControl`; restored PASS.
- BackpackDepth full route 0: missing field `mutationControl`; restored PASS.
- BackpackTrade full route 0: missing field `mutationControl`; restored PASS.
- BackpackKline full route 0: missing field `mutationControl`; restored PASS.
- BackpackMarkPrice full route 0: missing field `mutationControl`; restored PASS.
- decoded native fields/bits BackpackBalance full route 0; restored PASS.
- BackpackOrder full route 0: missing field `mutationControl`; restored PASS.
- BackpackOrderRequest full route 0: missing field `mutationControl`; restored PASS.
- decoded native fields/bits BackpackFill full route 0; restored PASS.
- BackpackCollateral full route 0: missing field `mutationControl`; restored PASS.
- BackpackPosition full route 0: missing field `mutationControl`; restored PASS.
- BackpackWsSubscribeMessage full route 0: missing field `mutationControl`; restored PASS.
- BackpackWsUnsubscribeMessage full route 0: missing field `mutationControl`; restored PASS.
- BackpackWsSignature full route 0: missing field `mutationControl`; restored PASS.
- BackpackWsTickerUpdate full route 0: missing field `mutationControl`; restored PASS.
- BackpackWsDepthUpdate full route 0: missing field `mutationControl`; restored PASS.
- BackpackWsTradeUpdate full route 0: missing field `mutationControl`; restored PASS.
- BackpackWsKlineUpdate full route 0: missing field `mutationControl`; restored PASS.
- BackpackWsBookTickerUpdate full route 0: missing field `mutationControl`; restored PASS.
- BackpackWsOrderUpdate full route 0: missing field `mutationControl`; restored PASS.
- BackpackWsPositionUpdate full route 0: missing field `mutationControl`; restored PASS.
- BackpackError full route 0: missing field `mutationControl`; restored PASS.
- decoded native fields/bits BackpackApiResponse full route 0; restored PASS.
- BackpackTime full route 0: missing field `mutationControl`; restored PASS.
- BackpackStatus full route 0: missing field `mutationControl`; restored PASS.
- BackpackAccount full route 0: missing field `mutationControl`; restored PASS.

After the earlier-row audit, full scoped cargo test passed 3 tests; strict host
and wasm Clippy passed separately with --all-targets --jobs 1. The only code
change is test ordering. Production source and manifests are unchanged from
965064ade6, whose parity/layer/extractor results are recorded above; those gates
were not relabelled as newly executed for this evidence follow-up. Extractor
remains blocked. This follow-up is retained locally under the fleet head-freeze
rule; origin stays pinned at reviewed 965064ade6 until explicitly released.

## Integration refresh to 5b3f63f2ea (authorized replacement of pinned head)

User explicitly authorized merging 5b3f63f2ea7af6b63c5c37d1763cf52b77357ad0
and one reviewed push, followed by a remote-head freeze. Merge commit:
85cfca7e5fe6eaadcd0f11e1146aa367c7d7e147. This includes the previously local
28-row mutation audit; the earlier local-only statement is historical.

Measured tomllib total147: ported8 / divergent33 / pending106. No rows were
reclassified during this integration refresh. Runtime capture now records
Node v26.7.0 / arm64 / darwin inside the actual TypeScript record oracle.
NaN alone is canonicalized; negative zero remains bit-exact.

Symmetric production Serde rename control: swap BackpackAccountWire fields
`autoBorrowSettlements` and `autoRealizePnl` in both decode and encode. Their
full-fixture values are equal, so that fixture cannot detect the native swap.
The existing independently varied boolean supplement catches it. Initially
wire property order failed before the supplement could run; the test now checks
all decoded native values before asserting any wire projection, retaining every
case and every wire comparison. Rerunning the swap failed the named assertion
`decoded native fields/bits BackpackAccount boolean autoBorrowSettlements route 0`.
Restoring the production source passed the complete types target: 1545 cases,
three ingress routes. No shared corpus or feeder was edited.

Gates remeasured on the merged implementation and updated witness:
- Full scoped Backpack cargo test: 3 passed, 0 failed.
- Strict host Clippy --all-targets --jobs1: passed.
- Strict wasm32-unknown-unknown Clippy --all-targets --jobs1: passed separately.
- Official parity invention/status gate: 147 rows /72 public Rust items /0 errors.
  Scope is all six Backpack manifests, exact Backpack source copy, and exact
  shared ed25519.rs copy for the two pre-existing primitive mappings. The isolated
  workspace has minimal indexing-only manifests and an ed25519 module root;
  no exemptions. This indexes those primitive mappings without claiming a new
  behavioral audit of crypto or the full shared crypto crate.
- Locked metadata layer/dependency audit: 2 authorized workspace edges, exact
  paths checked; serde runtime and serde_json dev at this historical slice.
  The later signing slice moved serde_json to runtime. Shared exemptions338/338.
- Extractor loader retried and remains BLOCKED: installed smol-toml/index.js is
  missing. No workaround and no claim that extractor drift passed.
- One Node subprocess per corpus; no parallel Cargo or Yarn shim invocation.
Sub-agent merged-state review: 0 actionable findings; exact-SHA review follows
commit creation. Subsequent work stays local after the authorized push.

Final capture-header follow-up: all three corpora (constants, record types, API
helpers) now emit node/arch/platform headers and Rust validates/logs them.
Measured all three: Node v26.7.0 /arm64 /darwin. Constants' six-table census is
unchanged inside its tables envelope. Full scoped suite again passed3 tests;
strict host and wasm Clippy again passed separately after this header change.
Production sources/manifests/dependencies remain identical to the preceding
147-row parity and layer measurements.

Ancestry disclosure: the supplied worktree began at 815ebf1381fe6b955681e58197edd745dc84efb9
(master's online-recovery merge), the first parent of initial integration merge
66a21867ac. Consequently the branch diff against 5b3f63f2ea also includes those
inherited online-recovery commits. No changes to those files were authored by
this Backpack work; they are outside these scoped gates and review. History was
not rewritten to remove them. Rust workflow now limits push branches to master
and feat/safe-cex-rust, inherited from the requested integration merge.

## Local signing preimage slice after pinned 82b032ca6b

Remote stays pinned at 82b032ca6b9a9cdd7976716b18d3fd698c98a3e1. This slice
is local follow-on work under the user's freeze instruction. Integration
5b3f63f2ea is already an ancestor; no second merge or remote push is needed.

Python 3.14 tomllib: total147; ported8 /divergent35 /pending104. Two new
CHOSEN mappings: buildSortedQueryString and buildSigningPayload. Their notes
cite the shared UTF-8 boundary and explain why introducing a second executable
host-value ABI would be worse than using prepared data. Custom prototypes,
getters/coercion callbacks, cycles, symbols and sparse arrays remain outside
that representation. BackpackSigningParams declaration and the crypto/header
composition remain pending; pre-existing primitive notes were corrected to
name composition as the outstanding work, not the now-mapped payload helper.

Actual backpack.signing.ts is transpiled/executed as a whole, exposing its
private sorted-query helper for the witness. Unused crypto/clock functions have
throwing tripwires. The single Node invocation returns41 query cases and2132
payload cases, plus UTF-16 and executable-coercion boundary observations.
Runtime header: Node v26.7.0 /arm64 /darwin. Native numeric typeof/bits are
captured before JSON; only NaN is canonicalized, signed zero stays exact.
The source string output is the signing preimage itself, so byte comparison is
the output contract, not a roundtrip substitute for a native object assertion.
Coverage includes distinct numeric/Unicode keys and values, UTF-16 sort order,
null/undefined omission, safe/reserved UTF-8 bytes, nested arrays and object
coercion refusals, nonfinite numeric spellings, and batch validation precedence.
Source surrogate-key/instruction output code units, source URIError for a
surrogate value, and Rust UTF-8 ingress refusal are compared explicitly.

Three actual production mutations, each followed by a passing restored scoped
signing target (reviewer independently read all6 failure/restoration logs):
- Reverse key comparator: buildSortedQueryString bytes case1.
- Swap timestamp/window numeric formatting: buildSigningPayload bytes case0.
- Double batch row delimiter: buildSigningPayload bytes case1456.

Local slice gates, all Cargo compile/test/lint invocations sequential --jobs1:
- Full scoped crate suite:4 tests passed,0 failed; earlier witnesses retained.
- Host strict Clippy --all-targets: passed.
- wasm32-unknown-unknown strict Clippy --all-targets: passed separately.
- Official scoped parity:147 rows /74 public Rust items /0 errors. Scope is the
  same exact Backpack source + existing ed25519 primitive copy, six Backpack
  manifests, no exemptions, as documented at the previous boundary.
- Locked metadata layer audit:2 authorized workspace edges (num/types), exact
  paths checked. serde and serde_json are runtime registry dependencies;
  serde_json's new runtime role is the internal shared String-coercion view,
  never a lossy numeric ingress or output-eligibility bypass.
- Exemption census:338 blocks /338 distinct rs; no edits.
- Extractor remains blocked by the previously measured missing smol-toml
  installation; no extractor pass or environment workaround is claimed.
Sub-agent draft/log review:0 code findings, one stale primitive-note correction
applied. Exact committed-SHA review follows. No shared feeder bytes changed.

## Authorized delivery refreshed through the Store notification cutover

User authorized another reviewed push. Merged dec68e5490cb2a24dd564bd833e1caabf4fc42d2
at33c5ba9b032aa0210b9128a71e51a7d57c813f5c, then incorporated the newly announced
9419b340d8a6ffdcecf801f3d384a92f788e1af4 before pushing, at merge commit
008cef11afdc168ff6140bbc233a6c95986403f5. Both merges left Backpack source and
manifests byte-unchanged from local signing slice088564102f. The earlier
local-only statements are historical: this delivery includes that reviewed slice.

Measured Python3.14 tomllib:147 total,8 ported /35 divergent /104 pending.
Constants/types alone comprise6 tables plus28 records. All34 rows have individual
production mutation controls: six table controls were rerun after dec68e5490cb,
and the28 recorded record controls retain named failure/restoration evidence.
Sub-agent re-audit read all12 new constants logs (6 named failures,6 restored
passes) and compared all28 record results with the28 manifest rows and failure
logs:0 missing,0 extra,0 mismatches. Those28 controls were not mislabelled as
newly run after the integration merge. Production source is restored byte-for-byte.

After9419b340d8a6, the symmetric account Serde-name swap was executed again:
`decoded native fields/bits BackpackAccount boolean autoBorrowSettlements route 0`
failed, then restored types passed1545 cases over3 ingress routes. The entire
merged crate suite passed4 tests. All capture headers record Nodev26.7.0,
arm64,darwin; only NaN bits are canonicalized, signed zero remains exact.

Final merged-state gates:
- Scoped cargo fmt --check: passed.
- Full scoped cargo test:4 tests passed,0 failed.
- Host strict Clippy --all-targets --jobs1: passed.
- wasm32-unknown-unknown strict Clippy --all-targets --jobs1: passed separately.
- Official scoped parity invention/status:147 rows /74 public items /0 errors,
  using the previously documented exact Backpack/ed25519 source scope.
- Locked metadata workspace runtime closure is exactly Backpack,num,types;
  zero dependency paths to Store. The declared2 workspace edges remain granted;
  serde and serde_json are runtime registry dependencies.
- Shared exemption census is now359 blocks /359 distinct rs, inherited from the
  Store cutover merge. No Backpack-authored exemption changes.
- Extractor loader was retried at the preceding dec68 boundary: still blocked
  by missing smol-toml/index.js. This is not a passed extractor gate.

Store cutover inventory: sub-agent source-callgraph audit found0 Backpack-owned
Store writes and0 public-data subscription/callback sites. It examined every
module's call paths plus the newly exported PublicDataStore/PublicDataSource
traits; this is not a claim about arbitrary caller-supplied generic Serde code.
Constants are data; API/signing helpers call local/shared pure helpers; wire
implementations call Serde, numeric capture/output traversal and the shape guard.
Manifest FINDING-note census is0, so no routed-contract row was promoted here.

Merged-state sub-agent review:0 actionable findings in Backpack scope. Exact
final-SHA review follows this evidence commit. The disclosed unrelated inherited
online-recovery ancestry remains outside that scope. No integration merge-up,
PR creation, shared feeder edit, parallel Cargo or Yarn shim invocation.
After the authorized push, the remote head is frozen again.

## Price/amount precision-swap inventory (local follow-on audit)

Audited the implemented Backpack Rust modules at pinned remote head
8ecb30b0085043c5ea2b2705f8c08b9ec2fc6d19 and the local auth-signing changes.
There are zero price-step derivations, zero amount-step derivations, and zero
compiled two-axis formatting sites to mutate. tickSize and stepSize are wire
record fields, Serde companions and output traversal only; trigger/bracket
prices and quantities likewise have no formatting implementation. The floor
operation in api.rs rounds the auth receive window, not an order value.

Actual source backpack.exchange.ts has separate price/amount precision paths in
buildBatchLimitOrderRequest and placeOrder, including trigger/bracket payloads.
Both rows remain pending with empty Rust targets. This is no applicable Rust
mutation site yet, not evidence of swap immunity. When those rows are ported,
require fractional inputs that produce different outputs under each step across
multiple distinct step pairs, both sides, limit and stop-market/bracket branches;
assert native payload fields before serialization and prove production swaps
compile, fail their named assertions with exit101, then restore and pass.
No precision mutation or precision-test pass is claimed by this inventory.

## Local request-signing and auth-header utility slice

This follow-on is local only; PR7269 remains pinned at
8ecb30b0085043c5ea2b2705f8c08b9ec2fc6d19. Its measured 8/35/104 triple was posted
in PR comment6141001333. Do not apply the following local counts to that PR.
Python3.14 tomllib now measures 8 ported /36 divergent /103 pending (147 rows).
The previously primitive-only signRequest row now maps the complete request
composition as well as shared signing; generateAuthHeaders moves from pending
to divergent. Both notes name the UTF-8/prepared-value and native future/error
representation boundaries. verifySignature remains a primitive-only mapping;
WebSocket signing and REST transport are not claimed.

Actual TypeScript plus actual installed Noble signing/hashing executes in ONE
Node subprocess for the complete auth corpus:198 signing cases and264 header
cases, with63 and80 successes respectively. Capture identifies Nodev26.7.0,
arm64,darwin. Inputs capture typeof/f64 bits before JSON, canonicalize NaN only,
and preserve signed zero. The cases cover permissive Node base64 policies,
malformed key lengths, payload error precedence, caller API key retention,
header order/native values, timestamp/window spellings, and UTF-8 boundaries.

Sub-agent review found the draft's lazy async clock capture differed from source
call-time execution. The final wrapper captures the clock and constructs signing
before returning its future; a source/Rust paired witness advances11 to22 after
construction and asserts clock calls remain1 and timestamp remains11.

Production mutation controls (all source restored):
- Append to the signed payload: `signRequest bytes case 0` fails.
- Use window for the timestamp header: `generateAuthHeaders native X-Timestamp case 0` fails.
- Move clock/signing into the async body: `generateAuthHeaders eager clock before poll` fails.
Each mutation compiles, fails its runtime assertion, then passes after restore.
The initial draft preimage control failed compilation for unused payload and was
rejected as evidence; the corrected control consumes and changes the payload.
Sub-agent read all6 corrected logs (3 runtime failures,3 restored passes) and
reviewed source, manifest mappings and boundary notes:0 actionable findings.

Final local gates for this auth slice:
- Full scoped Backpack suite:5 tests passed,0 failed (one Node per corpus).
- Strict host Clippy, all targets, jobs1:passed.
- Strict wasm32-unknown-unknown Clippy, all targets, jobs1:passed separately.
- Scoped cargo fmt check and git diff whitespace check:passed.
- Official parity-check on exact Backpack source plus existing shared ed25519
  source and all six Backpack manifests:147 rows /76 public items /0 errors.
- Locked Cargo metadata:3 authorized direct workspace edges (num/types/crypto),
  exact local paths verified; base64,serde,serde_json are registry runtime edges.
  Runtime workspace closure is Backpack,num,types,crypto; no Store dependency.
- Shared exemption census:359 blocks /359 distinct rs; no edits.
- Extractor module load retried:ERR_MODULE_NOT_FOUND for installed
  smol-toml/index.js. Extractor drift remains blocked, not passed.
No parallel Cargo, Yarn shim, shared feeder edits, PR-head pushes, or integration
merge-up. Exact committed-SHA review is recorded separately after this commit.

## Cardinality follow-on, kept separate from pinned PR7269

See CARDINALITY_AUDIT.md for the105 final recorded control/restore pairs:
8 original corpus-loss false greens,97 named failures,105 restorations passed.
Seven concrete decoded vectors each have exact consume-all-then-None controls
and guard-removal companions in the permanent tests/consuming_controls.py.
All14 consuming controls fail native assertions (not Serde trailing-element
errors); with new guards removed, the old full native-value equality still fails.
No source/oracle/schema fixture bytes changed. Native precision formatter sites
remain absent; their future per-field distinct-step controls are still required.

Integration ba6468e0076f then a09ea6489b was merged only into the separate
feat/rust-venue-backpack-auth branch. After restoration at the a09 boundary:
5 Backpack tests passed; strict host/wasm Clippy separately passed; scoped fmt
passed; parity147/76/0; tomllib8/36/103; shared exemptions359/359 unchanged.
Post-a09 Store inventory iteration1:0 writes/0 public-data callbacks, confirmed
by sub-agent; locked workspace runtime closure Backpack,num,types,crypto.
The local extractor still cannot load installed smol-toml/index.js. Known
integration KuCoin compile errors were reported by the user and are not repaired
or hidden by this lane. These scoped passes are not a workspace or CI green.
The freeze is user policy, not branch protection. No original-branch push or CI
rerun; the reviewed follow-on may be pushed to its own branch.


## PR 7269 follow-up: protect the boolean supplement

Integration b5f374213e91369040de4c9cd9cb42ee39709ab3 was fetched and merged
on the separate fix/rust-backpack-review-followup branch. The earlier reviewed
51c51e57c7dbe0689ec18139e69cc3e4939463c3 already pins the type corpus to
1545 independently in Rust, with per-record accepted/refused counts and completed
comparison counts. Those changes were not in the original PR 7269 head.

The exact review control was run after this merge: delete the boolean one-hot
supplement loop from types_oracle.cjs, execute the scoped types target, and restore
that file byte-identically. The mutant compiled and failed exit101 at
`TYPES-CARDINALITY source cases`, actual1528 versus expected1545. Restoring the
17 generated cases passed the same target exit0. Each target invokes Node once
for the full corpus; Cargo used --jobs1 and ran sequentially. Logs are retained
locally in target/backpack-cardinality/remove-boolean-onehot.log and
remove-boolean-onehot-restored.log. This is a generator-loss control, distinct
from the earlier consuming production-decoder controls.

Both historical dependency sentences now explicitly identify their slice and
the later move of serde_json to runtime. Sub-agent review verified that serde_json
was a dev dependency at those historical commits (965064ade6 and 82b032ca6b),
so replacing those historical observations would misstate their evidence.
The current Cargo.toml declares both serde and serde_json runtime dependencies.
No manifest or production code changed for this clarification.

Post-merge scoped verification: five Backpack integration targets passed, zero
failed; strict host Clippy and strict wasm32-unknown-unknown Clippy each passed
separately with --all-targets --jobs1 -- -D warnings. Scoped rustfmt and
`git diff --check` passed. This does not claim a workspace or CI run. No manifest
row changes: 8 ported /36 divergent /103 pending. Read-only sub-agent review of
the follow-up and both one-hot control logs returned zero remaining findings.


## Verification composition follow-on

Fetched integration115f2f1c82077057764e859d6980d6c955a32315 and merged it on
feat/rust-backpack-verify. The existing verifySignature primitive-only mapping
now includes the venue caller: payload construction, UTF-8 bytes, Node-compatible
base64 conversion and source catch-to-false. This completes that composition
without relabeling the strict shared Ed25519 primitive as Noble ZIP215.

One actual-TypeScript/Noble Node corpus contains507 cases:324 accepted,
180 refused,3 weak-identity-key cases accepted by TypeScript and explicitly
refused under the declared shared strict-native policy. Rust pins the507 total,
all19 label/subcorpus counts and [324,180,3] completed native comparisons.
Numeric observations use typeof/f64 bits before JSON, NaN-only canonicalization,
exact signed zero; capture runtime was Nodev26.7.0 /arm64 /darwin.

Three final compiling production mutants each fail exit101 and restore/pass:
- Drop the first payload byte: `verifySignature native boolean "valid" case 0`.
- Swap decoded key/signature arguments: same named valid-case assertion.
- Change caught-error fallback to true:
  `verifySignature native boolean "changed key" case 41`.
Logs: target/backpack-cardinality/verify-{message,key-signature,catch}.log and
corresponding -restored.log; verify-controls.json records exits. An initial
unused-payload compile failure was rejected as mutation evidence and replaced
by the compiling byte-truncation control above.

The new function returns a scalar bool, so it introduces no serde collection
decode or expected/native zip. Existing consuming-decoder controls remain as
documented in CARDINALITY_AUDIT.md. No implemented market precision consumer
was added; pending price/amount formatters still require per-field distinct-step
controls. Manifest counts remain8/36/103: composition progress in an already
divergent row, not a newly completed row. No shared source or dependency edits.

Scoped gates after restoring all mutations:6 integration targets passed,
0 failed; strict host Clippy and wasm32-unknown-unknown Clippy separately passed
(--all-targets --jobs1 -- -D warnings). Official isolated Backpack/shared-Ed25519
manifest gate passed147 rows/77 public items. Scoped rustfmt and diff checks
passed. This sheet makes no claim of a workspace test or remote CI run.


## Order status and time-in-force utility follow-on

Continued from reviewed/pushed0728b9e8dccad135c79904f49e10a298e0f86103 on
separate feat/rust-backpack-order-utils, keeping the verification head fixed.
Three source switch methods now have native helpers: mapOrderStatus,
mapTimeInForce and mapBackpackTimeInForce. The actual full source module executes
with actual TypeScript enums/constants; constructor transport/socket seams are
inert, and other dependency accesses trip. No constructor/REST/WS behavior is
claimed. The caller's typed enum and absence become Option<OrderTimeInForce>;
string-input helpers retain the declared shared UTF-8 ingress boundary.

One Node corpus independently pins13 status cases,5 forward time-in-force cases,
12 reverse cases and2 surrogate-boundary captures, plus [13,5,12,2] completed
comparisons. Assertions compare decoded native enum variants or returned wire
literals, not serialization roundtrips. Each scalar result is checked; no paired
collection walk or new serde collection decode is introduced. Both surrogate
observations return defaults in TypeScript and fail native UTF-8 string ingress.
The capture header records Nodev26.7.0 /arm64 /darwin.

Three production controls compiled, failed exit101, restored byte-identically,
and passed the same target exit0:
- Filled => Open: `mapOrderStatus native enum case 2`.
- ImmediateOrCancel => GTC: `mapTimeInForce native wire literal case 2`.
- IOC => GoodTillCancel: `mapBackpackTimeInForce native enum case 1`.
Logs: target/backpack-cardinality/order-utils-{status,tif,reverse}.log and
corresponding -restored.log; order-utils-controls.json records all six exits.
The manifest gate initially rejected missing explicit renamed-target notes for
standalone mappings of class fields. Those exact-target notes were added; the
restored source scope now passes147 rows/80 public Rust items. Real tomllib
measures9 ported/38 divergent/100 pending,147 total. No dependency changes.

Final scoped verification:7 integration targets passed,0 failed; strict host
and wasm32-unknown-unknown Clippy passed separately (--all-targets --jobs1 --
-D warnings). Scoped fmt and diff checks passed. Logs are order-utils-suite.log,
order-utils-host.log and order-utils-wasm.log in target/backpack-cardinality.
No workspace build or CI result is claimed by these scoped gates.


## Ordinary-limit predicate follow-on

Continued on feat/rust-backpack-order-predicate from reviewed/pushed
f1a0c14a510a74eb4af144aa62c5d881f9c1d6be, leaving both prior utility heads fixed.
`is_ordinary_limit_order` consumes the shared PlaceOrderOpts and mirrors all
seven source conditions. No venue-local options schema or numeric parser was
introduced. Its actual-source corpus has576 typed cases (6 types x32 masks x3
close states), fixed per-type/per-mask totals, and completed boolean comparisons
[2 accepted,574 refused]. Five explicit-null observations and two string-domain
observations compare source behavior with shared native admission. Capture:
Nodev26.7.0 /arm64 /darwin, numeric typeof/bits before JSON, NaN-only canonical,
signed zero preserved. Null and surrogate refusals are declared CHOSEN boundaries.

The production mutation changed trigger_price.is_none() to treat zero as absent.
It compiled and failed exit101 at
`ordinary native predicate type "limit" mask 1 close None` (true vs false);
the source case carries negative zero. Restoration passed exit0. Logs:
target/backpack-cardinality/ordinary-truthiness.log and
ordinary-truthiness-restored.log. This proves that single predicate condition;
it is not a per-field precision audit (no derived-step reads exist here).
No new collection decoder or paired output walk was introduced.

Final scoped gates:8 integration targets passed,0 failed; strict host Clippy
and strict wasm32-unknown-unknown Clippy passed separately with --all-targets
--jobs1 -- -D warnings. Scoped fmt/diff checks passed. Official isolated scope
passed147 rows/81 public Rust items. Real tomllib measured9 ported/39 divergent/
99 pending. Logs: ordinary-suite.log, ordinary-host.log and ordinary-wasm.log.
No dependency, shared feeder, exemption, workspace test or CI changes/claims.
