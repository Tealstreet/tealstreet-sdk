# Live TS / napi differential probe

Runs the maintained TS registry class and the compiled Rust `NativeExchange`
account API in separate Node processes. This is a standalone live probe, outside
the deterministic package test runners. It never deploys or writes to the keys
sheet.

Build from `sdk-rs` with bounded Cargo concurrency:

```sh
cargo build --locked -p safe-cex-ffi --features node --jobs 2
mkdir -p safe-cex-ffi/dist/node
# macOS; use libsafe_cex_ffi.so on Linux
cp target/debug/libsafe_cex_ffi.dylib safe-cex-ffi/dist/node/safe_cex_ffi.node
```

From the repository root, with workspace dependencies installed:

```sh
node sdk-rs/scripts/live-diff/run.mjs --venue binance
node sdk-rs/scripts/live-diff/run.mjs --venue bybit
node sdk-rs/scripts/live-diff/run.mjs --venue okx
node sdk-rs/scripts/live-diff/run.mjs --venue hyperliquid
# Capture exact public response bytes for a compiled regression:
node sdk-rs/scripts/live-diff/run.mjs --venue okx --capture-public
# All 27 core venues, in that priority order:
node sdk-rs/scripts/live-diff/sweep.mjs
# Or selected remaining venues:
node sdk-rs/scripts/live-diff/sweep.mjs bitget phemex woo
node --test sdk-rs/scripts/live-diff/*.test.mjs
```

Node, `tsx`, and OpenSSL must be available. Lighter, ApexOmni, and Bitrue also
need a C compiler and macOS or Linux for public WebSocket recording.
The probe creates a short-lived CA
in a private temporary directory and trusts it **only in its child processes**.
Its loopback recording proxies terminate HTTPS and forward to literal approved
exchange REST hosts with normal upstream TLS verification. It changes no OS
trust settings. Neither implementation's production transport is replaced.
Temporary keys and certificates are removed on normal completion.

Each read's first live response is reused for the second implementation, with
the original response bytes and status. The replay key ignores auth freshness
and Bybit's random correlation `requestId`; business parameters remain part of
the key. Raw outgoing request bytes are unchanged. Native reads that miss the
shared cache are labeled `INPUT_DIFF`, so moving prices cannot masquerade as a
mapper defect. Values preserve `NaN`, infinities, negative zero, absent versus
own `undefined`, and BigInts across process IPC. Object field order is ignored
for result comparisons; outgoing placement bytes are compared literally.

In authenticated mode, both implementations' own requests also reach the real
exchange, even on a replay hit. Their live status and response hashes are
recorded separately. A rejected signature or HTTP error is returned directly
instead of the cached success, so response replay cannot hide auth failures.

The report includes response hashes, request hashes, timings, up to 100 exact
value differences per check, and separate `PASS`, `DIFF`, `INPUT_DIFF`, and
`BLOCKED` outcomes. Reports default to ignored `.cache/live-diff/<venue>.json`
and contain no credential values or raw signed requests. Use `--output <path>`
and `--timeout-ms <1000..300000>` to change these defaults. Non-passing checks
exit nonzero. Equal errors and empty public market/ticker results never pass.

`--capture-public` saves original upstream bytes under ignored
`.cache/live-diff/fixtures/<venue>/<response-hash>.json`, with request metadata in
`<response-hash>.request.json`; `requests[].fixture` links them from the report.
Capture runs reject `--row`. To validate a separately built fix, pass
`--addon /absolute/path/safe_cex_ffi.node --rust-revision <commit>`; the report
records the TS checkout revision, supplied Rust artifact revision, and addon
SHA-256 so a separately built artifact is distinguishable from its source label.

