# Store notification inventory

Integration inspected: `9419b340d8a6`. This inventory applies to the current
constants and six wire-record implementations, not the pending runtime rows.

Production files are `src/lib.rs`, `src/constants.rs` and `src/types.rs`.
They contain zero Store writes, zero subscriptions, zero ExchangeError
conversions and zero boxed notification errors. Cargo dependencies are
safe-cex-types, safe-cex-num, serde and serde_json; there is no Store or
exchange-core dependency. No ABI migration is required for these mapped rows.

## CLOSED FINDING: core-store-subscriber-identity-projection

At the inspected integration SHA, `safe-cex-exchange/src/runtime.rs` implements
`From<safe_cex_store::StoreMutationError> for ExchangeError`. For a notification
whose opaque cause downcasts to ExchangeError, it returns the original clone,
retaining its name and metadata. Otherwise it constructs a message projection
and copies the subscriber cause's response. Neither path retains the enclosing
StoreNotificationError or its subscriber ID. Patch and InvalidOperation origins
are likewise projected to messages by that conversion.

`git grep store_subscriber_ids 9419b340d8a6 -- sdk-rs` returned no matches.
`git merge-base --is-ancestor ffe81b5652 9419b340d8a6` returned 1.

Closure verified after `git fetch origin feat/safe-cex-rust`, against canonical
integration `9af82bfeaefa3496cd8eea0a29b9ce7d3c1741a7`. The observations above
remain historical facts about `9419b340d8a6`; they no longer describe integration.

At the fetched SHA, `ExchangeErrorMetadata` declares `store_subscriber_ids:
Vec<u64>`. Both branches of the shared StoreMutationError conversion read
`notification.subscriber_id()` and append it. The recovered branch clones the
original error and appends the outer ID to its existing inner IDs; the fallback
branch retains the response and adds the current ID. IDs are Store-local, so
equal IDs from different Stores are not proof of the same subscriber. The
canonical notification tests assert recovered `[17, 42]`, unchanged original
`[17]`, fallback identity and an empty list for non-notification errors.

This closes subscriber-ID loss in canonical integration. It does not claim
retention of the original notification wrapper or repair other message-only
projections. Polymarket has no original-error box to simplify, no local sidecar
and no affected mapped row; its pending runtime rows remain pending.

This documentation follow-up stays local on a separate branch. The reviewed
venue head `8f870b0720b01271b406f359f2d0dc606787aac1` remains pinned, and its
older integration base does not acquire this fix through a documentation edit.
The next runtime slice must consume an integration tree containing the repair.
No core runtime gate was rerun for this source-inspection closure.
