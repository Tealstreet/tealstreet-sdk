# Paradex declaration slice — 2026-10-13

Historical tool locations below use portable variables: DEPENDENCY_ROOT is the
frozen dependency installation root; YARN_CLI is the Yarn 1.22.21 JavaScript
entry point. This privacy rendering does not claim those gates were rerun.

Scope: all 23 rows of `paradex.types.ts.toml`. Source read before implementation:
all eight Paradex TypeScript files (5,093 lines at this checkout). Integration
`a3d514bec82d6760d4e9e7a27186f992d175c36d` merged down through
`6cb771c946462fa73096f4b60d03082070a0a013`.

## Measurements

Real Python `tomllib`: venue total 153; **ported 7, divergent 17, pending 129**.
This slice moves 23 pending rows: 7 ported and 16 CHOSEN divergences citing the
shared UTF-8 boundary. The inherited SDK crypto FINDING is unchanged and is not
reviewed or repaired by this slice. No REST, SDK lifecycle, writes, or websocket
runtime implementation is claimed.

The actual TypeScript module executes in `tests/types_oracle.cjs`; imported
OrderSide/OrderType enum declarations come from the actual shared TypeScript.
Erased type declarations additionally receive compiler-checked and executed
identity witnesses. These are witnesses of declared shapes, not a claim that
Axios validates its generic response annotations at runtime.

- 15 type rows; 354 valid cases across 6 raw/owned/borrowed decode routes.
- 1,056 malformed-shape cases across 3 routes; 166 surrogate refusal cases.
- 27 inputs per predicate; 54 ordinary/prototype table reads.
- Native decoded field observations precede supplementary wire serialization.
  Distinct field strings and per-field numeric perturbations detect symmetric
  decode/encode swaps. Boolean cases include both values.
- Numeric cases: integer/decimal negative zero, negative underflow, both
  overflowing infinities, rounded unsafe integers, and adjacent binary64 values.
  Oracle capture canonicalizes only NaN; no JSON fixture represents NaN.
  Signed zero remains bit-exact.

## Proved failure controls

Each temporary mutation was restored. The restored crate suite passed 2 tests,
0 failures. The six mutation runs failed at the following assertions:

| Mutation | Named assertion |
| --- | --- |
| Remove AST heritage refusal | `inherited_field_control` |
| Change receive window to 20,001 | `RECV_WINDOW_before_serialization` |
| Remove STOP_LIMIT trigger membership | `trigger_predicate STOP_LIMIT` |
| Swap account/address wire names symmetrically | `native_field_observation ACCOUNT_RESPONSE` |
| Allow null for an optional nested property | `declared_shape_refusal MARKET_RESPONSE` |
| Add +0 to decoded updated_at, erasing -0 | `numeric_ingress_preservation ACCOUNT_RESPONSE` |

The signed-zero mutation is rejected inside shared numeric ingress at
`$.updated_at` before the decoded-field observer runs. This is a measured
refusal, not a claim that a lossy field reached the final comparison.

## Reproduction and gate scope

Measured gate results: crate tests 2 passed / 0 failed; host clippy exit 0;
wasm clippy exit 0 (each with `--all-targets --jobs 1 -- -D warnings`).
Wasm code was linted, not executed. Node syntax checks and `git diff --check`
returned exit 0. Sub-agent review found two witness gaps (serialization and
false boolean values); both were repaired and re-reviewed.

Run from the assigned worktree root:

```sh
export SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript
export PATH=/opt/homebrew/opt/rustup/bin:$PATH
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --jobs 1 -- --test-threads=1
cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 1 -- -D warnings
cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --target wasm32-unknown-unknown --jobs 1 -- -D warnings
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py
```

Structural scope: the actual extractor's `extractFile` over eight Paradex sources
and their resolved imports, compared against parsed manifests: 153 identities,
0 drift. This is not a package typecheck or a monorepo extractor run.

The actual parity checker receives a temporary workspace inventory containing
only this crate and an unchanged copy of the types manifest: 23 rows,
37 public Rust items, 0 exemptions. This is not a global parity claim.
The direct layer audit checks 2 production path edges against the workspace
policy, grants and Cargo package identities.

Global exemption measurement (read-only): **338 BLOCKS / 338 DISTINCT**.
Paradex exemption blocks: **0**, measured by parsed target ownership. No shared
fixture/corpus was edited or regenerated. The oracle captures are local to this
crate and execute fresh on every test process.

## Process-load followup

Temporary instrumentation at the actual Node entrypoint measured **2 invocations
before / 1 after** sharing the corpus through `OnceLock`. Both measurements ran
only `cargo test -p safe-cex-venue-paradex --test types --jobs 1 -- --test-threads=1`;
both passed 2 tests. Instrumentation was restored before committing. No per-case
subprocess loop exists, and the corpus remains fresh for each test process.

## Utility slice (2026-10-13)

Five rows in `paradex.income.ts` and `paradex.order-history.ts` now map to their
Rust counterparts. Real Python 3.14 tomllib measurement: **153 total; 7 ported /
22 divergent / 124 pending**. Divergences retain the shared UTF-8 declaration and
state the JSON transport domain limitation and why an executable object runtime
would be worse for these mappers.

The live utility corpus executes **40 scalar / 486 historical-order / 230 income**
cases in **one Node invocation per test process**. Capture header measured
`arch=arm64`, `platform=darwin`, `node=v26.7.0`, `nodeMajor=26`,
`v8=14.6.202.34-node.28`. The earlier types corpus now records and checks this
header too. NaN alone is canonicalized; signed zero remains bit-exact.

Both source helpers are exposed by appending exports, preserving their bodies.
Observers inspect native Rust fields before serialization. The history baseline
where amount equals filled and both prices collapse to undefined is retained;
a labelled supplement uses distinct amount/remaining and price/average values.
All twelve stock Object.prototype keys are exercised through the actual source
uppercasing path; none reaches an inherited table hit. Resolver calls and errors,
coercion order, lazy nullish fallback, negative fees, status precedence and all
flags are compared. Raw numeric tokens enter retained ingress before Value.

Five separate production mutations each failed exit 101 at a named assertion;
each restoration passed the one scoped utility test target with exit 0:

| Production mutation | Named assertion |
| --- | --- |
| Income helper nonfinite fallback 0 to 1 | `income_to_number_native` |
| History helper negates parsed finite result | `history_to_number_native` |
| Optional helper admits zero | `history_optional_native` |
| Swap native amount and filled | `history_native_fields` |
| Remove commission fee negation | `income_native_fields` |

No shared feeder or TypeScript production source was edited. No Yarn entrypoint
was used, and no exit 126 was treated as evidence. Cargo invocations were
sequential, scoped and used `--jobs 1`.

Sub-agent review identified that empty mapper corpora could pass. Exact corpus
censuses and required distinct-supplement labels now guard the comparison loops.
An empty-history oracle mutation failed exit 101 at
`utility_corpus_census histories`; restoration passed exit 0.

The structural gate for this slice includes the unchanged types manifest and
the two utility manifests, targeting 28 mapped rows and 42 public Rust items.
The inherited SDK crypto row remains outside this venue-only structural scope.
Final readiness results and adversarial verdict attach to the exact merged SHA
reported with the slice; this historical evidence file does not attach older
results to a later branch head.

## Local helper slice (after pinned 586054540c)

This follow-on remains local while the reviewed remote head is pinned. It does
not claim merge-up readiness or reuse that head's full gate sheet.

Real Python 3.14 tomllib: **153 total; 9 ported / 24 divergent / 120 pending**.
Four exchange rows now map to standalone source-corresponding helpers. The actual
AST bodies execute with actual TypeScript safe-math.adjust, with no exchange
constructor or network effects. Measured corpus: **33 scalar / 416 rounding /
32 interval** cases, one Node invocation per scoped test run. Header: arm64,
Node 26.7.0 (major 26), V8 14.6.202.34-node.28. NaN alone is canonicalized.

Each production mutation failed exit 101 at the named assertion; each separately
restored scoped `--test exchange_helpers --jobs 1` run passed exit 0:

| Production mutation | Named assertion |
| --- | --- |
| Positive price permits zero | `positive_price_native` |
| Fraction divides exactly one by 100 | `positive_fraction_native` |
| Reverse rounding direction | `round_price_native` |
| Hour multiplier 60 to 6 | `interval_resolution_native "5h"` |

Structural check passed: **88 rows / 46 public Rust items**, now including the
exchange manifest's pending rows as well as its four mapped helpers. Extractor
identity comparison remains eight source files, 153 symbols, zero drift; direct
layer audit validates two edges. Exemptions remain 338 blocks / 338 distinct,
with zero Paradex blocks. No shared corpus bytes were changed.

Earlier slice re-audit: typed decoders still assert native fields before
serialization, field-distinct values and optional booleans; mapper witnesses
retain production-collapse baselines and distinguishing supplements. Their
source/test bytes are unchanged in this helper slice. Full suite and strict
host/wasm clippy are deferred to this local slice's merge-up boundary.

## Local account-read slice (PR 7255 remains pinned)

Fetched integration `710e663156c8a3c43f8d40af2aa2076b960368d8` was merged
only into `feat/rust-venue-paradex-followup`. The remote PR head stays
`18878d8ca5556499c52b7dcb8e034e5dae2c62c6`.

Real tomllib measurement: **153 total; 10 ported / 25 divergent / 118 pending**.
Two source methods are now mapped: getPositionMode and getAccount. The latter
binds injected xhr.get, template-literal error coercion and log capabilities;
API/JWT setup and complete exchange composition remain pending. Account input
uses the declared response model; unchecked missing-account-number acceptance
is explicitly a chosen typed-ingress divergence, with source acceptance and
Rust refusal observed. No Store write or subscriber-error projection is added.

The live actual-method-body oracle runs **19 account cases plus position mode**
in **one Node invocation** per test process. Capture header: arm64, Node26.7.0
(major26), V8 14.6.202.34-node.28. It compares request paths, pre/post-settlement
traces, decoded native userId, affiliate null versus missing subId, and error
pointer identity. Inputs distinguish account, account_number and address.

Each production mutation failed exit101 at a named assertion, then a separately
restored scoped `--test account --jobs 1` run passed exit0:

| Production mutation | Named assertion |
| --- | --- |
| OneWay to Hedged | `position_mode_native` |
| User ID reads address | `account_native_user_id` |
| Suppress logger failure | `account_rethrow_identity` |

Sub-agent review found the affiliate assertion initially compared only with a
Rust literal. It now compares the native optional/null/string representation
with the actual TypeScript capture. A separate capture mutation replacing null
with a string failed `account_native_affiliate_null`; restoration passed.
The formatter contract now explicitly names template-literal ToString, including
Symbol refusal, rather than the different String(Symbol) behavior.

Venue-only structure gate passed **88 rows / 48 public Rust items**, eight
TypeScript files and 153 identities with zero drift, and two direct layer edges.
Global exemptions measured **359 blocks / 359 distinct**, zero Paradex blocks.
No shared fixture was regenerated. Earlier native-field/supplement/NaN-only
witness code is unchanged. Full crate suite and strict host/wasm gates remain
reserved for this local slice's later merge-up boundary; this is not a new
readiness claim on the pinned head.

## Request utilities after integration 3e9a264735

Two pending helpers now map: describeError and signerHeaders. The live oracle
extracts and executes their exact AST declarations in one Node subprocess.
25 describeError cases cover shared JSON formatting, successful undefined,
function/symbol results, BigInt/cycle/toJSON throws, a replacement String throw,
and UTF-16 lone-surrogate output. Exotic effects bind the explicit host
JSON.stringify/String capabilities; this does not claim a second intrinsic port.
Five signerHeaders cases produce four configs. Native pointer identity, full
entries, source keys, mutation visibility, per-case lengths, total outputs and
completed comparisons are checked independently. All earlier oracle files are
unchanged; the new capture header includes architecture and Node major.

Scoped `--test request_helpers --jobs 1`: 1 passed / 0 failed. Three production
controls compiled, exited101 at named assertions, and restored to exit0:

- Discard successful JSON output: `describe_native_outcome`.
- Return caught error instead of String fallback: `describe_replacement_identity`.
- Return empty configs for present headers: `headers_native_cardinality`.

Logs live under `.aimux/paradex/request-helpers-*.log`. Both rows are adapted,
ported control flow; generic host strings and borrowed objects avoid introducing
an additional UTF-8 or cloning divergence. REST caller/API composition remains
pending. This slice adds no Store writes or two-axis precision consumers.

## fetchBalance after the request-helper slice

One row maps as CHOSEN against shared retained-JSON/UTF-8 ingress, with explicit
why-mirroring-would-be-worse. Direct caller tick and startup via tick remain
pending; createAPI/initializeSDK are pending API/auth dependencies. No local Store or notification conversion was introduced. Log failures
propagate through the source's try/catch boundaries, including replacement errors.

The actual TypeScript method and actual describeError run in one Node process:
514 cases, reusing all 39 scalarRaw numeric/coercion inputs from the unchanged
utility feeder across 12 aliases, plus independent alias-precedence, wrapper,
primitive, used-fallback, authentication and effect controls. Native total/free/
used/upnl bits and absent fields are asserted, with four completed field checks
per success and 514 completed case checks. JWT reads occur after held SDK init;
GET remains held while the source drains cross-realm promise adoption. This
barrier replaced an insufficient two-microtask harness assumption, not fixture
values. The source eager-getter throw and surrogate log are boundary witnesses;
Rust JSON ingress refuses the surrogate. These are not claimed executable getters.

Four production mutations compiled and failed by name (exit101), then restored
and passed (exit0), all using one scoped balance target and --jobs1:

- total/free swap: `balance_native_total`.
- total+free in used fallback: `balance_native_used`.
- discard SDK initialization future: `balance_before_initialization`.
- discard response logger failure: `balance_expected_success`.

Evidence: `.aimux/paradex/balance-controls.json` and corresponding mutation and
restoration logs. This slice has zero price/amount precision consumers and no
new collection-producing path; complete trace equality detects missing effects.

## Income range orchestration

The actual fetchIncomeHistoryRange AST body runs once per 54-case corpus, with
55 expected outputs, unchanged numeric feeder inputs, CPU/Node-major header and
NaN-only canonicalization. Independent native lengths, per-case completed field
comparisons and total completed cases guard empty/subset output. Native request
bits, held async settlement, callback count and original error identity are observed.
Own-undefined reflection and JavaScript array/object aliasing remain explicit
CHOSEN boundaries of shared owned UTF-8 records. No production decoder is added.

Six production controls compiled and failed (101), then restored and passed (0):
empty output, subset output, wrong filter field and filtering the empty string
fail `income_range_native_cardinality`; swapped start/end fails
`income_range_native_request`; swallowed async error fails `source_success`.
Logs: `.aimux/paradex/income-range-controls.json` and the twelve matching control
and restoration logs. No Store mutation or price/amount precision consumer added.

The earlier five utility notes now say shared domain types, removing an unproven
numeric registration claim; their code and source artifacts are unchanged.
Ticker stays pending with the shared-types/core owner and timestamp call site
named. No timestamp workaround is included.

## fetchOrders orchestration

The source method and describeError execute in one Node process for 36 cases,
producing 19 mapped outputs. The mapper is the declared existing capability;
mapOrder's own fields and authentication/transport construction remain pending.
Rust independently defines every case's input body and effect configuration and
compares live per-case content before invoking production. Native mapped fields,
lengths, complete effect traces, callback error identity, initialization and GET
barriers are observed. Inputs use differentiated id/price/size strings; these
fixtures do not claim arbitrary numeric-token transport decoding. The GET
capability must supply retained JSON. The describeError fixture observes its
successful String fallback, not a failing describe-error capability.

The permanent sequential mutation runner is scripts/check_order_reads_controls.py.
Each target uses --jobs 1 and one Node invocation per whole corpus. It checks
empty, subset, extra output and empty output with the source count guard removed;
status filtering, initialization and endpoint controls; plus same-length source
case replacement and reversal against independent live content admission.
Control logs and restored runs live under .aimux/paradex/order-reads-*.log.
No precision-axis consumer, Store write or production collection decoder is added.

Nine controls compiled, failed by name (101), and restored (0): four cardinality
controls plus missing PENDING status fail orders_native_cardinality; discarded
initialization and wrong endpoint fail orders_before_init; same-length source
replacement and reversal fail orders_live_input_content. All original 36 case
values stayed unchanged. Additional source boundary probes observe inherited
results, three getter reads, and lone-surrogate id acceptance; shared Rust ingress
refuses the surrogate. The ordinary empty-own-object baseline produces no rows.

## Order mapper lookup dependencies

Both exact AST bodies execute in one Node process. Independent Rust key/argument
admission covers 32 keys, status plus default/true/false order-type paths (128
native observations). All native kind/text/arity/truthiness checks run before
wire comparisons. __proto__ retains object identity rather than being treated as
one of the eleven callable members. A source surrogate key takes its fallback;
shared Rust string ingress refuses it. The typed call boundary is declared
string plus bool/undefined; it does not claim unchecked nonboolean arguments.

Shared TableHit retains inherited identities without constructing a venue-local
runtime object. The pending mapOrder row must decide closed Order-field
representation explicitly; these helpers do not silently solve that gap.
The permanent scoped runner scripts/check_order_maps_controls.py tests conditional
selection, FILLED status, each helper's inherited hit, same-length key replacement
and reversal. No collection decoder, Store mutation or precision axis is added.

Wire observations here are a test supplement built from native enum/prototype
observations, not a production serializer test. The source observes typeof and
member-specific String output before JSON.stringify can erase a callable.

