# OKX venue port

Behavioral authority: [TypeScript OKX guidance](../../packages/safe-cex/src/exchanges/okx/CLAUDE.md), [shared exchange guidance](../../packages/safe-cex/src/exchanges/CLAUDE.md), and [shared socket binding](../safe-cex-exchange/WS_CONTRACT.md).

The port is in progress. Protocol tables, the two literal union types, and pure ledger/history/closed-position mappers are implemented; remaining exchange and socket manifest rows stay pending until their runtime behavior and independent oracle pass. `types.rs` preserves all global/EEA live/demo hosts, 28 endpoint entries, 13 intervals, order mappings, and telemetry endpoint lists. Lookup records use immutable pair arrays; the union types accept JSON strings only. Checked-in public declarations remain visible to the source index; the private RegionWire and InstrumentWire owners preserve serialization and typed From mappings and independently require string admission before their private Serde remote schemas. A deserialize_enum metadata inventory inspects those actual remote decoding schemas and compares each complete accepted literal set against the maintained TypeScript union. Rejected-shape probes cover tagged objects, arrays, null, booleans, numbers, case changes, whitespace, U+FEFF and unknown strings. The audit found and corrected externally tagged object acceptance in both unions; restoring it fails the protocol-table test.

`cargo test -p safe-cex-venue-okx --all-targets -- --include-ignored` runs the maintained TypeScript table oracle. It transpiles the original tables and original domain enum declarations, extracts the original literal unions through the TypeScript AST, and compares every value. The Rust CI step and its structural negative controls are mandatory; default Cargo tests omit the Node-dependent oracle.

Load-bearing implementation requirements for the remaining port:

- OKX fetchOrders, fetchPositions and fetchBalance return data without committing it. Callers own store commits after lifecycle and stale-snapshot guards.
- EU signed ACCOUNT_INSTRUMENTS narrows shared public markets through an account overlay before external public initialization. The overlay returns fresh market objects so one account cannot mutate another account's shared markets.
- Filled and canceled order stream events are final and use Store::mark_orders_terminal, protecting them from earlier REST snapshots arriving later.
- okxeu shares protocol classes with global OKX but keeps its own transport WsGroup and EEA authentication hosts. Demo variants keep their respective pap hosts.
- Preserve the local, non-Send shared surface and public WebSocket dispatch/performance invariants. No production connections, orders, deployments or rollout changes belong in this port.

The pure ledger/history/closed-position mappers now consume typed SDK output models and object-shaped wire records. The live oracle compares 1,220 cases, with exact binary64 numeric values (JSON numeric spelling is canonicalized only in this state-mapper comparison). It exercises fee/PnL/residual decomposition, magnitude-relative epsilon, contract conversion callbacks, unknown markets, zero/negative/malformed timestamps, price fallbacks, terminal history states, and malformed scalar coercions. This is a value/field-presence oracle, not a signed-byte oracle.

