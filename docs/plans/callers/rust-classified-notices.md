### Classified notices and code-and-message rows (beta classification PR2b)

Owner: claude-dazzy3. Plan: [RUST_ERROR_CLASSIFICATION.md](../RUST_ERROR_CLASSIFICATION.md),
"Beta classification (2026-10-28)", ruling R2 and the PR2b amendment. Stacked on #2336.

- **Engine notices.**
  - `Cx::notice_error(level, code, &VenueError)` and
    `Cx::retried_failure_error(site, &VenueError)` emit events with the same four
    optional fields as `startup.degraded`: `kind`, `venueCode`, `venueMessage` and
    `retryAfterMs`, each present only when the error has it.
  - An auth kind (`authInvalid`, `authExpired`, `authPermission`) is emitted as
    the existing `auth.rejected` event, which counts one auth failure.
  - No other event name is new or changed.
  - `retried_failure_error` keeps `retried_failure`'s Error-then-Warn cadence.
  - No venue calls these yet; each family PR adopts them for its read failures
    and WS order rejects (the kind rides on `order.rejected`, ruling R2).
- **Engine table rows.** `classify::Key::IntMessage(code, message)` matches one
  integer code with its exact message.
  - Within its intent scope it wins over the code's plain row whatever the
    table order. Any other wording falls through to the plain row, or to
    `unlisted` when there is none, which is today's behaviour.
  - It can never be `Satisfied` or a nonce refusal.
  - A family adopting one must back it with a recorded fixture
    (`error_corpus::assert_message_rows_recorded`).
- **SDK TS.** `ExchangeHealthMonitor.processEvent` reads `event.fields.kind` when
  no `classified` is passed. An `order.rejected` carrying `insufficientBalance` is
  then `insufficient_balance`, and a `request.failed` carrying `rateLimited` is
  `rate_limit`.
- **NX:** consumers of `engine-event` may read the four fields; none is required.

Deletion prerequisites: none added or cleared.
