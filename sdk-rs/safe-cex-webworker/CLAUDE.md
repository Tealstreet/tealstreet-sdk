# Worker port

The production exchange event bridge subscribes with `on_args` for log events
and posts that list once. Fill messages contain the first emitted argument.
`install_exchange_event_bridge` is shared by worker startup and native venue
regressions, so tests exercise its actual registration and forwarding logic.

Behavioral source: [worker guidance](../../packages/safe-cex/src/webworker/CLAUDE.md)
and its linked subsystem guidance. This crate consumes Exchange and Store
directly; hosts provide the existing ExchangeRuntime clock and local scheduler.

The shared exchange-core keyed ThrottleManager follows the source's leading call and resettable trailing
timer. This differs from the worker bridge's fixed-window lodash throttle.
Clear cancels the timers still registered by key, including the source bug where
an older orphan tail survives a newer leading call and clear. Drop cancels all
remaining owned tasks; timer futures hold
only weak references, so discarded host tasks cannot retain the manager.

Worker payloads preserve missing versus explicit-null data. Incoming/outgoing op
sets are pinned against the live TypeScript unions. Store payloads reuse the
existing StoreUpdate directly, including PackedTickerDelta, rather than duplicate
that contract. Incoming commands and ordinary worker payloads retain their owned JSON boundary.
CallMethod results and OHLCV DATA posts use required WorkerDataPost with existing
runtime Js, alongside separate native book and Error capabilities. DATA envelopes
have no Serialize implementation; browser materialization retains native numbers
and own undefined before postMessage. No default or JSON adapter is supplied.
Native Error DATA retains its intrinsic prototype and hidden message/stack/cause
properties before post, including UTF-16 message units. The real Wasm witness
compares TypeError descriptors and structured clone with the host JS engine.
Error DATA materialization ignores custom enumerable fields without invoking
their getters; cause is retained only as an own data property. Standard
name/message/stack reads stay observable, matching native Error structured clone.

The CLI executor is injected and owns its emitted handled results; the handler
never manufactures success from a resolved void future. Missing/rejected executors
emit exec-log, with exec-result only for a supplied reqId (including an empty one).

Full account position refreshes use the full Store mutator even for empty
results; scoped refreshes remain partial. Position refresh uses the canonical eager Promise driver: fetch/locks start
during helper or dispatch entry, and even ready fetches yield before Store/reply
work. Dropping a caller waiter does not cancel the request; runtime abandonment
releases its pending markers and reports cancellation to the bridge.

Market-load buffering uses separate 500ms self-rescheduling checks and a 30s
deadline. Timeout reports are cooled down for five minutes and contain only
bounded telemetry; loaded markets reset the episode. Timer guards are created
before spawn, release only their own slots, and never retain the handler.
Timeout Error construction and post are required fallible host capabilities.
WorkerErrorPost keeps the Error opaque; its Wasm binding creates a real Error at
the report point and posts it without JSON conversion. Structured clone retains
the stack and Error identity while resetting the custom name. Creation/post
failure reports through the runtime without committing cooldown or clearing pending.
A fired deadline stays registered after a throw until markets flush or dispose;
new subscriptions cannot arm a replacement deadline during that failed episode.
The concrete Wasm host witness uses examples/worker_error_host.rs, wasm-bindgen
and tests/worker-error-host.cjs; it does not claim browser startup composition.

Sentry worker errors read only nonempty string message/name/stack properties;
plain structured-clone objects work, primitive rejections use the supplied
fallbacks, and whitespace strings remain valid. Missing stacks stay omitted.

Sentry serialization accepts canonical Js values and reads inherited and hidden
message/name/stack through Get in source order. Getter failures preserve their
original thrown value and stop later reads; functions use fallbacks without Get.
The existing String DTO replaces isolated UTF16 units with U+FFFD under the
approved UTF-8 addendum; retained input units and valid pairs remain unchanged.
Payload enums reject JSON object-form enum tags; worker ops are string literals.
Private serde remote helpers expose the derived spelling inventory to unit tests,
while public Deserialize requires strings. Fixture mode and version are closed
literals too; test rejection as well as successful round trips when changing them.

Displayed-position processing consumes owned StoreData snapshots and a host wall
clock callback, read separately for the five-minute then one-minute queue of each
tracked position. Clock calls are skipped when its ticker is absent or falsy.
It preserves real-only keys, fake-position maxLeverage fallback, full-stop
markers, reduce-only chaser sides and one-second PnL sampling/queue eviction.
Symbol maps rebuild per snapshot; source array-identity caching is explicitly
divergent. The live TypeScript oracle compares complete serialized rows and
rolling-window transitions, including omitted fake keys and stale queue removal.

Worker wire records require JSON objects before private derived decoding. This
rejects serde's positional-array representation regardless of optional fields;
qualified-call tests cover every local record as well as the string-only enums.

Pass current options and metadata into each displayed-position pass. Freezing the
UPnL policy at construction loses live tuning changes; rebuilding the processor
to apply them would wrongly discard its rolling PnL queues.

CallMethodHandler keeps unassigned results null and assigned void results as own
undefined at the DATA post boundary. Unknown methods omit result; Error replies
retain own stack and present code. Post rejection enters the source error-reply
catch; if that post also rejects, the runtime receives the original failure. Pre-spawn reply guards settle dropped tasks once; metadata throttling
stays detached, so its late failure reports to the host rather than changing
the already-successful method reply. removeOrders accepts objects carrying id.

