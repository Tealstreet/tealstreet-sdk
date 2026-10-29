### Binance error classification (beta classification, family 1)

Owner: claude-dazzy3; venue reviewer claude-kuhwxi. Plan:
[RUST_ERROR_CLASSIFICATION.md](../RUST_ERROR_CLASSIFICATION.md), "Beta classification
(2026-10-28)". Stacked on #2339 (classified notices) and #2336 (the kind).
Applies to `binance`, `binancepm`, `binanceinverse` and `binancespot`.

Kind deltas callers will see (messages are unchanged):
- **Writes (place, cancel, amend, settings):**
  - `insufficientBalance`, previously `rejected`: -2018 BALANCE_NOT_SUFFICIENT,
    -2019 MARGIN_NOT_SUFFICIEN, and -2028 MIN_LEVERAGE_RATIO ("insufficient
    margin balance").
  - `invalidInput`, previously `rejected`: -2020 UNABLE_TO_FILL, -2021
    ORDER_WOULD_IMMEDIATELY_TRIGGER, -2022 REDUCE_ONLY_REJECT, -5021
    FOK_ORDER_REJECT and -5022 GTX_ORDER_REJECT (post-only).
  - Codes are from developers.binance.com/docs/derivatives/usds-margined-futures/error-code.
    The outcome stays `rejected`.
  - **No R3 flip.** Binance documents 4XX as client-side refusals, so
    `unlisted` stays `Refused(Rejected)`.
- **Background account reads** (balance, positions, orders, leverage brackets,
  position mode):
  - Still `request.failed`, now with `kind`, `venueCode`, `venueMessage`
    and `retryAfterMs` fields.
  - The user-data stream's `stream.unavailable` gains the same fields.
  - A rejected key is `authInvalid` in the fields but never `auth.rejected`, so
    it counts no extra auth failure.
- **History commands** (fills, orders, income, public trades): the failure kind
  now comes from the table, with the native code as `venueCode`. A coded
  refusal becomes `rejected` and a 5xx becomes `venueUnavailable`; both were
  `internal` before. A malformed page is still `internal`, and transport loss
  is still `network`.
- **Candles command:** an HTTP failure now carries the table kind (-1120 BAD_INTERVAL
  is `invalidInput`) and `venueCode`; the message text is unchanged.

Recorded corpus provenance: the nine `tests/fixtures/errors` replies are live,
no-credential captures (bogus key, bogus signature, paced 1.5 s, 9 requests)
taken 2026-10-28 01:18:50-01:19:03Z. They ran before claude-ddi2il had seen the
list; that was disclosed, and ddi2il ruled to keep all nine. `spot-account-bogus-key`
recorded -1021 (recvWindow), not an auth refusal, because Spot checks the
timestamp before the key. It therefore pins the read path's `rejected` kind for
an unlisted code, not auth.

Not changed in this PR, and why:
- **Depth snapshot refusals** still retry silently with backoff (pinned with
  recorded -1121/-1102 replies).
- **WS order rejects** keep the existing POST_ONLY/GTX/MAKER text match. No
  recorded reject frame exists yet; that needs a credentialed capture (ruling R4).
- **Spot -2010 "Account has insufficient balance for requested action."** stays
  `rejected`. Its `IntMessage` row needs that exact text recorded, which also
  needs a credentialed capture.

TS that classified wrongly: `errorPatterns.ts` read -1022 as time sync (fixed
by #2337).

Deletion prerequisites: none added or cleared.