All six controls compiled and failed at named assertions (101), then restored
and passed (0): conditional/FILLED mutations at maps_native_text, both inherited
fallback mutations at maps_native_kind, and replacement/reversal at
maps_live_input_content. Evidence: .aimux/paradex/order-maps-controls.json and
its twelve mutation/restoration logs. No source fixture values were repaired.


## SDK reads, disposal and bracket validation — tracked evidence correction

Measured code SHA: `75fa0f9a3592cb0193d673e80f74b397cacdf0b1`. Started from `7fcfce7950`;
merged integration `d3f2bb3940811e315aa9e4ab9d970f6b3c08783a`. This section transcribes the
previously measured gate sheet that was mistakenly left under ignored `.aimux/`.
These are historical measurements at that SHA, not new runs at this document's
commit. PR 7458 subsequently merged them at `4e0ca1accd`.

Moved **4 pending -> ported, 0 -> divergent**, measured with Python 3.14 tomllib
across all eight Paradex manifest files: 19/34/100 -> **23/34/96**
(ported/divergent/pending), total 153. Rows: `getParaclearBalance`,
`getSocializedLossFactor`, `ParadexExchange.dispose`,
`ParadexExchange#validateTpSlPrices`.

| Gate | Measured exit | Command |
| --- | ---: | --- |
| cargo-check | 0 | `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2` |
| sdk-native-and-controls | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_sdk_operations_controls.py` |
| extractor | 0 | `node "${YARN_CLI}" parity:extract:check` |
| structure | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py` |
| buckets | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py` |
| fmt | 0 | `cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check` |
| diff | 0 | `git diff --check` |

| Production control | Named failure | Mutation exit | Restored exit |
| --- | --- | ---: | ---: |
| balance | `balance_native_config` | 101 | 0 |
| factor | `factor_native_config` | 101 | 0 |
| dispose | `dispose_native_trace` | 101 | 0 |
| bracket | `bracket_native_value` | 101 | 0 |

Baseline exit 0; no survivors among **four controls attempted**. The oracle
independently pins 22 SDK read cases, six disposal cases and 44 numeric validation
cases. Native request fields, host/error identity, held await phases, disposal
state/order and binary64 validation inputs/results precede supplemental traces.

Not run: full crate/workspace suites, workspace compilation, host clippy, wasm
clippy, live API, actual Paraclear implementation or complete SDK/websocket
lifecycle. The inherited shared `signParadexTypedData` crypto row is excluded
from the venue structural scope. Reproduction runner:
`scripts/check_sdk_operations_controls.py`.

## SDK cache, auth-mode getter, failure reset and history delegate

Measured code SHA: `94a5328c77f47f7c70ca861db3c4caf20773c09b`; initial parent
`4e0ca1accd6fc8a81fb6d163581861c9860f8da9`. The gate/control runner asserted HEAD
held while measuring. Integration `df065d29b58fd12bdc103e2ef156c8de602663a0` was merged down at
`7143fe63979f0e2a87534107e4a7396abc62bcb8` after these runs. The following command exited 0, proving the tested
venue code and oracle inputs survived that merge byte-for-byte:

```sh
git diff --exit-code 94a5328c77f47f7c70ca861db3c4caf20773c09b 7143fe63979f0e2a87534107e4a7396abc62bcb8 -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex
```

This evidence is committed after the tested code, so the measured SHA remains
explicit rather than implying gates ran against a later documentation/merge SHA.
Delivery repeats merge-down and the unchanged-input proof immediately before push.

Moved **4 pending -> ported, 0 -> divergent**. Python 3.14 tomllib parsed every
Paradex manifest at the initial parent and tested code: **23/34/96 -> 27/34/92**
(ported/divergent/pending), total **153**. The changed-status key set independently
asserted exactly these four pending-to-ported rows:

- `ParadexExchange.get isPublicDataOnly`
- `ParadexExchange#resetSDKStateAfterInitFailure`
- `ParadexExchange.fetchHistoricalPositions`
- `getParadexSDK`

| Gate | Measured exit | Command |
| --- | ---: | --- |
| cargo-check | 0 | `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2` |
| state-native-and-controls | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_state_operations_controls.py` |
| extractor | 0 | `node "${YARN_CLI}" parity:extract:check` |
| structure | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py` |
| buckets | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py` |
| fmt | 0 | `cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check` |
| diff | 0 | `git diff --check` |

The structure gate's first invocation exited **1** because the runner omitted
`SAFE_CEX_TYPESCRIPT_PATH`. Its corrected invocation exported
`SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript`
and exited **0**. The native-control runner supplies the same environment itself.
No code or assertion was changed to correct that invocation.

| Production control | Named failure | Mutation exit | Restored exit |
| --- | --- | ---: | ---: |
| public-mode | `public_native_value` | 101 | 0 |
| reset-order | `reset_native_trace` | 101 | 0 |
| history-scope | `history_native_symbol_scoped` | 101 | 0 |
| sdk-cache | `cache_native_before_settled` | 101 | 0 |

Baseline exit **0**; each mutation compiled, failed its named test assertion
(**101**) and was restored to a passing run (**0**). No survivors among **four
controls attempted**. Enumeration stopped after one production control per new
row; this is not a complete mutation search.

`tests/state_operations.rs::actual_typescript_state_operations` invokes one Node
process that executes the real AST members. The independently pinned floor is
**211 cases**: 120 getter cases (three optional-boolean modes, eight credential
presence masks, five getter-failure modes); 80 resets (16 slot masks, five
setter-failure modes); three history delegations; eight SDK cache scenarios.
Native results, every reset slot, retained host/error identity, write/read order,
option identity and numeric bits, symbol scope, callback counts, pending phases,
module identity and cache state are asserted before supplemental JSON trace checks.
UTF-16 credentials/account input include a lone surrogate. No domain codec is
introduced by these rows.

Cache scenarios stop at warm cache, held success, synchronous/rejected load error,
both concurrent settlement orders and either overlapping load rejecting. They
prove independent in-flight loads, last-successful-settlement cache writes,
per-call result identity, failed-load retry and warm-cache reuse. Only the single
external dynamic-import expression is bound to a held load capability; the source
cache body is otherwise unchanged. Host callbacks retain the source property
boundaries; null reset writes are native None. Future polling supplies the Rust
executor boundary for source async methods.

Structural scope: actual `extractFile` over all **eight** venue source files,
zero symbol identity drift; **132** scoped manifest rows / **81** public Rust
items; **two** direct layer edges validated. SDK venue targets are included;
unchanged inherited shared crypto target is explicitly excluded. No new divergent
rows or venue exemption blocks were added.

Not run: full crate/workspace suites, workspace compilation, host clippy, wasm
clippy, live API, actual SDK package loading/Paraclear calls, inherited fill-fold
implementation, complete initialization/private-auth retry orchestration,
websocket lifecycle or inherited `signParadexTypedData` crypto implementation.
These gates cover the four named helpers at their declared host capabilities.
Reproduction runner: `scripts/check_state_operations_controls.py`. Raw logs stay
under ignored `.aimux/paradex/`; all measured exits, scopes and controls needed to
review this delivery are recorded here at this tracked path.


## Ten websocket lifecycle and subscription methods

Measured gate/control SHA: `9e13dd0a58efb87aefd7b596c821fb482bc450d1`. Initial slice parent:
`1181108d2dfeee7dd6cb1d21a3a39922e1e13b9e` (PR 7468 already merged). Implementation commit:
`d13971dc302ac2803fad326c787b52a5250a9b42`; the measured SHA adds the compiling
control correction described below. All seven gates and the corrected control
runner measured the stated SHA and asserted HEAD held.

Moved **10 pending -> ported, 0 -> divergent**. Python 3.14 tomllib parsed all
eight venue manifest files at the initial parent and measured code, yielding
**27/34/92 -> 37/34/82** (ported/divergent/pending), total **153**. The changed
status-key set separately asserted exactly ten rows, all pending -> ported:

- `ParadexPrivateWebSocket.connectAndSubscribe`
- `ParadexPrivateWebSocket#subscribeToPrivateChannels`
- `ParadexPrivateWebSocket.ping`
- `ParadexPrivateWebSocket.dispose`
- `ParadexPrivateWebSocket#throttledFetchOrders`
- `ParadexPublicWebSocket.connectAndSubscribe`
- `ParadexPublicWebSocket.onOpen`
- `ParadexPublicWebSocket.ping`
- `ParadexPublicWebSocket.dispose`
- `ParadexPublicWebSocket#hasActiveSubscriptionInterest`

| Gate | Exit | Command |
| --- | ---: | --- |
| cargo-check | 0 | `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2` |
| ws-native-and-controls | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_ws_lifecycle_controls.py` |
| extractor | 0 | `node "${YARN_CLI}" parity:extract:check` |
| structure | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py` |
| buckets | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py` |
| fmt | 0 | `cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check` |
| diff | 0 | `git diff --check` |

Environment: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript`.
Cargo ran serially with `--jobs 2`; the structural checker invokes its separate
venue parity utility with `--jobs 1`. No full suite ran.

| Production mutation | Named assertion | Mutation exit | Restored exit |
| --- | --- | ---: | ---: |
| public-connect | `public_connect_native_event` | 101 | 0 |
| public-ping | `public_ping_native_delay` | 101 | 0 |
| public-dispose | `public_dispose_native_base` | 101 | 0 |
| private-connect | `private_connect_native_event` | 101 | 0 |
| private-ping | `private_ping_native_delay` | 101 | 0 |
| private-dispose | `private_dispose_native_base` | 101 | 0 |
| public-open | `public_open_native_before_trace` | 101 | 0 |
| interest | `interest_native_value` | 101 | 0 |
| throttle | `throttle_native_delay` | 101 | 0 |
| channels | `channels_native_channel` | 101 | 0 |

Baseline exit **0**. Every corrected variant compiled, failed its named native
test assertion (**101**), was restored, and passed (**0**). **No survivors among
the ten compiling controls in the corrected sequence.** Enumeration stopped at
one control per newly ported row; no exhaustive mutation claim.

Initial instrument failure, retained explicitly: at `d13971dc30`, the sequence
caught/restored public connect and ping, then the public-dispose omission variant
exited **101 during compilation**, because removing the base callback invocation
left that callback unused under `-D warnings`. The control runner exited **1**;
this did not prove the native disposal assertion. Production was restored by
finally. The corrected variant retains the callback in an unused binding without
invoking it, reaches the native base-state assertion, and its restored run passes.
The corrected ten-control sequence reran the first two controls. No production
code, source fixture or assertion was weakened for this correction.

The actual-source witness invokes one Node process and independently pins
**834 cases**. The Rust definitions admit each live source input before running
production and assert completion against fixed floors:

| Method cases | Independent floor |
| --- | ---: |
| Public / private connect | 16 / 16 |
| Public / private ping, including deferred callback | 64 / 64 |
| Public / private disposal | 72 / 40 |
| Private order-fetch throttling | 14 |
| Public onOpen | 88 |
| Private channel subscriptions | 60 |
| Public subscription interest | 400 |

The oracle executes the exact ten AST members. Only their existing socket,
timer, collection, log and base-method capabilities are bound by the harness;
there is no source-body replacement. Compiled AST text is cached within that
Node invocation; each case executes in a fresh VM context with distinct host
state/errors. Native state, pointer identity, pending/deferred behavior, timer
delay and handle, callback identity, request fields, numeric ID bits and partial
failure are asserted before supplemental trace/wire observations.

Ping probes reread state at callback time and stop on disposed before connected.
Public disposal visits live Set/Map iterations, including distinct timers added
by clearTimeout callbacks, then clears each collection before base disposal.
Connections store the created socket before registering open/message/close.
Public onOpen clears sent and pending state before the connected check and only
closes a present socket on the disconnected branch. Interest channels retain
UTF-16, including a lone surrogate and extra dot suffixes, with native false,
true and original-error observations. The scheduler retains the current fetch
callback for the 1000ms throttle; the asynchronous fetch/store body is separate.
Private subscriptions construct three native requests and perform binary64
postincrement in Rust between ID getter/setter capabilities; signed zero, the
2^53 boundary, NaN and infinity are observed before JSON transport formatting.
Typed send binds the original JSON.stringify/send dependency; this gate does
not claim a new transport serializer. Socket and timer handles retain host
identity and can remain owned by the host after slot clearing.

Structural scope now includes the private websocket manifest: **150** venue
rows / **92** public Rust items, **eight** actual extractFile source files with
zero symbol identity drift, **two** direct layer edges. No new divergent rows
or venue exemptions. The two API rows and inherited shared crypto row are outside
this narrowed Rust target scope; source-symbol extraction still checks all files.

Integration `3e4e053b47fbfa44475712b68fdf53c23f5071cd` was merged down at `86b2a528904f37c2d3cfbf02e758af7e5008afbe`.
The following measured comparison exited **0**, preserving the tested venue
source, tests, scripts, manifests and original TypeScript inputs byte-for-byte:

```sh
git diff --exit-code 9e13dd0a58efb87aefd7b596c821fb482bc450d1 86b2a528904f37c2d3cfbf02e758af7e5008afbe -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex
```

Cargo.lock changed in that merge. The following extra scoped checks therefore
ran at the merge SHA; the seven original gate exits and mutation measurements
remain attached to their original measured SHA:

| Additional merge verification | Exit | Measured SHA |
| --- | ---: | --- |
| post-merge-cargo-check | 0 | `86b2a528904f37c2d3cfbf02e758af7e5008afbe` |
| post-merge-native | 0 | `86b2a528904f37c2d3cfbf02e758af7e5008afbe` |

The post-merge commands were the same all-targets cargo check shown above and
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --test ws_lifecycle --jobs 2`.
This evidence is committed after the measured code. Delivery repeats fetch/merge
down and source/test equality proof immediately before push, and reports the
actual delivered SHA without implying gates ran at the later documentation SHA.

Not run: full crate/workspace suites, workspace compilation, host clippy, wasm
clippy, live websocket/API connections, real timer/clock backend, complete
BaseWebSocket disposal, full public/private websocket construction, authentication,
message handlers or async fetch/store bodies, host collection intrinsic
implementations, or inherited shared crypto. These are ten method/control-flow
ports at the documented capabilities. Reproduction runner:
`scripts/check_ws_lifecycle_controls.py`. Raw logs are ignored under
`.aimux/paradex/`; all measured exits, assertion sites, initial instrument failure,
row delta and limits are recorded in this tracked file.


## Eight public subscription and cleanup methods

Measured code/control SHA: `f4ad597a2872de0121fc850d8963c9cb69a1a81b`. Initial parent:
`e4b591f197484f37ff9d1d3acaa40f1620f18ffc` (PR 7481 already merged).
All seven gates and the eight-control runner measured this code SHA; HEAD held
through the measurements. This evidence is committed afterward, so its later
commit is not presented as a SHA at which these gates ran.

Moved **8 pending -> ported, 0 -> divergent**. Python 3.14 tomllib parsed all
eight Paradex manifests at the initial parent and measured code: **37/34/82 ->
45/34/74** (ported/divergent/pending), total **153**. The changed status-key set
independently asserted exactly these eight pending -> ported transitions:

- `ParadexPublicWebSocket#sendJsonRpcSubscription`
- `ParadexPublicWebSocket#sendTradeSubscribe`
- `ParadexPublicWebSocket#subscribeToChannels`
- `ParadexPublicWebSocket.subscribeToOrderBook`
- `ParadexPublicWebSocket.unsubscribeFromOrderBook`
- `ParadexPublicWebSocket.subscribeToTrades`
- `ParadexPublicWebSocket.unsubscribeFromTrades`
- `ParadexPublicWebSocket#cleanupOhlcvSubscription`