Worker logging helpers retain nonempty report keys, finite positive cooldowns,
object-only tags/extra and JS truthiness for console suppression. Live source
cases pin reserved-field filtering and the inherited/hidden-field information
lost at the JSON input boundary; those omissions remain divergent.

Subscription handlers deduplicate symbol or symbol/interval listeners by subId.
Orderbooks retain the first subscriber envelope; candles fan out current envelopes
in insertion order and flush the previous candle before a timestamp transition.
Their private throttle mirrors lodash leading/trailing/maxWait and flush-orphan
timers. Host-discarded futures release timer slots; weak timers cannot retain the
handler. Inherited dispose clears market buffers only, preserving the source's
active-listener behavior. The live oracle uses actual locked lodash in both CI jobs.

Chaser/TWAP task records reuse shared market, side, origin, status and policy
models. Local records require maps and optional nonnullable fields reject null;
the terminate discriminator has a private serde wire enum and string-only public
Deserialize. Source-derived inventories cover generated field and literal names.
Shared decoder repairs now reject nested positional records and tagged unions;
the former widening witnesses remain permanent public-decoder rejection cases.
Optional worker fields distinguish absence from explicit null. Their private
serde helpers deserialize present values through the underlying nonnullable type;
`data: any` keeps its explicit null. A live AST inventory pins all eleven optional
nonnullable fields across envelopes, config, metadata and serialized errors.

Chaser fill accounting selects finite nonnegative placement-unit fills before
falling back to the order's display-unit fill. The live source oracle preserves
nonfinite fallback values and negative zero instead of JSON-normalizing them.

Chaser instances retain shared options/state and insertion-ordered tracked IDs.
Fill totals reproduce plain-object inherited keys and integer-key enumeration;
source attempt-limit and decreasing-fill behavior are deliberately preserved.
Disposal releases its list before invoking fallible callbacks, so reentrant
cleanup and newly registered disposers behave like the source without panic recovery.
The owned current-order Vec and immutable default omit mutable array identity;
live source mutation witnesses document those representation choices.

CallMethod dispatch uses the shared Exchange override hooks for order refresh and
private snapshots. Its oracle supplies distinct venue overrides and checks both
target and arguments; a base() bypass must fail even when reply payloads agree.

Chaser enrichment preserves the source plain-object lookup bug: inherited keys
can mark orders as chaser-owned, and an order ID of __proto__ changes the lookup
prototype. Truthy inherited non-chasers clear stale chaser fields to undefined.
The whole-order source oracle covers empty maps, prototype replacement and own
property precedence; do not silently replace this behavior with a clean map.

Active-chaser projection reads the current manager view and matches exchanges by
Rc identity, never exchange name. Each pass replaces the list and omits stopped
instances. StoreData owns copied ID arrays; the live alias mutation witness pins
that declared representation choice while options/state are read anew each pass.

Use shared js_truthy for JSON flags, including arbitrary-precision JSON numbers
that overflow to Infinity, and shared js_own_keys for chaser fill enumeration.
The logging oracle passes overflow lexemes as raw JSON text to avoid erasing the
input into null before the parity comparison.

Both native and parity CI execute the worker tests. TypeScript and actual lodash
resolve through explicit isolated-install paths. Nine source oracles passed in a
tracked-source mirror of c637001779 under /private/tmp with no ancestor
node_modules; every compiler-import mutation and the lodash-import mutation
failed MODULE_NOT_FOUND. The mirror was deleted. oracle_dependencies.rs keeps
missing dependency failures mandatory in Cargo, so fallback or skipping is not
an acceptable response to a broken isolated install.

Worker snapshot bookkeeping compares numeric update IDs using JS strict numeric
equality, so NaN forces sending and signed zeros compare equal. Ticker baselines
copy only the packed fields; deltas retain NaN and infinities as typed numbers.
A later ticker identity mismatch returns fallback after earlier rows have already
mutated the baseline, preserving the source transition even on that error path.

Store payload preparation keeps raw order/position update-ID gates separate from
derived displayed positions. Active chasers exempt orders until one payload after
the last chaser disappears. Hosted mode always sends full tickers and leaves the
packing baseline untouched. Sanitization strips only truthy info values and copies
update IDs, loaded flags and socket health; owned Rust snapshots also copy other
nested values, a CHOSEN alias omission with a live mutation witness.

Message-handler statistics use real BaseWebSocket state and core host reexports.
Public inverse always replaces public, even when uninitialized/empty; secondary
merges first and primary wins. Shared HashMap/JSON choices omit inherited keys,
order, aliases and nonfinite values; named array properties are refused without
panic. Core optional callback count and handler history remain absent when unset;
nine live field combinations distinguish absence from explicit zero and false.

DevHandler dispatches through real BaseWebSocket and shared regional HOW clients.
Its transport dependency is for production HOW controls, not socket test hosts.
Force-open errors stop the socket loop; tests install both HOW sockets with a
fake host so wrong-region dispatch cannot pass merely because clients are idle.

All thirteen worker source oracles at b5f9e19e4d passed in a tracked-source mirror
without ancestor node_modules. Every missing-compiler control and the missing
lodash control failed MODULE_NOT_FOUND; the disposable mirror was removed.

