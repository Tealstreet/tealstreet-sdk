### Bitget error classification (beta classification, family 4)

Owner: claude-dazzy3. Plan:
[RUST_ERROR_CLASSIFICATION.md](../RUST_ERROR_CLASSIFICATION.md), "Beta classification
(2026-10-28)". Stacked on #2339 (classified notices) and #2336 (the kind).
Applies to `bitget` and `bitgetusdc`.

Kind deltas callers will see (messages are unchanged). Bitget's docs render
their code tables client-side, so the codes are cross-checked against ccxt's
Bitget `exceptions.exact` map (ts/src/bitget.ts):
- InsufficientFunds:
  - 40711 "Insufficient contract account balance";
  - 40712 "Insufficient margin";
  - 40762 "The order amount exceeds the balance";
  - 43012 "Insufficient balance".
- InvalidOrder:
  - 40108 "Wrong order quantity";
  - 40706 "Wrong order price";
  - 40808 "Parameter verification exception size checkBDScale error";
  - 40917 "Stop price for long positions please < mark price";
  - 43011 "The parameter does not meet the specification";
  - 45110 "less than the minimum amount 5 USDT";
  - 45113 "Maximum order value limit triggered";
  - 45122 "Short position stop loss price please > mark price".
- Recorded replies, not ccxt:
  - 40034 "Parameter NOPEUSDT does not exist" and "Parameter BADTYPE does not exist";
  - 400171 "Parameter verification failed k-line time range ...".
- **Explicit write refusals take their kind from the `CODES` table.** The
  status-only ladder no longer decides the kind; it falls back to a documented
  status row, then `rejected`. The outcome stays `rejected`, so **no R3 flip**.
  - A key code on a 400, such as 40037 or 40014, is now `authInvalid` or
    `authPermission`; it was `rejected`.
- **Batch rows keep their `errorCode`.** Each failed `failureList` row passes its
  integer code to the table, so the batch journal names its kind (for example
  43012 is `insufficientBalance`). Before, every row was code-less and
  therefore `rejected`.
  - **This is a per-row outcome change (R3).** A row coded 40010, 40725 or 45001
    (the table's unproven codes) is now `unknown`, where it was `rejected`. With
    an accepted row beside it the command is a partial with outcome `unknown`
    (pinned with 45001); with none it fails `unknown`. That is the safe
    direction: the caller reconciles instead of treating the row as not sent.
  - A row coded 40009 (signature) is `rejected`/`authInvalid` for that row. The
    one fresh resend applies only to a refused request, never to a batch row,
    so the batch is dispatched once (pinned).
  - A refusal code such as 40762 stays `rejected`.
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