| Gate | Exit | Command |
| --- | ---: | --- |
| cargo-check | 0 | `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2` |
| subscriptions-native-and-controls | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_public_subscriptions_controls.py` |
| extractor | 0 | `node "${YARN_CLI}" parity:extract:check` |
| structure | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py` |
| buckets | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py` |
| fmt | 0 | `cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check` |
| diff | 0 | `git diff --check` |

Environment: `SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript`.
Cargo invocations ran sequentially with `--jobs 2`; the separate venue parity
utility uses `--jobs 1`. No full suite ran.

| Production mutation | Named native assertion | Mutation exit | Restored exit |
| --- | --- | ---: | ---: |
| rpc-id | `rpc_native_counter` | 101 | 0 |
| trade-channel | `trade_native_channel` | 101 | 0 |
| book-sub | `book-sub_native_method` | 101 | 0 |
| book-unsub | `book-unsub_native_method` | 101 | 0 |
| count-sub | `count-sub_native_count` | 101 | 0 |
| count-unsub | `count-unsub_native_count` | 101 | 0 |
| cleanup-version | `cleanup_native_version` | 101 | 0 |
| broadcast-channel | `broadcast_native_channel` | 101 | 0 |

Baseline exit **0**. Each of the eight production variants compiled, failed its
named native assertion (**101**), was restored, and passed (**0**). **No survivors
among eight controls attempted**. Enumeration stopped at one control per newly
ported row, not an exhaustive mutation search. Reproduction runner:
`scripts/check_public_subscriptions_controls.py`.

Initial instrument failure: the pre-commit native run exited **101** while
`serde_json` decoded the oracle stdout, with `unexpected end of hex escape`,
column 73373. The oracle's supplemental decoded-wire channel observation still
contained a raw lone surrogate. That is a valid source string/JSON escape that
the UTF-8 Rust Value surface cannot retain, not invalid source JSON. Production
assertions were not reached. At this point the new helpers/tests/oracle were
uncommitted on parent `e4b591f197`; the exit is **not** claimed to cover that
parent's committed tree. Only this observation changed from a raw channel string
to UTF-16 code units. Source bodies, corpus inputs and production logic stayed
unchanged; the corrected native run passed. The later pending-field assertions
also strengthened the native observer before the measured commit. No assertion
or acceptance case was removed to repair the decoder failure.

The actual-source oracle executes the exact eight class AST members and original
getOrderBookChannel declaration. Each live input is independently admitted before
production. Fixed floors are **1378 cases**, with separate completion guards for
1322 simple cases and 56 broadcast cases:

| Operation | Independent case floor |
| --- | ---: |
| JSON-RPC subscription | 504 |
| Trade-send deduplication | 270 |
| Orderbook subscribe / unsubscribe | 24 / 24 |
| Trade reference count subscribe / unsubscribe | 120 / 120 |
| OHLCV cleanup | 260 |
| Channel resubscription | 56 |

Native request ID/method/channel/JSON-RPC fields and pending ID/attempt/channel
values are asserted before supplemental representations. Native counters retain
binary64 bits, including signed zero, NaN, infinity and the 2^53 increment
boundary; only NaN is canonicalized. Optional attempts default only on absence.
Reference counts and generations apply source numeric truthiness before arithmetic.
Host/error pointer identity, current collection handle, callback counts, partial
failure state, force behavior and current log values are observed. Source method
returns are checked as undefined, corresponding to native unit success.

Channel broadcasting captures the markets collection once. Its first Array
forEach visits BBOs before a second forced-trade pass. Growing fixtures exclude
new entries from the first captured-length pass, admit them on the second pass,
and observe later growth in the logged current length. Replaced symbol values
and live Map insertion/removal during book resubscription are also observed.
The host forEach capabilities bind those original intrinsic boundaries. No
separate Rust-owned mirror of the Store, maps or callback sets is introduced.

Symbols, channels and polling keys at these helper/host-capability edges retain
UTF-16 code units, including lone surrogates. The generic orderbook delegates
retain the original symbol/channel host values and invoke the declared helper
capability; this does **not** claim an end-to-end binding through the separately
mapped UTF-8 Rust get_order_book_channel target or alter its divergence row.
These helpers do not change the shared domain String/Value surface or convert
Store/domain models. JSON.stringify/send is a documented request capability;
this is not a new serializer or live websocket transport claim. Source native
request fields are captured before that intrinsic's wire output; supplemental
wire observations preserve strings as code units rather than replacing units.

Structural scope: **150** venue rows / **102** public Rust items; **eight** source
files through actual extractFile, zero symbol identity drift; **two** direct
layer edges. No new divergent rows or venue exemptions. The two API target rows
and unchanged inherited shared crypto target remain outside the narrowed Rust
scope; source extraction still covers every venue file.

Integration `19cb1a2b735c3bc6db40bbb0cee849121cfd985b` was merged down at
`2480793e4282cf5f42b05ded09791cdf9cb2ced6`. The following measured comparison exited **0**, proving
tested venue source/tests/scripts/manifests/TypeScript/LogSeverity inputs remained
byte-unchanged:

```sh
git diff --exit-code f4ad597a2872de0121fc850d8963c9cb69a1a81b 2480793e4282cf5f42b05ded09791cdf9cb2ced6 -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex packages/logging/src/LogSeverity.ts
```

Cargo.lock and shared numeric/type code changed in that merge. Extra scoped
checks therefore ran at the merge SHA, separately from the original measurements:

| Additional merge verification | Exit | Measured SHA |
| --- | ---: | --- |
| post-merge-cargo-check | 0 | `2480793e4282cf5f42b05ded09791cdf9cb2ced6` |
| post-merge-native | 0 | `2480793e4282cf5f42b05ded09791cdf9cb2ced6` |

Commands: the all-targets cargo check above, and
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --test public_subscriptions --jobs 2`.
Delivery repeats fetch/merge down and the required source/test equality proof
immediately before pushing; the delivered SHA is reported on the thread without
implying these checks ran at the later documentation/delivery SHA.

Not run: full crate/workspace suites, workspace compilation, host clippy, wasm
clippy, live API/websockets, real timer backend, complete public websocket
construction/transport/auth/message runtime, OHLCV poll/recovery bodies,
subscription retries or listener lifecycle, native host Array/Map/Set intrinsic
implementations, original UTF-8 channel helper integration, or inherited shared
crypto. These are eight method ports over the documented original capabilities.
Raw logs remain ignored under `.aimux/paradex/`; reviewable measured exits,
controls, initial instrument failure, counts and exact SHAs are committed here.


## Private authentication and order signing method ports

Moved **8 pending -> ported, 0 -> divergent**, independently measured with
Python 3.14 `tomllib` over all eight Paradex manifests against parent
`d4d9ae0b019e4c5466a67294aa474f897ee38061`. Census **45 / 34 / 74 -> 53 / 34 / 66**
(ported / divergent / pending), **153** rows throughout. The changed-status set
is pinned independently to exactly the following eight symbols:

- `ParadexRequestSigner.constructor`
- `ParadexRequestSigner#toChainQuantum`
- `ParadexRequestSigner.signOrder`
- `ParadexRequestSigner.signModifyOrder`
- `ParadexJWTManager.constructor`
- `ParadexJWTManager.getToken`
- `ParadexPrivateWebSocket.onOpen`
- `ParadexPrivateWebSocket#authenticate`

Measured gate SHA: **`b54758046e2d12e97e252bbb6e78338d970072b9`**. The original code commit is
`c2a58760565b1b12f11c20eb367c3415283afdc1`; two control-runner fixes and one
log-lookup observer improvement precede this measured SHA. This is a fresh slice
from the supplied integration parent, without unreviewed foreign work as its
starting ancestor. Later merge/documentation/delivery SHAs do not pretend to be
the SHA at which these seven gates ran.

| Gate | Measured exit |
| --- | ---: |
| scoped-cargo-check | 0 |
| private-auth-controls | 0 |
| extract-check | 0 |
| structure | 0 |
| divergence-notes | 0 |
| rustfmt | 0 |
| diff-check | 0 |

Commands, in that order:

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_private_auth_controls.py
node "${YARN_CLI}" parity:extract:check
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py
cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check
git diff --check
```

The native tests and structural source extractor use
`SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript`.
The explicit Yarn invocation returned 0 and printed a successful extractor
check; no Volta-shim exit is interpreted as proof.

### Compiled production controls

One control per new row. Each control reached a compiled runtime test failure,
then the exact production bytes were restored and both scoped native targets
returned 0. The runner checks the named assertion and always runs restoration
before judging the mutation. **No survivors among the eight controls attempted
in this completed sequence.** This is a floor, ending at `modify-side`, rather
than a claim about every possible mutation.

| Control | Named assertion | Mutation exit | Restore exit |
| --- | --- | ---: | ---: |
| signer-constructor | `signer_constructor_native_write_order` | 101 | 0 |
| jwt-constructor | `jwt_constructor_native_url` | 101 | 0 |
| jwt-expiry | `token_native_expiry` | 101 | 0 |
| private-open | `private_open_native_authenticated` | 101 | 0 |
| authenticate-id | `authenticate_native_counter` | 101 | 0 |
| quantum-scale | `quantum_native_scale` | 101 | 0 |
| order-side | `order_native_side` | 101 | 0 |
| modify-side | `modify_native_side` | 101 | 0 |

The constructor control swaps ordered account/options writes; its first
successful fixture catches the native write order. Constructor fixtures also
assert resulting native properties and partial failures. JWT controls flip the
native URL branch or alter expiry; websocket controls alter authenticated state
or the native message counter; quantum and signing controls alter native scale
or side. Native payload/state/identity assertions precede supplemental request
and trace comparisons. No encode/decode round trip is used as the value oracle.

### Instrument failures and corrections

These failures remain findings about the instrument, not hidden clean runs:

- Pre-commit source observer exit 1: constructor AST members have no method name;
  the extractor now uses `isConstructorDeclaration`. No source body changed.
- Pre-commit source observer exit 1: a fixture rejected a token promise after a
  synchronous injected token-call throw meant the source never awaited it. Held
  init/auth/token fixtures now reject only when that await was actually reached.
- Two pre-commit cargo invocations exited 101 at compilation: the private-auth
  observer tried serializing Rc handles without serde's rc feature and needed
  explicit error typing/borrowing; the signing observer needed a literal format
  string and a `?Sized` future poll helper. These were test compile rejections,
  with no mutation discrimination claimed. Observers serialize borrowed UTF-16
  values; dependency features and production assertions were not weakened.
- A pre-commit native target exited 101 on trace-only JSON number representation:
  Rust's 1000.0 differed from the source observer's JSON integer 1000. Both
  observers now retain numeric trace values as f64 bits. Native numeric scale,
  expiry, counter and timestamp assertions still inspect the original values.
- A pre-commit signing target exited 101 when its error observer could not
  classify an actual BigNumber constructor rejection for malformed surrogate
  text. BigNumber 11.1.5, resolved from the actual source location, throws there
  before the method's finite guard. The source observer retains that dependency
  error and native dependency receipts now reproduce its original error identity;
  source input, source body and production guard logic were unchanged.
- At `c2a58760565b1b12f11c20eb367c3415283afdc1`, scoped cargo-check was 0,
  native baseline was 0, but the control runner exited 1 before mutation because
  its signer-constructor search also matched the JWT constructor prefix. No
  compiled mutant is claimed at that SHA.
- At `42530873ae4ecdef0099b0ff18da864a478f062e`, scoped cargo-check and native
  baseline were 0. The constructor mutant compiled and exited 101 at
  `signer_constructor_native_write_order`; the runner exited 1 because it had
  expected the later properties assertion. Production was restored automatically,
  but that aborted sequence did not run its post-control restore test. The
  correction names the actual ordering assertion and always measures restoration;
  no test assertion changed.
- At `3726fda99f0b30dd9d6ed4a7f26c840f6526ccd6`, all seven gates and eight
  controls passed their expected exits. Subsequent review added the original
  `parent.log` lookup before error interpolation, including lookup failure, and
  renamed the opaque prepared-log capability to describe that order. All seven
  gates and eight controls were rerun at the measured SHA above, rather than
  attributing the earlier results to changed test inputs.

No pre-commit failure is attributed to the committed starting integration tree.
Raw historical measurements remain ignored under `.aimux/paradex/`; these
reviewable SHA-bound results and qualifications are committed here.

### Actual-source scope and native observations

The oracle extracts and executes the original five auth/constructor bodies and
three signing bodies through TypeScript AST/transpileModule. It does not replace
the method bodies. The independently pinned floor is **599 observations**:

| Category | Cases | Last enumerated case |
| --- | ---: | --- |
| Request-signer constructor | 6 | provided options / options setter failure |
| JWT constructor | 32 | provided options / testnet / base URL setter failure |
| JWT token modes | 14 | token return getter failure |
| JWT concurrent races | 2 | external clear between callers |
| Private onOpen | 16 | disposed / auth-async failure capability remains unread |
| Private authenticate | 131 | initialized SDK / JWT manager / infinite request ID |
| Chain quantum | 90 | malformed surrogate input / error-factory failure capability |
| Order signing | 154 | BUY / LIMIT / infinite timestamp |
| Amendment signing | 154 | BUY / LIMIT / infinite timestamp |

Constructors preserve default options identity, ordered writes, UTF-16
subkey/address and exact live/test URLs. They port parameter defaults/body
writes; class allocation and class field initializers remain on pending class
rows. JWT getToken pins cache threshold, shared refresh deduplication, held
settlements, expiry writes, error logging, logger/finally error replacement and
unconditional finally clearing. A first creator may clear a newer refresh slot
after external clearing starts a second creator, matching the source.

Private onOpen pins the disposed guard, auth flag/id clearing and ordered info
log/ping/authenticate await. Authentication observes held init/token phases,
optional manager, native bearer/request/id fields and counter, signed-zero/large/
NaN/infinite IDs, getter/send failures and visible catch logging. The prepared-log
capability selects the original parent receiver/method before interpolation;
lookup, interpolation and logging failures propagate without silent success.

The quantum body preserves finite/NaN short circuit, negative and negative-zero
rejection, full UTF-16 validation messages and invocation of original BigNumber
operations: scale 1e8, ROUND_DOWN 1, fixed places 0. The source uses the installed
BigNumber; native tests pin independent decimal receipts and opaque stage
identity. This is guard/invocation parity, **not native decimal arithmetic**.

Signing native DTO assertions cover each domain/type/message member before
supplemental observations: timestamp, market, side, type, size, price and
amendment id, with six Order fields and seven ModifyOrder fields. Source method
control flow retains market/empty-price skipping, missing amendment id rejection
before clock, held signer await, signature/header identity, exact timestamp bits
and placement catch logging versus amendment propagation. Quantum conversion,
chain-id conversion and cryptographic signing bind the original dependency
boundaries, not newly invented algorithms. The inherited signParadexTypedData
FINDING and pending chain-id/generate-token/class rows are not promoted by these
method ports. UTF-16 code units remain intact through all observer and helper
string edges; lone-surrogate strings are never decoded into Rust UTF-8 strings.

Structural gate scope: **150 venue rows / 113 public Rust items**, eight actual
source files with zero symbol identity drift, two direct layer edges, zero venue
exemptions. The two API target rows and inherited shared crypto target remain
outside the narrowed Rust scope. Divergence census remains 33 CHOSEN + 1 FINDING,
51 target references and zero missing divergent targets.

### Integration merge verification

Integration `ef0963be66a6e6f5437788af56e43edf09e0915b` was merged down at
`e6e95a0a194006e98ca3ccf106ba6ab98f1b521a`. This measured comparison exited **0**, proving venue
source/tests/scripts/manifests and original Paradex/LogSeverity source inputs
remained byte-unchanged:

```sh
git diff --exit-code b54758046e2d12e97e252bbb6e78338d970072b9 e6e95a0a194006e98ca3ccf106ba6ab98f1b521a -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex packages/logging/src/LogSeverity.ts
```

Cargo.lock changed in that merge, so additional scoped checks ran at the merge
SHA, separately from the seven original measurements:

| Additional gate | Exit | Measured SHA |
| --- | ---: | --- |
| post-merge-cargo-check | 0 | `e6e95a0a194006e98ca3ccf106ba6ab98f1b521a` |
| post-merge-native | 0 | `e6e95a0a194006e98ca3ccf106ba6ab98f1b521a` |

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --test private_auth --test signing --jobs 2
```

Delivery repeats fetch/merge-down and the required source/test equality proof
immediately before push. The delivered SHA is reported on the thread; these
measurements are not represented as runs at a later documentation/delivery SHA.

Not run: full crate/workspace suites, workspace compilation, host clippy, wasm
clippy, package typecheck, live APIs/websocket transport, actual clock/Promise
backend, native BigNumber arithmetic, cryptographic signer, SDK module loading
or JWT generation, complete class/runtime construction, chain-id conversion,
or complete SDK/private-websocket composition. This slice ports eight method
bodies over their declared original dependency/property capabilities.

A subsequent delivery merge of integration `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444` produced
`d9c694ec2f168cbe1152625c88cc47bd539f4ce9`. The same required venue-input byte comparison exited 0.
Cargo.lock changed again, so the identical scoped all-targets cargo check and
private_auth/signing native targets were rerun at **that merge SHA**, both exit 0.
These later checks do not claim to run at a subsequent evidence/delivery commit.


## Private websocket ingress and native updates

Moved **8 pending -> ported, 0 -> divergent**, measured independently with
Python 3.14 `tomllib` over all eight Paradex manifests against starting parent
`c9f9322af452ee70b2d311e479b28aafbf9a82ee`. Census **53 / 34 / 66 -> 61 / 34 / 58**
(ported / divergent / pending), **153** rows throughout. The independently
pinned changed-status set is exactly:

- `ParadexPrivateWebSocket.constructor`
- `ParadexPrivateWebSocket.onMessage`
- `ParadexPrivateWebSocket#handleOrderUpdate`
- `ParadexPrivateWebSocket#formatCancelReason`
- `ParadexPrivateWebSocket#handlePositionUpdate`
- `ParadexPrivateWebSocket#handleBalanceUpdate`
- `ParadexPrivateWebSocket#throttledFetchPositions`
- `ParadexPrivateWebSocket#mapOrderStatus`

The seven gates below ran at **`5444b9b2598cb2f52912e8333b46f20be35ba129`**, after original code commit
`bcb1d40ff11ac8337de87de890ba20607c598e50` and the explicit mapping/value-retention
correction. This fresh branch starts at the supplied integration parent, without
unreviewed foreign work as a starting ancestor. Later merge/documentation and
push SHAs do not pretend these measurements ran at those later SHAs.

