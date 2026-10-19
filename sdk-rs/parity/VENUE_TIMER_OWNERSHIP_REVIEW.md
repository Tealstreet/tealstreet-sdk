# Venue timer ownership

The strict native replay barrier includes runtime tasks as well as Promise jobs.
A source timer implemented with an unclassified `spawn_local` stays blocking even
at a future sleep. Relabeling arbitrary asynchronous work would hide pending
HTTP, signing and handler continuations. These ports instead call the existing
core timer owners; the raw receipt and barrier algorithms are unchanged.

## Constructed regressions

At master d6dd08e746 (#1247 included), the actual registry constructors reproduce
both public startup failures over real NativeRuntime and loopback HTTP/WebSocket
transports. WOO reports one UNKNOWN/blocking task at data_reads.rs:122; Backpack
reports one at adapter_runtime.rs:79. Both have zero pending Promise work.
The same regressions require zero blocking work before clock advancement, then
observe WOO's funding request at 60 seconds and Backpack's delayed market
subscriptions at two seconds. Disposal suppresses subsequent callbacks. The
terminal transport is the only supplied seam; no receipt exemptions are used.

The existing `LocalDial` test wrapper now forwards `spawn_classified` and
`task_work_started` to its NativeRuntime. This preserves the production receipt
rather than turning all classified core operations into wrapper-local raw tasks.

The independent live-diff owner replayed the preserved WOO 260-frame and Backpack
38,289-frame tapes on ee045514f4. Both complete tape reruns confirm timer liveness;
their remaining semantic and performance findings below remain open. These are
separate from the constructed regressions, not full-parity acceptance.

Backpack disposal fixtures track real owned timeout handles. They retain the
existing assertions for depth cancellation before socket close and the source
retention of the ticker throttle; no cancellation assertion is dropped.

## Independent WOO release replay

The preserved 260-frame bundle was replayed on immutable ee045514f4 with the
original physical/parser/clock/raw-task fences. All four runs delivered 260/260;
terminal unknown/finite/ready/blocking tasks and Promise jobs were zero, with no
HTTP misses or provisional receipt. Both complete repeat graphs passed, as did
cross-engine books and candles.

This is a liveness result, not full parity acceptance: TS emitted 188 events and
native 186 (two additional TS Grafana HTTP events). All 129 ticker Store rows had
own-key-order differences, and 128 handler-stat rows differed. Other independently
checked Store fields, including health/request statistics, matched. The shared-load
CPU/wall ratios were 36.91x/24.17x; no performance acceptance is claimed. The newer
master handler-stat implementation was not in that frozen replay, so these
residuals are routed separately without attributing them to the timer change.
The overseer assigned HTTP telemetry to 0w5ohi, ticker order to 4ri6pb, handler
statistics to wdtab1 and CPU profiling to dwfl8p.

The independent raw receipt and field evidence are named
`woo-timers-ee045514f4-release-strict.json` and its `.field-evidence.json`
companion in the live-diff cache.

## Independent Backpack release replay

The same ee045514f4 RELEASE addon replayed the preserved 38,289-frame tape with
all original fences and 90-second timeouts: 38,288 application messages plus one
physical ping. Complete drain/liveness passed with terminal unknown/finite/ready/
blocking tasks and Promise jobs zero, no errors and no HTTP misses.

Overall semantic parity remains NONDETERMINISTIC: TS emitted 266 outputs and
native 259. Every individual native channel's complete repeat graph passed, but
the combined first Store/book order swapped across repeats. The stable book
difference is TS one empty event versus native a populated 418-bid/295-ask book,
then empty; native's first bid is price 1/amount 0.38647. TS/native Store counts are
252/250, first requestStats.ok 4/0, and HTTP telemetry has three request/response
pairs only in TS. Both candle graphs passed. CPU/wall ratios were 3.385x/1.655x.
These are retained open findings, not timer-source attribution or acceptance;
the independent owner notified the FFI ordering, HTTP telemetry and overseer lanes.

The frozen addon SHA256 is
`7b7557b00a92380e76cc9909268f395f1ca22753802254bd83aff4b7825cfa05`.
The common receipt index is `venue-timers-release-report.txt` and the complete
liveness receipt is `venue-timers-liveness-proof.json` in the live-diff cache.
`backpack-timers-ee045514f4-release-strict.json` has the original identity, field
and channel-repeat evidence companions. No tape bytes or drain rules changed.

## Venue census and direct cutover

The census inspects all 27 venue crates' production timer scheduling, including
sleep calls captured before a spawn and delayed work inside a finite owner.

| Venue | Former timer path | Canonical owner and callback phase |
| --- | --- | --- |
| WOO | data_reads schedule raw interval/timeout loop | BaseExchange.start_interval; Scope.set_timeout; the existing Promise owns each asynchronous callback |
| Backpack | exchange startup subscription delay; public listener retry; ticker/depth throttle; private debounce and timeout | BaseExchange/BaseWebSocket.start_timeout; existing finite callback continuations remain separate |
| BingX | Host.timer raw task | Scope.set_timeout; numeric tickets retain cancellation and remove themselves before callback invocation |
| ApexOmni | Host.schedule raw sleep plus callback future | Scope.set_timeout; start_eager owns the original future after the timer fires, so clearing a fired timeout does not abort its Promise |
| Lighter | socket-global raw setInterval loop | BaseExchange.start_interval; clearTimeout and clearInterval share cancellation while retaining their names |
| MEXC | account binding Host.timer raw task | BaseExchange.start_timeout; retained callback return values and existing source error reporting stay native |
| BloFin | tick sleep inside start_owned_operation; private debounce sleep inside a message operation | BaseExchange/BaseWebSocket.start_timeout; only the invoked asynchronous callback becomes finite work |

HL's old Realm timers are owned by the separate atomic #1264 cutover to canonical
Scope timers. This cohort does not edit that Realm or the shared Promise engine.
Binance's native timeout/tick paths already use reviewed timer registrations;
Bitunix and the remaining native socket/health/rate timers already delegate to
core timer ownership. They are not broadened by a generic classification rule.

Remaining sleeps in awaited REST retry, pagination, order pacing and generic
async loop bodies are operation work, not a timeout API. They continue to block
until their source-visible operation settles. General callback runners remain
finite work; a timer's HTTP/authentication tail never becomes an idle timer merely
because its current input happens to be another sleep.

One-shot callback ownership is released by the existing timer implementation;
interval cancellation is retained by the core operation. Native final teardown
continues through the existing exchange/socket owners. No task body, transport
receipt, parser fence or controlled-clock rule is exempted here.
