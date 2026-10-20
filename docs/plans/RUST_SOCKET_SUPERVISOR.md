# Socket supervisor audit fixes

Lane: `feat/rust-socket-supervisor`, based on `bd94abb524` on
`origin/feat/rust-rewrite`. Scope: socket authentication/liveness and the
shared supervisor; no TS venue deletion or unrelated trading fixes.

Production impact is the judging rule: silent/stale account or market data,
unseen failures, lost money/attribution or broken callers. Internal startup,
cleanup, disposal and retry choreography may differ freely from TS. The lane
does not treat cosmetic lifecycle differences as defects or require a TS-shaped
supervisor; native authentication/topic/heartbeat constraints remain necessary
to receive correct live data.

## Reverification

The rejected-login findings still hold for Bitget, BloFin, MEXC, WOO,
Crypto.com and ApeX Omni. Heartbeats can keep these unauthenticated sockets
alive indefinitely. MEXC/WOO also lack a pre-open deadline.

The audit's collective "no backoff" wording is too broad: AsterDEX, Bitrue,
BingX and Paradex public already use exponential delays. Their private paths
use fixed delays, and resetting attempts at transport open defeats protection
against repeated open/fail cycles. The remaining named paths use fixed retry.
KuCoin already re-POSTs its bullet token on reconnect; preserve that behavior
and remove immediate token-retry bursts.

## Phases

1. Add `engine::socket::Supervisor`: deterministic jittered exponential retry,
   pre-open deadline, independent authentication deadline, receive watchdog,
   protocol ping hook and stale-timer cancellation. Unit/integration tests
   cover timeout/heartbeat separation, retry deduplication, cap and stable
   session reset. Audit the API with the three venue reviewers.
2. Migrate rejected-login paths (Bitget, BloFin, MEXC, WOO, ApeX Omni) with
   native failure regressions. Preserve ApeX's immediate subscriptions: its
   proxy does not reliably expose login ACKs, so a validated account payload
   may establish readiness. Reject unrelated failures independently.
3. Migrate AsterDEX, Bitrue, BingX, KuCoin and Crypto.com, then Phemex public,
   Coinbase public, GRVT, Paradex and Polymarket. Each connection owns a
   supervisor. Keep native mandatory-pong deadlines, feed identity, token
   routing, generation fences, resync state and terminal GRVT auth policy.
   Add a reproducing regression for every corrected path and review each
   bounded venue group with a different sub-agent.
4. Run all brief gates for Binance and touched venues, including native
   tests/lints, WASM, public recording/replay and private replay. Record exact
   evidence and limitations. Update/strike only the fixed audit items.
5. Push; open PR into `feat/rust-rewrite`; publish adversarial reviews and
   repair findings. Merge only the reviewed head with green CI, then create
   a clean follow-up branch from the landing branch.

## Shared contract

The supervisor owns timers and retry policy. Venues retain socket IDs,
generation fences, authentication parsing, subscription/recovery and wire
ping formatting. A new connection starts its deadline before transport I/O;
an open connection transitions to authentication or ready. Inbound heartbeats
never extend the authentication deadline. Failure cancels active supervision
and schedules one retry; canceled timers cannot revive a retired connection.
Transport open alone never resets retry attempts; validated activity after a
stable ready interval does. Jitter derives from replayable clock/IO identity,
never credentials or hidden host randomness.

Authentication rejection retires ownership before closing, preserves proven
store data and backs off. Server-only heartbeat protocols (Coinbase, Paradex,
Crypto.com) retain their native behavior; no application ping is invented.
KuCoin and Polymarket retain explicit pong requirements even amid other data.

## Progress

- Plan and three independent current-code audits completed.
- Phase 1: `ca0639d13b` adds the shared policy. Ten new regressions and strict
  engine Clippy pass (the first nine plus a review-discovered fractional-clock
  regression). The initial full engine suite passed 88 tests in both default
  and product-semantics/options feature configurations.
- Phase-1 review caught subtraction/addition rounding disagreement at a
  fractional timeout, causing zero-delay timers. Expiry now compares the same
  absolute deadline used when scheduling. The reproducer includes
  `10731.000000000002 + 10000`, whose subtraction rounds below 10000.
- Phase 2: `d4726feebf` migrates five private auth paths, with 519 native
  tests passing (24 new regressions), formatting and strict Clippy. Peer review
  found ACK-less wallet-only ApeX readiness and a Bitget write-recovery timer
  bypassing retry; both have RED-before/GREEN-after native regressions and an
  independent passing rereview. TS has the same rejected-login liveness holes;
  `KNOWN_TS_FIXES` records them without replay-output normalization.
