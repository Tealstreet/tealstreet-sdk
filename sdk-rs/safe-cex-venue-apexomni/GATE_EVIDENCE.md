# ApexOmni record controls

The maintained oracle reads the current TypeScript declarations through the
compiler API and exercises actual production Rust decoders and serializers.
String-only enums reject tagged objects; public records reject positional arrays.

## Inherited optional-record evidence (f25a192127)

GetHistoryFundingOptions, KeyPair and ApiKeyCredentials distinguish absent
optional fields from present null. Each omitted field, all optional fields omitted,
and malformed present values are exercised through trait and concrete calls.

Author control: removing the numeric optional-field presence decoder compiled,
then failed `public_records_match_live_typescript` on the named `page` null input
at exit 101. The decoder was restored and the record suite passed.

Independent reviewer controls: removing `page`'s serialization omission attribute
compiled, then failed the named omitted-page assertion at 101. Separately changing
TypeScript's `publicKeyYCoordinate` from optional to required failed the named
KeyPair missing-field assertion at 101. Both sources were restored; the live
record test passed. The reviewer returned MERGE.

The inherited evidence at f25a192127 reported six protocol/record tests passed, formatting passed, host
all-target strict Clippy passed, and wasm32 all-target strict Clippy passed
separately with dependencies included. These checks do not claim completion of
the remaining venue rows.

## Types ingress audit after integration a3d514bec8

The adoption checkpoint is 3af3adc7b0; the merge-down is 510fcfa6c1. The
working-tree baseline was measured with Python 3.11 tomllib: 168 total,
18 ported, 15 divergent, 135 pending. Integration alone measured 168 total,
1 ported, 3 divergent, 164 pending. This audit does not claim those inherited
29 additional nonpending rows as new work.

All 12 implemented public records now capture at their public Deserialize entry
with JsNumberInput and register every output field explicitly. The 16 numeric
fields compare Node's pre-serialization typeof/bits over 14 raw number cases via
raw serde, owned Value, shared raw/owned/borrowed ingress, and native negative-zero
Value. Serialize now retains native f64 events; JS wire bytes are an explicit
shared js_json_stringify projection. No REST or WebSocket callers exist in this
crate yet, so there is no claim of venue runtime adoption.

Native field assertions cover each decoded record field with distinct values and
one-false-at-a-time boolean cases. Native variant assertions map all four venue
enums to the actual executed TypeScript's named members. The live schema mutation
adds an inherited required field; changing it fails the named schema assertion.
Both oracles require SAFE_CEX_TYPESCRIPT_PATH explicitly.

Author controls, each compiled and then restored:
- Number sign: `every_numeric_field_preserves_raw_and_existing_value_bits`
  failed at exit 101 with `numeric ingress changed Number bits at $.u`;
  the restored test passed.
- Boolean swap: `decoded_fields_match_typescript_schema_individually` failed
  at exit 101 with `PerpetualContractObject.enableDisplay decoded native field`;
  the restored test passed.
- Symmetric enum decode/encode swap:
  `decoded_enum_variants_match_live_typescript_members` failed at exit 101
  with `OrderSide.BUY decoded native variant`; the restored test passed.

A sub-agent source review found the boolean and enum fixture gaps and the
protocol oracle's fallback compiler lookup. Those three findings are repaired.
The final-SHA review is issued separately after this evidence is committed.

The symmetric boolean decode/encode swap also compiled and failed
`PerpetualContractObject.enableDisplay decoded native field` at exit 101;
restoring both mappings passed the same test. This is independent of the
serialization roundtrip and closes the same-value boolean fixture gap.

Measured audit scope: 12 implemented records, 16 numeric fields, 14 raw-number
cases, and 49 variants across four enums (7 cancel reasons, 37 markets, 2 sides,
3 order types). Record native checks cover the schema-derived fields, including
one-false boolean inputs. This is not an audit of the 135 pending rows.

Measured gates before the handoff commit:
- ApexOmni all-target tests: 10 passed (5 protocol, 5 records), no ignored tests.
- Host all-target strict Clippy: passed, --jobs 1, warnings denied.
- Crate formatting and git diff whitespace check: passed.
- CI wiring test: 252 passed after merge conflict resolution.
- Layer validator: 38 classified workspace crates; ApexOmni's new num edge is
  included. The structural validator reads metadata; it runs no other crate suite.
- Extractor drift: 7334 symbols in 511 source files; 0 changed manifests,
  0 added and 0 orphaned symbols after canonicalizing the two edited manifests.
- Python 3.11 tomllib exemption parse: 338 blocks, 338 distinct rs entries.
- Wasm32 all-target strict Clippy: passed separately, --jobs 1, warnings denied.
- Parity invention/status validator: passed, 7334 rows and 3262 public Rust
  items. This is the mandatory structural index validation, not a workspace test suite.

Wasm execution, REST reads, writes, signing adapters and WebSocket behavior were
not implemented or executed in this slice. The 135 pending rows remain pending.

## Constants and enum completion after 4ab766a9c4

Python 3.11 tomllib baseline: 18 ported / 15 divergent / 135 pending. This slice
adds the Market type alias (ported), Market value namespace (CHOSEN divergence),
TimeInForce and OrderStatus (ported): 21 ported / 16 divergent / 131 pending.
Actual TypeScript execution supplies every constant and enum value, plus Market's
identity with ApexMarket and its 37 members. Strict decoding includes rejected
shapes; native variants are checked independently of serialization roundtrips.

The maintained production mutation corpus covers 13 constant rows, six enum rows,
and two Market alias rows. The alias rows share a production type alias but have
separate named assertions. All 21 failures were observed in a compiled corpus at
exit 101, then all row checks passed after restoration. Example named assertion:
`ApexOmni protocol row RECV_WINDOW`. The runner makes two sequential Cargo calls
with --jobs 1; each test binary captures its entire TS corpus once via OnceLock.
Mutations are independent by row and comparisons continue after each mismatch.
The separate actual-TS source-mutation controls remain enabled.

Sub-agent review found runtime-only identity was blind to the erased Market type
alias. The corpus now uses a diagnostic-checked TypeScript Program to compare
that actual declaration's type with ApexMarket. A type-only source mutation to
OrderType fails `ApexOmni protocol row Market type: TypeScript declared alias
identity` at exit 101 while leaving the runtime alias unchanged. Restored protocol
checks pass. Rust declares an explicit type alias so the source index resolves the public item.

Final slice gates (integration a3d514bec82d6760d4e9e7a27186f992d175c36d is an
ancestor; no integration merge-up performed):
- Production mutation corpus: 21 named failures, exit 101; restored corpus exit 0.
- Scoped all-target suite: 12 passed (7 protocol, 5 records), zero ignored.
- Host strict all-target Clippy: exit 0, --jobs 1, warnings denied.
- Wasm32 strict all-target Clippy: separate invocation, exit 0, --jobs 1.
- Crate formatting and git diff whitespace checks: exit 0.
- Parity invention/status validator: 7334 rows, 3265 public Rust items, exit 0.
- Extractor: 7334 symbols / 511 files; zero changed, added or orphaned entries.
- Layer validator: 38 classified crates, exit 0.
- Python 3.11 tomllib exemptions: 338 blocks, 338 distinct Rust entries.
- Python 3.11 tomllib ApexOmni: 21 ported / 16 divergent / 131 pending, 168 total.
- Sub-agent re-review: zero remaining findings after compiler type-identity fix.

These results cover the constants/enum slice and inherited record regression
checks. They do not claim implementation of the remaining 131 pending rows.

## Eight private record rows after c34c3f93ff

Implemented WalletObject, OpenPositionObject, AccountsItem, AccountObject,
WorstPriceObject, FundingRateObject, HistoricalPNLObject, AccountBalanceObject:
80 fields, six numeric fields. Each follows the actual TypeScript declaration.
The UTF-8 boundary records eight CHOSEN divergences; pending rows remain pending.

Production controls swapped two string serde names symmetrically per row in one
compiled corpus. Eight named decoded-native assertions failed at exit 101 after
their structural roundtrips passed. Exact JSON property-order assertions also
caught five swaps; this does not substitute for native assertions. Restoring
sources passed both private corpus tests. Example assertion:
`WalletObject.userId decoded native field`.

Records.cjs invokes the actual TypeScript compiler with inherited property
resolution. Its normal corpus was captured separately by four tests before this
slice and is now captured once per process through OnceLock; source-mutation
controls each use one separate corpus invocation. Runtime metadata: Node v26.7.0,
major 26, arm64. No NaN case is claimed in this raw-JSON corpus; signed zero stays
bit-exact. Native boolean supplements vary one boolean at a time.

Final measured gates on integration a3d514bec82d6760d4e9e7a27186f992d175c36d ancestry:
- 14 scoped all-target tests passed; zero ignored; --jobs 1.
- Host and wasm32 strict all-target Clippy passed in separate invocations.
- Crate fmt and whitespace checks passed.
- Parity structural validator: 7334 rows, 3273 public Rust items.
- Extractor: 7334 symbols / 511 files, zero changed/added/orphaned manifests.
- Layer validator: 38 classified crates.
- Tomllib exemptions: 338 blocks / 338 distinct Rust entries.
- Tomllib venue: 21 ported / 24 divergent / 123 pending; 168 total.
- Read-only sub-agent source review: zero findings across 80 fields and controls.

Regression gates include the earlier constants, enums and public records.
The remaining 123 pending rows are not included in the implementation claim.

## Local-only six-record continuation after 5d8164c530

Origin remains pinned at reviewed 5d8164c5305f05950c16886c8284014ec1f709a4.
The following evidence belongs to the subsequent local commit, not that origin SHA.

MultiChainObject, SymbolObject, PerpetualObject, SymbolInfoObject, UserObject and
OrderObject add 114 fields, including 12 numeric fields. The inherited
PerpetualContractObject fields remain explicit in SymbolInfoObject. Four opaque
JSON paths capture numeric input before Value construction and register only
those fields as passthrough output. Absent userData and present null stay distinct.
Opaque payloads cover the JSON domain, not arbitrary JavaScript heap values.

Six production mutations compiled in one corpus and failed six independent native
assertions at exit 101, then restored and passed. Five mutations are symmetric
serde-name swaps; SymbolObject collapses global.feeAccountId onto feeAccountL2Key.
PerpetualObject's control reads its usdcConfig.currency id so nested-row mutations
cannot substitute for that row's own failure. Example named assertion:
`MultiChainObject.currency independent native axis`.

A sub-agent found that generic failure matching could accept nested-record
failures as proof of SymbolInfoObject heritage. The fixed TypeScript mutation
controls require each exact row assertion. A permanent counter-control drops only
the new inherited field from SymbolInfoObject's schema and proves the control
fails `config-inherited must fail its specific row SymbolInfoObject` at exit 101.
The restored control passes. Sub-agent re-review found zero remaining issues.