The table-lookup audit includes absent Object.prototype function names, not only
explicit own prototype-named symbols. Source fake-position guards accept those
functions; the canonical typed ticker/market boundary omits them. Both process
and append_fake_positions_from_chasers cite the CHOSEN closed typed output
boundary: shared TableHit preserves the guard, but Rust omits the whole fake row,
not just its function-valued fields. Serialization therefore does not converge.
The __proto__ object and prototype-write behavior remain distinct FINDINGs,
separate from the CHOSEN snapshot model. Chaser fill and enrichment also consume
the shared inherited-name inventory; no local twelve-name read list remains.

Worker dictionary-read census (AST element accesses; writes/delete/array indexes
excluded): 38 reads. Bridge routing has 13, chaser-manager symbol fallback has 3,
UI fetched leverage has 2, chaser fill has 3, chaser enrichment has 1, displayed
positions has 13, and registry/other metadata/op-count tables have 3. No venue
protocol literal table occurs in this worker subtree.

Five displayed-position key reads use colon-suffixed closed position sides, so
prototype member spellings cannot be produced. Two active-PnL reads use
Object.create(null), so the table has no inherited members. Four other reads
across registry, metadata and op counts use closed ExchangeName/StoreWorkerOp
sources; prototype spellings are outside those unions (unreachable, not a
property of the table). Chaser fill/enrichment inherited lookup bugs are already
preserved. Pending bridge, manager and leverage rows must preserve truthy
inherited hits before their fallback/guard, consuming the shared representation.

Chaser fill bounds and subscription throttle remaining waits use the shared
math_min/math_max primitives, preserving JavaScript NaN and signed-zero choices.

Active-symbol UI updates filter strings by JS trim truthiness but retain the
original string, then union existing symbols before incoming symbols in stable
order. The helper delegates actual storage/events to BaseExchange. Its JSON
payload boundary omits inherited fields/named array properties and refuses lone
surrogates; live witnesses retain those source cases rather than call them invalid.
Margin-symbol fetching is implemented as the explicit future helper described below;
the outer UiStateHandler dispatch remains pending.

Chaser-manager position completion and fallback fill helpers use the instance's
captured target but current options size and first matching live position. Only
Bybit Demo uses these signals. Missing/NaN contracts become zero before abs;
completion keeps the 1e-12 tolerance and fill bounds use shared Math extrema.
The live manager oracle compares 192 scenarios / 7488 transitions, including
size changes after construction and nonfinite targets, contracts and sizes.

All fifteen source oracles at 657a1ea11a passed in a tracked-source mirror with
no ancestor node_modules, including UI symbols and chaser-manager transitions.
Every compiler-import control and the actual lodash-import control failed with
MODULE_NOT_FOUND. The disposable /private/tmp mirror was deleted after validation.

Bybit Demo private-WS diagnostics use strict true flags and distinguish absent
close codes from null. Fallible shared String conversion propagates TypeError
name/message; it never panics or substitutes an object string. The startup wait
re-reads health after each of at most four 500ms sleeps and returns immediately
when paused. Its caller owns the future; no borrowed store state crosses await.
The JSON/UTF-8 boundary omissions remain witnessed and declared divergent.

Tracked-order selection preserves store order and duplicate matches, but returns
owned orders. A live reentrant log listener can mutate TypeScript's selected
references before completion; the Rust copy retains its earlier status. Both
selection and completion declare this ownership choice. Reduce-only completion
checks the closing side and nonzero notional; ordinary completion clears IDs
only for a nonempty all-closed selection. The oracle covers 6864 combinations.

The new health oracle at 6f8703c7c5 also passed its own tracked-source mirror
with no ancestor node_modules and a missing-compiler MODULE_NOT_FOUND control;
the mirror was deleted. Together with the prior fifteen-oracle validation this
covers all sixteen source oracles present at that commit.

TWAP interval calculation retains the source's separate multiply-by-60 and
multiply-by-1000 steps; regrouping changes binary64 results. Remainder placement
keeps the `amount <= 0` guard, so NaN proceeds, awaits the venue capability and
swallows rejection. The live oracle compares 2337 bit-level interval cases and
204 order/await/error cases; no numeric observation passes through JSON null.

Completion and pending-lifecycle chaser oracles at 4313450e30 each passed a
tracked-source mirror with no ancestor node_modules and a missing-compiler
MODULE_NOT_FOUND control. The mirror was deleted; these extend the prior sixteen
oracles' isolation evidence without relying on a root install.

TWAP state creation consumes shared lodash_random with numeric bounds, omitted
floating mode and the host RNG, plus shared safe-math operations. Invalid derived
array lengths return the source RangeError before any draw; checked reservation
refuses unavailable host capacity without a panic. The 4320-case oracle uses
fingerprinted installed lodash and records draw count and computed number bits.
The source's empty-array named -1 property stays a separate witnessed FINDING;
market alias copies and host random-source choice do not absorb that loss.

Active-TWAP projection reads the ordered manager field view on every pass. It
filters on options account/status, spreads state over options, then derives
signed size and symbol from options. It replaces the previous list, including
when empty. The 432-case actual-manager oracle observes computed number bits
before JSON, both directions of market aliases, the live ordersSizes alias and
the inherited zero-order named-array-property loss; owned snapshots declare
those losses separately from UTF-8 identifier refusal.