Hyperliquid intentionally rejects `fetchMarkets` and `fetchTickers` in both
implementations. Its probe uses public `start` and compares the delivered Store
markets/tickers, with public sockets disabled to preserve the REST snapshot.
Its keyless `userFees` 422/default-fee fallback remains visible in the report
and is accepted only during public startup. Aster's keyless leverage-bracket
401/default-leverage and unsupported bulk open-interest 400 fallbacks are
accepted only in their documented public fetch methods. Their HTTP responses
and error counts remain recorded. Counters are per operation, so an expected
earlier warning does not invalidate unrelated later reads. Polymarket
`POST /midpoints` and DXtrade `POST /marketdata` are classified as reads.

Lighter, ApexOmni and Bitrue hydrate tickers through public startup and their
actual WebSocket handlers. A small temporary `connect` interposer redirects
only their child processes' direct TLS sockets into a loopback recorder. It
does not change the OS routing table, DNS, production code, or global trust.
The TLS recorder admits only each venue's literal public WS host. TS receives
one real upstream stream; the recorder freezes it after live prices appear,
then replays exactly those text/binary messages and arrival delays into the
native socket. Native commands cross an event-loop barrier before ticker
values are inspected. This checks command liveness, not whether unread socket
frames have drained. If synchronous native processing prevents this barrier
from completing, the report records `NATIVE_EVENT_LOOP_STALL` and a timeout,
not a comparison of partially processed ticker values. Zero-price startup
stubs do not count as live ticker coverage. Lighter's real REST fallback is
captured too. `--capture-public` retains `.ws.json` message fixtures and
`.sent.json` subscription traffic, with hashes for both receivers.

The report classifies unavailable data explicitly: Coinbase's zero trading
INTX perpetuals become `NO_TRADING_MARKETS`; TXFlow's maintained TS scaffold
becomes `UNSUPPORTED_BY_TS`; BreakoutProp without a test-account row becomes
`AUTH_REQUIRED`. These are not successful live-data comparisons. See
[`CLASSIFICATION.md`](CLASSIFICATION.md) for the observed blockers and owners.

Authenticated mode reads credentials into memory **only** through
`corepack yarn keys list --json --show-secrets` in `~/cs/tealstreet-next`, as
documented in that repository's `scripts/keys/README.md`. Complete Google login
there first. Choose an explicitly assigned **test account** by sheet row:

```sh
node sdk-rs/scripts/live-diff/run.mjs --venue binance --row <test-row>
node sdk-rs/scripts/live-diff/run.mjs --venue binance --row <test-row> --trade
```

Authenticated checks add account validation, a private reconciliation snapshot,
the resulting Store balance, positions and open orders after public reads.
Trade mode currently supports Binance USD-M.
It requires every read-only check to pass, then sequentially places one
post-only DOGE buy per implementation, 1.5% below the captured bid, approximately
10 USDT notional (at most 15 after sizing). This follows the keys CLI's test
configuration. The same caller order ID and remote signer timestamp are used
for literal placement payload comparison. Normal account startup loads hedge
mode before placement; creating a Binance listen key is allowed during this
phase and each implementation receives its own key. Each order is read back, cancelled,
and confirmed absent before the next implementation runs. Live order reads
bypass previous snapshots. A lost placement response triggers caller-ID
reconciliation before cleanup; an ambiguous lookup or failed cancel must be
investigated on the test account before any rerun. The report records the
caller ID for this purpose. The recorder allows only Binance single-order
POST/DELETE during this phase; all other writes remain blocked.

Private validation and placement on other venues, account routing requiring
Tealstreet proxy/tunnel credentials, and clocks embedded directly in output
records need additional adapters. Such limitations must be reported explicitly;
public parity is not authenticated or order lifecycle proof.

## Public streams: capture once, replay four times

`streams.mjs` uses the actual TS exchange and ExchangeWorker Store delivery,
plus the compiled native account binding. Public host adapters cover Binance
USD-M, Bybit, OKX and Hyperliquid, in that order. No credentials are accepted.