Final measured gates:
- Scoped all-target tests: 20 passed (7 protocol, 13 records), zero ignored.
- Host strict all-target Clippy and separate wasm32 strict Clippy: passed.
- Cargo commands sequential with --jobs 1; no Yarn shim invoked.
- Crate formatting and diff whitespace checks: passed.
- Parity structural validator: 7334 rows / 3279 public Rust items.
- Extractor: 7334 symbols / 511 files, zero changed/added/orphaned entries.
- Layer validator: 38 classified crates.
- Tomllib exemptions: 338 blocks / 338 distinct Rust entries.
- Tomllib venue: 21 ported / 30 divergent / 117 pending; 168 total.
- Integration ancestry: a3d514bec82d6760d4e9e7a27186f992d175c36d, unchanged.

The normal record corpus invokes Node once per process; each source mutation uses
one invocation for its entire corpus. Runtime remains Node v26.7.0 / major 26 /
arm64. Numeric comparisons keep signed zero exact; no NaN bit equivalence is
claimed by the raw-JSON corpus. Earlier record and protocol regressions passed.
The remaining 117 rows are pending and are excluded from this implementation claim.

## Local interface completion checkpoint after c13f3aa8f5

Five rows: ISO8601, WithPrice, OrderWithClientId, CreateOrderOptionsObject and
OrderResponseObject; four records carry 43 fields. TypeScript source mutation
controls are specific to all five rows. Production controls compiled five row
mutations plus one WithPrice undefined-output mutation, observed six named
failures at exit 101, restored source and passed both filtered control tests.
Example: `WithPrice present undefined property omitted`.

The scoped records target passed 22 tests after the fixes. No full merged gate
sheet is claimed at this checkpoint: integration advanced to 9419b340d8a6,
and the continuation will merge it on a separate branch before full gates.
Tomllib: 21 ported / 35 divergent / 112 pending; zero pending interface rows.
Sub-agent source re-review found zero remaining issues after adding the native
WithPrice Some(()) output witness and its compiled mutation control.

NaN-only canonicalization now covers native numeric events before wire
projection. Capture metadata records Node v26.7.0, major 26, Node arm64 and Rust
aarch64 on this host. Raw signed zero and both native signs remain bit-exact.
Bare Serde F64Deserializer does not implement the shared raw-capture newtype
hook; the native-event adapter explicitly implements that hook. Raw/owned/
borrowed/shared production JSON entry paths are separately exercised.

## Integration-down checkpoint 3feeeb1ed0

A separate local branch feat/rust-venue-apexomni-next merged 9419b340d8a6.
The original remote branch remains pinned at 5d8164c530. The only textual
conflict was the CI wiring audit: integration's exact master/integration push
trigger replaces obsolete wildcard tests, while ApexOmni's oracle omission and
failure-masking controls remain. The CI wiring audit passed 252 tests.

Merged ApexOmni gates: 29 tests passed (7 protocol, 22 records); separate host
and wasm strict all-target Clippy passed; crate fmt passed; layer validator
classified 38 crates. Initial parity validation required explicit Rust rename/
split targets for ISO8601 and CreateOrderOptionsObject. After those note-only
repairs, parity passed 7334 manifest rows / 3339 public Rust items.
Tomllib exemptions: 359 blocks / 359 distinct entries. Venue remains
21 ported / 35 divergent / 112 pending; zero pending interface rows.

Global extractor is NOT green: 7341 source symbols / 512 files versus manifests,
with seven added symbols and seven changed manifests outside ApexOmni:
exchanges/base.ts, exchanges/base.ws.ts, exchanges/phemex/phemex.api.ts,
exchanges/phemex/phemex.types.ts, utils/unknownSendOutcome.ts,
webworker/chaserOrder/ChaserOrderManager.ts, and ChaserOrderOptions.ts.
No ApexOmni source was listed in the actual extractor's changed/added/orphaned
results. These external rows were not edited.

Sub-agent review followed lib.rs's six module declarations and read all seven
production files: zero Store writes. The parsed crate dependencies are only
serde, serde_json, safe-cex-types and safe-cex-num. No Store migration or boxing
exists in this slice. At 9419b340d8a6 the shared error conversion dropped wrapper
identity; a subsequent fetch measured 9af82bfeaefa3496cd8eea0a29b9ce7d3c1741a7,
whose runtime.rs now contains store_subscriber_ids and both subscriber_id push
sites. ApexOmni has zero FINDING rows referring to that former shared gap.


## Lifecycle slice: 10 rows

PR 7486 moved ten pending rows to divergent at implementation checkpoint
00540b4db3f54f5a6513d2ac5e6458d5461f6134. The independent parent was
1181108d2dfeee7dd6cb1d21a3a39922e1e13b9e; counts changed from
27 / 102 / 39 to 27 / 112 / 29. The justifications below belong to those
ten implemented bodies, independently of any earlier slice.

## Bucket justification for the ten moved rows

All ten are **CHOSEN**. These are direct boundary decisions for this slice,
independent of classifications in earlier ApeX slices. The shared string
boundary is defined at
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`.

| Source method | Reason for CHOSEN |
| --- | --- |
| `validateAccount` | Typed validation strings and shared errors replace arbitrary thrown JS values and non-string response messages. Unsupported truthy message types are refused visibly. Shared module SDK initialization and signer construction stay with the wallet host, avoiding a separate JS error heap and wallet lifecycle. |
| `start` | Shared Store/Market/Ticker records and polled concurrent futures replace mutable record aliases and eager Promise continuations. Public-data initialization and socket factories stay with their host. The first tick is owned background work; startup does not await it. A separate JS scheduler would detach this venue from the SDK runtime. |
| `fetchAndSetPrivateSnapshot` | The real wrapper polls its helper future rather than creating an eager JS Promise. It preserves delegation and shared errors while retaining one runtime executor. |
| `fetchAndSetPrivateSnapshotHelper` | Shared Position/Order/Balance records and polled concurrent reads replace mutable JS aliases and Promise scheduling. Native optional None collapses absent and explicit undefined, so the oracle projects optional model fields explicitly. The source WebSocket cutoff and merge priority remain in the body; a second record heap would bypass shared Store ownership. |
| `tick` | A host loop capability owns the next future and interval instead of ambient JS timers/Promise scheduling. The source guard and snapshot/error/interval sequence remain in the body; another scheduler would split runtime ownership. |
| `fetchOHLCV` | Shared options, Timeframe and Candle records replace arbitrary JS option objects and mutable candle aliases. Imported dayjs clock, Date TimeClip and calendar subtraction stay with the host; this preserves the distinction from the virtual exchange clock without introducing another calendar environment. |
| `listenOHLCV` | Native callbacks/cleanup return Result and owned Candle values instead of JS void callbacks, arbitrary thrown values and shared mutable references. Options, callback invocation and cleanup are delegated directly to the public socket capability; a separate callback heap would split socket ownership. |
| `listenOrderBook` | Native callbacks/cleanup return Result and owned OrderBook values instead of JS void callbacks, arbitrary thrown values and mutable aliases. Symbol, callback invocation and cleanup remain direct delegation through the public socket capability. |
| `signOrder` | The source payload:any and SDK signer become the existing typed SignContractPayload and opaque wallet capabilities. The unused receiver is elided into an associated function, prefix stays ignored, and the body calls the real native sign_contract before strict signature property access. A second JS payload/wallet heap would duplicate SDK ownership. |
| `setLeverage` | Shared typed Market/Position values and polled requests replace unrestricted JS records, aliases and eager Promise scheduling. Missing state is an explicit error; shared Math extrema preserve NaN/signed zero and shared toFixed formats the rate. Keeping one Store/transport model avoids parallel ownership. |

Native gates at that checkpoint passed 27 exchange and seven signing tests,
with ten compiled controls failing at named native assertions and both
restored targets passing. Scoped all-target check and extractor check exited 0.
The full scope and non-claims remain in LIFECYCLE_GATE_EVIDENCE.md.

## REST and trading slice: five rows

Independent parent: `9c151409d125381f67d08d2b644afa89c77a79c8`.
The branch is fresh from that merged integration head; no unreviewed slice
is its ancestor. Recursive Python 3.11 `tomllib` enumeration of this venue's
manifest directory, matched by file and `(ts, kind)`, finds exactly five
pending-to-divergent changes: `27 / 112 / 29` becomes `27 / 117 / 24`
(ported / divergent / pending), with all 168 identities retained.

All five bodies are implemented. Each row is **CHOSEN** for its own boundary
below. Shared string behavior follows
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`.

| Source method | Why CHOSEN is the appropriate bucket |
| --- | --- |
| `createAPI` | Owned prepared configs and shared typed options replace executable Axios/JS objects, getters, prototypes and aliases. Imported client, retry, AxiosHeaders, Date and tunnel operations remain host capabilities; the factory and both installed handler bodies execute natively. This keeps one transport runtime. Strict nullish-access errors, signatures, retry flags, header bypass and registration order remain explicit body logic. A second JS heap/scheduler would split transport ownership. |
| `fetchHistoricalFills` | Shared typed options/executions and polled requests replace arbitrary option objects, mutable record aliases and eager Promises. An explicit timeout argument carries the undocumented extra field absent from the closed shared options record; it is not silently dropped. Public-only detection uses shared Base credentials/options. Retained data preserves REST envelopes and source fallbacks, but cannot execute getters or reproduce a separate JS error heap. Keeping the shared Store and transport avoids duplicate ownership. |
| `placeOrder` | Shared placement enums/options and opaque imported wallet capabilities replace arbitrary JS records and SDK objects. Decimal coefficient/exponent operations preserve the exercised BigNumber calculations before Number conversion; mutable global BigNumber configuration and executable brand objects are outside this ABI. Invalid lot counts and non-string response IDs fail visibly. Host Date, RNG and SDK initialization stay with their owners; reproducing ambient state would create competing wallet/clock models. Source split rounding and inverse-table last-wins behavior are preserved, including over-allocation in the measured split case. |
| `updateOrder` | Shared Order/update/placement models collapse absent and explicit undefined option fields and replace mutable spread aliases. The real body reconstructs bracket legs, applies the requested update, cancels the base order, then invokes the native placement body. A parallel JS record heap would bypass Store ownership; a separate generic spread ABI is not introduced to disguise the typed boundary. |
| `cancelSymbolOrders` | Shared typed placement records replace the unrestricted Order spread into imported Base `placeOrders`. The recreation projects declared placement fields; source-only extra Order fields are outside that ABI. Missing required market state fails visibly. The body retains the source non-Limit predicate, bulk cancellation and reversed side. This keeps one placement model rather than an unrestricted second options heap. |

Actual-source floors are 27 API cases (3 factory, 12 request, 12 response),
13 fill-history cases, and 13 trading cases (7 placement, 3 replacement,
3 bulk cancellation). Native fields, numeric bits, signatures, consumed SDK
arguments, result categories/errors and operation order are asserted directly
before any wire assertions. History covers authoritative REST versus cached
fills, errors, malformed rows, duplicate rows, time/limit filtering and both
explicit public-only mode, missing credentials and a null REST envelope. Its source page ceiling
is three, but the source row-length guards stop a nonempty candidate after
one page; second-page coverage is not claimed.

