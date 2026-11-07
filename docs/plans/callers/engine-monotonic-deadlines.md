# Engine scheduling and synchronized UTC

The SDK's default Rust transport previously returned `Date.now() + offsetMs`
on every callback. A late host time push could move that clock by months:
startup watches and live request/socket deadlines expired after seconds, and
`startup_elapsed_ms` reported about 162 days. Waiting for one initial sync alone
would not protect live work from subsequent syncs.

The default transport now anchors epoch-shaped time once after clock startup,
then advances it only by `performance.now()` differences. Browser workers wait
up to 30 s for their first page-provided time push; absence refuses clock startup
explicitly instead of anchoring the engine to an unknown machine-clock offset.
The page still owns time polling; no new venue/network requests are added.

Host time pushes call `Engine::sync_host_time(raw_now, utc_now)` through the
WASM host. This updates only the per-engine protocol-clock fallback and signing
revision. A venue's own `observe_protocol_time` remains authoritative. Neither
API changes request ownership, deadlines or retry authority; stale pre-release
signing follows the existing revision checks. The host refuses old WASM packages
without this clock API. Explicit injected/replay transports keep their original
clock contract and receive no SDK clock updates. Native server scheduling is
unchanged. Node SDK engines receive the existing OS-clock refresh through the
same protocol-time notification, with no venue-time polling.

SDK journal bounds and local scheduled-cancel validation retain synchronized
UTC, independently of scheduling time. Frame timestamps remain epoch-shaped;
the existing source-frame clock-offset sampling continues to align frame-fed
fences. The worker timeout measures elapsed time with `performance.now()` at
both ends. The native startup mark is optional; absent, invalid or backward
marks produce no `elapsedMs` field (and hence no `startup_elapsed_ms` tag).

Controls reproduce both directions of a 162-day correction through the default
SDK clock, a late initial worker sync, missing worker sync, disposal, explicit
clocks and host API refusal. A real Binance pending read survives host sync at
12 s; subsequent requests pin synchronized timestamps and independent HMACs.
The native watchdog still emits at 60 real seconds with 60,000 ms elapsed.
No live accounts, orders, probes or deployment are used.
