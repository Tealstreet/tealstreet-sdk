### Insufficient-balance error kind (beta classification PR2)

Owner: claude-dazzy3. Plan: [RUST_ERROR_CLASSIFICATION.md](../RUST_ERROR_CLASSIFICATION.md),
"Beta classification (2026-10-28)", ruling R1.

This change adds `ErrorKind::InsufficientBalance` (wire `insufficientBalance`) and
carries it to every consumer. No venue emits it yet, so caller behaviour is
unchanged until a family PR re-points its balance rows.

- **SDK:**
  - The wasm host (`host.mjs` `ERROR_KINDS`, `host.d.mts`) now keeps the kind.
    Before this change it dropped unknown kinds silently.
  - `ExchangeErrorKind` gains `insufficientBalance`, and `ErrorCategory` gains
    `insufficient_balance`.
  - `exchangeErrorCategory` and the Sentry tags map the kind to that category.
  - `ExchangeHealthMonitor` treats it as a specific kind, so it wins over text
    patterns.
  - The CLI's `'error'` listener now passes the event's `opts.classified` to
    `processMessage`, which before classified CLI errors by text only. This
    applies to every typed kind, not only the new one.
- **NX (after mirroring):**
  - Exhaustive `switch`es or `Record<ErrorCategory, ...>` maps over these unions
    must add the new member.
  - Order-rejection toasts and filters that key on `order_rejected` should also
    accept `insufficient_balance`, which is the category for a refused order
    from a family that has adopted the kind.
- **Native hosts:** none. Serde emits the new camelCase name.

Deletion prerequisites: none added or cleared.
