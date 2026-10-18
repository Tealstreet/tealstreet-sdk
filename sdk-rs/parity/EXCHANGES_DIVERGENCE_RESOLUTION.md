# Exchanges-level divergence resolution

Frozen input:3fdfb335e79c5da663314c02c36bb4d175e5f2e5, after merging
origin041de6685a5b2c092bfe664057962c917e8b9bd4 into the accepted GRVT branch.
The accepted GRVT81cfc1163055f10e79d451f35cbf5ce114eb0daf is an ancestor;
the overseer's local8db2d836e9 integration merge was still gating and absent
from origin when this branch began. No unrelated unreviewed feature ancestor
was added. Delivery branch:fix/rust-exchange-divergence-audit. No push.

Python tomllib measures ten direct manifest/exchanges/*.toml files:
209ported/148divergent/0pending/1not-applicable. Venue subdirectories are
excluded. The frozen note prefixes are30FINDING,117CHOSEN and1unbucketed;
the previously quoted21-supported figure is withdrawn, not reused.

The requested order was followed: all30FINDING rows, then117CHOSEN, then the
unbucketed row. Each row was compared with maintained TypeScript and its native
counterpart. No parity-checker diagnostic decides the semantic classification.
The finite audit ends at148/148; it does not prove complete function conformance
or every host, malformed object, scheduler interleaving or venue composition.
Completion count:148audited,0unaudited. In particular, all117CHOSEN and the
one unbucketed input row are already included;209/148/0 is only the census.

| Frozen family | Divergent | Supported rationale retained | Rationale/documentation resolved | Code repaired | Existing finding retained |
| --- | ---: | ---: | ---: | ---: | ---: |
| BaseExchange |3|2|0|1|0|
| BaseWebSocket |7|0|4|2|1|
| External public data |24|0|24|0|0|
| Fill event units |2|1|1|0|0|
| In-process public data |8|0|7|1|0|
| Income paging |5|1|3|1|0|
| Native registry |49|49|0|0|0|
| Web registry |48|48|0|0|0|
| Handler types |2|0|2|0|0|
| Total |148|101|41|5|1|

The current outcome is101supported/148(68.24%),41rationale/documentation
resolutions, five repaired rows and one already-known options finding. These
are disjoint primary outcomes. The five repair rows comprise three scheduling
and two behaviour findings; the other41 comprise20explanation and21documentation
findings. The already-known row contains four option limitations, not four
newly demonstrated native defects. Status moves:0. The block is audited, not
cleared of FINDINGs.
There are49changed notes:30first-pass resolutions,18CHOSEN-pass edits and the
one existing-FINDING normalization. All358row identities/statuses, Rust targets,
layers and file declarations are unchanged. Final buckets:129CHOSEN,17MIXED,
1FORCED,1FINDING. The source-formatted arrays come from the canonical writer.

Declarations live once at MANIFEST-SPEC.md#shared-utf8-string-boundary,
#closed-typed-output-boundary, #owned-snapshot-vs-live-alias,
#numeric-ingress-defect, #divergence-taxonomy and #forced-surrogate-conflict.
Local choices cite #exchange-runtime-callback-choice,
#public-data-provider-choice, #shared-handler-and-ledger-choice and the existing
#registry-loader-choice. Row notes cite these anchors instead of duplicating
their declarations. Historical audit reports remain historical measurements.

## First30: findings before rationale

The first30 split is5code/25rationale-documentation. No status changes follow
merely from closing an annotation finding: each repaired row retains an actual
residual divergence. The table's iteration is within this requested first pass.

| Iteration | Source symbol | Primary type | Resolution |
| --- | --- | --- | --- |
|1|BaseExchange.waitForRemoteSigner|scheduling|Eager checks, notification and registration; cancellation/delivery price retained.|
|2|runReopenResync|scheduling|Ready results yield before reactions; eager fetch and finally order retained.|
|3|capWsDiagnosticPayload|behaviour|Visible representation error replaces silent surrogate substitution; whole event is refused.|
|4|processClose|explanation|Observable reporter rationale; duplicate-close source quirk retained.|
|5|reconnectNow|documentation|Adapters/errors are choices; no unproved FORCED cause.|
|6|recoverAfterOffline|documentation|Selected socket ABI throw loss, policy and timer identity priced.|
|7|parseCandleSubscriptionKey|documentation|Malformed private-key FORCED claim withdrawn.|
|8|GatewayChannel|documentation|Actual auth tag retained; source declaration mismatch named.|
|9|GatewayMessage|explanation|Public full-envelope validation priced separately from provider decoding.|
|10|External constructor|documentation|Actual token/exchangeId transmission witness attached.|
|11|External onOpen|documentation|Exact transmitted configuration units; whole construction/open admission loss named.|
|12|External onMessage|explanation|Retained raw JSON and selected payload decoder; repaired own-key cause not reopened.|
|13|handleTickers|documentation|Stored symbol and later exact comparison witness.|
|14|handleTicker|documentation|D800/FFFD rows retain bids42/2 after exact update.|
|15|handleOrderbook|documentation|Exact registered callback selected once.|
|16|handleCandle|documentation|Valid generated1m key selects callback once.|
|17|handleMarkets|documentation|Symbol passes through adaptation into parent update.|
|18|External subscribeOrderbook|documentation|Transmitted symbol and subsequent key selection.|
|19|External subscribeCandle|documentation|Valid generated-key transmission and callback selection.|
|20|resubscribeAll|documentation|Active subscription key retransmitted on open.|
|21|fillEventUnits|explanation|Symbolic prototype price; unmodified-table surrogate subclaim withdrawn.|
|22|In-process connect|behaviour|Live loaded-flags handle now permits reentrant second seed decision.|
|23|In-process subscribeOrderbook|documentation|Source capability receives exact symbol.|
|24|In-process subscribeCandle|documentation|Source capability receives exact valid options symbol.|
|25|IncomePageFetcher|documentation|Actual public pager retains both identifiers; whole page admission loss named.|
|26|IncomeRangeResult|documentation|Actual returned record retains its identifier.|
|27|identify|documentation|Public dedupe distinguishes both materialized identifiers.|
|28|paginateIncomeRange|scheduling|First admitted fetch starts before polling, with original budget guards.|
|29|MessageHandlersStats|documentation|Current IndexMap/PrototypeScalar record replaces stale carrier description.|
|30|MessageHandlers|documentation|Single-frame/error callback ABI priced as a choice.|

At b7c36aa1e9 the first two newly added native controls failed101 at
`SIGNER native invocation emits before first poll` and
`PAGER native invocation fetches before first poll budget=1 start=1`.
At8864799971 the other three failed101 at
`RESYNC native rejection reaction waits for a scheduler turn`,
`CAP native split surrogate must report representation refusal` and
`PROVIDER native retained loaded flag reread after ticker notification`.
These are native value/effect assertions, not serializer comparisons or compile
failures. Each passed0 after its production repair; delivery measurements bind
the restored controls and required gates to their actual SHA in the local receipt.

The prefix source gate executes the actual signer member, resync declaration and
whole income module. Its six independent pager cases end at invalid NaN start;
it also observes a public two-record dedupe/result. The provider source gate
executes both whole provider classes, including the reentrant flag change,
transmitted configuration, exact subscriptions, updates and delegation.
Diagnostic observability executes the maintained cap/controller/Jotai getter
chain in the types owner's test, ending at retained D83D/index4095/length4121.
Native assertions precede wire/source comparisons in their scoped targets.
Source-only program-observation tests are not claims of native host assembly.

The resync repair initially exposed old one-pump fixture assumptions. Those
tests now explicitly drive the extra continuation turn, retaining their original
snapshot counts, exception metadata, finally follow-up and latch-release checks.
An initial post-repair build failed for an unqualified Poll; qualification fixed
that compile error. Neither interim failure is reported as a passing gate.

## Audited118 after the first30

The registry97 consist of47web entries,47native entries, two objects and native
load. Every module/export pair, descriptor and callable registration was read;
last registry iteration97 is web woofi, frozen global iteration146. The existing
REGISTRY_CHOICE_RESOLUTION.md prices the observed dynamic-export, object-mutation
and continuation costs. Its disclosed paired serializer/field-swap survivor and
later native assertion remain in REGISTRY_CHOICE_GATE_EXITS.md. This slice makes
no new mutation claim for those unchanged97 rows and no factory-linking claim.

The other20CHOSEN rows were inspected individually: isEmptyStubCapability,
BaseExchange.constructor, BaseWebSocket.onMessage, External class/connect/
handleDataMessage/onClose/onError/send/scheduleHeartbeatCheck/stopHeartbeatCheck/
log/dispose, FILL_EVENT_UNIT_VENUES, In-process class/unsubscribeStore/constructor/
dispose/log and pageDirection. Four already had supported rationale;16needed
owner rationale. Constructor/pageDirection bucket punctuation was normalized
separately. These are18note edits, not18new findings or status moves.

The last frozen unbucketed createWebsocket row became an explicit existing FINDING.
Shared exchange core and the transport option carrier jointly own startClosed,
maxEnqueuedMessages, debug and selected custom-constructor support. A real
startClosed repair must retain a dormant closed wrapper/descriptor and defer
dial/construction effects until reopening; adding carrier fields is insufficient.
RWS's outer queue is separate from RelayWebSocket's inner100-frame queue.
Relay/Node branches overwrite the caller constructor as source requires; the
remaining custom-constructor gap concerns branches that retain it. No ordinary
Base.send assertion establishes closed-socket queue behavior. This known gap
remains open, without suppression, relabeling or an invented adapter target.

## Socket-options follow-up

Branch fix/rust-websocket-options merges origin53ee502232b005173d8a6f3f0d7cf94bad7f072c
into accepted9e95085d03 at1b79e7b880a4fadfda261b2df3fe020ece3cae20. The audit
progress remains148/148,0unaudited. This is a partial repair of the already-known
row, not another pass through117CHOSEN or another status move.

The native start_closed option now retains a present CLOSED managed wrapper
without scheduling a receiver, evaluating fresh headers or constructing its
inner connection. Reopening activates the previously resolved route and merged
provider. Outer required-header preflight remains eager; inner relay validation
is deferred. The plain native branch still ignores RWS options. Native options
are cleared for the activated inner dial so a host cannot nest another dormant
wrapper. Existing tuning, relay retry and source outer-reconnect ordering are
retained. maxEnqueuedMessages, debug and custom constructors remain FINDINGs
within this same divergent row. Activation policy/error delivery cite
MANIFEST-SPEC.md#exchange-runtime-callback-choice rather than duplicating it.

The initial control at47c8823ca7 failed101 at
`START_CLOSED native construction schedules no connection work`:1versus0.
Read-only review then exposed a reentrant old-cleanup/new-operation cancellation
gap in the new implementation; its native assertion also failed101 before the
generation-selected cleanup repair. Three compile-valid controls rooted at
b7be48e605 fail101 at the named native dormancy, old-cleanup cancellation and
newer-reentrant-activation assertions, with exact source restoration recorded
outside git. A second read-only review finds no remaining blocker in that scope.
Nine native tests cover the initial closed
state, lazy activation, plain branch, header preflights, pending dial cancellation,
early close, task discard, disposal, reentrant pending/ready replacement and
the retained outer-delay force-close race. Native assertions precede source
comparisons. The live source witness executes the actual createWebsocket member
and whole vendored RWS module. Its branch floor is4and its microtask drain stops
at10turns; a separate two-reopen source probe constructs2sockets while the native
ownership-policy witness constructs1. This is a measured residual, not a parity
claim or a quietly discarded control.

The original disjoint audit outcomes stay101supported/41rationale-documentation/
5repaired/1known-open. Counting any code work now yields6rows with a repair,
including that partially repaired known-open row; do not report6fully resolved
rows. First30 remains5code/25rationale-documentation. New status moves:0.
The direct-manifest census remains209ported/148divergent/0pending/1not-applicable.

## Scope and review

Bounded source reads find the signer wait called by Bybit start, and paging
wrappers in Binance, Bitget, Kraken and WEEX. Reopen resync is shared by sockets
selected as private with source key/privacy guards. External/in-process providers
are selected by BaseExchange external-public-data setup. These are source
inheritance/call edges, not proof that every native venue assembles those hosts.
There is no all25-venue execution claim. Native proofs use the real shared bodies
with injected recording hosts.

Three read-only sub-agent reviews cover callback/lifetime changes, provider
aliasing/decoding and all97 registry rows. They identified the additional inline
resync and live-flags defects, the cap substitution policy conflict, a fixture
borrow and inaccurate log-field rationale; those corrections are included.
No human review gate or production-host claim is introduced.

The frozen9e audit metadata retained158 discovered creators. Three producer bodies
changed here; two pre-existing stale entries were independently traced to
accepted9f1f609c76 and fdbded7a1b (worker post-hook error propagation and weak
watchdog owner dispatch). Only those five producers and reviewed helper hashes
are reconciled. Guard validation remains enabled. The first classification run
without all-features failed101 because its health witness was disabled; that
command is not the mandatory all-features classification lane. Repeating it
with all-features passes0. A separate
Core-watchdog-owner execution witness is not claimed by this metadata review.
The socket-options follow-up independently exports161creators: the prior158
plus three new connection/placeholder producers. One existing creation body
changes, and three coarse websocket dependencies are reconciled after review.
The new activation uses the existing Driver Drop guard and four named native
cancellation witnesses; existing classifications and guard validation remain.
Atff98c6e428 the three required gates and14native tests passed, but classification
failed101 on an incoming whole-file exchange.rs pin. Origin's75e3395a03 adds
synchronous signing helpers; separate read-only review confirms WorkerReplyGuard,
worker_reply and all of owned_tasks.rs remain byte-identical. The two dependent
records reconcile only that pin, preserving their producers, guards and witnesses.
The corrected commit receives fresh gate measurements; ff98 exits do not transfer.

The delivery gates are cargo fmt --check, workspace locked build and workspace
all-target host clippy. Canonical wasm clippy, wiring, layers and routing are
central overseer gates under the updated protocol; no local wasm rerun is made.
No full suite, network exchange probe, Node26 or other architecture is claimed.
Gate output, exact SHA, durations, native controls and tracked-tree checks live
in the local handoff receipt; generated captures are not committed.

At4473004413072f29c60c83eb66fe21d1f34615fe all43scoped Rust tests and
format/build passed, but workspace host clippy failed101 on the new pager
budget's negated partial-order comparison. Its positive-or-NaN spelling retains
the six-case native/source expectation. The follow-up delivery reruns the three
required gates; earlier4473004413 exits do not transfer to that SHA.

## Remote-signer waiter ownership follow-up

FIX-audit row 80 now retains the pending wait through canonical `start_eager`.
On f8110cd172, the compiled `discarded_signer_waiter_keeps_source_subscription_until_completion`
regression failed at `SIGNER_DISCARDED keeps source subscription` (0 versus 1).
Its Node instrument executes the maintained `BaseExchange.waitForRemoteSigner`
method and real TinyEmitter; discarding the Promise keeps its subscription until
availability or disposal, and either path removes it once. The native host and
shared core tests cover both paths, independent waiters and capture release.
The existing guard remains around the runtime-owned continuation. A connected
listener throw still leaves the source promise pending after its settled latch,
with the original listener diagnostic sent to the native host error channel.
This supersedes the caller-owned cancellation choice in the earlier audit table;
no broader host callback or raw thrown-value equivalence is claimed.