The SDK control profile executes actual source signing and validators with
controlled imported SDK primitives. Both source class and signer use the
same Array realm so `instanceof Array` validation is real. Source successes
are asserted independently. Tests compare decoded signing arguments before
submitted form data; they do not prove the external wallet SDK itself.
API source handlers use actual noble HMAC and AxiosHeaders; imported client,
retry, Date, tunnel and shared error-reporter internals are not independently
proved by this venue corpus. No claim covers every JS Promise/native Future
interleaving, cyclic/executable objects, mutable aliases, live credentials,
network service behavior, or wasm execution.

Five compiled controls change factory timeout, history quantity, placement
amount, replacement TP price and cancellation side. The four leaf controls
run together; replacement runs separately so placement cannot mask it.
Each reaches exit 101 at its named native assertion; restoring all sources
passes the scoped exchange target. The first reporter run used the wrong
placement assertion label (operations versus result length); the production
mutation was caught, and the corrected reporter subsequently passed.

### Measured checkpoint and exits

All gates below bind to `7a2b5bd0cdf6c1e1729dc99bc2192d23f0edae53`.
The five-control script restored its saved source bytes, and
`git diff --exit-code 7a2b5bd0cdf6c1e1729dc99bc2192d23f0edae53 --`
the crate's `src`, `tests` and `scripts` exited 0 after the gates.
Every oracle used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Cargo was serialized with `--jobs 2`.

| Gate | Measured outcome |
| --- | --- |
| Scoped `cargo check -p safe-cex-venue-apexomni --all-targets` | Exit 0 |
| `scripts/mutate_rest.py`, four compiled leaf controls | Cargo exit 101; factory timeout, history quantity, placement amount and cancellation side fail their named native assertions. Placement also makes its replacement caller fail. |
| Same script, isolated replacement TP-price control | Cargo exit 101; `updateOrder native operations` fails. |
| Same script, restored `--test exchange` target | Cargo exit 0; 34 passed; script exit 0. Five controls caught, zero survivors. |
| Scoped `--test signing` target | Exit 0; seven passed. |
| Extractor writer | Exit 0; zero manifests updated, added or orphaned. |
| Extractor check | Exit 0; would update zero manifests. |

Logs are generated under ignored `sdk-rs/target/apexomni-controls/`; this
authored, tracked sheet is the reviewable evidence. No full suite, workspace
build, host clippy, wasm clippy, wasm build or global mapping audit was run
for this slice. The remaining 24 pending rows are the actual public/private
WebSocket bodies; the imported socket capabilities do not claim those rows.

### Integration merge preservation

Merged `2c8ec8fac92c7f5aacdf00e40e225eae95c095b8` down into the branch at
`ab30c9b96c0ee19630af17b4a395185c748b95d4`. The only conflict was in
`sdk-rs/parity/exempt.toml`. Both sides' `(rs, reason)` contributions were
independently compared with the resolved TOML and retained. Crate `src`,
`tests` and `scripts` are byte-unchanged against the measured checkpoint:
the requested `git diff --exit-code` comparison exited 0.

At that merged SHA, scoped all-target check, 34 exchange tests, seven signing
tests and extractor check each exited 0. Recursive `tomllib` census against
the merged integration head still finds exactly five pending-to-divergent
rows and `27 / 117 / 24`. Mutation evidence remains bound to the unchanged
source at `7a2b5bd0cdf6c1e1729dc99bc2192d23f0edae53`.

## WebSocket slice: all 24 remaining rows

Fresh parent: `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444`, a merged
integration head, without an unreviewed slice as an ancestor. Independent
recursive `tomllib` expectations are 168 retained identities and
`27 / 117 / 24` before this slice, `28 / 140 / 0` after it. Exactly 24
pending rows move: one to ported and 23 to divergent. The remaining venue
pending count is zero; this is a symbol census, not end-to-end SDK proof.

### Bucket justifications for this slice

The shared string contract is
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`. Every CHOSEN
decision below is this row's own boundary decision; earlier slices are not
evidence for these classifications.

| Source row | Bucket and direct reason |
| --- | --- |
| Public class | CHOSEN: owned topic/handler collections, Rc callbacks and imported Base socket ownership replace mutable JS class/prototype/GC aliases. A second JS heap would split Store/socket ownership. Default instrument and pong handlers are installed by the real constructor. |
| Public constructor | CHOSEN: Rc ownership and weak event/handler references replace superclass/GC retention. Imported Base initialization belongs to the host; constructor arguments, default subscriptions and pong dispatch are checked. Keeping lifecycle ownership in one runtime avoids a second object heap. |
| Public connectAndSubscribe | CHOSEN: host clock, nanoid, dial and listeners replace ambient/DOM objects. Weak callbacks refuse an unavailable owner visibly. The body preserves endpoint selection, timestamp replacement, request id and listener order. Registered open/message callbacks run in the gate; a second listener/clock runtime would duplicate ownership. |
| Public onOpen | CHOSEN: typed inherited lifecycle/error capabilities replace dynamic JS receiver dispatch. The body retains disposal guard and subscribe, ping, reconnected order. Keeping one host socket owner avoids independent connection state. |
| Public ping | CHOSEN: the host owns monotonic performance time and inherited pingAt/send state instead of ambient mutable performance and receiver properties. The disposal guard, assignment and unlogged ping payload remain direct body logic; another clock would split socket timing. |
| Public subscribe | CHOSEN: owned typed topic lists replace unrestricted Object.values/flatten inputs and aliases. This keeps one topic heap. The actual body retains insertion order and chunks of 50; a 52-topic source case independently covers the chunk boundary. |
| Public onMessage | CHOSEN: UTF-8 frames and owned parsed data/callback lists replace unrestricted UTF-16 and mutable prototype/list aliases. Receipt/stats/pong belong to Base. Targeted topic extraction, exact then prefix dispatch, stats before parsing, raw ping response and topic-free pong stay in the body. A duplicate listener heap would split runtime ownership. |
| Public handleInstrumentInfoEvent | CHOSEN: typed shared Ticker/Market/Position models and patches replace arbitrary JS records and aliases. The real body preserves truthiness, mark/index fallback, percentage scaling, per-row linear PnL and unknown-market continuation. A separate record heap would bypass shared Store updates. |
| Public listenOHLCV | CHOSEN: typed Candle/options and Result callbacks replace live aliases and arbitrary callback throws. TypeScript assigns raw trade p to close, including strings; the numeric Rust Candle refuses a string close visibly. A separate untyped candle heap would bypass shared models. Numeric prices retain primary/secondary parsing, bins, rollover and backward guards. String volume precision is preserved before shared divide. The source uncancelled offline candle waiter is retained; weak owner lifetime differs from JS GC. |
| Public listenOrderBook | CHOSEN: owned shared OrderBook values and Result callbacks replace live book aliases, unrestricted level objects and throws. Host handles replace Node timers, preserving one scheduler. Snapshot/delta, shared sorted updates/totals, cleanup and cancellation of an offline waiter remain in the real body. |
| Private class | CHOSEN: shared parent/Store ownership, Rc callbacks, native timer handles and cumulative-fill maps replace JS field/prototype/GC aliases. The real new body represents implicit construction and initializes any-message-as-pong, clocks and empty trackers. A second Base socket/timer runtime would duplicate ownership. |
| Private throttleFetchOrders | CHOSEN: host handles and polled typed futures replace Node timer objects and eager Promise continuations. The real one-second cancel/replace/clear/fetch flow preserves both post-await disposal checks, full Store reconciliation and warning. One runtime owns reconciliation. |
| Private throttleFetchPositions | CHOSEN: host handles and typed polled futures replace Node timers/Promises. The source has no post-await disposal guard, and that absence is preserved, including a disposal-during-request case. This retains one Store/scheduler without silently correcting source behavior. |
| Private connectAndSubscribe | CHOSEN: imported routing, clock, nanoid, dial and listener capabilities replace DOM/ambient objects and aliases. The real body constructs timestamp/request id before route selection, passes parent name/options to the route, selects the tunnel when configured and registers three listeners. Installed callbacks run. Weak native ownership avoids a second listener heap. |
| Private onOpen | CHOSEN: polled Futures and opaque auth/reconnect effects replace arbitrary throws/eager Promises and detached scheduling. Failed login sends nothing and reconnects; success records lastConnectTime then subscribes/pings and starts snapshot work. The source lacks a post-auth disposal recheck and remains so. Background work belongs to the host runtime. |
| Private onMessage | CHOSEN: UTF-8 frames, owned JSON and shared typed records replace getters/prototypes/aliases. Imported Base receipt/diagnostic/stats/pong stay with their owner. Rejection warning, account dispatch, incoming ping args and five-second pong cadence are retained. A second frame heap would split socket state. |
| Private ping | CHOSEN: monotonic time and the inherited ping-timeout slot belong to the host; native handles replace JS timer identity. Pong/dispose see that same slot. Logged send, cancellation and five-second self-heal remain native body logic, avoiding a parallel keepalive scheduler. |
| Private handleWsAccountsV2Topic | CHOSEN: owned account envelopes replace executable property access and aliases. Strict contents access, array truthiness and fills, orders, positions ordering are retained. The native cache ordering is checked; another envelope heap would split records. |
| Private handleFillTopic | CHOSEN: owned prepared fill records and typed Execution cache replace unrestricted JS records/aliases. Each fill invokes the real record_ws_execution; no fill notification is emitted. A second cache heap would duplicate exchange ownership. |
| Private handleOrderTopic | CHOSEN: typed Order/Execution records, string IDs and retained info replace arbitrary records/aliases. The shared emitter payload refuses native NaN metadata visibly; source events can carry NaN. Host Date and virtual time remain distinct. Cancellation throttling/blacklist, freshness, deltas/fees, terminal tracker deletion, reasons and Store removal/closed updates stay in the body. Repeated terminal updates preserve source behavior. Keeping one Store/cache/emitter avoids a second event heap. |
| Private handlePositionTopic | CHOSEN: typed Position values and full-field native patches replace JS aliases and receiver rebinding. Both bindings use the same parent Store/mapped position body; native numeric fields preserve binary64. Existing positions update, cutoff time and reconciliation remain in the body. A separate receiver heap would split Store ownership. |
| Private auth | CHOSEN: typed options snapshots and opaque remote-signer capabilities replace mutable/getter-backed credentials, arbitrary signer objects and Promise/error heaps. Credentials for login are read after remote signing resolves. Request metadata, visible remote failure, base64-secret HMAC keying and the one JSON-string login argument stay in the native body. One host owns the wallet. |
| Private subscribe | PORTED: this nullary body has only fixed ASCII literals and one imported Base sendWithLog call. It touches no arbitrary strings, mutable records, clocks, callbacks or timers. The exact ordered three-channel frame and logging flag match actual source; source void corresponds to native unit, and imported send failure propagates without interpretation. No typed boundary is concealed by this classification. |
| Private dispose | CHOSEN: host handles replace Node timer identity and Base owns inherited keepalive teardown. The body preserves orders then positions cancellation/slot clearing before Base dispose and independently checked remaining timer counts. A second timer runtime would split cleanup ownership. |

### Native/source scope

The oracle executes the actual TypeScript public/private class declarations,
the actual parent mapping/cache bodies, actual DefaultStore and source math,
book/calendar helpers, JSON parser and local HMAC dependency. Base socket,
clock, timer, dial, routing, remote signer and snapshot-dispatch imports use a
controlled caller profile. It measures their consumed arguments and order,
not the internals of those imported libraries or a network connection.
Class initialization flags are observed from actual source class state.

There are 72 actual-source cases: three for each of 24 rows, with fixed source
and native comparison floors. The 25 native tests include a separate CHOSEN
candle boundary: source produces string close `20.25`; native returns the
explicit error and makes no fabricated numeric-close callback. Native
numeric bits, enum/field values, Store/cache state, operation order, clocks
and timer counts are asserted directly. Login strings are decoded before
their fields/signatures are asserted. There is no decode/re-encode equality
gate or serde model roundtrip acting as the native assertion.

Both connect gates invoke the actual registered native open/message
callbacks. The public subscribe case prepares 52 topics independently.
Cleanup cases distinguish the source's orphaned offline candle waiter from
the cancellable book waiter. Reconciliation cases include disposal during
the awaited request: orders suppress the write, positions retain it. Fill
frames emit no toaster; order frames own freshness-gated notifications.

One production-body control per row is compiled and evaluated at its named
native assertion, separately to prevent a leaf mutation masking its caller.
An initial fills mutation failed compilation because it no longer consumed
its loop variable; that attempt is not a discriminating gate. The replacement
mutation consumes the variable and reaches the native cache assertion.
Generated logs are ignored under `sdk-rs/target/apexomni-controls/`.

No claim covers every Promise/Future interleaving, JS prototype/getter/GC
behavior, arbitrary callback throws, imported Base/timer/signer internals,
live exchange/network behavior, wasm execution or cross-architecture proof.
The class/interface adapters do not fabricate bodies or replace a remaining
pending row with an inherited NotImplemented default.

### WebSocket measured checkpoint

Measured SHA: `7bf986f454188f986334e268842c1abc161d9f53`.
Every oracle ran with
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`;
Cargo commands were serialized with `--jobs 2`.

