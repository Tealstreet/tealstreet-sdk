### Bybit error classification (beta classification, family 2)

Owner: claude-dazzy3. Plan:
[RUST_ERROR_CLASSIFICATION.md](../RUST_ERROR_CLASSIFICATION.md), "Beta classification
(2026-10-28)". Stacked on #2339 (classified notices) and #2336 (the kind).
Applies to `bybit`, `bybitspot`, `bybitinverse` and `bybitdemo`. HyroTrader shares the
table.

Kind deltas callers will see (messages are unchanged), with codes from
bybit-exchange.github.io/docs/v5/error:
- **`insufficientBalance`**, previously `rejected`: 110004, 110006, 110007,
  110012, 110014, 110044, 110045, 110047, 110051-110053, and spot 170131.
- **`invalidInput`**, previously `rejected`:
  - price limits: 110003, 110120, 110121;
  - quantity truncated to zero: 110017;
  - triggers on the wrong side: 110092, 110093;
  - notional below the minimum: 110094;
  - invalid symbols: 10029, 181012, 170121;
  - spot size and precision limits: 170124, 170136, 170137, 170140, 170381, 170382.

  Contract-state refusals (110023, 110042, 110137) stay `rejected`.
- **HTTP 403** (Bybit's IP rate-limit ban, per the docs' "Forbidden request")
  now has kind `rateLimited`; it was `authPermission` from the engine's generic
  status rule. The outcome stays `unknown`, so there is **no R3 flip**. `unlisted` stays
  `Refused(Rejected)`, because Bybit documents any non-zero retCode as a failed request.
- **Account snapshot reads** (orders, positions, balance, fill check, account
  info) are still `request.failed`, now with `kind`, `venueCode`, `venueMessage`
  and `retryAfterMs` fields. A rejected key answers GET reads with HTTP 401 and
  an empty body, which is `authInvalid` with no venue code. It is never
  `auth.rejected`.
- **Market refresh:** a coded refusal now carries the table kind (10001 is
  `invalidInput`), where before it was `venueUnavailable` unless rate-limited.
  A discovery pause stays `rateLimited`.

Recorded corpus (`tests/fixtures/errors`, ten live no-credential captures):
- The list was approved by claude-ddi2il before the run, and the run used the
  guarded writer.
- One later fresh-stamp order/create was approved and run separately.
- `order-create-bogus-key` recorded 10002 (recv_window), because its timestamp
  was stamped at file build and was stale when sent. It pins the one-resend path.
- `linear-kline-invalid-interval` is not in the corpus: Bybit accepts
  `interval=7x` with retCode 0 and an empty list, so there is no error to pin.
- The two orderbook bodies replay on the market refresh page, since Bybit reads
  no REST order book.

Not changed in this PR, and why:
- **WS order rejects** keep the raw `rejectReason` text notice. No recorded
  reject frame exists yet; that needs a credentialed capture (ruling R4).
- **The startup credential ladder** (`CREDENTIAL_REJECTIONS`) is unchanged, so
  10005 still does not reject startup, as ruled.

TS that classified wrongly: none new for Bybit. The `auth_permissions` reading of
10005 is a kept policy difference (#2337 fragment).

Deletion prerequisites: none added or cleared.
