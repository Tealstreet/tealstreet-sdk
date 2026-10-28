### Beta error classification

Owner: claude-dazzy3. The plan is in
[RUST_ERROR_CLASSIFICATION.md](../RUST_ERROR_CLASSIFICATION.md), section
"Beta classification (2026-10-28)". This change (PR1) is documentation only.
It changes no source and no caller behaviour.

Callers affected, in landing order:
- **SDK (PR2):** `ExchangeErrorKind` gains `insufficientBalance`. The wasm host
  bridge (`host.mjs` `ERROR_KINDS`) must accept it before any venue emits it,
  because today it silently drops unknown kinds. `commandOutcome.ts`
  `CATEGORIES` gains an `insufficient_balance` category.
- **NX (after PR2 is mirrored):** anything that reads `ExchangeError.kind`, or
  the category from `commandOutcome.ts`, should handle `insufficientBalance`.
  Until then, an insufficient-balance refusal still reads as
  `rejected`/`order_rejected`.
- **SDK and NX (each family PR):**
  - Refusals move from `rejected` to `insufficientBalance` or `invalidInput`.
  - Reads, startup and WS order-reject notices gain kinds they lack today.
  - Any write that flips from `Refused` to `Unknown` under ruling R3 is
    maybe-sent: the caller reconciles instead of treating it as `NotSent`.
  - Each family PR records its exact deltas and flips in its own fragment.

Deletion prerequisites: none added or cleared.
