### Bitget error classification (beta classification, family 4)

Owner: claude-dazzy3. Plan:
[RUST_ERROR_CLASSIFICATION.md](../RUST_ERROR_CLASSIFICATION.md), "Beta classification
(2026-10-28)". Stacked on #2339 (classified notices) and #2336 (the kind).
Applies to `bitget` and `bitgetusdc`.

Kind deltas callers will see (messages are unchanged). Bitget's docs render
their code tables client-side, so the codes are cross-checked against ccxt's
Bitget exception map.
- **Explicit write refusals take their kind from the `CODES` table.** The
  status-only ladder no longer decides the kind; it falls back to a documented
  status row, then `rejected`. The outcome stays `rejected`, so **no R3 flip**.
  - A key code on a 400, such as 40037 or 40014, is now `authInvalid` or
    `authPermission`; it was `rejected`.
- **Batch rows keep their `errorCode`.** Each failed `failureList` row passes its
  integer code to the table, so the batch journal names its kind (for example
  43012 is `insufficientBalance`). Before, every row was code-less and
  therefore `rejected`.
- **`insufficientBalance`**, previously `rejected`: 40711, 40712, 40762, 43012.
- **`invalidInput`**, previously `rejected`:
  - 40034 (a parameter value does not exist, such as an unknown symbol);
  - 400171, the six-digit parameter-verification code, listed explicitly;
  - 40108 wrong quantity and 40706 wrong price;
  - 40808 size precision and 43011 parameter spec;
  - 45110 below the minimum and 45113 maximum order value;
  - 40917 and 45122 stop price on the wrong side of the mark.

  40774 (position-mode mismatch) stays `rejected`.
- **Account reads** (balance, positions and open orders after detection) are
  still `request.failed`, now with the kind fields from the reply's code or
  HTTP status. A rejected key is `authInvalid` and never `auth.rejected`; a key
  refusal on the startup probe still ends startup, as before. Transport loss is
  `network`.
- **Candles command:** a venue answer now carries the table kind and code
  (400171 is `invalidInput`; a 500 is `venueUnavailable`). A malformed body or
  transport loss stays untyped, as `caller_adoption` pins.
- **Unchanged:**
  - Public discovery keeps the engine's shared startup retry.
  - WS login keeps its own codes (30011/30012, and the 40018/40024 route refusal
    of #2332).
  - There is no WS order-reject classification yet (no recorded frames;
    credentialed).

Check order (claude-ddi2il ruling): Bitget checks the key before the clock. A bad
key with `ACCESS-TIMESTAMP` 1000 answered 40037, so the clock sample (40008) is on
the credentialed list. This matches OKX; Bybit checks the clock first on writes;
Binance Spot checks the clock before the key.

Recorded corpus (`tests/fixtures/errors`): the 9 approved no-credential captures,
run once on writer 026ad740861.

Deletion prerequisites: none added or cleared.
