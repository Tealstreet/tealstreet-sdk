### OKX error classification (beta classification, family 3)

Owner: claude-dazzy3. Plan:
[RUST_ERROR_CLASSIFICATION.md](../RUST_ERROR_CLASSIFICATION.md), "Beta classification
(2026-10-28)". Stacked on #2339 (classified notices) and #2336 (the kind).
Applies to `okx`, `okxdemo`, `okxeu` and `okxeudemo`.

Kind deltas callers will see (messages are unchanged). OKX codes are cross-checked
against the ccxt OKX exception map, because the OKX docs page renders its error
tables outside the fetched text.
- **Writes take their kind from the `CODES` table.** The local `rejection()`
  ladder no longer decides the kind; it falls back to a documented status row,
  then `rejected`. Its outcome logic is unchanged, so **no R3 flip**.
  - `authInvalid`, previously `rejected`: 50101, 50103, 50104, 50105.
  - `authPermission`, previously `rejected`: 50030, 50100.
  - `invalidInput`, previously `rejected`: 50016.
  - A native code now wins over the HTTP status. For example, 51008 under a
    429 is `insufficientBalance`; it used to be `rateLimited`.
- **`insufficientBalance`**, previously `rejected`: 51008, 51119, 51127, 51131,
  51502 (amend), 59103, 59108, 59200.
- **`invalidInput`**, previously `rejected`:
  - unknown instrument: 51001;
  - price out of its limit: 51006, 51116, 51122, 51137, 51138;
  - size below the minimum or off the lot size: 51007, 51020, 51100, 51118,
    51120, 51121;
  - market order value or amount over its cap: 51201-51203.

  Tier, risk and state refusals in the 51xxx range (for example 51004) stay
  `rejected`.
- **Account-cycle reads and the account config read** are still
  `request.failed`, now with `kind`, `venueCode`, `venueMessage` and
  `retryAfterMs`, classified from the reply's envelope code or HTTP status
  (401 50111 is `authInvalid`). They are never `auth.rejected`; startup key
  refusals still go through `rest_key_rejection`, as before. Transport loss is
  `network`.
- **Unchanged:**
  - Public discovery keeps the engine's shared startup classification (not the
    table). Making discovery table-driven is a shared startup change for a
    later engine PR.
  - Candles keep OKX's native soft empty result.
  - WS login codes outside 60005/60024/60032 are still `internal`, and there is
    no WS order-reject classification yet (no recorded frames; credentialed).

Check order, for reading an auth error under clock skew (recorded, ruling of
claude-ddi2il):
- **OKX:** checks the key before the timestamp and before the demo environment.
  A bad key with a skewed clock reports 50111, never 50102/50112, and a non-demo
  key on demo reports 50111, never 50101.
- **Bybit:** checks the clock first on writes (10002 before 10003) and the key
  first on GET reads (401 before the clock).
- **Binance Spot:** checks the clock before the key (-1021 before -2014).

Recorded corpus (`tests/fixtures/errors`):
- 11 approved no-credential captures, run once on writer 026ad740861.
- The 50102, 50112 and 50101 samples need a valid key and are on the credentialed
  list.
- The books and ticker bodies replay on the instruments discovery read, which
  shares their envelope.

Deletion prerequisites: none added or cleared.
