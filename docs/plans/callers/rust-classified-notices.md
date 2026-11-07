### Classified notices and code-and-message rows (beta classification PR2b)

Owner: claude-dazzy3. Plan: [RUST_ERROR_CLASSIFICATION.md](../RUST_ERROR_CLASSIFICATION.md),
"Beta classification (2026-10-28)", ruling R2 and the PR2b amendment. Stacked on #2336.

- **Engine notices.**
  - `Cx::notice_error(level, code, &VenueError)` and
    `Cx::retried_failure_error(site, &VenueError)` emit the caller's code
    (`retried_failure_error` always `request.failed`). They carry the same four
    optional fields as `startup.degraded`: `kind`, `venueCode`, `venueMessage` and
    `retryAfterMs`, each present only when the error has it.
  - An auth kind is never promoted to `auth.rejected`. That event counts an auth
    failure and slows account polling, and account auth evidence is
    `AccountAuth` (as Bitunix's `recovery.rs` already does). No event name is
    new or changed.
  - `retried_failure_error` keeps `retried_failure`'s Error-then-Warn cadence.
  - No venue calls these yet; each family PR adopts them for read failures and
    WS order rejects (the kind rides on `order.rejected`, ruling R2).
- **Engine table rows.** `classify::Key::IntMessage(code, message)` matches one
  integer code with its exact message.
  - It wins over that code's plain row within the same intent scope, whatever
    the table order. Precedence stays per scope, so an intent-scoped plain row
    still beats an unscoped code-and-message row. A code-and-message row must
    therefore carry its plain row's scope (for example Spot -2010 under
    `Place`); `assert_table` refuses an unscoped one beneath a scoped plain row.
  - Any other wording falls through to the plain row, or to `unlisted`, which is
    today's behaviour.
  - It can never be `Satisfied` or a nonce refusal, and every such row needs a
    recorded fixture with its exact text
    (`error_corpus::assert_message_rows_recorded`).
- **SDK TS.** `ExchangeHealthMonitor.processEvent` reads `event.fields.kind` when
  no `classified` is passed. An auth kind there counts only on `auth.rejected`,
  so a read or order refusal never latches an auth category.
  - **Behaviour change at landing:** events that already carry a `kind` field
    are now typed. Bitunix's `request.failed` with a non-auth kind becomes, for
    example, `rate_limit` instead of `unknown`, and `startup.degraded` becomes
    `exchange_down` or `rate_limit` instead of `unknown`. Bitunix auth kinds on
    `request.failed` stay `unknown`.
- **NX:** consumers of `engine-event` may read the four fields; none is required.

Deletion prerequisites: none added or cleared.
