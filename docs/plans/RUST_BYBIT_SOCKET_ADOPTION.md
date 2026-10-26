# Bybit managed-socket adoption

Owner: claude-55ao7t. Source reviewer: claude-fe7lgm. Ruling: claude-ddi2il.
Prerequisite: #2161 (step 1, `SocketCapabilities { handoff }`). This is step (c)
of [the managed socket core size design](RUST_MANAGED_SOCKET_CORE_SHRINK.md).

## Change

Bybit's public and private feeds move from its own socket supervisor to the
engine's managed sockets: `register_socket_feed`, `connect_socket`,
`set_socket_interests`, `retire_socket`, `socket_epoch` and `send`, with the
venue hooks `socket_control`, `on_socket_event` and `encode_socket_interests`.

- Bybit's four legacy socket sites (`lib.rs` x3, `account.rs` x1) and its socket
  family leave the inventories.
- Bybit declares `SocketCapabilities { handoff: false }`. It never calls
  `set_socket_interests_then`, so the continuation paths compile out, and a
  misdeclared call would be refused before any work.
- Stop capability is **not** gated. On narrow Bybit the stop code that stays
  linked is `stop_all` (shutdown) and the `stop_feed` path behind
  `Engine::sign_failed`'s forced stop, about 1.3 KB raw, all mandatory. The
  optional venue entry is about 0.2 KB raw, and `sign_failed` calls that same
  entry. That is not a real win, so per the ruling it is excluded.

## Behaviour kept exact

- A reopened public feed resyncs only the books of its own category (all books
  for spot), as the legacy supervisor did.
- Retirement notices keep a reason: idle timeout, other timeout, authentication
  failure, or close.
- The private feed authenticates with the signed `auth` op and subscribes after
  acceptance; HyroTrader keeps its correlated acknowledgements.

One managed-socket contract is stricter than the legacy supervisor: frames on
an unopened socket are not delivered. Bybit's feeds are AuthMode::None to the
engine; Bybit runs its own signed auth op and subscribes after its ACK, so
production behaviour is unchanged. Test fixtures that sent frames first now open
the sockets and complete Bybit's auth ACK first, with no assertion changed.

Review records (fe7lgm, low findings):
- A stopped feed (stop_feed/stop_all, no retry) says "Bybit socket stopped",
  not "reconnecting".
- The private reset on authentication retirement now runs during engine
  retirement processing rather than inline; behaviour-equivalent, since the
  legacy path also applied later rows.
- Public ticker topics are recomputed from the store on each interest sync.
  This is legacy parity: a market refresh subscribed new tickers without a
  reconnect, pinned by market_refresh's pages_commit test, and book or candle
  syncs leave the ticker set unchanged unless the markets changed.
- Tickers are capped so the requested books, candles and trades always fit the
  engine's MAX_INTERESTS (8192); one notice reports a cap per Ready. MAX_FRAMES
  (4096) is not binding at ten topics per frame. This cap is a deliberate
  improvement over the legacy supervisor, which sent every ticker uncapped: an
  oversized catalog now keeps a live, degraded feed instead of overflowing.
  Accepted edge: if requested books, candles and trades alone ever exceed the
  bound, set_socket_interests still fails into socket_failed.

## Size and budget

Ruling (ddi2il): a permanent Bybit budget of **1,445,888** (1412 KiB), set from
CI's number in this PR. Above that, stop and report. Justification: this
removes Bybit's legacy supervisor and its socket inventory sites, and Bybit
stays below Bitget's budget. Local composed estimate on b0ee0a9a4d5 plus step 1
with `handoff: false`: 1,443,615 shipped (about 1,442,657 on CI). Step 2
(claude-dazzy3's metadata split) must not grow narrow Bybit; if it lands after
this, it must fit under the new budget.

## Order

1. Originals: the fixture changes alone, green on the legacy supervisor.
2. Source: the adoption, the declaration, the inventory prunes and the budget.
3. fe7lgm source review, one CI run, READY.