TWAP helper, state and projection oracles each passed tracked-source mirrors
without ancestor node_modules at afdf9f2e12, dadd6365f9 and bf14bf161d. Each
missing-compiler control failed MODULE_NOT_FOUND; the state oracle also rejected
a missing installed lodash. All three disposable mirrors were deleted.

TWAP projection also retains a separate optional-presence FINDING: an own
state.origin set to undefined suppresses options.origin in the source spread,
where Rust Option cannot distinguish that from absence and keeps the fallback.
The witness checks own-property presence before JSON omits the source field.

BBO injection uses native ModelPatch numeric fields, retaining Infinity/NaN and
unrelated nonfinite ticker fields. The stale-value witness failed unchanged
after this switch. Outbound JSON book payloads still collapse nonfinites; the
oracle now records source number kinds before serialization to pin that separate
FINDING. Store errors use the host exception channel at the infallible listener
boundary, never a panic or silent Result discard. Options reads use core's live
owner accessor on each subscription/telemetry pass.

Numeric ingress audit inventories all 20 scalar numeric fields in the six local
task records. Raw map decoding preserves overflow and signed zero; a native
Value from -0.0 also survives. An already-parsed Value holding the integer -0
lexeme loses its sign when the map-only decoder re-buffers it. Seven live Node
tokens and three ingress paths originally pinned this FINDING. ChaserTerminate,
ChaserCompleteEvent and ChaserOrderState now use shared public ingress; the
remaining three task graphs are repaired by the complete adoption described below.

The same ingress sweep covers worker config, fixture/frame/config and payload
records. Their existing-Value integer -0 path also loses its sign; nested frame
records and JSON msg/data fields cross a rebuffer even on raw outer decoding.
Separate Node bit witnesses pin these paths rather than claiming raw ingress is
universally safe. The fixture graph and envelope/config now repair their public decoders as
described below; their separate bounded-depth choices remain divergent.

UI position leverage updates select buy leverage for OneWay or long positions,
sell otherwise, and issue one native Store batch only when numeric values differ.
NaN differs from itself; signed zeros compare equal. The source helper oracle
records the Store call boundary over 2058 cases; native Store assertions retain
nonfinite leverage and unrelated NaN fields. Store errors propagate as Result.
The leverage oracle also passed in a disposable tracked-index source mirror
without ancestor node_modules; a missing compiler failed MODULE_NOT_FOUND.
Wrong-side, bitwise-equality and JSON-patch mutations compiled and failed named
leverage assertions; restored native patches passed. The mirror was deleted.

The pending TWAP lifecycle has a 252-case actual-source witness. A placement
await may finish after pause/terminate and start then rearms its timer; terminate
still removes manager membership and retains completed status. Preserve this
source race when porting. This source-only gate is not a Rust lifecycle claim.
The witness invokes the rearmed timer after both interruptions: it actually fires,
then the paused/completed guard prevents another placement. Guard, rearm and
cleanup source mutations fail named assertions. The isolated tracked-source
mirror passes all cases; missing compiler/lodash fail MODULE_NOT_FOUND.
Every lifecycle row asserts pre-settlement/settled placement counts and executed
slice count: merely generating zero, negative-zero and NaN cases missed a
source truthiness mutation. That formerly surviving mutation now fails by name.

Chaser price selection consumes shared predicted bid/ask, extrema and adjust
helpers. Shared TableHit preserves inherited market TypeError and ticker NaN;
missing-market logging re-reads options after reentrant listeners. Its 479-case
source oracle constructs the real DefaultStore and asserts native numeric bits.
A separate __proto__ market write poisoning a later precision lookup remains a
shared Store FINDING; no worker-local prototype-write shim is introduced.
Own-only lookup, wrong clamp and array-first mutations compile and fail named
price assertions. The oracle passes all 479 cases in a disposable tracked-source
mirror without ancestor node_modules; a missing compiler fails MODULE_NOT_FOUND.
Source log/error listener throws also retain a separate shared-emit FINDING:
a log throw prevents completion, while an error throw happens after completion.
Core infallible listeners cannot propagate either source TypeError yet.

Worker log forwarding spreads own context over exchange/account defaults and
uses shared integer-key enumeration. Its post callback is fallible; errors
propagate to the caller without a local catch or panic. The live-method corpus
pins 810 envelopes and separate signed-zero/overflow bits. JSON context cannot
retain own undefined overriding defaults; its source structured-clone witness
keeps that choice distinct from the shared UTF-8 boundary.
The inline log level union remains a closed Rust enum with source-AST spelling
checks and no public deserializer. Drop-context, wrong-op and raw-key-order
and swallowed-post-error mutations compile and fail WORKER-LOG; the 810-case isolated tracked-source
mirror passes and missing compiler fails MODULE_NOT_FOUND.
A raw nested-key witness failed with top-level ordering alone; the shared
recursive ordering operation now preserves JS enumeration at every depth.
Native NaN and infinities survive the source structured clone but collapse at
native JSON Value construction before forwarding. Separate pre-serialization
kind witnesses retain this boundary; raw overflow lexeme retention does not
claim that native nonfinite construction is lossless.

