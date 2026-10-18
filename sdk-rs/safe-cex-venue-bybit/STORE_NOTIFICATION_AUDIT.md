# Store notification audit

Audited Bybit at `5f36bbf8d3`; no integration merge during the announced freeze.
This is a finding in the shared notification contract, not a venue-local repair.
The shared owner has Binance's witness `097041a8c7`.

## Census and scope

Read all 520 venue rows from 26 manifests. The starting 103 ported rows contain
45 callable declarations (29 const-fn, 11 field-fn, 5 method) and 58 data/type
rows. A TypeScript AST walk resolved all 45 callable declarations, with none
missing and no `store` identifier in any of their bodies. Direct source calls to
`this.store.update` total 35, owned by 17 rows, all pending; there are no direct
`this.store.setSetting` calls in these 26 files. Counts are over the manifest's
source files, not a directory grep. Four journal rows remain pending separately.

The direct update calls belong to linear (13 calls / 7 rows), inverse (11 / 5),
spot (9 / 3), and HyroTrader (2 / 2). Owners are start, private snapshot and
orders/positions/balance fetch orchestration, position/margin mode changes,
linear local reduce-only reconciliation and balance-UPnL synchronization, and
HyroTrader margin-account/private-state reconciliation. None was claimed ported.

A direct-write scan alone would incorrectly report no affected ported rows.
Reviewing the eight ported socket operations finds three indirect writers:

| Previously ported row | Fixed internal path |
| --- | --- |
| BybitPublicWebsocket.subscribe | non-heartbeat send → reconnect |
| BybitPrivateWebsocket#subscribe | non-heartbeat send → reconnect |
| BybitPublicWebsocket#sendOhlcvDiagnosticPayload | non-heartbeat send → reconnect |

With no socket and not already reconnecting, source `BaseWebSocket.send` invokes
async `reconnect` without awaiting it. Its synchronous prefix calls `onClose`,
then `processClose`, then `BaseExchange.syncSocketHealthToStore`, then
`DefaultStore.updateSocketHealth`. The update commits before `notify`; the first
throwing listener stops iteration. The async reconnect promise rejects; **the
send and subscribe callers do not await that rejection**. It also aborts the
remaining close/reconnect work after health publication.

Rust's corresponding `BaseWebSocket::reconnect` → `process_close` →
`WsExchangeParent::sync_socket_health_to_store` → `BaseExchange::publish_socket_health`
uses `Store::update`. `Subscriber` and that Store operation return `()`: they
cannot represent the source rejection. A panic is not a usable replacement,
particularly on wasm. These three rows are now FINDING, not ported.

The two ping helpers are excluded by the source's heartbeat early return before
reconnect. The two connect helpers claim dialing only, explicitly excluding
open/close callback lifecycle; no new lifecycle parity is inferred here.
`emitOhlcvDiagnosticReceive` has no fixed Store-write path. Arbitrary effects of
user diagnostic callbacks are not counted as internal Store operations. The
other 37 ported callables are pure helpers or read-only mapping/routing operations.

The linear, inverse and spot `handleTickersEvent` rows already were divergent.
All call `updateTicker`, which commits then notifies. Their notes now name the
independent notification FINDING: native nonfinite numeric patches repair value
transport, not subscriber failure. No additional status changes for those rows.

## Maintained witness

`node sdk-rs/safe-cex-venue-bybit/scripts/check-store-notification.mjs` extracts
actual source methods for send, reconnect, onClose, processClose, both health
aggregation methods, updateSocketHealth and notify. It checks committed state,
first-listener invocation, skipped second listener, identity of the rejected
reconnect error, and the heartbeat exclusion. It is wired into the ignored live
Node suite run by mandatory `venue-oracle:bybit` in the types CI job.

The fixture assembles only this method slice: it is not a production constructor
or full socket lifecycle witness. Diagnostics, timer startup and socket inventory
are explicit fixture inputs. It does not claim a Rust notification-error test:
the current Rust callback signature cannot return one. The Rust limitation is
verified from the shared contract and concrete call chain above.

Control: replacing the first listener's throw with return exits 1 at the named
`throw skips second subscriber` assertion. The unchanged witness passes. This
pins source behavior for the shared repair; it must not be deleted when the
Rust side becomes fallible.

Result: three ported rows reclassified; **100 ported / 21 divergent / 399 pending**,
plus four journal pending. No production changes and no shared owner files edited.

## Auth row added after the initial sweep