| Gate | Measured result |
| --- | --- |
| `scripts/mutate_websockets.py` | Exit 0. All 24 separately compiled controls reached Cargo exit 101 and their named native assertion. Every per-control log contains successful compilation and a failed test result; zero survivors. |
| Same script, restored WebSocket tests | Exit 0; 25 passed. |
| Scoped crate all-target check | Exit 0. |
| Scoped `--test exchange` target | Exit 0; 59 passed, including the 25 new WebSocket tests. |
| Scoped `--test signing` target | Exit 0; seven passed. |
| Extractor writer | Exit 0; updated zero manifests, added zero, orphaned zero. |
| Extractor check | Exit 0; would update zero manifests. |
| `git diff --exit-code` against measured SHA, crate `src`, `tests`, `scripts` | Exit 0 after controls restored. |
| Independent recursive venue census against parent `9d6f40b96f` | 24 moved: one pending-to-ported, 23 pending-to-divergent; 168 identities retained; 28 / 140 / 0. |

No full suite, workspace build, global Rust mapping audit, host clippy,
wasm clippy or wasm build was run. Those are separate claims from the scoped
crate compilation and source extractor exits above.

### WebSocket integration preservation

Merged integration `65e086a3c9039a79e4c7e9f354aba31d8f83009e` down.
The exemption conflict contained complete table headers on both sides;
the initial resolver incorrectly inserted another header, failed validation,
and its shell continued into a merge commit. Follow-up commit
`61335677d9c31bf2c58a3ded15d467fa740fc249` removes the conflict markers
without rewriting history. Resolved TOML contains no empty rows; independent
`(rs, reason)` sets from both merge parents are fully retained.

At that corrected merged SHA, scoped all-target check, 59 exchange tests,
seven signing tests and extractor check each exit 0. The crate `src`, `tests`
and `scripts` comparison against `7bf986f454188f986334e268842c1abc161d9f53`
exits 0: the body controls still cover byte-identical inputs. The independent
census against merged integration remains 24 moved (one ported, 23
divergent), 28 / 140 / 0, with 168 identities retained.

## ApexOmni continuation: first six recorded findings

Baseline integration parent `d8772a9e8b42c71faf7f56f192642aa70bf90108` has28 ported /140 divergent /0 pending,168 rows in16 recursively scanned manifests, stopped at file index15. The initial67 FINDING /73 CHOSEN note-text partition is unvalidated, with FINDING taking precedence over historical CHOSEN text. This continuation targets that67-row cohort; it does not audit all140 divergent rows.

The first six selected finding rows, in source order within their files, split into **three behavior-defect rows and three rationale rows**. Three confirmed defect rows among140 divergent rows is a2.14% floor; three among the selected six finding notes is50%. The latter is a selected prefix, not a venue-wide estimate or evidence that the fleet's3–4% pattern has been exceeded. No status is promoted: the manifest triple remains28/140/0. The proposed note tags become61 FINDING,76 CHOSEN and3 MIXED; the61 unreviewed finding notes and73 original choice notes remain unvalidated.

| Row | Cause and disposition |
| --- | --- |
| serializeApexPayload | Rationale: exact returned-query observation now supports the FORCED clause under [the canonical observability ruling](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict), alongside the existing prepared-data choice. MIXED; no native behavior change is claimed. |
| serializeApexPayloadLegacy | Rationale: the same observed query requires its own actual-source call and its own native positive comparisons. MIXED; no native behavior change is claimed. |
| generateApeXRandomClientId | Rationale: returned code units at the account substring are retained in source observations; the existing injected clock/RNG choice remains independently stated. MIXED. |
| ApexOmniExchange.getAccount | Defect repaired: checked property reads preserve the source null/undefined error location, TypeError kind and message; primitive account payloads and numeric userId remain native values. Existing host/data choices remain CHOSEN. |
| ApexOmniExchange.fetchPositions | Defect repaired: null account envelopes reject before fake positions or config writes; non-array positions retain the source optional-map TypeError. Existing typed output and host/data choices remain CHOSEN. |
| ApexOmniExchange.fetchOrders | Defect repaired: filtering finishes before mapping, including a later null id that must preempt earlier mapper warnings/refusals. Map.has retains key type and non-array responses retain the source filter TypeError. Cancellation cleanup retains NaN ages, matching the source delete-if-greater predicate. Existing shared typed-output choices remain CHOSEN. |

