# Rust parity sweep, part 2

Branch: `feat/rust-parity-sweep`, from `origin/feat/rust-rewrite`.

Scope: audit every PR #1298–#1324 with `gh pr view`, cross-check the
`RUST_REWRITE.md` roster against current TypeScript, and close supported
capability gaps. Phemex income and definitive client-ID lookup are explicit
targets; extend history beyond a page only where TS supports that today.
The other lane owns Paradex remote signing, Bitrue history/client-ID/hedge/
margin mode, and KuCoin spot. Do not edit those surfaces. Keep the existing
1,258,291-byte per-venue WASM budget.

## Phases and verification

1. Discovery/public data: read the brief/spec and merged PR limitations;
   independently audit TS capability evidence and classify gaps by venue.
   Record specific public/product gaps before implementing them.
2. Private data: close supported account/settings/signing gaps identified by
   discovery, preserving failed-read and stale-snapshot safety.
3. Trading: close supported order/conditional/mode gaps identified by the
   audit, preserving caller-ID preflight and one-write semantics.
4. Queries: implement Phemex documented real-value income using shared
   `paging::RangeWalk` and a direct closed-inclusive client-ID endpoint;
   close TS-supported history/lookup gaps in other merged venues. Refuse
   incomplete/ambiguous results rather than claiming an authoritative miss.
5. Harness/gates: deterministic regressions from documented contracts;
   public/private gates for Binance and every touched venue, bans/fmt/clippy,
   affected native/SDK tests, typecheck/lint, combined host/WASM checks, all CI.
6. Landing: merge latest landing branch, push, PR into `feat/rust-rewrite`,
   post adversarial sub-agent reviews, fix/review to convergence, merge only
   on green exact-head CI, cut a fresh follow-up branch.

Every phase has a sub-agent implementation review and a stable commit.
Full auto: no human checkpoint, no stash/rebase/force push/master changes.
The final PR lists remaining TS-unsupported and live-key-only capabilities
with evidence, plus each gate's counts and CPU/memory/artifact measurements.

## Progress

- Fresh branch created from fetched landing; clean starting worktree.
- Discovery running: independent PR audits #1298–#1310 and #1311–#1324,
  plus independent roster/capability audit. Primary lane inspects Phemex.
- Confirmed Phemex TS income is a base fallback; fills are symbol-scoped,
  latest-page only (limit capped at 200); generic order history is a base
  fallback. Do not advertise unbounded TS history that does not exist.
- Phemex official docs describe real `amountRv`/`balanceRv` ledger rows and
  `/api-data/g-futures/orders/by-order-id` with `clOrdID` for closed-inclusive
  lookup. Implement these explicit user targets without guessing scaled
  numeric execution enums or relabeling derived PnL as venue-reported data.
- Initial cross-review found Binance USD-M and OKX TS definitive lookup
  overrides absent from Rust. Detailed gap census and phase scopes pending
  completion of the independent discovery audits.