`BybitPrivateWebsocket#auth` is now claimed as divergent, with this same FINDING.
Both local and remote success paths call `BaseWebSocket::send_with_log` in
`src/ws_private/local_auth.rs` (`auth_local` and `auth`). An auth frame is not a
heartbeat, so a no-socket send follows the same health-publication chain above.
The notification ABI cutover must cover these callers through the shared send
path; no venue-local notification operation was added. Counts after this slice:
100 ported / 22 divergent / 398 pending plus four journal pending.

## Error-conversion cutover guard

Production Rust store-write conversion sweep after ff14619b18 found one inferred
conversion: `ws_public::handle_tickers_event` calls `Store::update_ticker` then
converts its error to String. This shared handler implements the three already
divergent linear, inverse and spot ticker rows. The closure now explicitly binds
`safe_cex_store::model_patch::StorePatchError`, so the notification error-type
cutover must fail compilation at this site instead of silently flattening a new
error identity. This does not repair notification semantics or promote a row.

All other production map_err sites were inspected: their inputs are numeric/string
coercion, serde decoding, auth decoding or request-interceptor errors, not direct
Store mutation errors. The indirect socket notification paths above remain in
the audit and are not counted as direct StorePatchError conversions. Orderbook
mutation helpers operate on the local book value, not a notifying Store writer.

### Callee-first repeat sweep

After the broader ruling, enumerated all 27 mutation-shaped method names from
`safe-cex-store/src/contract.rs::Store` (including unit-returning methods,
update_socket_health and record_request_outcome, excluding the update_ids reader).
Searched production Rust method and qualified calls by those callees, excluding
src test modules, then inspected each result's handling rather than searching
for map_err. Exactly one direct Store mutation call exists: the update_ticker
call above. Its result propagates after the now-explicit StorePatchError closure;
there is no other if-let-Err/stringification, inferred conversion, .ok(),
unwrap_or_default or blanket-From propagation on a direct Store mutation result.
The two production Store consumers are inverse_position (read-only ticker lookup)
and ws_public (ticker lookup plus that write). Shared socket-mediated writes
remain separately inventoried above; this is not a claim of zero indirect paths.


## Notification ABI consumption

Local merge 3ff93b1a64 consumes notification branch af44700b07. Earlier audit
paragraphs above record the pre-cutover contract, not the new signatures.
The original compatibility commit 9ef5152a09 left PR 7230 pinned at 6af1bc537c.
After that PR and canonical notification PR 7239 merged, fix/bybit-notification-cutover
was based on integration 9419b340d8 with only the venue migration; market WIP is excluded.

| Site | New propagation boundary |
| --- | --- |
| public/private connect grafana-info | parent.emit Result reaches caller before dial |
| diagnostic send | socket send precedes fallible diagnostic event |
| diagnostic receive | helper returns core ExchangeError; on_message propagates |
| public on_message | track_last_received failure precedes fallback/dispatch |
| reconnect auth-error reporter | failure reaches owned-tail reporter before reconnect |
| reconnect auth-false branch | reconnect Result reaches owned-tail reporter |
| remote auth unavailable reporter | failure precedes false return and frame send |
| ticker update | TickerEventError::Store retains StoreMutationError without projection |

The six WsParent fixture methods now return Result. Test-only pong, disposal and
Store setup calls assert success. The response-owned assertion in
ws_public/tests/exchange_composition.rs is untouched; only disposal calls there
consume the notification Result.

The actual DefaultStore ticker witness checks stable subscriber id, original
cause/response, committed numeric patch and skipped second subscriber. Socket
fixtures check deferred and inline owned tails, actual no-socket reconnect,
DefaultStore health commit and skipped subscriber, and original ExchangeError
metadata. Integration 9af82bfeae closes subscriber-ID loss: shared conversion
appends Store-local IDs inner-to-outer. The actual Store reconnect witness pins
an existing two-ID chain plus the observed notification ID. Subscription handles
are unsubscribe closures, so the witness obtains the ID from a typed Store error.
No venue sidecar or redundant identity box exists. TickerEventError::Store remains
because it preserves all three mutation variants and the original typed cause,
which the ExchangeError projection does not replace.

No row moves to ported. Three send helpers retain a narrower FINDING: core's
send synchronously propagates reconnect failure, while source send continues
and only its unawaited reconnect promise rejects. The Rust witness also pins
that subscription delay is skipped. check-store-notification.mjs retains the
source side. Ticker rows remove only the notification component; their separate
prototype-membership divergence remains. Auth retains its independent causes.