Risk postprocessing consumes the shared fallible risk API. A required host report
callback runs before explicitly replacing prior risk with an empty map; a report
failure propagates and leaves prior risk intact, matching throwing console hooks.
The old native prototype panic witness failed unchanged when the shared Result
repair landed. Actual DefaultStore subscribers now witness the derived clear.
The actual-source corpus covers 96 inherited-key/report-failure cases, 25
reachability/empty controls and three dictionary ordering cases, with source and
native numeric bits captured before JSON. Owned snapshots rebuild maps each pass;
a same-array append witness pins the source's stale cached map separately.
Shared profile field ordering still places positionSize before text/fee where the
source inserts it last; this remains a routed FINDING, not normalized parity.
No-clear, no-report and clear-before-report mutations compile and fail named
recovery assertions. The processor does not fabricate zero risk rows.
The risk oracle also passes in a disposable tracked-source mirror with no
ancestor node_modules; explicit isolated compiler/lodash paths resolve there,
and missing-package controls fail MODULE_NOT_FOUND. The mirror was deleted.

Fixture/frame/config public Deserialize now captures shared JsNumberInput before
strict map decoding, including every nested fixture buffer. Frame.msg uses the
shared js_deserialize_passthrough adapter and an explicitly named eligibility
exception; it keeps Value without disabling the whole-output numeric guard.
Three old per-record loss witnesses failed unchanged after opt-in, including
raw outer JSON frames[0].atMs. The permanent matrix compares 810 source/native
bit observations across nine field paths, ten spellings and nine entry routes:
bare raw str/slice, owned/borrowed Value, shared raw/owned/borrowed, and native
negative-zero Value owned/borrowed. Shape and optional-null rejection stay strict.
These three rows retain a separate CHOSEN shared input-depth boundary: Node
accepts 140 nested msg arrays while the shared buffer refuses them. Ordinary
20-level messages remain accepted. The bounded capture avoids a worker-specific
unbounded recursive decoder; it is not a claim that TypeScript rejects depth.
Frame/outer Value-rebuffer and omitted-msg-adapter mutations compile and fail
FIXTURE-NUMERIC ACCEPTANCE. Restoring all legacy decoder bodies plus the legacy
msg visitor compiles and accepts the deep owned input, failing the named depth
refusal assertion: the depth boundary is a measured accepted-set narrowing,
not an old refusal relabeled as new evidence.

WorkerConfig and StoreWorkerPayload also capture JsNumberInput at public ingress.
The optional data field uses shared js_deserialize_passthrough inside present_value,
so explicit null remains present while absence remains None. WorkerConfig's
eligibility uses the exhaustive shared macro for every field, including the shared
IndexMap key/value traversal; it neither adds an orphan impl nor re-buffers JSON.
Two committed old witnesses flipped unchanged (owned config and raw payload).
The record oracle adds 450 comparisons over five envelope/config paths, ten
spellings and nine routes, including nested fixtures. Shared capture's depth
limit remains CHOSEN on both rows, including ignored deep config properties;
Node and legacy bare decoders accept those inputs while the bounded capture
refuses them. Ordinary nested values, absent data and explicit null remain covered.
Config/payload rebuffer, omitted data adapter, and null-erasure controls compile
and fail named acceptance/presence assertions. Separate compiled legacy decoders
accept each deep input and fail the named depth-refusal assertion, pinning the
new acceptance narrowing rather than presuming the old decoder refused it.

Carrier regressions compare original native numeric bits and real shared JS
stringify with maintained source output before using finite JSON trace adapters.
Notification error recovery preserves all original metadata and the original
cause reference while appending the actual fanout subscriber ID; those two
provenance additions must be included in the expected complete metadata.

Store notification failures remain separate from numeric patch failures. The
five concrete calls in four worker paths are inventoried in STORE_NOTIFICATION_AUDIT.md:
position refresh has full and partial calls, leverage has one batch, removeOrders
has one base delegation, and BBO has one ticker patch. The actual-source oracle
constructs DefaultStore and invokes the worker handlers: mutation precedes the
first subscriber throw, later subscribers are skipped, refresh/removal report
failure, leverage propagates, and BBO posts before its ticker notification throws.
Twenty source cases distinguish Error from TypeError and preserve the original
name/message/code in removal replies and error spelling in refresh replies.
Name-flattening and code-dropping controls fail their named assertions.
Three formerly ported handler rows are FINDING until the shared full notification
ABI carries these failures; four already-divergent rows name this additional seam.
This is a bounded direct-call audit, not a claim about every indirect venue call.
The BBO patch result explicitly names StorePatchError, so the Store ABI cutover
must reject that old error-to-message conversion instead of inferring the new
notification error type and silently flattening its identity.

Three primitive chaser task records now use JsNumberInput and strict object
private-wire decoding at public Deserialize, with exhaustive output eligibility.
Their three old owned-Value zero witnesses failed unchanged, as did the original
20-field task matrix before its expectations were adapted only for these records.
The dedicated oracle compares 450 native/source bit observations over five fields,
ten spellings and nine routes. All three retain a CHOSEN depth refusal even for
ignored properties: Node and compiled legacy bare decoders accept 140 nested
arrays that the shared capture refuses; ordinary 20-level inputs remain accepted.
ChaserOrderOptions, TwapOptions and TwapState now also opt in through the
complete shared enum/Market graph, as described below.
Each of the three Value-rebuffer controls compiles and fails TASK-NUMERIC
ACCEPTANCE; the same compiled legacy decoder independently accepts the deep
ignored field and fails its type-named TASK-NUMERIC shared depth refusal.

