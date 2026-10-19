# Controlled rate windows in the native fixture

The maintained constructor installs axios-rate-limit with maxRPS5. Its timer
resets a window independently of a pending request. Reconciliation timers fire
at1000ms,3000ms and8250ms. The old fixture let new jobs overtake pending jobs,
so a later-created reset could run before the existing3000ms reconciliation.
The fake HTTP retry clock also advanced time while registering a sleep.

`reconciliation-timeline-source.cjs` executes the maintained schedule, clear,
reconcile, fill-target, fill-pagination and mark-seen members. It uses the actual
installed axios-rate-limit implementation with a controlled timer queue and
Axios adapter; fixed symbol/category lookup and HTTP responses are terminals.
It extracts the source constants, endpoint and warning severity. No timer is
suppressed. The native witness constructs Bybitdemo, places the real reduce-only
order and enters reconciliation through its original timers and signed HTTP.

Both runtimes issue5,10,15,20 requests by1100,2100,3100,4100ms. The second read
starts at request index11, before the first read completes. Consequently the old
assertion of20 requests before that second root was an instant-clock artifact.
A shared FIFO of20 responses also let the two reads consume each other's pages.
The fixture retains that profile and compares its complete request sequence.

The second profile keys responses by cursor. Both reads then traverse all20
pages:40 requests by8100ms, with20 distinct query cursors. The actual source
reports both the pagination error and reconciliation warning after each capped
read; native compares their complete messages, severity and console arity.
The order remains stored at every checkpoint. Existing successful, repeated-
cursor and partial-fill cases retain their exact request and retirement guards.

The harness preserves pending-job order before newly spawned jobs, and its HTTP
retry sleep delegates to the existing pending virtual sleep. Only explicit
virtual-time advancement releases it. Five clean-master runs reproduced the
original5-versus20 assertion; five corrected runs compare the live source traces.
Compiled controls restore the overtaking queue, reduce the real production page
cap to19, and reintroduce clock advancement at registration. Each must fail its
named witness; exact source restoration must pass.

## Venue clock census

The source scan finds26 `sleep_ms` definitions across venue source and tests.
The advancing clocks outside this fixture have no consumer of the shared
window-reset scheduler and retain their existing per-request contracts:

| Clock | Consumer |
| --- | --- |
| Bybit tests/support/api_factory_runtime.rs::RetryClock | api_factory per-request retry delay |
| MEXC tests/client.rs::Runtime | client per-request retry and measured delay |
| Polymarket tests/support/mod.rs::Runtime | API per-request retry delay |
| Hyperliquid tests/api.rs::Clock and PendingClock | Hyperliquid's separate weighted token bucket |
| Hyperliquid tests/rate_encoded.rs::Clock | The same separate token bucket, encoded-request classifier |

Bybit's current window scheduler uses ExchangeRuntime::sleep. That sleep was
already pending; the ordering fault was in this fixture's pump. Its old retry
clock is made pending too, without changing any production scheduler. The
constructor fixture in src/ws_public/tests/exchange_composition.rs already has
pending sleeps and an explicitly advanced reset scheduler; it remains intact.