| Gate | Measured exit |
| --- | ---: |
| scoped-cargo-check | 0 |
| private-ingress-controls | 0 |
| extract-check | 0 |
| structure | 0 |
| divergence-notes | 0 |
| rustfmt | 0 |
| diff-check | 0 |

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_private_ingress_controls.py
node "${YARN_CLI}" parity:extract:check
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py
cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check
git diff --check
```

Native tests and structure extraction bind
`SAFE_CEX_TYPESCRIPT_PATH=${DEPENDENCY_ROOT}/node_modules/typescript`.
The explicit Yarn extractor check returned 0 and its output reported success;
no Volta-shim result is interpreted as a gate.

### One compiled control per row

Each control reached its named compiled runtime assertion with exit **101**.
Exact production bytes were restored automatically, followed by a measured
native target exit **0**. **No survivors among the eight controls attempted in
this completed sequence.** The runner checks restored bytes and the unchanged
HEAD; it measures restoration before judging each mutation. The enumeration
ends at `balance-upnl`; this is a floor, not every possible mutation.

| Control | Native assertion | Mutation exit | Restore exit |
| --- | --- | ---: | ---: |
| constructor-binding | `constructor_native_bindings` | 101 | 0 |
| status-filled | `status_native_value` | 101 | 0 |
| cancel-reason | `reason_native_value` | 101 | 0 |
| position-throttle | `throttle_native_before_state` | 101 | 0 |
| message-auth | `message_native_authenticated` | 101 | 0 |
| order-filled | `order_native_filled` | 101 | 0 |
| position-notional | `position_native_notional` | 101 | 0 |
| balance-upnl | `balance_native_upnl` | 101 | 0 |

The controls change a constructor channel binding, filled status mapping,
cancel-reason text, exact immediate-fetch threshold, authenticated state,
filled arithmetic, position notional arithmetic or balance upnl. The throttle
control fails at captured native scheduled/execution state; every DTO member,
helper value, auth flag and identity observation precedes supplemental request,
log and trace comparisons. No production encode/decode round trip is the oracle.

### Failures and corrections retained

- A pre-commit scoped library compile exited 101 on a stale emit_rejection trait
  signature after its call was changed to an owned UTF-16 message. The production
  declaration was corrected; this ordinary development compile failure is not
  described as a mutation proof or an observer failure.
- A pre-commit native target exited 101 at compilation because a constructor
  fixture closure needed an explicit Result<(), Error> type. No compiled runtime
  rejection is claimed for that attempt.
- Two pre-commit native runs exited 101 at the same message-call discrepancy.
  The second added the input profile to the assertion diagnostic, identifying
  `message_native_calls_unknown-channel_none`: the source fixture replaced a
  local params object but the event still referenced the original orders.ALL
  params. The independently constructed native fixture used the intended unknown
  channel. Fixing the source fixture's event wiring resolved the discrepancy;
  source method bodies and production were unchanged, and no assertion weakened.
- Dynamic trace keys were proactively represented as UTF-16 units in both
  observers before the first decoded runtime baseline. A channel may contain a
  lone surrogate; no new serde lone-surrogate rejection is claimed here. Payload,
  log and helper string observations likewise retain units instead of replacing
  or decoding them as Rust UTF-8 strings.
- At `bcb1d40ff11ac8337de87de890ba20607c598e50`, scoped cargo-check, all eight
  compiled controls/restores and extractor check were 0. The structural gate
  then exited 1 because 51 real public host-trait associated types/methods lacked
  rs mappings; the sequence stopped there. No seven-green-gate claim is made
  at that SHA. The missing associated items are now explicitly mapped only to
  the method bodies consuming them, with full renamed targets in notes. No
  exemption, scope exclusion or weakened structural check was introduced.
- Subsequent review removed a bool projection from the flags.includes capability.
  After the first flags getter passes Array.isArray, a second getter can return
  an object with an includes method returning a string. The native capability and
  record now retain that opaque return, and 25 additional actual-source cases
  observe it. This was a production interface correction found during review,
  not a measured failure attributed to the previous committed input set. All
  seven gates and all eight controls were rerun at the measured SHA above.

Pre-commit failures are not attributed to the committed starting integration
parent. Historical gate/control metadata is retained under ignored
`.aimux/paradex/`; the reviewable exits, qualifications and exact SHAs are here.

### Actual-source floor and native values

The oracle extracts the eight original members through the TypeScript AST and
executes their actual bodies. Enum declarations, URL/side constants, trigger
predicate and createPositionId are extracted from their actual source too.
Original properties, methods, parser, timer and intrinsic boundaries are bound
as explicit capabilities. No source method body is substituted.

The independent enumeration floor is **2,346 cases**, ending on these inputs:

| Category | Cases | Last enumerated input |
| --- | ---: | --- |
| Status and cancel-reason helpers | 54 | formatCancelReason / empty string |
| Constructor | 24 | testnet / handler setter failure / epoch 1 |
| Position-fetch throttling | 960 | NaN clock / all booleans true / execute fault |
| Order records | 350 | changing type getter / pending-write fault remains unread |
| Position records | 250 | changing side getter / pending-write fault |
| Balance records | 300 | late-getter profile / pending-write fault remains unread |
| Message dispatch | 408 | legacy format / error.message fault remains unread |

Constructor tests inspect lexical parent identity, URL map selection before
options lookup, native channels/dispatch kinds and partial writes. Handler
invocation happens after methods are supplied, in two epochs, preserving dynamic
lookup and ping/pong zero-argument calls versus raw data arguments. Base class,
field initializers and complete class allocation are separate pending class work.

Both literal-map helpers preserve inherited Object prototype values rather than
narrowing them to a status enum or message string. Own keys map locally; unknown
truthy prototype functions/objects retain native identity, while falsy values
fall back. Cancel-reason replacement retains every UTF-16 unit. Unicode lowercase
expansion remains an original String intrinsic capability, with selected source
receipts including dotted I, Deseret and lone-surrogate input; no native Unicode
implementation/version equivalence is claimed.

Message dispatch inspects native auth/close/subscription state, handler argument
identity, initial-length Array.forEach growth and fresh data.params reads after
callback replacement. Auth result handling, disconnected close/return, error
reset and legacy dispatch preserve source getter and callback order. Null result
property failures propagate visibly as original host errors.

The native order DTO checks every member before supplemental comparisons: raw
id/symbol/side/type/status/timeInForce/info values, exact numeric price/amount/
filled/remaining/lastUpdate and reduceOnly. A changing size or remaining getter
is not cached. A boolean reduce_only test may be followed by a string-valued
second read; a changed flags getter can likewise return a string from includes.
Those values reach the reverse-type dependency and record without bool coercion.
Cancellation text is built natively before the original notification call, then
warning/store/info log/refresh follow source order, including partial failures.

Position DTO checks cover id/symbol/info identity, both side reads, contracts
abs, native notional, exact PnL/leverage/entry/liquidation floats, cross/oneway
values and arbitrary last-updated/created metadata. Invalid/closed/zero paths
skip pending/store/refresh, while NaN does not falsely satisfy the zero guard.
Original helper, parser and logging implementations remain capabilities.

Balance selection uses original Array.find with the native truthy/object
predicate and optional nested values. All candidate getters run before the first
String/Number.parseFloat conversion. Only finite candidates survive; native
fallbacks preserve total/free/used/upnl, including positive zero from
Math.max(total-free,0). The original positions.length no-op getter still runs
before update. Invalid/caught paths emit the original visible logs.

These generic DTO/host edges preserve arbitrary host metadata and UTF-16 values;
no shared String/Value domain conversion, decoder, Store mirror or complete
websocket runtime is claimed. The 51 mapped associated items describe original
capability requirements, not implementations of their dependency bodies.

Structural scope: **150 venue rows / 181 public Rust items**, eight actual source
files with zero identity drift, two direct layer edges, zero venue exemptions.
Two API target rows and the inherited shared crypto target remain outside the
narrowed Rust scope. Divergence census remains 33 CHOSEN + 1 FINDING.

### Merge-bound verification

Integration `65e086a3c9039a79e4c7e9f354aba31d8f83009e` was merged down at
`40cbc1b37968cfcfea55c4e53417debb7e6eec02`. The expanded venue-input comparison exited **0**:

```sh
git diff --exit-code 5444b9b2598cb2f52912e8333b46f20be35ba129 40cbc1b37968cfcfea55c4e53417debb7e6eec02 -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex packages/safe-cex/src/types.ts packages/safe-cex/src/utils/createPositionId.ts packages/logging/src/LogSeverity.ts
```

This includes the newly consumed original types.ts enums and createPositionId
source, in addition to venue source/tests/scripts/manifests and LogSeverity.
Cargo.lock changed, so additional scoped checks ran at the merge SHA:

| Additional gate | Exit | Measured SHA |
| --- | ---: | --- |
| post-merge-cargo-check | 0 | `40cbc1b37968cfcfea55c4e53417debb7e6eec02` |
| post-merge-native | 0 | `40cbc1b37968cfcfea55c4e53417debb7e6eec02` |

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --test private_ingress --jobs 2
```

Delivery repeats fetch/merge-down and the required gate-SHA source/test proof
immediately before push; delivered SHA is reported on the thread. These checks
are not claimed at a later documentation/delivery SHA.

Not run: full crate/workspace suites, workspace compilation, host clippy, wasm
clippy, package typecheck, live APIs/websockets, actual JSON parser/transport/timer
backend, native host property/String/Number/Array/Object intrinsic implementations,
shared UTF-8 Order/Position conversion, Store/runtime construction, executePositionFetch,
fetchThrottledOrders or complete private websocket composition. This slice ports
eight named method bodies over their declared original capabilities.

A subsequent delivery merge of integration `82274b95ecbae81c5c8b20c4d3f65d60cf64a360` produced
`ae62b6a030f100e9b4baf88491e04207d0f4055a`. The same expanded venue-input byte comparison exited 0.
Cargo.lock changed again, so the identical scoped all-targets cargo check and
private_ingress target were rerun at **that merge SHA**, both exit 0. These
measurements are not claimed at a subsequent evidence/delivery commit.


## Public feed ingress, retry and candle recovery — eight pending rows

**Moved 8 pending → ported, 0 → divergent.** Independent Python 3.14 tomllib
census over all eight venue manifests: **61 ported / 34 divergent / 58 pending →
69 ported / 34 divergent / 50 pending**, 153 rows in both trees. Baseline
`602630e279`; code and gate SHA **`5de7cde75b450b16fdfede4e046852d3f100f91e`**. The census keys rows by source file
and symbol and asserts the exact eight transitions; duplicated type names across
files are not silently collapsed.

Rows: `ParadexPublicWebSocket.constructor`, `.onMessage`,
`#retrySubscribeChannel`, `#handleBBOUpdate`, `#handleMarketSummaryUpdate`,
`#handleTradesUpdate`, `#updateOHLCVFromTrade`, `#maybeRecoverOhlcvTradeStream`.
No divergence was introduced; existing 33 CHOSEN + 1 FINDING remains.
Structural scope: 150 venue rows / 244 public Rust items, eight actual source
files with zero identity drift, two direct layer edges and zero venue exemptions.
The two API target rows and inherited shared crypto target remain outside the
narrowed Rust inventory scope; their bodies are not newly claimed.

### Seven scoped gates at the code SHA

| Gate | Exit |
| --- | ---: |
| scoped-cargo-check | 0 |
| public-ingress-controls | 0 |
| extract-check | 0 |
| structure | 0 |
| divergence-notes | 0 |
| rustfmt | 0 |
| diff-check | 0 |

Exact commands (environment supplies the existing TypeScript module):

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_public_ingress_controls.py
node "${YARN_CLI}" parity:extract:check
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py
cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check
git diff --check
```

Native baseline 0. Eight compiled production controls, one per new row:

| Control | Named native assertion | Mutation | Restore |
| --- | --- | ---: | ---: |
| constructor-network | `constructor_native_url` | 101 | 0 |
| message-stats | `message_native_stats` | 101 | 0 |
| retry-delay | `retry_native_delay` | 101 | 0 |
| bbo-bid | `bbo_native_fields` | 101 | 0 |
| summary-percentage | `summary_native_fields` | 101 | 0 |
| trades-mark | `trades_native_fields` | 101 | 0 |
| candle-volume | `candle_native_volume` | 101 | 0 |
| recovery-force | `recovery_native_force` | 101 | 0 |

Each mutation reached a compiled runtime failure (`test result: FAILED`) and its
named native assertion. Exact production bytes were restored in a finally block
before the restore target ran. **No survivors among the eight controls attempted
in this completed sequence.** No compiler rejection is counted as runtime proof.

### Native observations and source oracle

`tests/public_ingress_oracle.cjs` extracts the unchanged eight AST members and
original venue constants and LogSeverity. No source input or method-body rewrite
is used. **1,950 cases:** 14 constructors, 270 feed updates, 1,200 retries,
448 recovery cases, nine candle cases and nine message cases. All dynamic string
observations use UTF-16 units; numeric observations use binary64 bits with NaN
canonicalized. Signed zero is preserved; NaN payload equivalence is not claimed.

Independent native assertions run before supplemental comparisons: constructor
URL and parent identity; BBO's exact bid/ask/index fields; every market-summary
field, including native percentage multiplication; trades' last/mark and candle
arguments; retry delay/next attempt, timer identity/registration and deferred
send guard; recovery state, forced subscribe and warning; candle fields, volume,
spread metadata, signed zeros and callback/cache identity; message stats, pending
deletion, callback length and fresh params reads. Supplemental observations also
compare fault paths, partial effects, UTF-16 logs and ordered getter traces.

Native feed methods retain fallback order, finite/positive omissions and visible
caught-error logs. The original throttle factory receives 100ms, leading=true
and trailing=true, and binds the indicated native body. Original parsing,
String/Number/JSON and collection intrinsics remain capabilities. Math's internal
numeric coercion is separate from the original Number call, preserving that
boundary rather than equating their treatment of arbitrary host values.

Retry separates its native immediate and deferred bodies. The original timer
capability receives the native delay and next attempt, binds
`execute_subscription_retry` to the captured channel/timer/live state and
registers the returned original timer handle. The deferred body deletes before
checking disposal and active interest. No actual timer backend is implemented.

Candle traversal retains live map insertion: a callback adding a second matching
subscription is visited in the same pass. Message Array.forEach preserves its
captured length when a callback grows the handler array, and each subscription
callback rereads params when an earlier callback replaces it. Cached candles
preserve the original spread object and overwrite only high/low/close/volume;
new candles initialize all six fields. String-valued volume uses the original
binary-add capability. Original UTF-16 metadata and extra spread fields survive.

### Development/instrument failures retained

These occurred before the code commit and are not measurements of that tree:

- First decoded native run exited 101: the test host parsed array property
  `market` as an integer index and panicked with ParseIntError. Array non-index
  properties now yield undefined, independently confirmed by the source oracle.
- Two decoded runs exited 101 at the supplemental missing-symbol log: the test
  JSON-stringification receipt repeated `index_price`. The first textual repair
  did not match the formatted code; the actual duplicate was then removed.
  No source body, source input or production behavior was changed for this repair.
- A native build exited 101 (E0046) when the newly declared empty-object capability
  lacked its test-host implementation. This was a compile failure, not a control.
- Pre-commit structure exited 1 because the new empty-object associated target
  lacked its exact rename note. The note was corrected without exemptions.
- The decoded native observer then exited 101 comparing JSON numeric ID 7.0 with
  source ID 7 despite equal native ID value. Both supplemental ID observations
  now retain binary64 bits, preserving the value instead of depending on serde's
  integer/float JSON variants. Native ID assertions were not weakened.
- The first census assertion rejected a name-only row map because repeated type
  names collapsed rows below 153. It produced no accepted census. The corrected
  file-plus-symbol census above was measured at the committed code SHA.

Review also removed extra timestamp/open projection from the cached-candle
branch before commitment: the source preserves these through spread. This is a
review correction, not a measured regression attributed to a committed tree.
No post-commit failed sequence is hidden or promoted to seven green gates.

### Integration merge and delivery provenance

Integration `c1b0ce61f2247f5604b26ed5a00b142be75d90ef` was merged down at `12271c0069d00a4f5cf65960bddcf5425f5b81eb`.
The required gate-SHA source/test byte comparison and expanded venue-input proof
exited **0**. The expanded proof covers source, tests, scripts, venue manifests,
original Paradex TypeScript bodies and LogSeverity.

```sh
git diff --exit-code 5de7cde75b450b16fdfede4e046852d3f100f91e 12271c0069d00a4f5cf65960bddcf5425f5b81eb -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex packages/logging/src/LogSeverity.ts
```

Cargo/dependency inputs were byte-unchanged; no redundant native rerun is claimed.

Delivery repeats integration fetch/merge and byte proofs immediately before push.
The pushed SHA is reported on the thread. Seven gate measurements belong to the
code SHA above; any additional merge checks belong only to their stated merge SHA.

Not run: full crate/workspace suites, workspace compilation, clippy, package
TypeScript typecheck, live APIs/websockets, actual parser/transport/timer backend,
a native lodash implementation, native JavaScript intrinsic implementations,
shared domain conversion, complete public websocket construction/runtime,
order-book handling/listening or REST OHLCV polling. This slice changes no shared
type or trait, so the new shared-change workspace-check rule is not triggered.


## Order placement and cancellation commands — nine pending rows

**Moved 9 pending → ported, 0 → divergent.** Independent Python 3.14 tomllib
census across all eight manifests, keyed by file and symbol: **69 ported /
34 divergent / 50 pending → 78 ported / 34 divergent / 41 pending**; 153 rows
in both trees. Exact nine status transitions were independently asserted against
fresh base `dc74077e61`. Code and seven-gate SHA: **`426b0b48ed890ff2461933e82675f689eb2bf9c5`**.

Rows: `ParadexExchange#generateClientId`, `.placeOrder`, `.placeOrders`,
`#placeMainOrder`, `.cancelOrder`, `#cancelNativeBatchChunk`, `.cancelOrders`,
`.cancelAllOrders`, `.cancelSymbolOrders`. No new divergent row/bucket;
existing divergence remains 33 CHOSEN + 1 FINDING.