Buffered dispatch preserves the source callback boundaries: info logging happens
before snapshotting the current buffer, and may append, clear or throw. The
buffer is emptied before handlers run, so handler appends remain queued. Each
handler failure is coerced and reported with the original error identity;
coercion or reporter failure aborts the remaining snapshot. No RefCell borrow
crosses a callback. Mutable payload access lets the reporter observe a handler's
op change. Caller clones remain owned snapshots, unlike source aliases; this
and the shared UTF-8 error-text boundary are witnessed CHOSEN divergences.
Coercion-time alias mutation is observed separately: source String(error) can
change the current payload op through the caller alias before reporting it;
mutating a Rust caller clone leaves the in-flight reported op unchanged. The
60-case live-method oracle compares complete event/pending/error sequences and
separately observes seven payload number spellings as binary64 bits.
Compiling controls for snapshot-before-log, no-clear, swallowed reporter failure
and skipped coercion each fail a named BUFFER dispatch assertion. The source
oracle also passes in a tracked-source mirror without ancestor node_modules;
a missing isolated compiler fails MODULE_NOT_FOUND rather than skipping.

ChaserOrderOptions, TwapOptions and TwapState now capture JsNumberInput at public
Deserialize before strict private-wire decoding. Their three committed old loss
witnesses failed unchanged after opt-in; the original scalar census also flipped.
The source-AST graph inventory fails on unsupported types and enumerates 38
numeric paths, including nested termination, order-size arrays and Market fields.
Ten spellings across nine entry routes produce 3420 pre-serialization bit checks.
Market.info retains the shared named passthrough and recursive output checking;
its nested numbers and missing-versus-present-null distinction are measured.
These three rows remain CHOSEN on bounded depth: Node and compiled legacy bare
decoders accept 140-deep ignored values that shared capture refuses. Ordinary
20-level values still work. Independent legacy controls pin both numeric loss
and the new depth refusal separately for each public decoder.
The task-domain oracle also passes in a disposable tracked-source mirror with no
ancestor node_modules; a missing explicit compiler fails MODULE_NOT_FOUND.
Depth controls test the deep bare-owned path before ordinary shared paths, so
legacy decoder rejection by the shared numeric guard cannot mask acceptance of
the formerly accepted deep input.
The domain census explicitly refuses interface inheritance until base resolution
is implemented. A reviewer mutation adding a numeric field through an extended
interface previously escaped the direct-member walk; the permanent inherited-field
control now fails at the named unsupported-heritage assertion.

The pending logError method has a source-only capture of 72 sequences and 360
calls. It executes the actual method and three helpers, with host clock, stack,
post and console hooks. Cooldowns commit before serialization errors; post errors
reach console fallback, whose own error escapes. Clock/post/fallback counts are
asserted per call, including zero-effect getter failures. Strict-boundary and
dropped-fallback mutations fail named assertions. This is not a Rust port claim.

logError now has a Rust forwarding operation with explicit fallible host clock,
error materialization, post and fallback callbacks. Cooldown state commits before
materialization; only post errors reach fallback. No cooldown borrow spans host
callbacks. The full 360-call comparison checks numeric bits recursively instead
of treating 0.0 versus 0 JSON spelling as a value difference. Native clock bits
are measured before serialization. Callable getters cannot survive the chosen
JSON materialization boundary, and coercion-time source tag aliases differ from
owned Rust snapshots; both effects remain witnessed CHOSEN divergences.
The host must materialize Error versus unknown values faithfully; the existing
serializeWorkerErrorForSentry helper has different fallback rules and is not used.
The error oracle passes 72 sequences/360 calls in a tracked-source mirror with
no ancestor node_modules using the CI-generated frozen dependency install.
Missing compiler fails MODULE_NOT_FOUND. Cooldown-boundary, omitted-write and
omitted-fallback Rust controls compile and fail named WORKER-ERROR assertions;
the omitted-write witness names the required entry before examining its bits.

Field-identity supplements inspect native decoded fields before serialization:
`task_identity` covers all six task records, including the outer envelope and
nested Market fields; `payload_identity` pins wire operation names to native
variants. Original task generators remain byte-identical; their roundtrips are
shape evidence, not field-identity evidence. Independent boolean cases are
required because two flags cannot be distinguished by an all-true fixture.

Numeric captures canonicalize only NaN, preserving every finite, infinity and
signed-zero bit. Risk capture uses `tests/capture_bits.mjs` and the matching Rust
capture helper; the capture test pins both NaN signs/payloads, negative zero,
finite values and infinities. Live capture architecture is recorded with the
risk output and in oracle headers; preserved task artifacts receive provenance
through the separately labeled supplement rather than rewritten old bytes.

Chaser projection fixtures give size and maxAttempts distinct values, and inspect
all native ActiveChaser fields without its serializer. The former 90-case gate
accepted a compiling size/maxAttempts swap; the identity gate rejects it.
Order-book callbacks likewise construct native levels with distinct amount/total,
so codec symmetry and equal-value fixtures cannot hide a forwarding swap.

TWAP state and postprocessor gates exercise 24 source-derived Market identity
cases after the production copy, with native fields inspected independently of
serialization. The labeled task supplement supplies these crate-local inputs.

