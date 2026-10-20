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

- Plan and three independent current-code audits completed; implementation pending.
