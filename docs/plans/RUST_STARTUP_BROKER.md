# Cross-venue startup, broker and unknown-cancel repairs

Branch: `feat/rust-startup-broker`, based on `origin/feat/rust-rewrite`.
Scope follows the OKX, BloFin and cross-venue port reviews in `RUST_AUDITS.md`.

## Phases

1. Broker attribution: inventory every registered name/family against current TS
   (including preserved reference implementations), preserve production/proxy
   gating and caller IDs, repair missing fields/headers, add request regressions.
2. Startup: introduce one engine-owned read-backoff/row policy; apply first to
   OKX/BloFin, then audit and migrate all families. Successful catalogue rows
   survive malformed neighbours; logs contain no raw payloads. Blanks become zero
   only where TS does. Start private initialization after markets, independently
   of ticker success. Never retry a trading write or replay stale signatures.
3. Unknown-ID cancellation: send a correctly routed native request when possible,
   otherwise return an explicit command error. Preflight mixed batches so an
   unrouteable ID never becomes silent success. Do not change cancel-all races.
4. Verification/landing: review each phase independently; run engine/venue/host,
   harness/private/public replay and root checks; post correctness reviews on a
   PR into `feat/rust-rewrite`; repair findings and merge only on green CI.
   Strike only repaired audit findings, retaining unrelated subfindings.

## Landing split (Sam's addenda)

Land this work in three sequential PRs so reviews remain scoped:

1. Broker attribution and cancel semantics, including KuCoin symbol-stop safety
   and independent handling of unknown IDs.
2. Public startup resilience and KuCoin authenticated REST independence from
   private socket welcome.
3. Private-row tolerance (retain existing entries for skipped identities),
   `SetTickerSymbols` across every venue, and actual app stop payload request
   tests (all venues; Orderly implementation owned by the order-races lane).
   Read the app's stop form/chart/hotkey/bracket/trailing constructors from
   `~/cs/tealstreet-next/apps/web/src/order/` as the payload source.

Sam's judging rule: lifecycle differences are allowed. Fix only incorrect or
missing/stale production state, money/attribution loss, invisible failure or
broken downstream callers. Pure lifecycle parity changes are out of scope.

## Boundaries and verification

`feat/rust-audit-binance-bybit` owns other Binance/Bybit findings;
`feat/rust-order-races` owns cancel-all races. This branch confines edits in those
families to broker/startup/unknown-ID cancellation and merges incoming landing
changes without stashing/rebasing or changing the landing structure.

Each fix receives deterministic request/failure regressions. Startup tests cover
transport and HTTP-200 envelope failures, capped backoff, fresh retry state,
malformed neighbours, blank numeric fields and private initialization without a
successful ticker seed. Broker tests cover generated/caller IDs and enabled/
disabled attribution; cancel tests cover unknown-only and mixed IDs. Existing
private snapshots retain their stricter all-or-nothing safety contract.

## Progress

- Planning: brief, spec, root/package contracts and port review findings read.
  Fresh branch created; independent broker/cancel implementers and startup
  survey are active with disjoint write responsibilities.
- Broker phase: all 48 registered names inventoried in
  `RUST_BROKER_ATTRIBUTION.md`; missing public factory headers and BloFin gated
  placement attribution repaired. Independent review also caught configured
  KuCoin partner signatures on authenticated futures public reads; repaired and
  covered separately from spot's unsigned public-endpoint exception and
  public-data-only accounts.
- Cancel phase: eight silent-drop planners now reject absent routing context
  before any write. KuCoin sends unknown native IDs, continues mixed batches,
  and preserves stops on symbol cancellation. Independent integration review
  found that the SDK generic cancel-all loop no longer reached KuCoin stops;
  the SDK now sends the explicit account scope, including for an empty store.
  Native cancellation and SDK scope regressions pass.
- Public startup checkpoint: implementation and phase reviews saved on
  `feat/rust-public-startup` at `bfce1c1012`; it is not part of PR 1. Remaining
  review fixes include incomplete Hyperliquid quote contexts and early-private
  valuation when public prices are unavailable. No public phase completion is
  claimed yet.
- Integration: landing merged through `fc809a9d85`. Exact-source full checks
  are running before the broker/cancel PR. Native and wasm Clippy, root
  typecheck (44 tasks) and lint (28 tasks) pass. Harness timing and rebuilt
  wasm checks remain pending; no failed run is treated as a pass.