Snapshot preparation supplements use distinct domain fields and native output
reads for both sanitization and forced preparation. Input snapshots precede source
execution, preventing a mutating source from rewriting its own reference input.

The displayed-position clock corpus pins six changing-clock scenarios, including
a 999/1000 ms split, multiple positions, fake rows and absent tickers. Source
clock-call counts are compared for every representable scenario. NaN clock input
preserves the source negative guard: it enqueues rather than treating NaN as early.
The old single-timestamp implementation failed the source 999/1000 witness at
`PNL-CLOCK native one-minute baseline`. Compiling cached-clock, discarded-second-
value and inverted-NaN-guard controls fail separate named assertions; restoring
the implementation passes all 199 scenarios and 222 steps in one Node invocation.

Store writes consume the fallible notification contract: committed mutations stay
visible while the first subscriber failure stops fanout. Refresh and method
handlers route errors through their existing reply catches; leverage propagates.
BBO reports the retained error to the runtime because its core callback remains
infallible; the synchronous propagation gap is still a declared finding.

Chaser log and error emissions propagate listener failures in source order:
a log failure precedes markComplete; an error-listener failure follows it.
Command failure logging likewise precedes exec-result, and a secondary failure
in its detached executor task is reported through the runtime exception hook.

Position refresh checks disposal before the Store write. A successful subscriber
may dispose during notification; the refresh still posts its success reply after
that write returns. The notification witness pins this reentrant ordering.

ActiveChaser identity uses a live AST census of all 11 fields and seven compatible
field pairs. Every pair has contrasting present values across 360 projected
records; native field reads keep serializer symmetry out of the comparison.

Chaser price tests independently pin price step 0.01 against amount step 1 across
four existing buy/sell and fractional-distance cases. A compiling production
price-to-amount precision swap fails the named native price-step assertion.

Notification consumer witnesses retain inner-to-outer subscriber IDs through
leverage conversion and BBO host reporting. BBO posts before its Store commit,
then first-failure fanout stops before later subscribers and reports full error
metadata once. Store-local IDs are not global identities. The core void callback
still cannot propagate the source synchronous throw; no local ABI is invented.

Enrichment's labeled field-identity supplement preserves the original oracle
bytes and independently reads retained Order/extra fields after production.
The AST census covers 22 plus nine properties and 46 compatible field pairs;
24 owned/unowned outputs contrast optional trigger enums, flags and quantities.
Source total/per-case counts and native expected/actual length assertions guard
the comparison before zip; a dropped unowned output cannot silently skip a row.

The leverage numeric matrix also requires retained position count and IDs before
zip. A compiling replacement of the patch with an empty-list update previously
passed all 2058 cases; it now fails the named cardinality assertion.

Active-symbol parity requires 108 source cases, four input and expected steps per
case, fixed native four-step execution and 432 comparisons. An outer count alone
does not protect an oracle-driven inner loop from empty or partial sequences.

The numeric capture probe executes an independent 11-token inventory, checking
source row/pair counts and native comparison count. Empty, partial and extra
iterations must fail even when the source-total guard is bypassed.

Snapshot parity fixes its execution at 72 × 17 preparation steps and 358 ticker
actions across 71 traces. Source per-case counts, independent native bounds and
full-output collection comparisons prevent truncated traces from skipping work.

Task identity fixes each record's case/leaf counts and executed comparisons. Its
current-order-ID and TWAP-size fixtures also pin native collection length; a
first-element check alone catches empty output but misses extra decoded values.

Displayed-position parity fixes native execution at 199 scenarios and 222 steps.
Independent per-scenario bounds prevent oracle truncation from skipping work;
clock-field comparisons require equal native/source row lengths before zip.

Store notification consumers return StoreMutationError unchanged for leverage
and preserve its cause through the existing ExchangeError conversion for BBO
runtime reporting. The infallible BBO callback remains a propagation finding.

Scalar-ingress parity fixes seven tokens across 20 native fields and three routes,
counting 420 actual bit assertions. A field census alone does not establish that
any numeric comparison ran when the oracle supplies the loop bound.

Task-record validation fixes the six field/optional inventories independently of
the oracle: 53 requiredness probes and 14 null probes require present sample keys.
Termination literals complete all three native enum identity checks in a separate
loop before three independent wire spellings, plus 15 rejection checks. Source inventory equality anchors the
field lists; counters detect skipped iterations, not independent field provenance.

The scalar-ingress target separately counts 20 lexical negative-zero input checks
before decoding, in addition to its 420 decoded-field bit assertions.

Both chaser stop-reason constants are compared against executed TypeScript module
exports in a separate source supplement. The unconfirmed-placement string does
not implement or close the independently pending lost-placement recovery path.

TwapManager now owns insertion-ordered live id/options/state views. Replacing an
existing Map key retains its slot; removal only drops membership, never performs
task cleanup. Serialization reads the current record ID, filters only Completed,
and gives numeric own-result keys their shared js_own_keys order. Root options
and state references remain live; replacing a state object leaves an earlier
serialized reference attached to the old state. Nested task models retain their
already-declared ownership boundaries.