Only row-specific observations are added to the annotations; representation declarations remain at [the shared string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [the typed-output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [the snapshot boundary](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), and [the taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy). The existing checked-property implementation moves from api_runtime.rs to data.rs and is mapped to both the installed API handlers and the three consuming exchange bodies. No second string, Store, transport or executable-object model is introduced.

The source response supplement executes the maintained class through the existing exchange oracle's host fixture.33 independent cases stop at index32:7 account,10 position and16 order cases, stopping at per-method indices6/9/15. Six order cases cover NaN, both infinities, age10000, age10001 and future timestamps; sorted native cancellation entries are checked directly. Native results and exact error kind/message precede operation-order and Store-config comparisons; no decode/re-encode assertion is used. A first native filter mutation can agree on the valid-then-null case, but the missing-market-then-null case detects its premature warning; the separate bad-type-then-null case also constrains error precedence.

Accepted omission, retained visibly: for numeric order.id17, source returns numeric17 in an order while native Order.id:String refuses the whole result with the documented typed-field error. The numeric-key fixture remains in the corpus; it is not dropped after failing the first run. A separate mutation restores String coercion inside Map.has and fails this case. This output restriction is a local shared-record choice, not an application of the inherited-enum-only rule. mapOrder's separate finding notes remain open.

The surrogate supplement executes the three actual declarations plus their actual qs dependency. Each row has10 independently authored cases, stopped index9:6 decoded native positives and4 whole-input refusals. The source captures typeof and UTF-16 units before JSON encoding. The String-valued object probe uses the shared numeric JSON ingress and compares decoded native strings before invoking the native body. All ten decoded-input/refusal checks and the independent6/4 floors finish before any returned-query output assertion. It demonstrates whole-input refusal at that probe, not an invocation of a native serializer on an unrepresentable JsJsonValue. Literal backslash-u text, materialized units, valid pairs, scalar Unicode, replacement text, FEFF/NEL and empty strings are separate cases. The source's malformed-unit qs encoding remains a source observation, not a repaired serializer bug.

### Measured scoped tests and controls

At `85a490f790`, the initial three envelope tests measured101: account and position tests passed, while the numeric-order-id case exposed the native typed refusal. At `df3c11bc08`, the combined test attempt measured101 during compilation because serde_json::Value does not implement JsNumberOutput. That compiler failure is not a mutation control. The probe was corrected to a String-valued BTreeMap with the shared registered output contract.

The complete control receipt is attached to exact payload `31651eaf2db49598df0f426f21b4b304fc29f7d3` in sdk-rs/target/apexomni-response-surrogate-controls/31651eaf2db49598df0f426f21b4b304fc29f7d3/controls.json. Seven compiling production substitutions and three separate in-memory TypeScript substitutions are ten controls, not ten full-domain proofs:

| Measured command group | Exit | Seconds |
| --- | ---: | ---: |
| Six simultaneous native substitutions, both scoped targets with no-fail-fast |101|147.25|
| Restored exchange and pure targets,63+9 tests |0|46.57|
| Native Map.has String-coercion substitution |101|24.64|
| Restored order-envelope target,1 test |0|25.17|
| Actual primary serializer source D800→D801 substitution |101|17.12|
| Actual legacy serializer source D800→D801 substitution |101|0.81|
| Actual client-ID source D800→D801 substitution |101|1.18|
| Restored source-observation targets,3 tests |0|0.69|

Every negative group required its named runtime assertion, not merely101; there were no survivors or invalid controls in this run. The six native labels cover both serializer filters, the client-ID delimiter, getAccount null-root, fetchPositions null-data, and fetchOrders premature mapper operations. The three source substitutions preserve UTF-16 length while changing the observed value/encoded bytes. All source files were restored and tracked-clean before merge-down. The receipt's native_patch_sha256 field hashes the concatenated mutated source payload, not a Git diff.

Incoming integration `eadcf36393a2553140e39f5e44f4b8888cd3f314` was merged as `e719fa5c660ebc209e94a17448860784976d7015`, with96 named rust.yml steps before and96 after. This incoming change does not edit ApexOmni sources or fixtures. The authoritative shared .aimux/briefs/rust-venue-apexomni.md was rewritten with the measured baseline and divergence-finding scope; its content hash is c689ceaa583ffb6e7657e0d5e9bbd62ccfb0e23583a1b298ec42bde25706622f. The shared brief is operational state outside this code delivery.

Sub-agent review and exact-handoff scoped/build/host-clippy/wasm-clippy receipts follow after the final commit. The ten controls above remain attributed to31651eaf2d, not to a later SHA. The delivery receipts will be under sdk-rs/target/apexomni-delivery-gates/<handoff-SHA>/gates.json. No push, PR or integration merge is performed. Full venue/workspace runtime suites, extractor-check, wiring/layers/routing/report checks, separate wasm build/runtime steps and CI execution are not claimed here.

Canonical parity:extract writer after these annotation changes exited0 and reported Updated2 manifests,0 added,0 orphaned symbols, Done41.92s. All168 parsed row identities, kinds and statuses were compared with the pre-annotation merge checkpoint and remained unchanged. This is writer/reconciliation evidence, not an extractor-check or behavior claim.

### Adversarial review corrections

Read-only sub-agent review of9123a3db5380b1208d693c253b35e64c25d9d6c3 returned DO NOT MERGE with two findings. Both were accepted: fetchOrders had a pre-existing NaN cache-age predicate defect (within the already counted behavior row), and surrogate query assertions could preempt later decoded-native checks. Cleanup now keeps NaN ages; six independent timestamp cases and a separate reverting control constrain it. All surrogate decoded-input/refusal checks now precede all returned-output comparisons. The running row split remains3 behavior /3 rationale, not four behavior rows. Final immutable repair review and controls are measured separately from the historical31651eaf2d receipts.

The immutable repair checkpoint9aa84e0d898d0838d7601e03cc1bcec2c5f7a3a9 includes integration7bac54167ca1344621ff98cfc492c2838926bffb. Its bounded sub-agent re-review returned MERGE: both accepted findings closed, no further concrete issue in the six-row scope, read-only inspection only (no reviewer runtime gates). Canonical writer after the revised fetchOrders annotation exited0: Updated0 manifests,0 added,0 orphaned, Done23.78s. The checkpoint's source/review claims do not substitute for final delivery gates.

The final handoff receipt is keyed by the exact delivered SHA under sdk-rs/target/apexomni-delivery-gates/<handoff-SHA>/gates.json, with per-command logs. Required commands are workspace build, host workspace all-target clippy, canonical wasm clippy helper, and cargo fmt --check. Build/clippy use RUSTFLAGS=-Dwarnings, matching rust.yml; Cargo commands remain sequential with two jobs. Scoped exchange/pure/utils tests and the eleven named native/source substitutions (including the separate NaN cleanup control) run at that same SHA. Actual exits and failures belong to those receipts, not to an anticipated result in this paragraph. Original six-row status/count census remains28/140/0 and no pending row moved. Sixty-one original finding notes remain unaudited.

## Raw execution assets and remaining evaluation-order repairs

The preceding six-row local delivery is a629ebb27afcc548809457755faf3a03c1daf813. Its exact-SHA receipt records format0, scoped exchange/pure/utils0 (82 tests), eleven-control controller0 (each substitution101 at a named runtime assertion, restores0), workspace build0 and host workspace clippy0. The previous queued runner started wasm before cancellation reached it; the run and descendants were stopped after the user removed that lane gate. No wasm exit is claimed. The required lane gates are now format, workspace build and host workspace all-target clippy; wasm, wiring, layers and routing run centrally. These results cover a629ebb27a only.

This next unit is built directly on that reviewed but unmerged local delivery. The first deliverable remains independently fetchable from feat/rust-apexomni-findings-20261016; this branch adds four existing finding rows. Integration7bac54167ca1344621ff98cfc492c2838926bffb is already contained. No other unreviewed venue dependency stack was added.

| Existing finding | Repair and remaining row-specific cost |
| --- | --- |
| recordWsExecution | Stop inventing String commission assets from raw numbers, arrays, objects or booleans. Unsupported raw assets visibly refuse the whole execution before cache insertion. Keeping the shared String asset field is a local choice: widening it would also change fee-currency collection in safe-cex-exchange/src/position_intervals.rs::accrue_fill. Source retains the raw asset in a usable execution; no wire convergence is claimed. |
| fetchHistoricalFills | Preserve raw assets alongside the existing shared Execution until source coercions, time bounds, deduplication and truncation finish. Only final output admission can refuse an unsupported asset. The existing catch warns and returns cached WS fills or empty, whereas source returns the authoritative REST execution; this larger result loss is explicitly accepted for the same shared currency-field choice. Later source errors and rows removed by selection must not be masked by that choice. |
| fetchMarketsHelper | Complete the filter and all retained-row numeric evaluations before shared Market field admission. Later null rows or throwing numeric fields preempt earlier boolean/String refusals. Checked envelope/filter failures retain TypeError name/message. After successful envelope reads, admitted Store config updates remain observable before filter or mapping rejection. A successfully mapped raw nonboolean active field or numeric id still refuses the whole shared Market result, an explicit local model choice with a larger output loss. |
| mapPosition | Preserve short-circuit leverage selection; unused custom/default values cannot throw. Parse unrealisedPnl before missing-market fallback. Assert direct native position fields, exact errors, logs and clock counts. Existing prepared input, retained-info, typed-output and injected-clock choices remain. |

Representation declarations stay at [the String boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [owned snapshots](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) and [the taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy). The raw asset and Market field rulings above are local field choices; they are not applications of the [inherited-enum declaration](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary). No second public execution, Market, Store or object heap is introduced.

The unchanged maintained class declarations execute through the existing loaders. The supplement has independent floors of17 market cases (stopped index16),8 WS assets (stopped index7),7 malformed WS coercions (stopped index6), and21 historical cases (stopped index20). Sixteen historical cases cross8 assets with both cache states:10 explicit final-result refusals and6 direct positives. Five additional historical cases constrain later null/coercion errors, time filtering, duplicate replacement and truncation. Three retained continuation observations constrain mapPosition (stopped index2); the other continuation observations are not claimed as newly closed rows. All native assertions inspect decoded fields/errors, operation traces or Store/cache state; no new decode/re-encode comparison is used as evidence.

At30242486a35d197dc5338b8f3ef6df6c9f9176cb, the scoped exchange attempt exited101 during compilation because ExecutionsOptions does not implement Default. That is not a mutation result. Explicit option fields fixed the fixture. At a11cbe4356ad3f180bcde0c854edb077296126b9, all66 exchange tests passed (exit0); seven initial regressions each compiled and failed their named runtime assertion at101, and each restore passed at0, with no survivors/invalid controls. After staging complete mapping/selection and adding later-row cases,98a54a5d6e566a272987663cba2aa5b18d057fbd again measured scoped exchange0 (66 tests).

The reusable controller now defines17 independent native controls: ignored custom/default leverage, missing-market unrealisedPnl, complete filtering, numeric-before-admission, WS/REST asset coercion, complete-map boolean/String admission, numeric Market id coercion, historical admission before later errors, time filtering, deduplication or truncation, WS side/price error kinds, and missing config admission. Four additional AST-bounded, in-memory actual-TypeScript output substitutions independently change position PnL, market amount precision and WS/REST fill price, requiring the native comparisons to fail. Those final controls are not claimed as executed by their definition: final-SHA exits, named assertions, restores and any survivors belong to sdk-rs/target/apexomni-market-execution-controls/<handoff-SHA>/controls.json. Reviews are static adversarial sub-agent reviews; reviewer runtime gates are never inferred.

Four notes are reconciled in place with all symbol identities/kinds/statuses retained. The census remains28 ported /140 divergent /0 pending,168 rows in16 manifests. Pending movement is0. Ten selected findings are now examined:7 rows with behavior defects (including two coercions replaced by explicit priced refusals),3 rationale rows. This is not a contiguous or complete venue audit;57 original FINDING notes and the original73 CHOSEN-text notes remain unaudited. Confirmed defect density is a7/140=5% floor, above the reported fleet3–4% pattern;7/10 in this selected set is not a venue estimate.

Final delivery receipts are keyed by the exact handed-off SHA under sdk-rs/target/apexomni-exchange-gates/<handoff-SHA>/. Required commands are cargo fmt --check, cargo build --workspace --locked --jobs 2, and cargo clippy --workspace --locked --all-targets --jobs 2. Cargo invocations are sequential, using RUSTFLAGS=-Dwarnings. Appropriate scoped exchange/pure/utils tests and mutation controls run separately. NOT RUN: completed wasm clippy/build/runtime lanes, full workspace/venue runtime suites, separate FFI feature jobs, extractor-check, wiring/layers/routing/report checks, CI execution, and live venue/network/signing integration. No push, PR or self-merge.

Adversarial static review of96234a7929df2146a248659c20646073036b3131 returned DO NOT MERGE with two accepted findings. WS conversion paths retained generic Error names; they now reuse the existing TypeError-producing text/float adapters, with seven actual-source malformed field cases and direct native errors/cache assertions. Missing contractConfig previously failed retained-data conversion before the Store update; now it removes a stale native config entry, updates Store, then reaches the matching TypeError. Source retains an own undefined property while the owned native snapshot omits it, an explicit field-presence cost under the snapshot declaration. A stale-value fixture demonstrates the removal; raw source state captures the present undefined property. Five additional envelope/config cases constrain this path. The shared TypeScript Store update body assigns the supplied object and notifies, retaining the observed property.

At b13f3f1e738b13128d86d2389ee367fe0542a004, the repair's scoped compile exited101 because the new text/float uses lacked imports; this is not a mutation result. The imports were corrected at50b3c9ae468e786194ba34c17a44a13e4cab917b, where all67 exchange tests passed (exit0). The four source-substituted producers also execute successfully as Node programs; native discrimination is claimed only after the final controller receipts record its required named failures. The immutable repair re-review and exact-SHA delivery gates follow separately; no earlier receipt is promoted to coverage of a later commit.

Canonical writer after four-note reconciliation exited0: Updated0 manifests,0 added,0 orphaned, Done20.60s. After the review corrections it again exited0: Updated0 manifests,0 added,0 orphaned, Done17.45s. These are annotation-writer results, not extractor-check exits; no manifest was regenerated.

At a3c8721e8f4c3f89df6795fc0067c74c25c3b755, final format exited0 and exchange/pure/utils exited0 (67+9+10 tests). The full controller executed all17 native and4 actual-TypeScript substitutions: each runtime test exited101 and each restore0, with no survivor. Twenty expected labels matched. One controller expectation was wrong: history-before-time-filter was caught by the named native operations assertion (an extra warning), while both returned arrays were empty because the cached fill was also outside the requested window. The controller honestly exited1 for that mismatch and blocked its queued build/clippy steps. Its original receipt and invalid-control entry remain unchanged. This is a verification-harness finding at fetch_historical_fills before the time-bound guard, not an uncaught production mutation.

The expected label is corrected to the actual native operations assertion. A --only selector supports a proportionate targeted rerun; default execution still requires all42 mutation/restore attempts, and an explicit selection independently requires2 attempts. No assertion is removed or relaxed. The corrected control and all three required delivery gates are measured at the subsequent handoff SHA. Other20 controls remain attributed to a3c8721e8f and are not claimed as rerun at that later SHA; source/test tree identity can be inspected separately. The earlier controller exit1 is not presented as a passing gate.

## ApexOmni candle, order, prefix and call-time findings — 2026-10-16

This continuation addresses four original FINDING notes, leaving their statuses divergent. It does not audit the original73 CHOSEN-text cohort. The previous local delivery f7f516ddcccaf123aa595149830da645f35ef2d5 separately measured format0, workspace build0, host workspace clippy0,86 scoped tests0, and the corrected historical-time control101/restore0/controller0. Those exits cover that SHA only; its earlier full controller1 at a3c8721e8f remains recorded above.

The new fetchOHLCV body preserves optional map outcomes, strict null-row t reads and stable NaNs-last sorting. Twenty-four authored actual-source response cases (stopped index23) include18 successes and6 errors. The mapOrder body now follows source table-key evaluation and side-call failure, completes both bracket price parses before bracket clocks, and delays shared output admission until source coercions/interpolations/clocks finish. Twenty-eight cases (stopped index27) contain9 priced whole-vector refusals,11 matching errors,8 positives including4 optional omissions. Required enum refusals and optional omissions have separate callable and prototype-object observations; all direct native values/errors/operations/clocks run before8 source wire witnesses (stopped index7). The inherited-object {} loss is priced separately from callable omission under [closed typed outputs](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary). Numeric id/reduceOnly refusals remain local model choices, not inherited-enum cases. Retained Order.info follows [owned snapshots](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

Actual dropPrefix returns D800, DC00, D800 followed by A, and A followed by DC00 after removing the initial prefix. Its required native argument/result are &str; four whole-String admission refusals and8 positive native returned-unit cases are inspected before wire. The12 cases stop at index11. The actual-source D800-to-D801 substitution preserves one returned unit and8 encoded JSON characters while changing bytes, and fails the returned-unit assertion before any wire assertion. This row cites [the surrogate observation ruling](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict) and [the String boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), rather than duplicating the declarations.

Two actual updateOrder cases (stopped index1) show eager source base/bracket selection versus native first-poll Store selection. Clearing3 orders after the source call still yields source placed-1, while native first poll sees no base. Filling an initially empty Store makes the source call reject, while native first poll starts cancellation of the inserted base. Both source eager-prefix request and cancellation counts are0; the old finding's suggestion of eager cancellation effects was not sustained. The retained selection timing is explicitly CHOSEN under [the taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy): an eager wrapper is possible, but would freeze snapshots before a queued or dropped future begins its host command. Normal settled update behavior remains covered by existing trading cases.

At ba7558e105918440dc4fdffd4f611b437690cd4e, scoped tests exited101 during compilation because table_lookup is private. This is not mutation evidence. The public re-export import was corrected at e09b94ff4499cf910b572dd13f2041cb815d85df, where all90 scoped tests passed:70 exchange,9 pure,11 utils. Immutable static sub-agent review at521cfa418f found no additional defect; it required the separate inherited-object wire witness, subsequently added before annotation closure. Runtime outcomes are never inferred from static review.

The15 authored substitutions are defined by mutate_prefix_boundary.py (2), mutate_candle_shapes.py (5), mutate_update_calltime.py (2), and mutate_order_continuation.py (6), stopped at aggregate control index14. Each requires a named runtime assertion failure at101 and a restored run0. Native source bytes are restored in finally blocks; TypeScript controls edit only the selected actual declaration in memory. Controllers reject compiler failures as discrimination and record survivors/invalid controls instead of dropping them. Exact control exits belong to sdk-rs/target/apexomni-return-gates/<control-SHA>/controls-summary.json and each component controls.json. Control receipts are not promoted to a subsequent metadata-only handoff SHA; source/test tree identity is separate evidence.

The manifest reconciliation edits only the four existing notes, preserving symbol identity, target, layer and status. The venue census remains28 ported /140 divergent /0 pending across168 rows in16 manifests (stopped file index15). Status rows moved0. The reviewed original-finding split is14 rows:9 with repaired behavior defects and5 rationale rows;53 original FINDING notes remain. This selected set is not a contiguous audit or a venue estimate. Confirmed behavior density is a9/140 floor (about6.4%), above the reported fleet3–4% pattern.

Delivery gates are measured independently at the exact final SHA under sdk-rs/target/apexomni-return-gates/<handoff-SHA>/gates.json: cargo fmt --check; cargo build --workspace --locked --jobs 2; cargo clippy --workspace --locked --all-targets --jobs 2. Cargo commands remain sequential with RUSTFLAGS=-Dwarnings. No canonical wasm command is scheduled. NOT RUN by this lane: wasm clippy/build/runtime, central wiring/layers/routing, extractor-check, full workspace/venue runtime suites, separate FFI feature jobs, CI execution and live venue/network/signing integration. No push, PR or self-merge.

At ddcfae87c7b85b0a112345bc34729523198cb984 all four new controllers exited0. Independent floors required30 attempts:15 named runtime failures101 and15 restores0, with no survivors or invalid controls. Prefix/candle/update/order attempt floors were4/10/4/12. The source timestamp, source amount, source deferred-prefix and source unit substitutions all failed runtime assertions; a compiler error could not satisfy these controls. Afterward integration aab488a8e2ac3a06ad1f53777831d21659aac289 was merged at b7aba427be5aa635dac71d04b059d9a502c65869. Venue source/test trees did not change in that merge, while shared dependency code did; the control exits remain attributed only to ddcfae87c7. Required delivery gates and the scoped suite run separately after this merge.

The canonical annotation writer after these four note edits exited0 with Updated0 manifests,0 added,0 orphaned, Done14.10s. This is a writer result, not extractor-check. Final-SHA scope/control and required gate exits are reported only from their new receipts; post-integration dependency changes are not assumed equivalent to the earlier workspace.

Static final review of c7e60b987bc02ebc7821fd1961ef9f466598a1ee returned DO NOT MERGE for a note-preservation regression. The updateOrder rewrite had dropped its existing present-undefined versus absent-price cost; the fetchOHLCV rewrite understated the existing typed-options/alias cost. Both annotations now retain those costs, their local model rationale and canonical snapshot citation alongside the newly bounded proofs. The two price121 scheduling cases do not repair the update presence distinction, and the24 response-shape cases do not discharge the prior options domain. No new production or fixture defect was reported by this static pass.

After retaining the reviewed costs, the annotation writer again exited0: Updated0 manifests,0 added,0 orphaned, Done26.46s. This is not an extractor-check exit.

### Remaining original findings: socket order plus declared record carriers

The preceding accepted split is9 behavior /5 rationale,14 original findings examined with53 left. This continuation proposes one further behavior row (private handleOrderTopic) and52 rationale rows (two onMessage rows and50 declared record carriers). Its complete original-cohort split is therefore10 behavior /57 rationale over67 rows; this is a proposed delivery classification until the final controls and gates below finish. It does not audit the original73 CHOSEN-text cohort. Status rows moved0:28 ported /140 divergent /0 pending over168 rows in16 recursive manifests, stopped file index15. No status, symbol identity, layer or Rust target is promoted or regenerated.

The private order fix retains source some short-circuit and null/property error order. String-key cache admission no longer conflates numeric17 with String17; native Order admission still refuses the numeric-id mapped order, explicitly priced separately from that repair. Date and virtual clock owners remain distinct, including a symmetric clock-swap control whose independent freshness cases distinguish the values rather than just their call counts. Fourteen source cases stop at13:9 matching errors,4 paired successes and1 whole-order typed refusal after the parent clock. Earlier Store/cache, emitter NaN, alias, cancellation, cumulative-fill and timer annotations remain intact.

Frame parsing is a feasible local choice, not automatically forced by ASCII escaped transport. Private raw input and outgoing pong &str can both retain the exact escaped frames. Shared whole-Value parsing loses the whole pong for escaped unpaired args, and even an ASCII pong when an ignored extra contains the unit. The public row separately prices the shared Ticker.id/String-map-key carrier refusal and the chosen frame parser. Its malformed-id fixture cannot install an equivalent invalid native Store; its independent ignored-extra case uses identical admitted ASCII ticker ids and proves a dropped update from103 to117.25. Sixteen authored cases stop at15:8 whole-frame refusals and8 native input/peer positives; all native assertions precede8 private source-wire witnesses. Earlier callback ownership, receipt, diagnostic, stats and dispatch annotations are retained. Canonical classifications are cited by anchor in each row; no global declaration is copied here.

Two unused-extra D800-to-D801 source substitutions survive maintained-handler output assertions at private/public onMessage/jsonParse(packet.ignored). They are retained scope findings, not passing negative gates or evidence that every parser-materialized unit is a required string carrier. The first encoded source control was inert because its replacement matched a materialized unit rather than an ASCII JSON escape. That invalid harness control was diagnosed at78850df5ef, corrected at3ea24b4f44, and inspected source-only; it is not counted as a runtime failure. Historical source-only receipts remain under sdk-rs/target/apexomni-socket-draft-observations. Final source-scope observations are measured separately at the handoff SHA.

The record supplement inventories50 independently authored names and declared String paths through the actual TypeScript compiler schemas. Seven cases per row stop at target49/case349, with150 whole-record/union refusals and200 decoded native leaf positives before50 source-wire witnesses. Source values are observed immediately after JSON.parse through the actual declared leaf, before serialization. Optional SymbolInfoObject.baseCoin is present and maps to Some(String); ISO8601 tests its String variant. The complete native carrier is refused for the three unpaired cases, including its otherwise usable fields. Existing field, union, optional-presence, numeric and consumer annotations are retained. These selected-leaf supplements do not rerun the historical all-field/numeric/schema proofs. One KeyPair production decoder mutation is distinct from50 row-named supplementary observer-harness substitutions; the latter corrupt the observation and are not maintained-TypeScript body mutations.

Static review of3fcf4ddec499cdc25f3b1a01bfe573f214c82eca found the stale whole-frame refusal floor6; independently authored cases require8, corrected at852d272bed. It accepted the bounded carrier classifications and found no other concrete blocker. At ce6ae5f270d5a3ea0c542cdc891b538f53c60a84 scoped compilation exited101 because the generic record helper omitted required Serialize/JsNumberOutput bounds; this is not mutation evidence. The bounds were fixed at83cdffe2f695d5170532f16171725196b8349c42, where all93 scoped tests passed:72 exchange,9 pure,1 record supplement and11 utils. Integration ec5ebe2b90dbad470436cf74040f9af89086f4eb was merged before that run; it already contains the prior accepted deliveries. These measured exits cover83cdffe2f6 only.

Final-SHA receipts under sdk-rs/target/apexomni-next-proof/<handoff-SHA>/ record the scoped suite, three controllers and required gates separately. Controller floors are10 socket-order attempts,8 socket-frame attempts and102 record attempts:60 named substitutions with60 restores, comprising7 production native mutations,3 actual-TypeScript body substitutions and50 observer-harness substitutions. These are required expectations, not completed exits until the receipt records them. No survivor or invalid control may be omitted. Latest delivery gates are only cargo fmt --check and cargo build --workspace --locked --jobs2, sequential with RUSTFLAGS=-Dwarnings. Host clippy and wasm are centrally owned, not run by this lane at the handoff SHA. Also NOT RUN: wiring/layers/routing, extractor-check, full workspace/venue runtime suites, separate FFI jobs, prior-unit mutation controllers, historical all-field/schema/numeric controls, CI execution or live network/signing integration. No push, PR or self-merge.

After the53 in-place note edits, the canonical annotation writer exited0 with Updated0 manifests,0 added,0 orphaned symbols and Done25.33s. This is a writer result, not extractor-check. The stale signer consumer inventory was corrected to point to separately annotated rows rather than still calling them pending.

Final metadata review at23b4d0c712 accepted the53-row carrier/cost separation and historical annotation preservation, with one classification blocker: the repaired order row has chosen residual costs and no separate forced cause, so it now opens CHOSEN. The repair prose says Repaired Rust rather than misusing the canonical CORRECTED classification. Review found no additional concrete blocker; its review-only helper initially lacked tomllib (exit1) and the stdlib comparison replacement exited0, neither a runtime gate. Incoming origin38e2cb0e7b1e3f45979b34a3d0f552bf0390c11d was merged at6bff500008 before final gates; this merge did not change ApexOmni files.

The early fleet3–4% baseline quoted in the chronological entries above was superseded by the coordinator's later measured fleet results. No current fleet-wide defect estimate is inferred here. This lane reports its row split directly.

At a09c018f263e0b296920a65a453678625f798e2d the complete scoped supplement and control run finished. Measured exits: cargo fmt --check0; scoped tests0 (72 exchange +9 pure +1 record-unit +11 utils =93); socket-order controller0 with5 runtime failures101/5 restores0; socket-frame controller0 with4 runtime failures101/4 restores0; record controller0 with51 runtime failures101/51 restores0; workspace build0. All120 named attempts matched their independently authored expectations, with no negative-control survivors or invalid controls. The60 substitutions comprise7 native production mutations,3 actual-TypeScript body substitutions and50 observer-harness substitutions. Two separate ignored-extra source substitutions still survive maintained-output observations, retained as scope findings at the named onMessage/jsonParse(packet.ignored) call sites. They are excluded from the passing negative-gate count. Receipts: sdk-rs/target/apexomni-next-proof/a09c018f263e0b296920a65a453678625f798e2d/gates.json and its source-scope-findings.json, plus the three controller directories keyed by that same SHA.

The original67-row finding cohort is now classified:10 behavior /57 rationale; this continuation closes53 original finding notes (1 behavior repair,52 rationale), leaving0 original FINDING notes. The73 original CHOSEN-text rows remain outside this audit. Manifest status movement is0; the independently expected census remains28 ported /140 divergent /0 pending,168 rows in16 files, stopped file index15. Static sub-agent verification at a09c018f26 returned MERGE after the taxonomy correction.

After those measurements, origin0286f63437e4320e8c12886d5fef8fc6dfa2eac3 was merged at996e91123279d7b20b9bc738fb2f418a2a46fd06. The ApexOmni source/test/script and manifest trees are unchanged by that merge; shared/integration changes are not assumed behavior-equivalent. The93-test and120-control-attempt exits above cover a09c018f26 only and are NOT rerun or claimed as measured at the post-merge delivery SHA. Per the latest coordinator instruction, post-merge delivery measures ONLY cargo fmt --check and cargo build --workspace --locked --jobs2 at the exact handoff SHA; its separate receipt records both exits. Host clippy, wasm, wiring, layers and routing are centrally owned and NOT RUN by this lane at that SHA. No other runtime, historical, extractor-check, FFI, live-network or CI claim is added. No push, PR or self-merge.

## Assembled Exchange/registry binding

Measured after master dd0e266704 integration:

-27 assembled tests pass:16 mandatory methods,2 listeners, caller ID, private
  login/subscription/Pong, one legacy resend, repeated refusal, HTTP400 metadata,
  write503 non-replay and two disposal continuations.
- `binding-signature.cjs` executes actual source placeOrder and signContract with
  the pinned real npm zkLink SDK. Its full URL-encoded body equals native: amount
  1.2345/price100.127 at distinct0.001/0.01 steps become1.235/100.13, with exact
  signature5212a2f9ea21406e5f25c046c509388bb7b25469400905119ba807a186ed93a2fc89f328a8883dcd3d1c8c6107158554c319bd784d5b7d9b26bebc7dfd610c01.
  The harness reuses trading.cjs's source/compiler setup and substitutes real
  SDK imports; it does not generate a native expected signature.
-18 independent wrong Exchange/listener bodies each compile and fail their
  named E2E; restored18 bound_ tests pass. Two precision operand swaps compile
  and fail wire size/price assertions independently; restoration passes.
- Native registry proof starts real HTTP/WS, signs/places, receives the placed
  order through private WS and cancels it. A subscriber records the WS update
  immediately so later REST snapshots cannot erase proof of delivery. HMAC uses
  the source's base64-spelled UTF8 key and authenticates actual transmitted form
  bytes. Native proof plus13 registry tests pass.
- Removing the actual Apex table entry compiles and fails native construction
  naming the missing Rust-constructed venue; restored native proof passes.
- Global parity passes with0 findings (7459rows/17119public items). All new
  binding helpers map to actual class/constructor assembly; imported SDK crypto
  retains its separate primitive witnesses. No regenerated manifests.

Controls are reproducible through the four scripts named in CLAUDE.md. Logs
for this execution are /tmp/apex-{binding-controls,precision-controls,registry-controls,capability-controls}.txt.
The source bodies' existing divergences remain; late-disposal refusal and owned
book snapshots are explicitly priced. Six inherited methods genuinely reject in
TypeScript; those are exact source behavior, not newly implemented endpoints.

Final full-venue/oracle, host/wasm lint and post-master integration results are
recorded in the PR validation text. No live credentials or external venue orders
were used; P4 live verification remains separately parked on Sam.

The shared-factory regression adds a real custom adapter to the test transport
provider. Both initial signing and legacy resend preserve the callable adapter
and serializer; the fixed20-second factory timeout remains independent of the
venue receive window. Two compiled substitutions each lose one of these
capabilities and fail the named adapter witness; restoration passes.

Post-integration gates: full source oracle161 tests (72 exchange,7 protocol,9 pure,
28 record,6 signer,7 signing,21 transaction,11 utility);27 assembled binding E2Es;
native registry1+13; host all-targets Clippy for venue and registry; wasm library
Clippy for venue; fmt; registry equivalence; layers88; global parity0. The source
oracle retains the actual four-position late-disposal observation while asserting
the chosen P4 native guard leaves the initial positions unchanged; other state
axes still compare directly. Read-only extractor reports no ApexOmni drift; only
Polymarket and registry.native metadata were stale at this integration base.
The dispatch inventory updates only reviewed ApexOmni calls; public_data_only's
base can_sign delegation is guarded against a future source canSign override.

Final master refresh c537b291d4 (merge f10b86b3ec): source oracle161,
assembled E2Es27, native HTTP/WS proof1 plus registry13, registry equivalence1,
layers88, global parity0 (7459rows/17126public items), host/wasm strict Clippy and
workspace fmt all pass. The live source override inventory passes1 and the
cancellation inventory passes6 with its explicit exporter ignored. The full
shared dispatch guard stops at AsterDEX base_dispose's unreviewed dispose
call, so no full-workspace dispatch pass is claimed. Apex-only discovery was
reviewed against its36 committed call sites and its sole guarded delegation;
all other lanes' inventory rows remain unchanged. The compiled negative-control
receipts precede this final master refresh; the positive suites above were rerun
against the resulting shared dependencies.


## Native hosted signing

Source audit: generic Base signHttpRequest and Apex private auth support the
shared browser relay; WS uses apexomni-base64 (base64-spelled secret as HMAC key).
Source validate/place still initialize the L2 signer from omniKeySeed. Native
HTTP/WS signing now shares the account capability; the API secret stays remote
while the source-required Omni seed remains local for contract signing.

The original compiled hosted native E2E fails the not-wired refusal, then passes.
Five native loopback tests cover local and remote lifecycles, actual transmitted
HTTP/WS HMAC, fractional placement/caller ID, account-name presence/absence,
delayed auth-before-subscribe, HTTP refusal without unsigned requests and WS
refusal/throw without private frames or an auth-invalid diagnostic.

Four independent capability/ownership substitutions each compile and fail the
named native assertion, then restore to pass (hosted-signing-controls.py). A
stronger disposal witness retains the wrapper and disposes synchronously inside
the signer's first poll. It compiled and failed before the post-poll disposal
guard; after repair both ordinary and reentrant disposal close the signing
future without private frames. Source detached Promises continue; this native
P4 cancellation choice is explicit in the onOpen row.

On master263d9a8c81 plus the hosted implementation, source oracle161, assembled27,
native5 + registry14, layers88/registry equivalence, cancellation6 (one explicit
exporter ignored), host/wasm strict Clippy, fmt, live TS override1 and global
parity0 (7459 rows/17146 public items) pass. After the final disposal guard,
native5, assembled27, host/wasm Clippy, fmt and parity0 pass again. The full
shared dispatch guard remains blocked by Backpack's stale disposal evidence,
owned by #213; none of that lane's rows are edited. Own36 call sites retain
exact discovery after reviewing just the two constructor fingerprints.

The shared form-string prerequisite is landed #232. Its actual-source remote
adapter/Axios witness covers four string representations; original bridge fails
and repaired bridge passes. Opaque encoded bytes still refuse before signing.


## Twenty-six independently reviewed map-record owners (2026-10-17)

Fresh native/default compiler proposals at master `68af3fa734` identify these
26 previously unclassified concrete record decoders: public12, private8 and
config6. The report is metadata for manual review, not executable contract
input. Every declaration and full decoder body was reviewed against the actual
public/private/index TypeScript interfaces. All 303 declared native fields,
including inherited SymbolInfo fields, keep their types, declaration order,
renames and default/presence/custom-passthrough attributes. Serialize, all output
registrations and surrounding production bodies are unchanged. The ordinary
function-local derived Fields schemas become private serde(remote) companions;
the existing owner numeric capture and object guards remain.

The original boundaries already rejected all 26 independently authored complete
valid positional arrays. The original native baseline passes **26 tests / zero
failed / zero ignored** on **494 cases through seven decoder routes**. The remote
conversion passes the same suite. There are 248 positive direct-native-field
observations and 246 negative field/shape cases; each owner has its own test.
Native observations precede owner Serialize and cover every field, Option
presence, nested nonempty collections and retained JSON leaves, 20 individually
false required booleans and their omission refusals, unknown ignored fields,
optional absence/present-null behavior and all 34 direct f64 fields with -0,
binary64 rounding, subnormal and positive/negative overflow inputs. Opaque values
stay within the existing retained JSON domain.

The first draft test compile exhausted the default json! macro recursion depth;
the observation output now constructs an ordinary Map from field/value pairs.
That compile refusal is excluded from runtime proof. A formatting invocation
with SDK-prefixed paths from the sdk-rs directory found no files and is also
excluded; the corrected explicit pinned rustfmt and workspace check pass.

Independent draft review verifies all 26 remote schemas equal their former
Fields schemas after whitespace normalization, all surrounding production code
is unchanged and all 494 fixture expectations/complete arrays match the reviewed
source types, presence and bits. Actual production probes and isolated semantic
controls are recorded separately once executed; no pending run counts as proof.


The first production attempt refused all contracts before native observations:
the expected native fields were nested below `fields` instead of the required
top-level keys. No result from that refusal counts as successful production
proof. Actual direct-native getters and independently authored expected fields
now use the maintained top-level observation shape, and the lane runner
preflights all actual owner contracts through maintained `contractFor` before
compiling probes. The corrected fresh production run passes **26 actual runtime
results / zero classifications / zero failed owners**, with **394 independently
authored actual production cases** and complete compiler checks on all 26 private
companions. Independent review of this correction is clean.

Native all-target Clippy passes with warnings denied after replacing seven
redundant test-only mapping closures with function pointers. The native harness
now has fixed per-owner case/positive counts and an actual route counter:
26 owners, 494 cases and 3458 decoder observations. Workspace format passes.
Global structural verification passes **7459 rows / 17214 public Rust items**.
Scoped invention verification retains the complete workspace resolution and
passes **287 public Apex items**, preserving all 82 existing owned exemptions
and adding none. Its temporary exemption input selects only the existing owned
rows; the original full-exemption invocation reported unrelated foreign names
outside the selected invention scope and is excluded. These parser checks reuse
the maintained binary after unchanged Rust/Cargo/lock/config inputs were verified.


`scripts/accepted-map-record-controls.py` completed all **26 isolated compiled
controls**. Each removes only its exact owner's object wrapper, requires Cargo
exit101 and its named failure containing the exact complete valid literal array,
then restores all three original source files byte-for-byte and passes all26
native tests. The final exact-source check and all26 native tests pass again.
No sibling mutation is installed during any owner's control. Initial preflights
refused rustfmt-wrapped calls and the CurrencyObject suffix inside
PerpetualCurrencyObject; both happened before mutation and are excluded from
negative evidence. The corrected anchors require the exact owner boundary and
allow only formatting whitespace/trailing-comma spelling.

Master `87c99c175c` integrates as `8edda0a135` without conflicts, retaining all
master contract/policy shards, including merged #365. The actual Apex source,
fixtures, production runner and independently authored observations
are byte-identical to the checkpoint that completed the controls and production
run. The 26 genuinely new Apex rows remain the only additions to their shard;
no flat file, exemption or previously removed row is restored.


Independent readiness review found a P2 in the maintenance controller: global
byte replacement discarded the regex's owner boundary. The completed v3 run
used unique old byte sequences for every owner, so each executed mutation was
isolated. The controller now replaces only its exact matched span. A focused
same-format CurrencyObject/PerpetualCurrencyObject fixture fails with the former
global replacement and passes after the span fix; all three Python tests pass,
including wrapped-call and missing/ambiguous-owner refusal cases. For all 26
actual production owners, the fixed span selector's entire mutant bytes equal
the mutant bytes executed in v3. Sources are unchanged; no hypothetical collision
is counted as a native guard failure. Importing the controller for its pure test
now performs no source write or Cargo invocation.

Final integration readiness: full scoped ApexOmni tests pass **215 / zero failed /
zero ignored**, across 13 target receipts including docs and all maintained live
TypeScript oracles. Separate host and wasm32 all-target Clippy pass with warnings
denied (12.30s and 37.04s). Fresh post-integration production passes **26 actual
runtime results / zero classifications / zero failed owners** on the reviewed
394-case production corpus; private companion checks run again. Frozen install,
workspace format, global structural **7459 rows / 17214 public items** and scoped
invention **287 Apex public items** with the same 82 existing exemptions all pass.
CodeRabbit's manual review returned a rate limit; it did not execute a completed
review. The other 103 originally censused owners, aliases, receiver policies,
local functions, construction, features and wasm behavior remain independent
obligations. No whole-venue completion is claimed.


## Twenty-two transaction/order/signer map owners (2026-10-17)

Starting from master `723da2dd14`, then fast-forwarding to `31917cde56` (including
#374), all19 transaction structs plus CreateOrderOptionsObject,
OrderResponseObject and TxEthSignature are individually reviewed against their
actual TS declarations, enums/unions and complete native decoder bodies.
Only their ordinary local Fields schemas become private Serde remote schemas.
All167 declared fields keep their original types/order/rename/default/custom
field decoder attributes. Surrounding enum, flag, helper, Serialize and output
code is unchanged; the two undefined-only records remain independent obligations.
Every prior master Apex contract remains exactly preserved as a JSON value;
only22 genuinely new owner rows are appended.

The original21-owner baseline passed, then the independently witnessed one-field
Onchain record expanded this coherent cohort to22. The final original-boundary
baseline and converted-boundary suite each pass **22 / zero failed / zero ignored**
on **835 cases through seven decoder routes** (5845 actual decoder observations).
Fixed per-owner cases/positive counts and actual route counters are enforced.
Native expectations are525 positive all-field/presence/variant/bit observations
and310 invalid field/shape cases. They include every direct enum tag, actual
ISO8601 and auth-union branches, nonempty nested signature/transaction records,
nullable null versus absence, optional booleans false, all numeric0|1 flags with
negative-zero bits and wrong-value/type refusals, two independently varied fee
rate tuple elements and wrong tuple lengths, and raw binary64 rounding,
subnormal and positive/negative overflow tokens. Every owner has its literal
complete valid Serde-order array; the existing object boundaries reject them.

Fresh native/default actual production verification passes **22 runtime results /
zero classifications / zero failed owners**, executing560 authored production
cases. Each private companion's entire actual nonblanket compiler implementation
set is inspected, including the exact returned owner identity/source span.
ChangePubKeyOnchain uses the maintained ordinary observation path, which directly
matches its only actual native field before the wire audit. This avoids an
invented field or an exemption; its native unit witness still executes all seven
routes. Native all-target Clippy passes with warnings denied (10.69s).
Independent draft review checked all22 schemas, all835 native expectations,
all560 production expectations and exact preservation of master26 contract rows:
no findings.

All22 isolated object-guard controls completed: each removed only that owner's
outer object boundary, compiled, and failed its exact named complete-valid-array
admission assertion at exit101. Each control restored all three source modules
byte-for-byte and passed all22 native witnesses; the final restoration repeated
the exact byte check and all22 tests. No compiler/tooling failure is evidence.
The shared pure exact-span selector's three isolation regressions also pass.

After merging master `41e20791f8` without conflicts, frozen install and format
checks pass. The full scoped ApexOmni suite passes **237 / zero failed / zero
ignored**, with14 target receipts including the maintained live TS oracles.
Separate host and wasm all-target Clippy runs pass with warnings denied
(11.55s /25.82s). Final fresh native/default production verification passes
**22 actual runtime owners / zero classifications / zero failures**, including
all560 authored production cases and complete actual companion inspection.

The maintained invention gate passes globally (7459 manifest rows /17214 public
Rust items) and with Apex-only public obligations (287 items), retaining exactly
the same82 existing Apex exemptions and adding zero. The cached checker is
reused only after checking its actual Rust/Cargo/lock/config inputs are identical
to its build; the sole source-directory difference is CLAUDE documentation.
Every existing master Apex contract is preserved exactly as a JSON value.

CodeRabbit returned a review limit and did not complete a bot review. This
classifies only the selected22 native/default decoder owners. Other owners,
undefined-only records, functions, aliases, receiver policies, local helpers,
construction, feature and wasm accepted behavior remain independent obligations.
No whole-venue completion is claimed.


## Twenty-three scalar/validation helper bodies (2026-10-17)

The full actual signer-utils source and selected exchange/utility declarations
were read along with every native body. Production bodies remain byte-identical;
only cfg(test) witness wiring is appended. The cohort individually separates16
raw-value admission/projection/coercion decoders from7 typed/non-decoder roles.
Native witnesses pass23 /zero failed /zero ignored on634 independently authored
cases, including prepared Undefined/BigInt/NaN/nonfinite/negative zero, exact
error strings, range/type boundaries, private display projection, native RandomId
variants, entropy/clock counts, fee Math callback inputs/errors and borrowed
prefix-slice pointer identity. Their fixed census and actual comparison counts
are checked. The16 decoder contracts contain549 independently authored cases;
all prior master contract values are preserved.

The initial test-adapter return-type build error and initial unused-adapter
production compile failure are tooling failures and are excluded from proof.
No expected outcome was changed in response to native output.

All23 isolated semantic controls compiled and failed their exact named native
assertion at exit101. The16 decoder controls also failed the selected production
owner's actual output comparison. Each restoration passed all23 native tests;
restored decoder runs additionally passed all16 production outputs and7 explicit
classifications. The final restoration verified all three source modules exactly
and repeated the native/production checks. Classification receipts for the seven
non-decoders are not claimed as runtime decoder coverage.

After merging master `a5284334d4` without conflicts, the full scoped suite passes
**260 / zero failed / zero ignored**, across14 target receipts including live TS
oracles. Host and wasm all-target Clippy pass with warnings denied (19.04s /
43.97s). Final fresh native/default production verification passes **16 runtime
results /7 classifications /zero failed owners**, covering549 authored decoder
cases. Frozen install, format and the maintained structural gates pass:7459
manifest rows /17214 public items globally;287 Apex public obligations with the
same82 existing exemptions and zero new ones. Cached structural-checker reuse
was checked against its actual Rust/Cargo/lock/config inputs; only documentation
differed. All48 master Apex contract values are preserved, plus23 new rows.

Independent draft review found no issues. CodeRabbit returned a review limit;
that status is not evidence of a completed bot review. These results classify
only the selected23 native/default bodies; remaining owners, aliases, receiver
policies, locals, construction, feature and wasm accepted behavior stay open.


## Retained Apex REST/config cutover

Baseline 10198169fe compiles the real binding regression against the previous
response path. `rest_info_keeps_opaque_utf16_and_native_numbers` fails at exit101:
Order.info opaque text has units [65533] instead of original [55296]. This is
silent replacement, not a compiler failure. Actual maintained TS exchange methods
preserve opaque order/balance/market/config text and keys, NaN/infinity/-0 and own
undefined (`scripts/exchange.cjs --retained-info`).

The canonical terminal response/host/mapper cutover removes the UTF8 prepared
projection. Three new real-binding tests passed on the restored implementation;
market/config observations inspect the committed native config first, then model
info and Store, and finally run actual signed placement with those opaque siblings.
Strict named string/bool row refusals remain visible and keep valid siblings.
The obsolete nested `fields::retained` local-function key is retired: that nested
function no longer exists; canonical typed diagnostic projection and prepared
request capture have distinct direct call sites.