Income category lookup and bill emission now retain shared TableHit until the
closed output boundary in [MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary).
The source's inherited values are witnessed separately from native refusal;
empty and fee-only bills retain success, with source coercion/effect ordering.
Client lookup carries inherited mapper input explicitly and retains the mapped
status for nonterminal states. Its store-hit copy remains the declared ownership
choice in [MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

The addressable mandatory gate is `yarn venue-oracle:okx`, backed by locked Cargo tests including ignored live oracles. CI and its structural audit invoke that root script; it uses the existing isolated compiler.

The shared coercion dependency is integrated: parseFloat, radix-10 parseInt and fallible String(any) are distinct shared primitives. All mapper APIs return Result carrying the source TypeError. The restored ledger/history/closed claims have 165 throwing-object probes alongside the previous 1,012 cases. The journal adapter has 310 cases (230 previous plus 80 error paths); its owned JSON projection follows the core historical-position ABI and explicitly does not claim a shared readonly PositionRecord type. Its prototype close-reason bug is preserved at the serialized boundary.

The global market-policy slice adds 401 source cases, including exact suffix semantics, index fallback and preserving borrowed market identity and filter order. EU overrides remain pending. The fallible core MessageHandler/dispatch boundary is integrated: callback coercion errors propagate and stop remaining callbacks through the existing core path.


The REST-operation slice consumes the core response-bearing HttpClient directly;
no secondary derivation helper or lossy adapter wrapper is kept. Lookup and
normal/algo placement have a 118-case original-source oracle, real response-body
witnesses, isolated p-iteration/lodash execution and permanent two-sided controls.
Normal placement batches at 20; algo placement stays one request per payload.
The API factory, local signing and retry composition remain pending. These
adapted operations take explicit receiver dependencies; mapOrders stays pending
and its recorder is not evidence for its behavior. Native lookup returns an owned
Order copy on store hit, and the chosen typed status/string-ID contracts reject
or correct malformed dynamic values as declared on their divergent rows.
The fallible core dispatch is now integrated; socket callback rows are no longer
held on that seam. They must propagate errors through the existing core path.

Public dispatch and sequence recovery use the core handler registry. The 144-case original-source corpus (including all twelve inherited-member dispatch outcomes) now includes the repaired HOW overflow case: seqId:1e400 is interpreted as Infinity and accepted before JSON wire projection. The old finding expectation demonstrably failed after the shared fix. Inherited function-handler dispatch now uses shared TableHit/ProtoMember: own empty lists shadow inherited names, eight positive-arity functions record core statistics then return the source TypeError, and the three zero-arity functions plus __proto__ return at the length guard. The core global prototype-write refusal remains a CHOSEN FINDING; this does not claim parity of inherited referent mutation. The source behavior and hardening mutation remain pinned. Connection and market-list subscription remain pending; the five ticker handlers are adapted receiver operations as described below.

REST model consumers read RestResponseBody::json_value, preserving native HOW
values rather than requiring a byte round trip. This is a model view, not an
execution of the factory's custom response parser; that stage must use the
current-data transform explicitly when wired.

TickerMarkInput captures whole-entry JsNumberInput before the private map-only wire decoder and registers every output field with the shared expression macro. Its previous owned/borrowed integer -0 finding is repaired; raw, owned, borrowed, nested and tagged paths retain bits, including overflow and negative underflow. Remaining input choices are the canonical UTF-8 representation boundary and the shared bounded-depth profile, each with a source witness; no Value-first repair is used.

The actual OKX createAPI source installs bigInt HOW and inherits Axios default transformResponse. A JSON-quoted scalar is parsed twice through HOW: "123" becomes number 123 (bits 405ec00000000000), while direct Axios returns a string; quoted true/null promote too. RestResponse bytes cannot carry response phase/type provenance, so createAPI and its constructor caller stay pending with a core-owned FINDING, not a venue-local shim. The source oracle executes actual factory, HOW adapter and installed Axios defaults, replaces only I/O/host hooks, and records typeof and bits before JSON serialization. The maintained family has one createAPI factory row; the second held binding is its OKXExchange.constructor caller, not an invented second factory.

Historical pre-cutover evidence: Open-interest and funding-rate public handlers are implemented as adapted Store receivers. They apply only the first record, use shared fallible String plus parseFloat, and retain non-finite/signed-zero numbers in ModelPatch. Their rows remain divergent with the routed notification FINDING: patch Result does not make Subscriber fallible. STORE_NOTIFICATION_AUDIT.md lists both production writes for the breaking cutover. The source-handler oracle records exact numeric bits before serialization; Rust runs real DefaultStore writes and counts ordinary notifications, but these tests do not claim throwing-listener parity or shared prototype-dictionary write parity. The adapted frame boundary requires array data; precise malformed-frame diagnostics and non-array iterable destructuring remain a chosen difference.

Historical pre-cutover evidence: Mark/index public handlers extend the adapted receiver slice with explicit parent dependencies. Mark-loaded runs before mark coercion and remains observable if coercion throws. Index coercion precedes market selection; borrowed IDs retain selector order, and each loaded callback precedes its matching Store write. The 85-case source oracle compares the full select/loaded/write trace, not merely call presence, and exact numeric bits for all five fields through real DefaultStore. Loaded-state/selector implementations are separate rows and are not claimed by these adapted handlers. Both new rows retain notification and shared dictionary-write FINDINGs, and the existing array-frame choice.

Historical pre-cutover evidence: The main ticker handler retains source parse order, shared round_usd percentage rounding and the original volume units (volCcy24h to volume, vol24h to quoteVolume). Its 209-case family oracle executes maintained roundUSD as well as handler bodies, compares binary64 bits and retains separate values for unrelated inputs. Fractional percentage probes prevent a bypass of rounding from passing an integer-only corpus. All five public ticker handlers remain divergent on the routed notification contract and shared prototype-write boundary, with every write listed in STORE_NOTIFICATION_AUDIT.md.

Historical pre-cutover evidence: The five handler error conversions explicitly bind StorePatchError so the upcoming notification-capable return ABI cannot silently flow through an inferred Display conversion. Its arrival must cause a compiler error and a deliberate notification-error migration at each audited write; no parallel API is introduced.

Historical pre-cutover evidence: The notification forcing-function audit is callee-first across all ten production Rust files, not a map_err search. Its current census finds exactly five direct Store mutation results, all explicitly pinned to StorePatchError; no alternate inferred/formatting/discard or blanket-From path was found. STORE_NOTIFICATION_AUDIT.md records exclusions and the distinction between direct-result handling, host callback effects and core-owned health publication. This is scoped evidence, not proof of notification parity.

Placement typed-ID refusal is deferred across the whole operation, including later HTTP batches. Source null-row or rejection errors take precedence; a final typed refusal does not skip requests the source performs. Ten maintained cases assert exact later errors and request counts, not merely Err presence.

Public subscribe is an adapted own JSON topic-map receiver, using shared JS own-key ordering, one-level flattening and batches of 100 through real core send. Its 35-case actual-source oracle records exact frames and send-failure identity. The row remains divergent for the JSON receiver/UTF-8 input choices and the indirect not-ready send/reconnect socket-health notification FINDING, explicitly inventoried; connected tests are not throwing-listener parity.

Review found the source flattened topic objects remain live references: a first-send mutation changes the second frame. The source alias witness now pins before/after topics and wire; Rust pins the unchanged later snapshot. This is CHOSEN owned-snapshot-vs-live-alias, not covered by the JSON/UTF8 choice. A double-escaped lone-surrogate witness independently pins source wire acceptance against Rust UTF8 ingress refusal; ordinary astral Unicode/BOM are positive cases.

Older history evaluation-order repair: the original 1178-case oracle compared values and errors but not injected toBase effects. Rust parsed uTime before amount/filled, making zero conversion calls where source made two before the same TypeError. Fourteen added probes across seven fields and two throwing forms now compare callback order/count/arguments and exact errors (1192 total cases). posSide coercion precedes type/status, and amount/filled/reduceOnly precede updatedTime. A source early-uTime mutation fails at1 on source history conversion before coercion error; the compiling Rust early-uTime mutation fails at101 on history conversion effects before uTime error. Existing prototype-output FINDINGs remain unresolved; this repair adds no parity claim.

History prototype lookup now consumes shared TableHit and defers its CHOSEN closed-output refusal until all source field evaluation and contract conversions complete. Missing retains defaults; constructor and __proto__ retain their inherited branch before refusal. The source lowercases lookup keys, excluding the other ten mixed-case prototype names, which have positive fallback probes. Eight inherited-plus-later-error cases pin later TypeError precedence and conversion counts; pre-serialization source function versus prototype-object identity remains distinct. This closes the history lookup FINDING at the adapted mapper depth; income prototype output now has the same declared closed boundary.

Field identity supplements are separate from original corpora. A compiling closed open/close-time swap passed the original 1220 mapper cases but fails native-field assertions on distinct timestamps in the supplement. Direct decoded TickerMarkInput/region/instrument values prevent symmetric serde-name swaps from cancelling through re-encoding. FIELD_IDENTITY_AUDIT.md and NAN_CAPTURE_AUDIT.md distinguish candidate counts from measured controls and preserve outside-crate archive bytes; ticker captures canonicalize NaN only, retaining signed-zero and finite bits.

Public ping is an adapted disposal-owned core receiver: monotonic timestamp
before literal ping send, with represented send errors propagated. Its source
oracle asserts effect order and timestamp retention through later pong latency.
The disconnected heartbeat finding closes against fallible WsParent::log: the
old success witness flips to the source listener TypeError after the clock,
with subscriber IDs retained and no send or reconnect.

Notification cutover: five typed StoreMutationError conversions now delegate to core ExchangeError::from; incoming health/pong results propagate with ?. The shared subscriber-ID finding is closed by landed structural retention, with original cause metadata and Store-local inner-to-outer IDs pinned through real DefaultStore. STORE_NOTIFICATION_AUDIT.md records seven sites and exact cause/stop/commit witnesses; no row is reclassified ported by this migration.

Pretty-symbol supplement: 655 maintained-source cases compare native optional presence, distinct base/quote strings and exact errors. Original market-policy corpus is unchanged; this new oracle feeds only this crate and runs one Node process for the whole corpus. The source swap exits 1; compiling Rust swaps exit 101 on `pretty-symbol native base quote identity 0`, and eager argument coercion exits 101 on `pretty-symbol cold branch must not coerce 390`. String classifications follow [the observability ruling](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict), with separate [market helper input choices](../parity/MANIFEST-SPEC.md#okx-market-record-choice). `tests/string_observability.rs` checks native decoded currency identity, whole-record/batch refusal and pretty-symbol identity before the selected source observations. No numeric bit capture is used by this string-output oracle.

String-selected witness assertions have their own execution floors, beyond outer corpus counts: pong 1, source ticker mark/index-success/index-error 19/18/1, inherited REST status 5, and constructor/prototype position filters 18 each. tests/branch_controls.py independently disables all seven sites and requires named failures; filtered every is never accepted without its nonempty count. These guards supplement the existing corpora without changing emitted records.

The pure isAlgoOrder and getAlgoOrderPlacementPriority methods are adapted free operations over the shared declared OrderType domain, using exhaustive matches. A six-case maintained-source oracle checks native bool and priority identity independently, with fixed iteration bounds and explicit six comparisons per operation. Caller order partition, priority sorting and placement remain separate pending operations; these helpers make no malformed dynamic-input claim.

`family_policy` ports standard/EU market endpoint selection, EU SWAP/FUTURES
ordering, position-side selection and business socket resync/statistics policy.
Position mode must be loaded; one-way omits posSide, hedged stop/TP/trailing
orders reverse side, and reduce-only reverses ordinary orders while conditional
orders keep their side. These helpers introduce no transport, Store write or
representation boundary. Native policy values are asserted before wire oracle
comparisons; constructor, account loading and execution remain separately pending.

The private/business websocket endpoint getters in family_policy select the
existing region table by native OkxRegion and optional testnet boolean. Absent
and false choose livenet; true chooses testnet. Native URLs for every declared
region/profile are checked before live-source wire comparisons.

The journal mapper returns ordered canonical JsJsonValue fields, retaining native
f64 nonfinites and signed zero. Its tests assert those values before explicit
js_json_stringify projection; there is no legacy Value-returning mapper path.

Production binding (`src/binding/`): `OkxExchange::new` assembles the OKX class
graph on the shared js_runtime host, with no second runtime. The JS BaseExchange
receiver gets OKX class fields and every manifest member as an own property that
runs its ported body. `ops.rs` adapts the bodies' closure tables, `typed_members.rs`
the closure-typed bodies, `sockets.rs` the three socket classes, and `globals.rs`
the okx.types.ts tables, mappers and createAPI collaborators. Generic needs go
into js_runtime, not here. `tests/exchange_binding.rs` drives every trait method
over a scripted transport; `tests/exchange_binding_controls.py` mutates each
dispatch and requires its named test to fail, then passes 14/14 restored.

The order-read and websocket-account harnesses now include the production
intrinsic dispatcher and use shared `js_runtime::value::{Js, Thrown}`. Their
private value models and intrinsic match tables are removed; scripted callbacks
supply only fixture collaborators. The existing 53 order-read and 56 websocket
source scenarios retain their native assertions, source comparisons, effect
traces, error identity and live topic references. Websocket wire observations
now pass through the shared JSON stringify/parse functions.

This cutover exposed production bugs hidden by the old mocks: five-group order
and new-market topic spreads consumed only two groups, and dedupe colon joins
replaced lone UTF-16 surrogates. The dispatcher consumes all supplied iterables
in order and appends colon-separated strings as UTF-16. A production Exchange
regression loads distinct normal/OCO/trailing/conditional/trigger orders and
checks all five groups reach symbol cancellation.

The createAPI error interceptor's `formatOkxApiErrorMessage` needed `is-array` and
`string`, absent from the production table, so every non-"0" OKX response threw
"unknown OKX intrinsic is-array" with no `response`; `template-concat` also joined
only two parts. Income paging likewise lacked `add`. `tests/exchange_binding.rs`
drives rejected setLeverage/placeOrder and a non-empty bills page through the
real binding. Nine permanent controls in `tests/production_intrinsic_controls.py`
restore each broken behavior and require a named compiled failure followed by an
exact source restoration and passing case. `tests/history_stream_api.rs` still
supplies its own fake intrinsics; it is not evidence for the production table.

The real binding's private orders, positions and account frame tests use actual
socket dispatch, Store and event emitter. Terminal orders commit Closed/Canceled
before the final fill, emit the exact source fill object once per execution, and
retain terminal memory against stale REST snapshots. The JS Store bridge delegates
to the existing native operation rather than implementing another terminal map.
`tests/terminal_store_controls.py` faults the bridge registration, terminal-memory
delegation, empty-Store handling, subscriber-error propagation and typed ID
admission, requiring named compiled failures and exact restoration.

The concrete parallel-map host starts every callback with the shared eager promise
runtime and joins already-started promises. Market/leverage and order-read bodies
share that implementation. The real EU binding holds a SWAP response and verifies
FUTURES starts before it completes for both normal and algorithmic orders, matching
the actual TypeScript members with the installed p-iteration dependency.

Mapper collaborators use a fallible callback ABI. The installed native globals
preserve each original `Thrown` value and stop at the first failed conversion,
symbol resolver or settle-currency resolver. The typed pure callers lift successful
values into `Result` without changing their oracle assertions. Six source probes
and six installed-global probes pin opaque error identity; a constructed Exchange
history read pins one conversion, one error event and no returned rows. The
seven native regressions fail on the previous swallowing implementation.

The production networkErrorOnRead import applies the installed axios-retry
cancellation/timeout exclusions and is-retry-allowed DNS/TLS denial set. Native
response/code guards preserve source truthiness and repeated property reads;
code membership compares UTF-16 directly, without invoking conversion hooks.
The actual constructed HTTP witness compares36 GET profiles and one write guard
with the maintained wrapper and installed dependencies. All30 denial-set codes
and ERR_CANCELED stop after one attempt; allowed socket errors still retry.