The source __proto__ result assignment changes its prototype without an own entry.
The typed result preserves own entries and omits that prototype, with a source
witness and a declared CHOSEN result-model boundary. twap_manager() shares one Rc
per host thread; hosts can own isolated managers. ManagedTwap is the manager field
contract, not a claim that the pending TWAP placement/timer lifecycle is ported.

Margin-symbol fetching publishes its placeholder before starting the venue future.
An initial subscriber failure escapes without cleanup; completion failures use
source catch cleanup, and a failed null-result cleanup is retried once. Every
cleanup reads the current map, preserving unrelated concurrent symbols. Stock
inherited names suppress fetch through shared TableHit. Typed inputs, custom map
prototypes and explicit future scheduling remain declared boundaries.
Cancellation reports through the runtime without deleting Store state: a symbol
and isFetching flag do not identify the request that owns a marker. An older
cancelled future must preserve a marker published by a restarted fetch.

Margin placeholder and cleanup maps use shared js_own_keys enumeration, so numeric
symbol keys precede insertion-ordered non-index keys in both Store publications.
The live numeric-key witness compares native key order before wire values.
Its separate cleanup supplement inserts a concurrent numeric key after non-index
keys, then checks native cleanup order and values before source projections.

`postprocessor_context.rs` ports the two abstract postprocessor bases as stored-input contexts plus required `process` traits. Their constructors retain opaque account, exchange-name, options, and exchange carriers directly; callers can use shared references to preserve source aliasing and UTF-16 carriers without string narrowing. Concrete processors must implement `process`; there is no default no-op. The focused context oracle covers eight constructor/override cases, with native retained-value assertions before source-profile comparisons and compile-failure witnesses for missing overrides.

`postprocessor_registry.rs` retains ordered callable constructor handles in shared mutable independent/dependent lists. Registration compares canonical `Rc` object identity and appends only absent handles. Hosts supply the seven imported constructor handles and reuse one registry per source-module instance; automatic global builtin wiring is not implemented, so the two list rows carry CHOSEN host-linking/lifetime divergences. Wrapping the same factory twice creates distinct constructor identities. The registry oracle executes the source with supplied imports and covers nine fixed registration/mutation steps; native identity, cardinality, and constructed values precede source-profile assertions.

`telegram.rs` implements TelegramWorker's opaque constructor/callback/getter and typed native message/polling lifecycle. Callable identity is retained, and the callback receives the worker explicitly to preserve JavaScript's implicit `this`. Runtime methods use TelegramHost for synchronous GET/POST construction and native futures; the executor adapter can return a construction error before producing a future. Source behavior retained includes nine warm-up responses, SameValueZero IDs, dispatch-before-caching duplicate IDs, direct processing without cache writes, JavaScript whitespace/number text, fresh exchange lookup on command rejection, and 1000ms polling. Typed records/UTF-8, canonical errors, host task ownership and polling versus microtask settlement are specific manifest divergences. Tests use the actual TS class with fixture HTTP/executor/timer imports, native values before profiles, and an independent default command-slot test.

Worker offline recovery dispatches through the object-safe Exchange hook. Each
socket enforces its protocol recovery flag after disposed/force-closed guards;
the base visits every socket before reducing the boolean results.

The task identity and numeric graph source walkers include Market.option's closed
literal fields and optional price limits. Independent native field projections
cover all option terms, price minimum and Order.clientOrderId. Fixed inventories
include 36 market-identity cases, 59/43 TWAP option cases/leaves, 56/41 TWAP state
cases/leaves and 3960 numeric comparisons; future source additions still fail
closed instead of shrinking the compared fields.

Command dispatch rejects missing, null and primitive payloads before looking up
or calling its executor; arrays retain the source object admission. Position
failure replies use JavaScript Error string rules for empty names and messages.

The exchange event bridge consumes `WorkerDataPost` directly: fill is one native
value, and log/error events retain their variadic argument array. Assembly hosts
must supply `post_event(WorkerDataPayload)`; ordinary JSON control payloads keep
their separate `post_payload` capability. Do not serialize native emitter values
before invoking the required DATA post.

OHLCV fetch and subscription requests decode the shared AnyTimeframe interval. The exact 27 source literals are checked against the live type AST; both real handlers forward every interval, and native listeners must deliver each candle. Monthly, extended and seconds intervals must never disappear at worker decoding.

Constructed AsterDEX private-frame/worker emitter witnesses live in
`../safe-cex-parity/tests/aster_worker_emitter.rs`, at the tooling integration layer.
The venue retains private facade tests without importing the worker crate. Both paths preserve
variadic event arguments; the real fill bridge retains Infinity and object identity.

The ticker baseline corpus includes four nullable-index actions in addition to
its original 354 comparisons. Actual maintained worker execution observes an
updated null baseline without a packed null value; null-to-number emits the
numeric index delta. This preserves the source typeof-number filter.

TWAP resume/terminate keep their synchronous state/timer prefix and return the
canonical eager Promise waiter, so even ready/no-op/error results yield before
an awaiting caller continues. Posting failures preserve earlier state commits
and stop later cleanup/removal; tests compare actual source instances before
polling as well as after settlement.

Chaser placement/completion use the canonical eager Promise driver. Caller waiter
abandonment cannot cancel late-placement cleanup or fill reconciliation. Typed
leaf results cross source await checkpoints before logs, retries, fills or events;
ready, failed, no-op and held results are compared with the actual TS manager.
