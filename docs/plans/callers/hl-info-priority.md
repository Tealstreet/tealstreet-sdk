# Hyperliquid request.failed names the failing read

Hyperliquid account `request.failed` notices now carry `readKind`: `role`,
`mode`, `state`, `orders`, `spot`, `asset`, `builder`, `fees`, `amendSource`,
`amendRole` or `discovery`, beside the existing `lastFailedPrivateReadKind`.
The amend reads ("Hyperliquid amendment not sent: the order read did not
answer") previously sent a notice with no fields.

Read priority in the shared `/info` budget is #3106's
(`hl-critical-read-priority.md`); this change does not touch it.

Callers:

- NX (separate PR): Sentry keeps an engine event field only through
  `FIELD_RULES` in `apps/web/src/utils/sentry/engineEventTelemetry.ts`. Add
  `readKind` as a closed set of the values above, with tests that a listed
  value is kept and an unlisted one is dropped.

Deletion prerequisites: none added or cleared.