### Seven scoped gates at the code SHA

| Gate | Exit |
| --- | ---: |
| scoped-cargo-check | 0 |
| order-commands-controls | 0 |
| extract-check | 0 |
| structure | 0 |
| divergence-notes | 0 |
| rustfmt | 0 |
| diff-check | 0 |

Exact commands (environment supplies the existing TypeScript module):

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_order_commands_controls.py
node "${YARN_CLI}" parity:extract:check
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py
cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check
git diff --check
```

Native baseline 0. Nine compiled controls, one per new row:

| Control | Named native assertion | Mutation | Restore |
| --- | --- | ---: | ---: |
| client-id-prefix | `client_id_native_value` | 101 | 0 |
| single-main-id | `place_native_ids` | 101 | 0 |
| mixed-batch-size | `mixed_native_chunks` | 101 | 0 |
| main-response-id | `main_native_id` | 101 | 0 |
| cancel-one-endpoint | `cancel_one_native_endpoint` | 101 | 0 |
| cancel-pair-status | `batch_cancel_native_success` | 101 | 0 |
| cancel-many-stride | `cancel_many_native_chunks` | 101 | 0 |
| cancel-all-endpoint | `cancel_all_native_endpoint` | 101 | 0 |
| cancel-symbol-market | `cancel_symbol_native_config` | 101 | 0 |

Each control reached its named native assertion in a compiled runtime failure
(`test result: FAILED`). Production was restored byte-for-byte in a finally
block before its restore target ran. **No survivors among the nine controls
attempted in this completed sequence.** Compile rejections are not runtime proof.

### Native values, effects and source observations

The unchanged source AST members run in `tests/order_commands_oracle.cjs` with
the original ENDPOINTS, signerHeaders and LogSeverity declarations. Dependency
stubs are installed only for methods outside the member being measured. The
original Error binding is shared with the fixture, so instanceof observes the
actual fixture Error; no cross-VM constructor mismatch is normalized afterwards.

**1,008 source cases:** 63 independently declared method/profile combinations
crossed with 16 fault sites. Per-method source counts are 96 client-ID, 64 single
placement, 144 mixed placement, 224 main placement, 16 single cancellation,
304 paired cancellation, 128 cancellation batches, 16 all-order cancellation and
16 symbol cancellation. A separate native held-future test verifies that logs,
commands and HTTP requests remain absent while SDK initialization is pending,
then checks the resumed canonical ID.

Independent native assertions precede supplemental observations: UTF-16 client
ID and 64-unit boundaries, ordered main/bracket IDs, ten-order batch boundaries
and ordered returned IDs, live array growth and custom iteration, canonical main
response ID, native POST payload/endpoint/header value, four distinct response-ID
reads, echoed-client-ID rejection and emission, exact DELETE paths/configurations,
two-ID correlation, rejection text, cancellation chunk membership, all-chunk
attempts, original single-error identity and aggregate error text. Original caller
options are checked by identity at dependency invocation; caught primary errors
retain native Rc identity.

The client-ID helper counts UTF-16 units, including astral pairs and lone
surrogates. Main placement preserves nullish-only caller-ID fallback, SDK await
before signer/log/HTTP effects, canonical response validation and fresh getter
reads. The repeated response-ID fixture yields separate validation, log and
return values rather than caching the first getter result.

Mixed placement uses original for-of iterator/next/close capabilities, including
custom iteration order, live growth across awaits and abrupt cleanup before a
propagated predicate error. Native batch flush clears its accumulator before
awaiting the existing batch method. Flush errors propagate; only individual
placement errors are visibly logged and skipped. Caller-ID preflight stays
outside the single-placement catch, and the main ID precedes bracket IDs.

Paired cancellation validates the exact request size, non-empty string IDs and
uniqueness before DELETE. Native correlation distinguishes missing, unknown,
repeated and uncorrelated NOT_FOUND results; malformed responses aggregate all
anomalies before rejected-ID handling. QUEUED_FOR_CANCELLATION and ALREADY_CLOSED
both succeed. Original entries iteration and abrupt cleanup remain capabilities.
Multi-order cancellation rereads source array length across awaits, attempts all
chunks, logs each failure, rethrows the exact single error and generates an
aggregate error only after multiple failures. All/symbol/single cancellation
retain distinct native request shapes and visible completion/error logs.

Dynamic observations retain UTF-16 units and binary64 number bits. The test host
binds selected original property, String.replace, String/JSON, Error, collection,
promise and HTTP semantics; it is not a replacement implementation of these
intrinsics. `signerHeaders`, payload building, IOC pricing, native order placement
batches and TP/SL placement remain explicit existing dependencies, with their own
manifest statuses rather than implied ownership by these command rows.

### Development failures and review corrections retained

These occurred before the committed code/gate SHA and are not controls:

- Library compile exited 101 (E0282) because the cancellation-loop result needed
  an explicit `Result<(), H::Error>` type before cloning its error. Fixed locally.
- First native test build exited 101 under `-D warnings`: two unused test helpers
  and an unused fixture field. They were removed; no warning suppression added.
- Expanding the independent ten-order assertion produced native exit 101 at
  `mixed_native_ids`: its expectation still enumerated twelve IDs. The fixture's
  independently declared ten/twelve profile count now sets that expectation.
  Source bodies, source inputs and production were unchanged for this correction.
- The first temporary script-writer invocation had a Python SyntaxError before
  writing the control runner. The corrected writer succeeded; this is not a
  mutation execution or any part of the seven-green-gate claim.

Review before measurement replaced array-index emulation in mixed placement with
original iterator capabilities and added abrupt close. Paired cancellation was
likewise switched to original entries iteration. The oracle's dependency setup
was corrected before its first decoded execution so it does not overwrite the
member being measured; the Error binding was also made explicit before that
execution. No committed-tree failure is invented for these review corrections,
and no source input or assertion was weakened to obtain a passing result.

### Structure and integration provenance

Paradex actual extractFile: 8 files; symbol identity drift 0. No package typecheck claimed.

Paradex exempt blocks: 0

parity: invention and status gates passed (150 rows, 303 public Rust items)

Paradex direct layer audit: 2 edges validated.

Two API target rows and the unchanged inherited shared crypto target remain
outside the narrowed Rust inventory. Full venue census is 153 rows.

Integration `845cd310a963e53fc50dd9180a152e034262b6b4` merged down at `83f6d73dbbf0dd3e32a352d945ee4b9df8f9ded2`.
Required gate-SHA source/test proof and the expanded venue-input proof both
exited **0**. Exact expanded proof:

```sh
git diff --exit-code 426b0b48ed890ff2461933e82675f689eb2bf9c5 83f6d73dbbf0dd3e32a352d945ee4b9df8f9ded2 -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex packages/logging/src/LogSeverity.ts
```

Cargo/dependency inputs changed; additional scoped checks were measured at
that merge SHA, not any subsequent evidence/delivery SHA:

| Additional gate | Exit | Measured SHA |
| --- | ---: | --- |
| post-merge-cargo-check | 0 | `83f6d73dbbf0dd3e32a352d945ee4b9df8f9ded2` |
| post-merge-native | 0 | `83f6d73dbbf0dd3e32a352d945ee4b9df8f9ded2` |

Delivery repeats fetch/merge-down and gate-SHA byte proofs immediately before
push. The pushed SHA is reported on the thread. Seven gates belong only to the
code SHA above, and any additional checks only to their stated merge SHA.

Not run: full crate/workspace test suites, workspace compilation, clippy,
TypeScript package typecheck, live HTTP/API orders or cancellations, real SDK
initialization/signing, a native JavaScript intrinsic/Error/event-loop/transport
runtime, shared domain conversion, complete exchange construction, payload/IOC
pricing/native order-batch/TP-SL dependency implementations. No shared type or
trait changed; the shared-change workspace-check trigger does not apply.


## Authentication initialization and REST headers — eight pending rows

**Moved 8 pending → ported, 0 → divergent.** Independent Python 3.14 tomllib
census across all eight manifests, keyed by file and symbol: **78 ported /
34 divergent / 41 pending → 86 ported / 34 divergent / 33 pending**; 153 rows
in both trees. Exact eight transitions asserted against fresh base `70fa531e1c`.
Code and seven-gate SHA: **`12b7b3dbbcde67e0efd4767599a13dc13de4359c`**.

Rows: `ParadexPrivateHistoryAuthNotReadyError.constructor`,
`isParadexPrivateHistoryAuthNotReadyError`, `wait`, `initializeParadexSDK`,
`ParadexExchange#initializeSDK`, `#ensurePrivateHistoryAuth`, `.validateAccount`,
`authenticateRequest`. No new divergence bucket; 33 CHOSEN + 1 FINDING unchanged.

### Seven scoped gates at the code SHA

| Gate | Exit |
| --- | ---: |
| scoped-cargo-check | 0 |
| auth-initialization-controls | 0 |
| extract-check | 0 |
| structure | 0 |
| divergence-notes | 0 |
| rustfmt | 0 |
| diff-check | 0 |

Exact commands (environment supplies the existing TypeScript module):

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_auth_initialization_controls.py
node "${YARN_CLI}" parity:extract:check
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py
python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py
cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check
git diff --check
```

Native baseline 0. Eight compiled controls, one per new row:

| Control | Named native assertion | Mutation | Restore |
| --- | --- | ---: | ---: |
| history-error-name | `history_error_native_name` | 101 | 0 |
| history-predicate | `history_predicate_native_brand` | 101 | 0 |
| wait-delay | `wait_native_delay` | 101 | 0 |
| sdk-environment | `sdk_native_environment` | 101 | 0 |
| exchange-manager | `exchange_init_native_manager` | 101 | 0 |
| history-retry-delay | `history_auth_native_delay_values` | 101 | 0 |
| validation-success | `validation_native_error_field` | 101 | 0 |
| request-auth-header | `request_auth_native_headers` | 101 | 0 |

Every mutation reached its named assertion in a compiled runtime failure
(`test result: FAILED`). Production was restored byte-for-byte in a finally
block before each restore target ran. **No survivors among the eight controls
attempted in this completed sequence.** Compile rejections are not controls.

### Original source and native observations

**846 source cases:** 150 SDK helper, 150 exchange initialization, 175 history
readiness, 200 account validation, 125 REST authentication, 30 error constructors,
four nominal predicates and twelve waits. These are independently declared
profile/fault counts. Native assertions precede supplemental comparisons.
A separate held-token native test checks that no header receiver is selected,
no header is written and no log is emitted while the token future is pending;
then checks resumed config identity and all three header writes.

The oracle extracts the original declarations/members and original ENDPOINTS,
retry-delay and LogSeverity declarations. **One module-import site** in
initializeParadexSDK, `import('starknet')`, is asserted unique and bound to the
original external module-import capability. No other body rewrite is used.
Existing getParadexSDK, SDK/account/wallet/signing constructors, promises,
headers, intrinsics and console/log receivers remain dependency capabilities;
these helpers do not reimplement those bodies.

Native SDK records check account/config/provider identity and the exact three
fields, prod/testnet selection, subkey versus Ethereum construction and the
payloadSigner early return's lowercase address and absence of key/wallet use.
Original options/config values reach constructors by identity. The Ethereum
account method/receiver is selected before reading its argument properties.
Original config promise-resolution `then` lookups remain part of the dependency
receipt; no native JavaScript promise-resolution algorithm is claimed.

Exchange initialization checks native account/signer/manager slots and original
constructor arguments, ordered credential guards, SDK await, logging and partial
writes. History readiness checks zero/one/two/three initialization attempts,
500ms/1500ms retry delays, resets and exact original final error identity in its
history wrapper. Original rejection observation does not replace the awaited
promise; the error remains observed by the retry/wrapper flow. Missing address
never invokes SDK initialization, while waits and final readiness errors remain.

Error construction checks native parent message, two distinct original message
reads when detail is truthy, exact error name, originalError identity and ordered
partial writes. Predicate checks original nominal instance/prototype semantics,
rather than matching the error name. Complete class/field-initializer construction
remains the separate pending class row. Wait composition checks original promise and
resolver identities, exact binary64 delay (including signed zero,
NaN and infinity), constructor executor failure and settlement boundaries through
the original promise/timer capabilities.

Validation checks native success/error fields, original account endpoint and
visible notification, retaining repeated API-message extraction. REST auth checks
native Authorization/Content-Type/Accept names, values and order, three separately
selected header receivers, exact config identity, empty-token rejection and
partial mutation: second-setter failure retains the Authorization write. Original
header setter selection precedes token interpolation. Dynamic observations retain
UTF-16 units, with binary64 bits for numbers and canonical NaN payloads.

### Development/instrument failures retained

All occurred before the committed code/gate SHA and are not mutation results:

- Native build exited 101 under `-D warnings` for unused array scaffolding copied
  into the test host. The unused enum variant/helper and their arms were removed;
  no warning suppression was added.
- First decoded target exited 101 because its source oracle rejected: the
  existing-promise fixture injected an initialization fault during setup, outside
  the measured member. Setup now constructs that already-existing promise without
  treating setup as a measured initialize call; its await/rejection behavior and
  declared attempt identity are retained in both independent fixtures.
- The next decoded target exited 101 at the SDK trace. Two different observations
  were separated: real original promise-resolution `then` getter reads were
  missing from the native dependency fixture, while the source snapshot observer
  itself read the config proxy and contaminated the method trace. The dependency
  fixture now preserves the real `then` receipt; source snapshots use WeakMap-held
  original targets to avoid observer getter side effects. Source bodies, live
  inputs and production were unchanged for this repair.
- A later decoded target exited 101 at validation: the oracle had omitted the
  original ENDPOINTS binding, so source validation caught a ReferenceError and
  returned 'ENDPOINTS is not defined'. Binding the original declaration repaired
  the instrument without changing source inputs or production.
- A decoded target exited 101 at the partial-header count: that independent
  expectation was mistakenly applied to profiles rejected before any setter ran.
  It now applies to the declared valid-token/basic profile, while missing-manager
  and empty-token profiles retain zero-write observations; the empty-token
  independent assertions remain. The
  actual partial-write assertion was not relaxed.

Review before measurement split Ethereum account receiver/method selection from
its argument reads. This is a review correction, not a fabricated measured
failure of a committed tree. No compiler failure is counted as a runtime control. The rejected committed
sequence below is retained separately from the completed measurements above.

### Rejected committed sequence and stronger native check

At **8d1837d504299a2825ede1843e84f41fa57d5e04**, scoped cargo check exited 0,
but the control runner exited **1**. The first five controls completed compiled
101/restore 0. The sixth, history-retry-delay, compiled and exited 101 at
`auth_supplemental_operations ensurePrivateHistoryAuth/basic/opts.paradexAddress#0`:
a fault fixture observed the corrupted delay before the runner's required native
assertion ran. The runner rejected that assertion mismatch. Production restored
exactly and its restore target exited **0**; this restore did run. The remaining
two controls were not attempted in that sequence. **This was not seven green
gates or a completed eight-control sequence.**

The corrective commit added an independent 500ms/1500ms prefix assertion for
EVERY observed history retry delay, including fault cases, before supplemental
comparisons. The revised control requires `history_auth_native_delay_values`.
No production byte or live source input changed, and no old assertion was
relaxed. All seven gates and all eight controls were rerun at the measured
corrective SHA printed at the top of this section. The rejected sequence's
runner output and partial records remain in ignored raw evidence, and its
failure/restoration are recorded here rather than promoted into a passing claim.

### Structure and integration provenance

Paradex actual extractFile: 8 files; symbol identity drift 0. No package typecheck claimed.

Paradex exempt blocks: 0

parity: invention and status gates passed (152 rows, 351 public Rust items)

Paradex direct layer audit: 2 edges validated.

The real Rust inventory now includes the API manifest, including its remaining
pending createAPI row; it is no longer excluded now that API production targets
exist. All 152 venue rows except the unchanged inherited shared crypto target
are inventoried, with no new venue exemption. Full census is still 153 rows.

Integration `0f7fd2ce92e79d0c38e87ea5dd15b21927ea8c1a` merged down at `4f3c7ae748da45c1d54240d64c218f2dc0962fe5`.
Required source/test and expanded venue-input proofs both exited **0** against
the code/gate SHA. Exact expanded proof:

```sh
git diff --exit-code 12b7b3dbbcde67e0efd4767599a13dc13de4359c 4f3c7ae748da45c1d54240d64c218f2dc0962fe5 -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex packages/logging/src/LogSeverity.ts
```

Cargo/dependency inputs were unchanged; no redundant rerun is claimed.

Delivery repeats integration fetch/merge-down and gate-SHA byte proofs immediately
before push. Pushed SHA is reported on the thread. Seven-gate measurements belong
to the code SHA above; any merge checks belong only to their stated merge SHA.

Not run: full crate/workspace suites, workspace compilation, clippy, TypeScript
package typecheck, real SDK/account/wallet/crypto initialization, actual HTTP/auth,
real timer/promise/event-loop/header/intrinsic implementations, full private error
class or exchange/runtime construction, shared domain conversion or pending
request/JWT signing and API factory bodies. No shared type or trait changed;
the shared-change workspace-sweep trigger does not apply.


## Bootstrap and market data — eight pending rows

**Rows moved: 8; all ported; 0 divergent; 0 regression.** Fresh branch from
`77746a5b49`. Independent tomllib census keyed by manifest path plus TypeScript
symbol across all eight manifests: **86/34/33 → 94/34/25**
(ported/divergent/pending), 153 total rows. The changed set is exactly:

- `ParadexExchange.start`
- `ParadexExchange.tick`
- `ParadexExchange.fetchMarkets`
- `ParadexExchange.fetchTickers`
- `ParadexExchange.fetchOHLCV`
- `ParadexExchange.fetchPositions`
- `ParadexExchange.fetchOrderBook`
- `ParadexPublicWebSocket#handleOrderbookUpdate`

Code and seven-gate measured SHA: **d9d9562845a1af07b9507b1538f2e1a639a699b3**. The measurements
below belong to that code SHA, not a later evidence or delivery commit.

### Seven scoped gates

| Gate | Exit | Command |
| --- | ---: | --- |
| scoped-cargo-check | 0 | `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2` |
| market-data-controls | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_market_data_controls.py` |
| extract-check | 0 | `node "${YARN_CLI}" parity:extract:check` |
| structure | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py` |
| divergence-notes | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py` |
| rustfmt | 0 | `cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check` |
| diff-check | 0 | `git diff --check` |

No full crate or workspace test suite, clippy, TypeScript package typecheck,
real HTTP/trading, real clock/timer/socket implementation, complete exchange
construction or pending method bodies are claimed. No shared type or trait
changed; the shared-change workspace compilation trigger does not apply.

### Eight compiled controls

Baseline target exit 0. Each row has one production mutation compiled into the
native target; each runtime failure reached its named independent native
assertion before supplemental comparisons. Production restored exactly in
`finally`, and each restored target exited 0.

| Control | Native assertion | Mutation | Restore |
| --- | --- | ---: | ---: |
| start-public-connect | `start_native_public_connection` | 101 | 0 |
| tick-delay | `tick_native_schedule` | 101 | 0 |
| market-leverage | `markets_native_max_leverage` | 101 | 0 |
| ticker-clock | `tickers_native_timestamp` | 101 | 0 |
| documented-candle-time | `ohlcv_native_timestamp` | 101 | 0 |
| position-precedence | `positions_native_actual_precedence` | 101 | 0 |
| rest-book-total | `book_native_bid_total` | 101 | 0 |
| book-broadcast | `book_ingress_native_broadcast` | 101 | 0 |

**No survivors among the eight completed controls attempted.** This does not
claim unattempted mutations or a compiler rejection as a runtime control.

### Source oracle and native boundaries

The oracle extracts the eight actual members using the TypeScript AST and
executes their unchanged bodies. It binds actual ENDPOINTS, PositionSide,
PositionMode, MarginType, timeframeToSeconds, createPositionId, omitUndefined,
describeError and LogSeverity declarations. Imported lodash omitBy/sumBy are
original installed dependencies. The public handler's throttle constructor
capability asserts 100ms and leading/trailing true before exposing its original
callback; no actual throttle implementation is claimed.

The decoded target covers **4,320 declared cases**: eight methods × thirty
profiles × eighteen fault selectors. Selectors not reached in a profile remain
successful executions, rather than being counted as injected failures.
Fixtures declare raw input values and dependency replies independently of the
source outcome. UTF-16 code units retain lone surrogates, binary64 bits retain
signed zero and infinities, and NaN observations use a canonical payload.

Native assertions observe public connection state, the actual scheduled delay,
market leverage, ticker timestamp, documented candle timestamp, actual-position
precedence, REST bid total and captured native broadcast books before any
supplemental result comparison. Further native checks cover disposal update
counts and absence of tick timers after disposal, original position-info
identity, live callback-set growth and shared book identity across callbacks,
and original failure identity. Supplemental receipts compare complete outputs,
HTTP endpoints/params, dependency order, store writes, timers, ticker updates,
broadcasts, logs and disposal state.

Rust owns the bootstrap order and disposal branches; tick's balance upnl write
and recurring schedule; active perpetual filters and leverage transforms;
ticker fields including virtual-clock timestamps; documented candle validation
and legacy fallback; actual-over-placeholder position map precedence; REST
book totals/spread diagnostics; and public book snapshot/delta normalization,
price sorting and broadcasts. Original property, array/reduce/iterator/sort,
map, promise, logging, store, clock, callback and imported dependency
capabilities remain explicit. No shared string/model boundary was weakened.

### Development and instrument failures retained

These failures occurred before the code SHA above, and are not attributed to
that committed tree or counted as controls:

- An initial source-only harness trial exited 0, but review found dependency
  assignment had overwritten some measured members with stubs. The harness now
  captures each actual implementation before dependency setup and restores it
  before calling the measured member. This was corrected before the first
  decoded native comparison. The source-only trial is not parity evidence and
  its exit 0 was not used to claim a passing gate.
- Library compile exited 101 under `-D warnings` for an unused helper. The
  orphan helper was removed; no warning suppression was added.
- Decoded target exited 101 at startup error logging: the native fixture
  formatted an original Error as a plain object. Its error value now retains
  the original Rc error and original Error stringification/message behavior.
  Source bodies and production were unchanged by this fixture repair.
- Decoded target exited 101 in OHLCV fallback: the source harness omitted the
  imported omitBy required by the original omitUndefined declaration. Binding
  the installed original dependency repaired the observer. No source-body or
  production change was used to make that observation pass.
- Decoded target exited 101 at numeric REST-book logs: the native toFixed
  dependency fixture rendered Infinity as Rust's `inf`. The fixture now uses
  original JS non-finite number text; the numerical profile and production
  computation were unchanged.
- After widening the oracle, native fixture compilation exited 101 because
  String::from_utf16_lossy returns String, not Cow, and an into_owned call was
  invalid. Removed that call without suppressing compiler diagnostics.
- A separate pre-commit mutation-needle validation command hit a Python
  SyntaxError before evaluating any site. It was corrected and confirmed
  eight unique production sites. No mutation was executed by that command.

Review before measurement made status-count reduction native, including
repeated row-key reads and inherited accumulator semantics, and moved
position-map writes into native callbacks over original iteration. REST pair
destructuring now captures price/size once before repeated parseFloat calls.
Native control observations use actual connection/delay/book state rather than
serialized receipts. These are review corrections, not fabricated failures of
a committed tree. No assertion or source input was weakened.

### Structure and integration provenance

Actual extractFile covers all eight TypeScript files, symbol identity drift 0.
Scoped invention/status check: **152 venue rows / 397 public Rust items**,
excluding only the unchanged inherited signParadexTypedData shared crypto row.
Full census remains 153. **0 venue exemptions**, two direct layer edges valid.
Every new capability inventory item is mapped to actual source consumers.

Integration `2a24bb3c48b25e781167071443dd603b0d0d2b8e` merged down at `b0ada37c9dc7c908fa36b495c7894f2c6a96eca5`.
Required source/test byte proof exit **0**; expanded
venue/source-input proof exit **0**, against the code SHA.

```sh
git diff --exit-code d9d9562845a1af07b9507b1538f2e1a639a699b3 b0ada37c9dc7c908fa36b495c7894f2c6a96eca5 -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex packages/logging/src/LogSeverity.ts packages/safe-cex/src/types.ts packages/safe-cex/src/utils/createPositionId.ts packages/safe-cex/src/utils/omitUndefined.ts
```

Cargo/dependency inputs changed. Scoped all-targets compile and the new native
target were rerun at the merge SHA above, rather than attributed to delivery:

| Merge check | Exit |
| --- | ---: |
| scoped-cargo-check | 0 |
| market-data-native | 0 |

Delivery repeats fetch/merge-down and both code-SHA byte proofs immediately
before push. The full pushed SHA is reported on the thread.


## Authenticated account REST — eight pending rows

**Rows moved: 8; all ported; 0 divergent; 0 regression.** Fresh branch from
`671cdadf56`. Independent tomllib census keyed by manifest path plus TypeScript
symbol across all eight manifests: **94/34/25 → 102/34/17**
(ported/divergent/pending), 153 total rows. The exact pending-to-ported set:

- `ParadexRequestSigner.signRequest`
- `ParadexRequestSigner#toChainIdHex`
- `ParadexJWTManager#generateNewToken`
- `ParadexExchange.fetchIncomeHistory`
- `ParadexExchange.fetchHistoricalOrders`
- `ParadexExchange.fetchHistoricalFills`
- `ParadexExchange.setLeverage`
- `ParadexExchange.setMaxSlippage`

Code and seven-gate measured SHA: **8330e6d92f80aa8f630b6e4f7f119bc77529d1a8**. These measurements
belong to that code SHA, not a later evidence or delivery commit.

### Seven scoped gates

| Gate | Exit | Command |
| --- | ---: | --- |
| scoped-cargo-check | 0 | `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2` |
| account-rest-controls | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_account_rest_controls.py` |
| extract-check | 0 | `node "${YARN_CLI}" parity:extract:check` |
| structure | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py` |
| divergence-notes | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py` |
| rustfmt | 0 | `cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check` |
| diff-check | 0 | `git diff --check` |

No full crate/workspace suite, workspace compilation, clippy, TypeScript package
typecheck, actual HTTP/auth/trading, actual cryptography, real SDK imports or
complete pending class/API-factory composition is claimed. No shared type or
workspace trait changed. The new host trait is venue-local; the shared-change
workspace sweep trigger does not apply.

### Eight compiled controls

Baseline target exit 0. Each row has one production mutation compiled into the
native target. Each runtime failure reached its named independent native
assertion before supplemental comparisons. Production restored exactly in
`finally`, then each restored target exited 0.

| Control | Required native assertion | Mutation | Restore |
| --- | --- | ---: | ---: |
| request-message | `request_native_message` | 101 | 0 |
| chain-id-hex | `chain_native_hex` | 101 | 0 |
| jwt-expiration | `jwt_native_expiry_header` | 101 | 0 |
| income-sort | `income_native_sorted_first` | 101 | 0 |
| orders-sort | `history_orders_native_sorted_first` | 101 | 0 |
| fills-timestamp | `history_fills_native_seconds` | 101 | 0 |
| leverage-cache | `leverage_native_cached_value` | 101 | 0 |
| slippage-payload | `slippage_native_payload` | 101 | 0 |

**No survivors among the eight completed controls attempted. Compiler rejects
and a 101 from an unrelated site are not included in that claim.**

### Actual-source oracle and native observations

The source oracle extracts the eight actual AST members. Seven bodies execute
unchanged; signRequest replaces exactly one asserted dynamic import site,
`import('starknet')`, with its explicit loader capability `loadStarknet()`. No
other method-body rewrite occurs. The harness binds actual ENDPOINTS, ORDER_SIDE,
PositionSide/PositionMode/MarginType/OrderSide/OrderType, omitUndefined and
LogSeverity declarations. Its widened auth fixtures execute the actual private
error-class declaration and nominal instanceof predicate, with a harness-only
export exposing that unchanged private declaration. Original installed omitBy
is bound. Measured implementations are captured before dependency setup and
restored before invocation.

The target executes **1,112 declared cases**: 296 baseline cases (8 × 37
profiles), plus 816 cases (8 × 6 selected profiles × 17 fault selectors). A
selector not reached remains a successful execution, rather than being counted
as an injected failure. This uses representative fault profiles instead of
repeating the whole profile cross-product in every control.

Fixtures share declared raw inputs/dependency replies independently of the
source outcome. Object keys AND values preserve UTF-16 code units, including
lone surrogate market-map keys. Numeric receipts retain binary64 bits, signed
zero and infinities, with canonical NaN payloads. Original errors retain native
Rc identity. BigInt/TextEncoder remain explicit original intrinsic capabilities;
the source side executes Node's real BigInt/TextEncoder.

Independent native assertions observe signed message data, chain hex, JWT
expiry headers, sorted income/order IDs, fill timestamp seconds, cached leverage
and slippage payload before supplemental comparisons. Further direct checks
cover the 31/32 UTF-16-unit hash boundary, TextEncoder replacement bytes, SDK
identity, five auth fields, reserved-header overwrite plus supplemental-header
retention, income repeated-cursor stop, the 200/50/5 page limits, distinct-fill-ID
collision with last value retained, existing-position spread retention, and no
leverage-cache write after a failed POST. Full receipts compare outputs,
transport/signing parameters and dependency order, logs/consoles, emissions,
position writes and leverage writes.

Rust owns request message/typed-data construction and signature shape
normalization; numeric/hex/decimal/encoded chain-ID branches and byte folding;
JWT timestamp/expiry, typed auth data, header precedence, response diagnostics
and token selection; history guards, cursors, caps, deduplication, sorting and
limits; historical-fill normalization, fallback and collisions; leverage
clamping/cache/position updates; and max-slippage payloads.

Hashing, signing, mapped history DTO replies, HTTP, property/collection/promise
operations and host implementations remain explicit dependencies. The income
and historical-order mapper callbacks are declared fixture capabilities, not
new executions or claims of those already-owned mapper bodies. Native
historical-fill parsing is implemented in this slice. No existing divergence
was relabeled, and no shared string/model contract was weakened.

Source historical fills retain each fill ID but deduplicate on
order/time/price/quantity/side. Distinct IDs with otherwise identical fields
therefore collapse, and the last execution wins. This existing source behavior
is witnessed rather than silently corrected in a ported row.

### Development and instrument failures retained

All occurred before the committed code/gate SHA and are excluded from controls:

- Native fixture compile exited 101 with E0308 (a transformed map-key comparison),
  E0599 (two into_owned calls on String) and E0506 (assigning a still-borrowed
  page reply). Corrected UTF-16 key comparisons and String use, and cloned the
  page fields before reassignment. No warning suppression or assertion removal.
- Decoded target exited 101 at generateNewToken/basic transport receipts. The
  native observer treated the text/json response receiver as a function argument,
  while the source methods receive zero arguments. Receipts now keep zero
  arguments and retain the original response receiver separately. Production
  and source bodies were unchanged by this observer repair.
- After widening nominal auth-error checks, decoded target exited 101 because
  the source oracle exited 1: it tried to inspect an undefined exported private
  class prototype. The unchanged private error declaration is now explicitly
  exported by the harness. No source-body or production modification repaired
  this failure.

Review before measurement represented object keys as UTF-16 units, avoiding the
known decoded-JSON lone-surrogate observer boundary before it failed here.
Dynamic native error construction uses a UTF-16 capability, not lossy UTF-8
projection. Request callable selection precedes argument construction. The
optional JSON error-detail failure retains the already-visible HTTP response
text/status and ultimately returns the HTTP failure. No assertion or source
input was weakened, and no pre-commit result is attributed to the committed tree.

### Structure and integration provenance

Actual extractFile covers all eight TS files, symbol identity drift 0. Scoped
invention/status check: **152 venue rows / 415 public Rust items**, excluding
only the unchanged inherited shared crypto target. Full census is 153.
**0 venue exemptions**; two direct layer edges valid. Every new trait capability
and reused supertrait item is mapped to actual source consumers.

Integration `91bdde133d2f407dffa8f2140f94e87eac7c1d5f` merged down at `430c928de967ff736fcd24cb803d0561677d943a`.
Required source/test proof exit **0** and expanded
actual-input proof exit **0**, against the code SHA:

```sh
git diff --exit-code 8330e6d92f80aa8f630b6e4f7f119bc77529d1a8 430c928de967ff736fcd24cb803d0561677d943a -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex packages/logging/src/LogSeverity.ts packages/safe-cex/src/types.ts packages/safe-cex/src/utils/omitUndefined.ts
```

The broader input proof exited 1 because root package.json changed the Kucoin
oracle target list. Cargo.lock, Cargo.toml, shared types/numeric code and yarn.lock
were unchanged. Scoped all-targets check and account_rest native target were
conservatively rerun at that merge SHA for the command-input change, rather
than attributed to delivery:

| Merge check | Exit |
| --- | ---: |
| scoped-cargo-check | 0 |
| account-rest-native | 0 |

Delivery repeats fetch/merge-down and both code-SHA byte proofs immediately
before push; pushed full SHA is reported on the thread. Seven-gate measurements
and any later merge checks retain their own distinct measured SHAs.


## Order execution and setup — eight pending rows

**Rows moved: 8; all ported; 0 divergent; 0 regression.** Fresh branch from
`ad731bb9ef`. Independent tomllib census keys every row by manifest path AND
TypeScript symbol across eight files: **102/34/17 → 110/34/9**
(ported/divergent/pending), 153 total. Exact pending-to-ported set:

- `ParadexExchange#resolveMarketIocExecutionPrice`
- `ParadexExchange#buildMainOrderPayload`
- `ParadexExchange#placeNativeBatchChunk`
- `ParadexExchange#placeTakeProfitStopLoss`
- `ParadexExchange.updateOrder`
- `ParadexExchange.constructor`
- `ParadexPublicWebSocket.listenOrderBook`
- `ParadexPrivateHistoryAuthNotReadyError`

Code/gate measured SHA: **be84792bb098f3af3a058e60da94a6a0fd541ffe**. These measurements belong to that SHA,
not a later evidence or delivery commit.

### Seven scoped gates

