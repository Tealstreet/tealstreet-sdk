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

The existing LocalDial test wrapper now forwards spawn_classified and
 task_work_started to its NativeRuntime. This preserves the production receipt
rather than turning all classified core operations into wrapper-local raw tasks.

The independent live-diff owner has pinned ee045514f4 for the preserved WOO
260-frame and Backpack 38,289-frame tape reruns. Those tape results are separate
from the constructed regressions; this review does not claim their parity or CPU
ratio before their terminal receipts exist.

## Venue census and direct cutover

The census inspects every venue crate's production timer scheduling, including
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
