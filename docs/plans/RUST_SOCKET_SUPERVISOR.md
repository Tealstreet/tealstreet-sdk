# Socket supervisor audit fixes

Lane: `feat/rust-socket-supervisor`, based on `bd94abb524` on
`origin/feat/rust-rewrite`. Scope: socket authentication/liveness and the
shared supervisor; no TS venue deletion or unrelated trading fixes.

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
