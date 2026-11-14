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
The page still owns routine time polling. A detected wall-versus-monotonic
discontinuity also queues a one-off server-time resync, including in browser
workers, to replace the cached server offset after a user adjusts the local
clock. Concurrent resyncs in a realm share the clock's in-flight request.
The resync starts in a microtask so even synchronous Node clock notifications
cannot reenter Rust during a clock read or physical HTTP handoff.

Host time pushes call `Engine::sync_host_time(raw_now, utc_now)` through the
WASM host. This updates only the per-engine protocol-clock fallback and signing
revision. Small host updates preserve a venue's own `observe_protocol_time`
calibration. A change larger than 1,000 ms in the host UTC-minus-scheduling
baseline invalidates that calibration and replaces it with current host UTC.
On every default-transport clock read, wall-clock and monotonic deltas are
compared: a divergence larger than 1,000 ms records a pending UTC correction.
`transport.now()` never calls the engine. An optional host clock-source hook
samples and drains that observation at `EngineHost.invoke`, after the handoff
fence and before entering Rust. A refused correction remains pending for the
next entry; a later observation can replace it. This handles laptop suspend
where the monotonic clock stops, including status-2 HTTP pacing wakes and clock
steps between grouped sends. Scheduling time itself never jumps. The physical
HTTP-group handoff and all its reentry guards are unchanged. Neither
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

Venue UTC audit: history/candle default bounds and retention windows, absolute
order expiries and scheduled-cancel validation, direct REST/WS signing and
protocol timestamp fields use `cx.protocol_now()`. Absolute venue rate-limit
reset/unblock timestamps are subtracted from protocol UTC, then their durations
are scheduled on `cx.now`. This covers every venue crate. `cx.now` remains the
clock for request/socket deadlines, pacing, protective receipt ages, local
reconciliation and opaque correlation IDs (including KuCoin ping/connect IDs).
The existing WOO legacy signing inventory entry is rebound to the changed
explicit `utc` call line (one site before and after; no signing site added).
The now-scalar WOO signer call no longer lexically receives a `Cx`; its old
provenance fingerprint is pruned, with no new request site or allowance.
Coinbase DRB `prepare(now)` only meters credits and local deadlines; it emits no
UTC field. Receipt-age clocks retain scheduling time. Kraken Spot execution
fences/order-update fallback timestamps and the KuCoin Spot missing book
timestamp use protocol UTC so comparison against venue timestamps stays in one
domain. BreakoutProp is untouched.

Controls reproduce both directions of a 162-day correction through the default
SDK clock, a late initial worker sync, missing worker sync, disposal, explicit
clocks and host API refusal. A real Binance pending read survives host sync at
12 s; subsequent requests pin synchronized timestamps and independent HMACs.
A frozen-monotonic two-hour sleep is detected without a host time push; real
Binance signing controls cover both venue-calibrated and fallback clocks, and
history `endTime` uses corrected UTC. The sleep and UTC-bound regressions
were RED on the submitted source before these fixes.
The native watchdog still emits at 60 real seconds with 60,000 ms elapsed.
A second RED-to-GREEN handoff control executes the actual status-2 group wait
continuation under the SDK default clock: wake no longer kills the engine, and
a step between sends no longer fails a released member. The next engine signing
entry sees awake UTC (a stand-in packet/HMAC fixture; real Binance signing is
pinned separately above). Kraken Spot and KuCoin Spot regressions check corrected
fallback time and refusal of a later explicitly stale venue row/page.
No live accounts, orders, probes or deployment are used.