```sh
node sdk-rs/scripts/live-diff/streams.mjs --venue binance \
  --addon /absolute/path/safe_cex_ffi.node --rust-revision ACTUAL_BUILD_SOURCE \
  --output .cache/live-diff/binance-streams-60s.json
node --test sdk-rs/scripts/live-diff/*.test.mjs
```

The capture keeps the startup prelude and **60 seconds after subscriptions**.
Binance market/public and OKX public/business sockets share one ordered tape.
Exact text/binary payloads, control frames, socket occurrences and relative times
are retained. REST response bytes and the fixed candle-seed request range join
the tape in a `.bundle.v8` file under `.cache/live-diff/fixtures/<venue>`.
The child-only TLS hook also covers OKX port 8443 and Binance spot port 9443;
upstream certificates and child SNI still validate normally.

Four fresh accounts (two TS, two native) replay that identical tape. Replay REST
misses fail closed without opening an upstream connection. Reports include frame
receipts, missing indices, actual addon SHA, raw V8 output paths, each side's exact
repeat comparison, and cross-side ordered comparisons for Store slices,
orderbook, candles and each emitted event. The combined sequence also detects
ordering differences across channels. Raw output preserves undefined, NaN, -0
and typed values; nothing is sorted, aligned or coalesced by the probe.

`qualifiedStatus` prevents a changing within-side sequence or incomplete input
from being attributed as a final cross-side bug. Replay clocks and timer deadlines
are controlled; the real production Store throttle still runs at its 250ms cadence.
A command barrier alone is not a processing fence. Per-socket application and
physical transport receipts now gate `nativeDrainVerified`; incomplete receipts
keep comparisons provisional. Failed playback retains diagnostic partial outputs.
Empty book/candle/ticker channels cannot satisfy coverage. `--capture-ms 1000`
is a smoke check only, never the live gate.

To repeat a captured case without new live input, copy the `capture.bundle` path
from its report and pass `--replay /absolute/path/to.bundle.v8 --venue binance`
with the addon and source label. Use the same Node major version for V8 artifacts.
Both sides still instantiate their production transports/parsers/subscriptions.
TS attaches its exchange instance to the existing ExchangeWorker delivery methods
rather than starting a second account; venue startup runs with fixed host clock
inputs before replay timer control begins. Subscription handles in native callback envelopes are stripped only to
match the direct TS callback boundary; event and Store data are unmodified.

There is no standalone public trade listener in the SDK. Bybit's production
candle subscription consumes `publicTrade` frames and emits derived candles;
other venues' candle channels do not establish standalone trade-event coverage.
That capability gap stays explicit in every report.

### Controlled replay and native drain receipts

The replay worker now opens the compiled addon with an explicit `replayEpochMs`
host, using the captured candle bound as its epoch. Both sides receive fixed
startup Date/performance readings. Replay then advances the clock to each exact
captured frame time, including timer deadlines along the way. Each application
frame must reach its per-socket handler completion receipt before the next frame
is sent. Output data is compared unchanged; timestamps are not stripped.

The final receipt checks consumed/processed application counts per physical URL
occurrence and native transport counts for messages, ping, pong and close. Only
matching receipts set `nativeDrainVerified: true`. Ready Promise jobs are included
in the native checkpoint, and outstanding source Promise jobs keep drain false.
TS HTTP completion and returned WebSocket listener Promises are fenced through
their response/continuation turns as well. Startup subscription work settles
under real scheduling before the replay clock is activated. Startup still uses real scheduling, so exact two-run repeatability on
each side remains mandatory for attribution. The trailing Store delivery runs
under the replay clock, through the real production throttle.

Missing/extra frames, unfinished handlers, unread control frames and clock
rollback fail closed. An ack-only instrument tape proves receipt wiring but
provides no ticker/orderbook/candle coverage or 60-second venue result. Full
venue comparisons await the shared Store hot-path repair before being rerun.

The addon bytes are copied before child startup; the recorded SHA identifies the
loaded snapshot even if another lane rebuilds the supplied addon path.