Subscriber-ID consumption control: temporarily removing both shared ID appends
compiled, then failed `reconnect notification identity` and
`fallback reconnect notification identity`; the unmodified core is restored.
The fallback socket witness retains response metadata without an ExchangeError
cause; the recovered witness preserves its existing inner ID chain.


## Reconnect timing observation after notification cutover

The source witness now composes the actual public subscribe, private subscribe,
sendWithLog and diagnostic helper with the existing actual Store/base method chain.
With no socket, not already reconnecting, and a throwing Store subscriber:

| Observer | TypeScript | Rust |
| --- | --- | --- |
| Public subscribe caller, 11 topics | fulfills after two sends and two 10ms delays | first send returns Err; one health publication, no delay or second chunk |
| Private subscribe caller | returns normally, emits its send diagnostic | returns Err before the diagnostic |
| Diagnostic-send caller | returns normally, emits its send diagnostic | returns Err before the diagnostic |
| Failure observer | each unawaited reconnect promise rejects with the subscriber error | direct caller receives ExchangeError with the subscriber ID chain |
| Store subscribers | health commits, first subscriber throws, second skipped | same commit and ordered failure |

These are visible on the covered paths, not hypothetical downstream differences.
The source harness attaches rejection observers immediately without awaiting inside
send; it does not claim a particular browser/Node unhandled-rejection policy.
The Rust direct-call witness pins no detached runtime exception report. The owned
private reconnect-sequence tail is a different caller: its Result goes to the
runtime exception reporter, already covered by the inline/deferred tail tests.
Closed-socket heartbeat remains excluded; successful wire sends are unaffected.

## Per-topic timer boundary inventory

Three rows carry explicit unimplemented FINDING notes. Classifying them divergent
was attempted, but parity rejects each with `divergent requires rs`. They therefore
remain pending with empty rs/layer; no fake target or validator change is used.

| Source call site in bybit.ws-public.ts | Required operation |
| --- | --- |
| clearOhlcvNoDataTimeout:120–124 | remove and cancel precisely the topic's live timeout |
| armOhlcvNoDataTimeout:127–140 | clear prior task before checking handlers; schedule 25s, delete handle before callback, guard disposed/handlers, recover then rearm |
| armExistingOhlcvNoDataTimeouts:143–148 | own-key iteration of populated kline topics, derive publicTrade symbol, arm each |
| onOpen:108 | rearm existing subscriptions |
| listenOHLCV primary/secondary parser:443,500 | reset the topic timer before invoking the candle callback |
| listenOHLCV subscription:522 | arm after subscription and handler registration |
| listenOHLCV cleanup:546 | clear only when final primary handler is removed |

The source class has no timer-specific dispose override: its pending callback sees
isDisposed and returns without recovery or rearm. Core's owned task disposal can
cancel all work, but its public API cannot remove/replace one topic operation.
The separate 100ms subscription-retry timer is not one of these three rows.
No generation guard, local task registry, or invented core API is substituted.

## Production ticker callback consumption

The extracted ticker handler retained `StoreMutationError`, but the production `PublicProtocol::register_handlers` callback still replaced it with a message-only `ExchangeError`. The native-registry regression in `tests/native_host_bybit.rs` now exercises the four actual Bybit-family constructors and the registered ticker callback, asserting the committed update, skipped later subscriber, original error name/response/cause and appended subscriber-ID chain. The callback uses the existing canonical Store-error conversion. `scripts/ticker-event-fixtures.mjs` also asserts that each maintained TS ticker method propagates the exact subscriber error after committing the patch.

## Ticker prototype lookup reconciliation

The earlier inherited-symbol finding is repaired independently of notifications.
The native `ticker_prototype_native` target constructs every public Bybit family
with NativeRuntime, real DefaultStore and production message handlers. Its 192
source cases execute the maintained ticker methods and complete DefaultStore,
including stock Object.prototype members, own shadows, a ticker installed as the
dictionary prototype, malformed prices and a real ticker whose ID is `undefined`.
The committed regression observes no error on master where TS throws TypeError;
the canonical Store lookup makes parsing and subsequent target resolution agree.
The older 1,944-case test now asserts equality for the twelve inherited cases
instead of asserting the buggy skip. This closes ticker-symbol membership only;
onMessage's handler-table prototype dispatch and private onOpen remain separate.