| Gate | Exit | Command |
| --- | ---: | --- |
| scoped-cargo-check | 0 | `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2` |
| order-execution-controls | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_order_execution_controls.py` |
| extract-check | 0 | `node "${YARN_CLI}" parity:extract:check` |
| structure | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py` |
| divergence-notes | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py` |
| rustfmt | 0 | `cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check` |
| diff-check | 0 | `git diff --check` |

### Eight compiled production controls

Native target baseline exit 0. Each mutation compiled, exited 101 at its named
independent native assertion before supplemental comparisons, restored exact
production bytes in finally, then the restored target exited 0.

| Control | Required native assertion | Mutation | Restore |
| --- | --- | ---: | ---: |
| ioc-price | `ioc_native_execution_price` | 101 | 0 |
| payload-stp | `payload_native_stp` | 101 | 0 |
| batch-correlation | `batch_native_client_correlation` | 101 | 0 |
| bracket-child-ids | `brackets_native_child_ids` | 101 | 0 |
| amend-stable-id | `amend_native_stable_id` | 101 | 0 |
| constructor-rate-limit | `constructor_native_rate_limit` | 101 | 0 |
| book-registration | `book_listener_native_callback_set` | 101 | 0 |
| history-class-descriptor | `history_class_native_field_descriptor` | 101 | 0 |

**No survivors among the eight completed controls attempted. Compiler rejects
and a 101 from an unrelated site are not included in that claim.**

### Actual-source scope and native observations

The oracle extracts the six actual exchange AST bodies (five order methods plus
construction), the actual public listenOrderBook body, and the complete unchanged
private history Error class declaration. No source body is rewritten. Actual
mapOrderType, mapTimeInForce, reverseMapOrderType and validateTpSlPrices bodies
are bound as dependencies. Actual isPositiveFinitePrice,
normalizePositiveFraction, roundPriceToStep, signerHeaders, safe-math helpers,
omitUndefined, installed omitBy, domain/venue enums, ENDPOINTS and LogSeverity
are bound. Measured members are captured before fixture dependency assignments
and restored before invocation.

**739 declared cases:** 294 baseline (six members × 49 profiles), 432 selected
fault cases (six × four profiles × 18 selectors), nine listener lifecycles and
four class-allocation cases. An unreached fault selector is a successful case,
not counted as an injected failure. Every control runs this scoped target.

Fixtures supply raw inputs and dependency replies independently of source
outcomes. Keys and values preserve UTF-16 code units, including lone surrogate
market keys, messages and channel symbols. Numeric receipts preserve binary64
bits, signed zero and nonfinites with canonical NaN. Native error Rc identities,
callback functions, captured sets and the supplied class prototype are retained.

Rust owns IOC touch/fair-price/band/slippage/step control flow; payload trigger,
trailing, reduction, omission and signing composition; native-batch validation,
last-header selection, full error accounting and client-ID response correlation;
TP then SL construction, partial success and visible diagnostics; amendment
reconciliation, presence-based overrides and stable-ID confirmation; exchange
constructor assignment order and SDK rejection observation; and captured-set
book listener lifecycle plus the deferred rejection callback body.

The class row owns its remaining derived allocation and originalError field
initializer phase. Original Error allocation/prototype is supplied by a nominal
allocator capability, whose exact class argument and retained native prototype
identity are checked. Field definition passes undefined and three true
attributes. Whole source class construction is composed with the already-ported
native constructor-body helper; that constructor row is not recounted. This
adds neither a JS engine nor a replacement Error/prototype implementation.
Exchange and WebSocket class field initialization remains on their pending class
rows. Original HTTP/signing/SDK/Base/socket/promise factories, property/coercion
and collection implementations remain explicit dependencies. Original source
promise dispatch invokes the native book rejection callback through its host.
No real event-loop, networking, trading, cryptography or complete exchange/API
implementation is claimed. No shared type, workspace trait or string boundary
changed; the shared-change workspace compilation trigger does not apply.

Before supplemental outcome/operation/log/emission/write comparisons, direct
native checks assert IOC price 102, STP EXPIRE_TAKER, reversed-response client-ID
correlation, both child IDs, signed PUT ID, maxRPS 10, registered callback set,
class field descriptors and prototype/original/error identity. Additional direct
checks assert last batch headers; partial batch rejection; first child retained
when the second POST fails; no PUT after failed preflight; absent amendment size
retained versus explicit undefined rejected; duplicate subscription count;
repeat cleanup count; and deletion of a replaced map entry using the old set.

**Source behavior preserved:** duplicate registration of the same callback
resubscribes when set size remains one. Cleanup retains the captured original
Set; when it empties, cleanup deletes even a replaced current map entry. Repeated
cleanup repeats unsubscribe/stat cleanup. These behaviors are witnessed, not
silently corrected while labeling the row ported.

### Rejected sequence and development failures

At **50e4fa52df3a9eb70d3122fde0f88fc5b6d39367**, scoped cargo-check exited 0,
but the control runner exited 1 with SyntaxError: unterminated string literal.
The script writer had expanded escaped newlines during regex replacement. No
baseline or mutation was attempted by that runner. **NOT seven green gates,
NOT a runtime control.** The writer now uses a replacement callback; Python AST
parse and all eight unique mutation sites were checked. The fix was committed
without amending, then all seven gates and eight controls reran at the measured
SHA above. Production assertions and inputs were not weakened.

Earlier development failures occurred before either committed SHA:

- Source harness exited 1 with a missing closing object brace before executing
  any case. Repaired the observer; no source-body rewrite.
- Native comparison exited 101 at buildMainOrderPayload/stop-order: the fixture
  guessed stop_loss while actual OrderType.StopLoss is stop_market. Corrected
  declared fixture profiles and the dependency mapping to the actual enum.
  Production and the source method body were unchanged by this fixture repair.
- Native fixture compilation exited 101 with an unused expected_prototype and
  unconstructed ClassObject variant after a text edit missed the allocator site.
  Corrected allocation to retain and observe the actual supplied prototype;
  no warning suppression or fake serialized native boolean assertion.

Review also corrected a guessed trailing-stop literal to the actual
trailing_stop_market before committed measurement, and widened the native
presence observer to distinguish explicit undefined from absent object keys.
No pre-commit result is attributed to a committed tree. None is a completed
mutation control.

### Structure and merge provenance

Actual extractFile covers all eight files with symbol identity drift 0; 153 total
rows, 152 venue rows after excluding only the unchanged inherited crypto row.
Zero venue exemptions; two direct layer edges validated. New public items and
reused host/supertrait capabilities are mapped to actual source consumers.

Scoped structure receipt: `parity: invention and status gates passed (152 rows, 432 public Rust items)`.

Integration `c2e36c3d08a9b98d0431bd2e306fbe60c3aab82d` merged down at **aad1512dad06aba9aa7d6c20d1caa5840860b8db**.
Required source/test and expanded consumed-input byte proofs both exited 0:

```sh
git diff --exit-code be84792bb098f3af3a058e60da94a6a0fd541ffe aad1512dad06aba9aa7d6c20d1caa5840860b8db -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex packages/logging/src/LogSeverity.ts packages/safe-cex/src/types.ts packages/safe-cex/src/utils/omitUndefined.ts packages/safe-cex/src/utils/safe-math.ts
```

Cargo/dependency/command inputs unchanged; no redundant merge rerun claimed.

Delivery repeats fetch/merge-down and required/expanded code-SHA byte proofs
immediately before push, plus dependency proof against the last measured tree.
Any new dependency change requires the scoped check/native target at that exact
merge SHA. Full pushed SHA is reported on the thread; earlier gates retain their
own measured SHAs. Raw logs/corpus remain ignored under .aimux/paradex.


## Remaining runtime assembly — last nine pending rows

**Rows moved: 9 — 6 pending→ported, 3 pending→CORRECTED-divergent,
0 regression.** Fresh branch from `97d0e03c6f`. Independent tomllib census keys
all rows by manifest path AND TS symbol across eight files:
**110/34/9 → 116/37/0** (ported/divergent/pending), 153 total. Zero pending is
not a claim of zero divergence. Exact new rows:

- `ParadexRequestSigner` — ported
- `ParadexJWTManager` — ported
- `ParadexExchange` — ported
- `ParadexPrivateWebSocket` — ported
- `ParadexPublicWebSocket` — ported
- `createAPI` — ported
- `ParadexPrivateWebSocket#executePositionFetch` — CORRECTED-divergent
- `ParadexPrivateWebSocket#fetchThrottledOrders` — CORRECTED-divergent
- `ParadexPublicWebSocket.listenOHLCV` — CORRECTED-divergent

Code/gate measured SHA: **71a9aaefcc269e0fc386cc9b0ba819fd19137fd2**. These measurements belong to that SHA,
not a later evidence or delivery commit.

### Seven scoped gates

| Gate | Exit | Command |
| --- | ---: | --- |
| scoped-cargo-check | 0 | `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --all-targets --jobs 2` |
| runtime-assembly-controls | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_runtime_assembly_controls.py` |
| extract-check | 0 | `node "${YARN_CLI}" parity:extract:check` |
| structure | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_structure.py` |
| divergence-notes | 0 | `python3.14 sdk-rs/safe-cex-venue-paradex/scripts/check_divergence_notes.py` |
| rustfmt | 0 | `cargo fmt --manifest-path sdk-rs/Cargo.toml --package safe-cex-venue-paradex -- --check` |
| diff-check | 0 | `git diff --check` |

### Nine compiled production controls

Native target baseline exit 0. Each mutation compiled, exited 101 at its named
independent native assertion before supplemental comparison for that case,
restored exact production bytes in finally, then restored target exited 0.

| Control | Required native assertion | Mutation | Restore |
| --- | --- | ---: | ---: |
| request-signer-field | `signer_native_initial_field` | 101 | 0 |
| jwt-expiry | `jwt_class_native_expiry` | 101 | 0 |
| exchange-leverage-map | `exchange_class_native_leverage_map` | 101 | 0 |
| private-position-throttle | `private_class_native_position_throttle` | 101 | 0 |
| public-recovery-map | `public_class_native_recovery_map` | 101 | 0 |
| api-timeout | `api_native_timeout` | 101 | 0 |
| position-pending-reset | `positions_native_pending_reset` | 101 | 0 |
| orders-loaded | `orders_native_loaded` | 101 | 0 |
| ohlcv-delay | `ohlcv_native_poll_delay` | 101 | 0 |

**No survivors among the nine completed controls attempted. Compiler rejects
and a 101 from an unrelated site are not included in that claim.**

### Actual-source scope and independent expectations

The oracle executes untouched AST declarations for five complete source classes
and createAPI, plus the three original fetch/poll members. TypeScript transpiles
class fields with useDefineForClassFields into observable original property
definitions; neither class nor method bodies are rewritten. The observer binds
original BASE_URL, BASE_WS_URL, RECV_WINDOW, timeframeToSeconds and LogSeverity
declarations. Parent/base allocation, original dependency factories and existing
member bodies are explicit capabilities rather than new JS/transport engines.

**330 declared cases:** 60 class cases (five × two profiles × six selectors),
174 API construction/request cases (42 baseline, 77 factory/serializer fault
cases, 55 selected request fault cases), four response-error cases, 72 private
fetch cases (two × four × nine selectors) and 20 OHLCV lifecycles. Successful
class cases construct two actual instances. An unreached selector is success,
not a counted injected failure; the enumeration stops at these cases.

Native observations precede each supplemental receipt. Keys AND values retain
UTF-16 code units, including surrogate addresses, symbols, messages and map
keys. Numeric receipts and timer receipts compare binary64 bits with canonical
NaN. Values, collections, prototypes, callbacks and original errors retain
native Rc identity; no encode/decode roundtrip serves as a native assertion.

Class rows cover their remaining allocation/field-initialization phase, without
recounting separate constructor/member rows. Original nominal prototype and
parent construction are supplied allocation capabilities. Rust owns ordered
field definitions, undefined/null/scalar defaults, fresh collections, lexical
arrow-field installation, public message-handler wrapper composition, original
100-ms leading/trailing throttle factory calls, and exact returned-value
retention. Client-ID rule composition owns the original regexp arguments and
native mint/honorable callback bodies; its pattern factory and helper/UUID
operations remain original dependencies.

Class witness assertions independently observe initial signer field BEFORE its
constructor assignment, JWT expiry, leverage/recovery Maps, private 500-ms
throttle, field descriptors, exact prototype identity, separate allocations,
fresh functions/collections, distinct collections within an instance, captured
arrow receiver and exact returned throttle callback rather than its input.
The public ping handler is invoked after its target method is replaced; native
lookup retains the captured receiver and drops the ping argument, matching the
unchanged source wrapper even when called with a foreign this receiver.

Native SDK and socket constructor helpers are reused in the witness. Exchange
constructor-body dependency replies are declared fixtures in this target; its
already-owned constructor body is not newly executed or claimed here. Socket
constructor handler assignment remains on its existing constructor row. Class
phase observations do not claim an implementation of original prototypes, Base,
all registered member bodies or library throttle internals.

Rust owns Axios factory configuration and serializer/retry callback bodies;
transport gating; request/response interceptor registration; public/account and
signature/SDK guard order; order partner-header injection; proxy-error synthesis;
and retained error prototype with a nonwritable/enumerable/configurable message.
Native checks observe timeout 20000, exact xhr/config/response/SDK-promise
identity, preserved original error causes, inherited response/config/request
identity and actual message descriptor attributes. The source and native sides
bind previously owned authenticateRequest as a declared dependency capability;
this target does not newly execute that authenticator body. qs, retry-condition,
Axios, retry and transport adapter are also original dependency capabilities.
No actual HTTP/auth/trading, actual crypto/SDK imports or library internals are
claimed. No shared type/workspace trait/string contract changed; the shared
workspace sweep trigger does not apply. No full suite, workspace compile,
clippy or package TypeScript typecheck is claimed.

### Three explicit CORRECTED divergences

The source executePositionFetch and fetchThrottledOrders use empty catches and
resolve undefined after failures. Native functions return the retained original
error through Result. The source poll similarly suppresses fetch/broadcast/
recovery errors; native poll returns the error after the same conditional
reschedule, and detached dispatch observes that Result. The source bodies are
unchanged. These intentional visible-failure corrections are labeled divergent,
not silently repaired and labeled ported. The manifest states why for each row.

Independent fault expectations come from declared reached selectors, not from
source outcome or native output. Native private failures are asserted Err with
original identity while source successful resolution is independently asserted.
OHLCV fetch, callback and recovery faults assert exactly one retained native
error plus the same 3000-ms retry timer. Native basic cases assert pending reset,
timestamp, loaded.orders, one candle emission and actual cached close.

**Source behavior preserved elsewhere:** orders update after a fetch even if
socket disposal occurs during that fetch (source has no post-await check there).
Position fetch checks both socket and parent disposal after await. OHLCV listeners
use latest callback sets during cleanup, unsubscribe on every cleanup call,
share an existing loop, retain active subscriptions while callbacks remain,
reuse live opts during cleanup, and repeat cleanup when called repeatedly.
Polling skips unchanged timestamp/close/volume, respects generation changes
and in-flight deactivation, retains partial callback/cache effects on failure,
and reschedules after the same caught fault. Full shared operation/state/log/
write/emission/timer receipts match the source; the only declared failure outcome
corrections are separately asserted rather than normalized into parity claims.

### Development and instrument failures retained

All occurred before the committed code/gate SHA; none is a mutation control:

- Native compilation exited 101 with E0425 for a guessed numeric helper name
  (actual shared function is js_number_string), and E0282 for constructor helper
  closure error inference. Fixed the name and explicit error types; no warnings
  suppressed or assertion removed.
- Source-only oracle initially exited 0 but two microtask awaits observed detached
  polling before VM promise settlement: basic emission/timer were absent before
  cleanup. This trial is NOT parity evidence. Observer now drains an event-loop
  turn and independently asserts basic one-emission/3000 timer. Bodies, replies
  and inputs were not changed to repair the observer.
- Native decoded target exited 101 at public-class operation comparison because
  source throttle receipt naming exhausted its three-name counter on the second
  instance. Reset the observer naming cycle; both actual allocations remain.
- Native decoded target exited 101 in function receipt decoding (missing function
  tagged-value support). Added function decoding; it did not replace inputs,
  source bodies or production to fit the comparison.

Review corrected an initial guessed recv-window literal to actual RECV_WINDOW
20000 before committed measurement. The observer uses collection/regexp brands
across VM realms, avoiding vacuous empty-object projections; error descriptor
observations retain actual native attributes rather than predicted booleans;
returned throttle callbacks are distinct native factory values whose identity
is independently checked. Timer comparisons use binary64 bits to avoid the
known 3000-versus-3000.0 observer boundary. No pre-commit result is attributed to
this committed tree. The committed control runner's Python AST and all nine
unique production sites were verified before invocation; no failed committed
gate/control sequence is omitted.

### Delivery instrument failure

The first normal merge-down attempt exited 128 with git's Unable to write index.
Independent reads showed HEAD still at the code/gate SHA, no MERGE_HEAD or tracked
diff, 70 GiB available and no stale index.lock. No root cause is claimed. No
index deletion, reset or history rewrite was used. Retrying the same normal
fetch/merge path succeeded; the actual post-merge proofs/checks are listed below.
This failed delivery operation is neither a runtime control nor a behavior gate.

### Structural scope and integration provenance

Actual extractFile covers eight files with symbol identity drift 0, 153 total
rows and 152 venue rows after excluding only the unchanged inherited crypto
row. Zero venue exemptions; two direct layer edges validated. Actual source
consumers map every new public item/capability. The local divergence-note
presence checker now validates public trait owners and their actual associated
fn/type/const declarations, so CORRECTED rows can map their real host consumers.
Root declaration, canonical bucket/why and full scoped invention/status checks
remain enforced; no exemption or weakened target assertion was introduced.

Scoped structure receipt: `parity: invention and status gates passed (152 rows, 483 public Rust items)`.

Integration `0e014c0aa040874ed83ad9f13d9ae617c57a6435` merged down at **eb87af38baed02aac6983abf28245b86f9172ccf**.
Required source/test and expanded consumed-input byte proofs both exited 0:

```sh
git diff --exit-code 71a9aaefcc269e0fc386cc9b0ba819fd19137fd2 eb87af38baed02aac6983abf28245b86f9172ccf -- sdk-rs/safe-cex-venue-paradex/src sdk-rs/safe-cex-venue-paradex/tests sdk-rs/safe-cex-venue-paradex/scripts sdk-rs/parity/manifest/exchanges/paradex packages/safe-cex/src/exchanges/paradex packages/logging/src/LogSeverity.ts packages/safe-cex/src/types.ts packages/safe-cex/src/utils/omitUndefined.ts packages/safe-cex/src/utils/safe-math.ts
```

Broader dependency/command input proof exited 1. Changed paths:
- `package.json`
- `sdk-rs/Cargo.lock`

Scoped checks reran at that exact merge SHA, not delivery:

| Merge check | Exit |
| --- | ---: |
| scoped-cargo-check | 0 |
| runtime-assembly-native | 0 |

Delivery repeats fetch/merge-down, required/expanded gate-SHA byte proofs and
dependency proof against the last measured tree immediately before push. Any
new dependency change requires scoped check/native target at its exact merge
SHA. Full pushed SHA and integration census are reported on the thread. Raw
logs/corpus remain ignored under .aimux/paradex.


## Typed-data signing adapter: measured scope at e1799b07c68935f840ef9127685992de701c32c5

Rows moved: **0**. The signing adapter is implemented, but the row remains
FINDING for the existing unimplemented empty-enum-tuple dependency encoding.
The numeric-string wrong hashes and missing adapter are repaired within this
measured scope; this is not a complete SDK-acceptance claim or a re-bucketing
of unimplemented behavior. TypeScript remains unchanged.

The committed sequence ran with a clean tracked tree and unchanged HEAD:

| Named gate | Exit |
| --- | ---: |
| source-extractor | 0 |
| venue-structure | 0 |
| divergence-notes | 0 |
| scoped-all-targets-check | 0 |
| scoped-all-targets-clippy | 0 |
| signing-runtime | 0 |
| stark-input-runtime | 0 |
| stark-existing-regression | 0 |
| signing-controls | 0 |
| stark-input-controls | 0 |

Structure covers eight actual extracted files, 153 manifest rows, 504 venue
public items, zero venue exemptions and three direct dependency edges. The
signing row is included; shared crypto targets are explicitly outside the
venue-only invention projection. The note checker resolves their declarations.
Shared crypto behavior is exercised by the separate scoped runtime targets.

The source oracle extracts the unchanged helper and binds only its dynamic
import to a capability. Native state/error/result identity assertions precede
supplemental receipt comparisons. It pins 448 profile/fault cases and an
independently enumerated 2-address x 2-key x 3-caller crypto inventory. Order,
ModifyOrder and JWT payloads come from the actual source builders. Native
bindings match all twelve actual Account.signMessage results. The reviewer's
independent SDK recomposition matched 12/12; canonicalizing the authenticated
definition instead changed 12/12. Original fields and type hashes are retained;
only a native metadata copy is canonicalized. Six distinct domain permutations
and separate type-hash/field-order controls pin this distinction.

Every following control compiled, exited 101 at its named native assertion,
and passed its post-control restore run with exit 0:

| Control | Required assertion |
| --- | --- |
| remote-key | remote_native_request |
| remote-receiver | remote_native_receiver |
| account-address | hash_native_address |
| local-input | local_native_input |
| bigint-receiver | bigint_native_receiver |
| felt-pair-order | signature_native_value |
| bigint-conversion | signing_native_order |
| native-address | crypto_adapter_native_request |
| native-key | crypto_adapter_native_signature |
| numeric-normalization | numeric_string_native_hash |
| original-domain-type-hash | domain_native_original_hash |
| original-domain-field-order | domain_native_original_hash |
| original-account-prime | account_native_refusal |

No survivors among these thirteen attempted controls. Compiler rejects and a
101 without the required assertion are not included in that claim. Both runners
restore production in finally blocks before evaluating mutation results.

Pre-commit failures are excluded from this sequence: disk exhaustion stopped
compilation before runtime; a wrong-realm BigInt prototype hook missed source
callback observations and caused a rejected supplemental mismatch; initial
native composition rejected the actual caller domain order; direct deserialization
of native struct/object wrappers produced compiler errors before the repair used
their enclosing enums. An edit-script ValueError changed no production and its
follow-on compile retried the old error. None is a completed mutation control.
The corrected BigInt observer patches its fresh VM intrinsic, with explicit
method getter/call faults and receiver/argument assertions. Assertions were not
weakened to absorb the observer error.

Independent sub-agent review returned MERGE for implementation at this SHA,
with static and Node checks only, not an independent Cargo execution claim.
Its earlier-observer audit reviewed accepted ac0f5fccf24207b26c279e67da2a88f2c72a7aae:
27 Paradex and 69 Bybit JavaScript files, seventy Python scripts and both gate
sheets; ten fixture programs replayed, the remaining JavaScript reviewed
statically. Same-VM Number/Object hooks, supplied host objects and injected
constructors were distinguished from unrelated realm intrinsics. An observer
positive control detected a fresh-VM Error, and five replayed accepted fixtures
had zero cross-realm Error misclassifications. No newly invalid accepted claim
was found in that scope. The older Bybit Map observer failure was already a
withdrawn pre-commit run. The audit does not revalidate historical native gates
or fixtures outside these two crates.

No full workspace/full crate suite, wasm build, credentialed or live-network
run is claimed. Measurements above belong to the stated code SHA, not to a
later evidence, annotation, merge or delivery commit.

## Signing follow-through: measured attempts after e1799b07c6

No manifest relabel follows these preflights. At 62e9c7e45d the sequence was
withdrawn after four zero exits when review found unused enum metadata activating
revision-1 hashing for revision 0. No controls completed. At d58a6ce5fb eight
non-control gates passed; two signing controls completed 101/0 before an
interruption. The third had no completed restore test. A future raw-hash recipe
was statically found to orphan its helper; that recipe had not run, so no compiler
rejection is claimed. The helper was retained in 17b8343df0.

At 17b8343df0 two enum controls completed 101/0; the third compiled and failed
101 at empty_tuple_native_accepted before its required hash assertion. The runner
exited 1 and restored production automatically, but no post-control restore test
ran. That sequence is rejected. Moving the variant swap after tuple assembly in
9c74ed87b6 produced eight completed named 101/0 enum controls. Its broader sequence
was then withdrawn during scoped all-targets check after three other zero exits
because new source numeric fixtures exposed an actual wrong hash. These partial
sequences are not a replacement for a complete committed gate sequence.

Source-first native baselines, each ordinary runtime failure rather than a
mutation: 0f6bdb0370 failed numeric_json_native_hash at raw 9007199254740993;
96fb5978b8 independently failed recursive enum i128 acceptance. The repair uses
shared js_bigint and declared-field projection. 8bca519dc5 failed selector_native_hash
at revision-0 transfer; b19790e027 repaired SDK names and numeric hex reduction.
d62eff3542 failed text_native_hash at revision-0 felt NUL; ea413f8ae9 repaired
unpadded nibbles and the original byte-array frame. At 709a1bae84 the enum target
failed enum_entries_native_accepted extra-known and the separately run selector
coercion target failed selector_coercion_native_accepted 0/0. At 4d61612933 the
revision target failed domain_revision_native_refusal 0/string-one. The candidate
repair in 537e44a3e8 passes those four scoped targets, including the unchanged
129-case partial-slot corpus. Those runs preceded that repair commit and are
preflight evidence only. No new complete sequence or completed control count is
inferred from them.

The 505fff3831 sequence is WITHDRAWN: ten non-control gates exited 0 and three
enum controls completed named 101/0. Review identified that the old array-order
recipe both reversed input and bypassed recursive composition. It demonstrated
broader array-path sensitivity, not isolated ordering. The fourth control was
interrupted during compilation (exit -2), production restored automatically,
and no post-control restore test ran for that attempt. No completed fourth
control or complete sequence is claimed. The replacement reverses only encoded
array words immediately before their native hash, preserving recursive leaf
composition and all production guards. The earlier 9c74ed87b6 control retains
its narrower array-path interpretation.

A separate preflight strict-clippy run at c8e2ba7b47 exited 101 for a test's
manual range pattern; 505fff3831 changed only 0|1|2 to 0..=2. That compiler/lint
result is not a runtime control.

## Complete signing sequence at 846f57205fbaea2dad39b63c725107a4da9cede6

All fifteen named gates below exited 0 at this committed SHA. Production was
restored and HEAD remained fixed after every control. Measurements belong to
this SHA, not the later annotation or delivery commit.

| Gate | Exit |
| --- | --- |
| source-extractor | 0 |
| venue-structure | 0 |
| divergence-notes | 0 |
| scoped-all-targets-check | 0 |
| scoped-all-targets-clippy | 0 |
| signing-runtime | 0 |
| stark-input-runtime | 0 |
| stark-numeric-json-runtime | 0 |
| stark-sdk-encoding-runtime | 0 |
| stark-existing-regression | 0 |
| stark-empty-controls | 0 |
| signing-controls | 0 |
| stark-input-controls | 0 |
| stark-numeric-json-controls | 0 |
| stark-sdk-encoding-controls | 0 |

Each of these thirty-two controls compiled, exited 101 at its required native
assertion, restored production, and passed its restore test with exit 0.
No survivors among the thirty-two controls attempted. Compiler rejects and a
101 from an unrelated site are not included in that claim.

| Control | Required native assertion |
| --- | --- |
| revision-specific-quoting | unused_enum_native_hash |
| empty-zero-word | empty_tuple_native_hash scalar/Zero |
| variant-index | empty_tuple_native_hash scalar/Zero |
| original-raw-type-hash | empty_slot_native_hash |
| ignored-empty-slot | empty_slot_native_hash |
| array-order | empty_tuple_native_hash array/Zero |
| single-merkle-leaf | empty_tuple_native_hash merkle1/Zero |
| merkle-leaf-composition | empty_tuple_native_hash merkle1/Zero |
| remote-key | remote_native_request |
| remote-receiver | remote_native_receiver |
| account-address | hash_native_address |
| local-input | local_native_input |
| bigint-receiver | bigint_native_receiver |
| felt-pair-order | signature_native_value |
| bigint-conversion | signing_native_order |
| native-address | crypto_adapter_native_request |
| native-key | crypto_adapter_native_signature |
| numeric-normalization | numeric_string_native_hash |
| original-domain-type-hash | domain_native_original_hash |
| original-domain-field-order | domain_native_original_hash |
| original-account-prime | account_native_refusal |
| json-number-rounding | numeric_json_native_hash 0/scalar/9007199254740993 |
| json-i128-rounding | numeric_json_native_hash 1/i128/9007199254740993 |
| enum-i128-normalization | numeric_json_recursive_native_accepted i128/enum/number |
| declared-preset-projection | numeric_json_recursive_native_accepted u256/root-extra |
| selector-default-name-hash | selector_native_hash 0/default |
| selector-coercion | selector_coercion_native_accepted 0/0 |
| selector-hex-reduction | selector_native_hash 0/hex |
| shortword-nibbles | text_native_hash 0/felt/nul |
| bytearray-pending-length | text_native_hash 1/string/empty |
| enum-first-entry | enum_entries_native_hash extra-known |
| original-domain-revision | domain_revision_native_refusal 0/string-one |

The venue witness observes native state before supplemental comparisons across
448 profile/fault cases and twelve actual caller/account/key compositions. The
shared witnesses cover 986 source cases with explicitly declared choices:
158 numeric-string/account/domain cases, twelve cardinality cases, fourteen
exact empty tuples, 129 partial slots, 33 unused enum schemas, 401 raw numeric
cases, 72 selectors, 110 text cases, 27 enum-entry cases and thirty original
revisions. These are not 986 universal acceptance/hash-equivalence claims.
The permanent slot counter-measurement preserves 3/3 hashes when ignored values
change and changes 3/3 when authenticated definitions change. The actual caller
witness retains the separate twelve-caller domain recomposition comparison.

The row remains divergent. Its repaired FINDING becomes CHOSEN only after this
complete sequence: exact array arity, closed schemas/reserved names, bounded
parsing/traversal and the cited canonical text boundary remain explicit. The
measured recursive felt-shadow overflow alone carries ref-bug. No unknown
mismatch is waived by a generic schema label. The sub-agent reviewed the narrow
implementation and all ten source programs; it ran no Cargo. Its isolated
structure replay did not complete, while the root structure gate passed.

Not run: full workspace check/test/clippy, full crypto or venue suites, live
exchange requests, or historical native controls outside this signing unit.
The earlier realm audit has its separately stated scope and limits above.

Post-annotation validation at 88fd41bc98: an initial extractor invocation exited
1 because its compiler-path environment was omitted. With the required
SAFE_CEX_TYPESCRIPT_PATH, extraction and structure exited 0; notes exited 1
because the new CHOSEN note lacked the literal required why-mirroring marker.
The rationale was present but is now explicitly labeled. No gate was weakened;
these annotation checks do not change the completed 846f57205f measurements.


## Signing merge-down validation at 6f0c6860064b0837c445b413b0540a6569d052c5

Merging integration f2229103d079a43a5b99c41b206b1deb6e51c102 left the scoped
crypto/Paradex production, tests, numeric/types crates, workspace Cargo manifest
and lockfile unchanged relative to 846f57205f. The broader merge did change core
cancellation work, layer declarations/checks, root package scripts and CI/mirror
policy, so the following conservative checks were rerun at this merge SHA.

| Gate | Exit |
| --- | --- |
| scoped-all-targets-check | 0 |
| signing-runtime | 0 |
| numeric-json-runtime | 0 |
| extractor | 0 |
| structure | 0 |
| notes | 0 |

The fifteen-gate and thirty-two-control measurements remain attributed to
846f57205fbaea2dad39b63c725107a4da9cede6. They were not repeated at this merge
or the later documentation delivery commit. No shared types or trait changed
in this signing slice, and no workspace-wide gate is claimed.

Final sub-agent review independently confirmed 153 rows: 116 ported, 37
divergent and zero pending; the divergent buckets are 34 CHOSEN and three
CORRECTED with no remaining FINDING. It read all thirty-two failure/restore log
pairs and the root reports. Its MERGE is limited to this signing implementation,
annotation and evidence, not unrelated integration changes or universal SDK
parity. A separate doc-only review approved the portable rendering of historical
tool paths; it ran no Cargo and did not reinterpret historical invocations.

## Numeric REST order creation times

`map_order` now passes retained numeric `created_at` values directly through the
shared `js_time_clip`; strings retain the existing date parser/timezone hook.
This matches the actual `new Date(o.created_at).getTime()` source expression.
The former numeric-date refusal fixture is now an admitted native-field case.
Twelve additional independently specified raw numeric inputs cover epoch
milliseconds, fractions, signed zero, both TimeClip edges and overflow.

Before repair, the actual-source mapper target failed
`order_native_boundary_refusal 115`, and
`numeric_created_at_survives_constructed_fetch_orders` failed during startup with
`Paradex order field created_at must be a string`. Both pass after repair. The
mapper target checks 128 source cases: 89 native Orders and the remaining 39
explicit representation refusals; every native date bit and retained info value
is checked. The constructed regression also verifies the loaded Store order and
subsequent REST read, rather than calling only the standalone mapper.

Scoped mapper and constructed REST regressions, strict host all-target/all-feature
and Wasm library all-feature Clippy, crate formatting and parity checks pass
(7,459 rows, 17,214 public Rust items). No shared decoder/type/signing changes are
included. The separately audited Infinity-signing claim did not reproduce on the
actual native binding and is excluded from this repair.

## Raw REST order date and size coercion

The maintained `ParadexExchange.mapOrder` calls `new Date(o.created_at)` and
`parseFloat` on size/remaining_size. The native mapper now uses the existing
shared JSON String/parseFloat conversion for quantities. Date conversion keeps
missing as NaN, null/false as positive zero, true as 1, Numbers through TimeClip,
and other JSON data through String and the existing date parser.

The independent actual-source/native mapper corpus grows from 128 to 152 cases:
113 native Orders and the same 39 explicit enum/identifier representation
refusals. The additional cases cover missing/null/boolean dates, JSON arrays and
objects, numeric/signed-zero/null/boolean/container quantities, and missing size
or remaining_size. Those 39 existing refusals never represented the new date or
quantity cases; they remain separate shared-model boundaries.

The five-order production `&dyn Exchange` regression drives start, fetchOrders
and native Store loading with null/false/true/missing dates, numeric quantities
and a valid dated sibling. It checks lastUpdate 0/0/1/NaN plus the valid UTC date,
amount/remaining/filled bits, raw info presence and all five committed rows.
The old production mapper compiles and fails startup on numeric size. Keeping
that size fix but reverting only Date conversion compiles and fails startup on
created_at instead. The independent mapper corpus also compiles and fails at
its new null-date case on the old production mapper.

The shared host-timezone capability is deferred by the overseer: the native
server runs in UTC. Both production mapper callers retain their UTC-host
assumption; no venue-specific offset or new runtime hook is added. The actual
source UTC/Los_Angeles difference for an offset-less date is recorded in the
shared known-residuals audit. The existing generic parser callback is retained.

After exact restoration, all 13 constructed exchange tests and the complete
152-case actual-source/native mapper comparison pass again. The mixed batch
checks both the returned native Orders and committed Store fields, including
NaN and the missing created_at info property.