- Phase 3: `f0927afde2` migrates AsterDEX/Bitrue/BingX/KuCoin/Crypto.com
  (415 native tests); `9634dba18a` migrates Phemex/Coinbase public, all GRVT
  feeds, Paradex and Polymarket (530 native tests). Both groups passed scoped
  formatting, strict Clippy and independent correctness reviews. Focused
  baseline runs reproduce the original flap/token/auth/stale-keepalive defects.
- `1e5ba12c51` merges landing `1b8592dbd2` (PR #1338); the sole conflict was
  independent tests appended to KuCoin's public suite, both retained. Landing
  introduced KuCoin Spot with fixed reconnect; it is now included in phase 3.
- Bundle investigation before that merge: Paradex's first shared-policy build
  was 1,260,249 bytes against the unchanged 1,258,291 budget. Constructor
  constant folding removes redundant validation code; exact power-of-two
  backoff scaling uses an integer shift. Measurement/verification continues
  against the merged landing rather than extrapolating from old builds.
- Fresh 20-second Paradex public recording succeeded: 26,674 frames, two HTTP
  responses and both requested books. Performance/replay gates remain pending;
  no live private connectivity is claimed.
- KuCoin Spot extension: `07fb8c0295`; default 105 tests and Spot-enabled 158
  tests pass (overlapping suites, not additive), with six new regressions
  exercising both roles. Formatting, strict Clippy and independent review pass.
- `d106389e12` preserves constant-policy constructor folding and exact integer
  power-of-two retry scaling. On merged landing, Paradex is 1,252,515 bytes
  against the unchanged 1,258,291 budget (5,776-byte headroom); no further size
  refactoring was necessary. Engine default 93/all-feature 99 tests pass.
- Merged root TypeScript checks pass: typecheck 44 tasks, lint 28 tasks and
  tests 27 tasks. Workspace native tests pass (2,829 tests); strict workspace
  native and WASM Clippy pass. Public recording/replay verification continues.
- Gate-plan review covers 21 frozen public profile controls, 23 fresh public
  profile gates from 21 captures, 29 generated private correctness cases and
  17 single-feature size selectors. Before any CPU measurement, the initial
  six-second fresh trims were superseded by a complete-capture policy: retain
  all recorded timestamps (ceil of the raw maximum timestamp, at least 20s),
  with every HTTP anchor. This retains native heartbeat/watchdog opportunities.
  Frozen fixtures, metrics, thresholds and output normalizers are unchanged.

## Public alias discovery and coverage limitation

On 2026-10-20, Bitget USDC's live contracts/tickers returned 49 native symbols,
including `BTCPERP` and `ETHPERP`. Rust and TS both retain those native IDs.
The initial `BTCUSDC,ETHUSDC` book selection was rejected by the venue; the
recording matrix now uses `BTCPERP,ETHPERP`. Corrected capture and replay must
pass before claiming USDC public-book or performance coverage.

Classic inverse contracts returned HTTP 200/code `00000` with an empty catalog
for both `coin-futures` and `COIN-FUTURES`; USDC also returned identical data
for lowercase/uppercase queries. Query casing is therefore not the cause.
The inverse capture's `BTCUSD,ETHUSD` book requests were rejected. Its two
remaining ticker rows, `BTCUSDU26` and `ETHUSDU26`, have null bid/ask prices,
zero volume and a delivery timestamp before capture; they do not establish
available perpetual books. Bitget's [official classic coin-margined delisting
announcement](https://www.bitget.com/support/articles/12560603893788) schedules
BTCUSD/ETHUSD and seven other classic perpetuals for delisting on September 17,
2026 at 15:00 UTC+8, distinguishing them from UTA products.

Fresh classic-inverse public-book replay/performance coverage is unavailable
against this empty live catalog. The rejected zero-book capture is discovery
evidence, not a passing gate; no replacement public fixture is fabricated.
Keep the Rust/TS inverse implementations and native/generated-private coverage;
this limitation neither retires the product nor adds UTA inverse routing.
Frozen inputs, thresholds and normalizers remain unchanged. Investigation
includes the original captures and four independent official public HTTP
responses; no authenticated live connectivity is claimed.

## Remaining migration inventory

These retain their current venue-owned supervision for a follow-up; this lane
does not claim that every venue now uses the shared module:

- Public ApeX Omni, Bitget, BloFin, MEXC and WOO.
- Private Phemex and Coinbase.
- Binance (all products), Bybit (all products), Backpack, Hyperliquid (including
  spot), Kraken (including spot), Lighter (including Robinhood), OKX, Orderly
  brokers, TxFlow and WEEX.

BreakoutProp uses polling rather than a venue WebSocket and needs no socket
supervisor migration. Native mandatory-pong/subscription/session state remains
venue-owned even on migrated feeds.
