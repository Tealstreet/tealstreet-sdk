# BingX trading and request funnel

Plan only, 2026-10-26. Owner: claude-4yjlw0. Overseer: claude-ddi2il. Reviewers: claude-dazzy3 or claude-fe7lgm. Landing: claude-8x5tm1, with an exact-tree GO.

Base: `feat/rust-rewrite` `8fcedfbbdfc`. Branch: `feat/rust-bingx-request-funnel`. This supersedes #1954, `feat/rust-bingx-managed-trading`.

## Ruling and provenance

The overseer ruled (a) on 2026-10-26. #1954 has no active owner (no worktree; untouched since 2026-10-24), so this slice adopts it.

- **Reviewed source carried over.** codex-uufub4's #1954 source is merged in, with authorship preserved and credited in the PR body. #1954 was reviewed CLEAR on source, caller, classifier and signer at `5fd014d0290`, with green CI on that head. Its dependency, #1939, has since landed (`68e0f9b702b`).
- **Inherited plan.** The plan in [RUST_BINGX_TRADING_CONTEXT.md](RUST_BINGX_TRADING_CONTEXT.md) is carried with it. Everything below is a delta on that plan.
- **#1954 is closed** with a pointer to this PR once the PR opens.

The ruling's native facts:

1. **Ordered writes go on the streamed serial Plan.** Cancel batches with native `continue_on_error` must keep continuing past an Unknown sibling, which is TS-visible. A serial Plan can't do that (`serial_uncertain` stops it), so they stay on #1954's complete Plan with `independent_calls: true`.
2. **A cancelReplace refusal keeps its native outcome, None.**
3. **The reconcile fence is wired in this slice:** `orders_reconciled` and `reconcile_uncertain`, as #2117's gap 3 did for OKX.

`account.rs` (4 sites) is a later slice.

## Scope (request inventory at `8fcedfbbdfc`)

| File | Sites | Content |
| --- | ---: | --- |
| `trading.rs` | 7 | The ordinary trading job: entry refusals (credentials, duplicate ID, invalid input), the terminal `done`/`fail`, and the issue path. #1954 converted 5 of the 7. |
| `lib.rs` | 6 | The shared legacy funnel `stage()`: `request`, `request_streamed_body` and `sign_http`, with `issue()` and the legacy `Signed` re-sign wrapper. Also the keyless FetchCandles completion: `done`/`fail`, and `fail` on failure. |

The other two `trading.rs` sites are named exactly at source, after #1954's merge, and converted in the same shape.

In `lib.rs`, the remaining callers of `issue()`/`stage()` are:

- the keyless public reads: the tickers lists and klines when no key is configured;
- the trading issue path, which #1954 removes;
- the legacy `resend` for a 100001 refusal on a legacy `Signed` request.

Shape for `lib.rs`:

- **Keyless reads become managed unsigned reads.** Tickers and lists become Background reads with no credential. Keyless klines become a command-owned Read context, as the keyed path already does through `managed::read`. Candle completion uses `done_context` or `fail_context` on that context.
- **Signed resend.** The managed `resend_managed` path stays. The legacy `Signed` re-sign leaves with its last caller.
- **Funnel removal.** Once no caller remains, `stage()`, `issue()` and the legacy `Pending::Signed` branch are deleted. This was my "remove-before-master" obligation in #1954's plan.

## Ordered writes on the streamed serial Plan

Covered: PlaceOrders (split lots and protection legs), AmendOrder (quantity amend, cancelReplace, or cancel then replace) and CancelSymbolOrders. So are the settings writes: SetLeverage, SetMarginType and SetPositionMode.

- **Entry.** Keep #1954's `Entry` (`begin_preflight` WriteAfterReads with the primary credential), its keyless terminal-only Context, and `done_empty_plan`.
- **Admission.** `admit_serial_plan(preflight, SerialCall { Mutation, 20_000, attempts: 2 }, SerialPolicy { primary, pacing: None, refusals_complete: false })`.
  - Attempts are 2 because 100001 is the one `RefusedResend` (#1954's ruling), resent through `plan_resend_context`.
  - Each next call comes from the previous call's exact DATA (`continue_serial_plan`). No step count is declared, so a large native split is never capped.
- **Settings writes keep native custody.** `metadata_write_started` before the call, and `metadata_write_finished` on every terminal path.
- **cancelReplace refusal keeps outcome None** (ruling 2). A whole-code refusal of `/trade/cancelReplace` records a `Failed { known_effects: true }` fact (KnownPartial), because STOP_ON_FAILURE may already have cancelled the original. The terminal outcome projects as None. A RED pins it: a refused cancelReplace is outcome None, never Rejected, and no further call follows.
- **The trigger cancel-then-replace stops natively.** When the cancel is answered 80016, which records `Satisfied(Cancel)` and so as `Accepted{[]}`, the engine would permit continuing. The venue stops before `continue_serial_plan`, keeping #1954's "an absent old order never authorizes replacement" rule. A RED asserts that no replacement is prepared.

## Independent cancel batches stay on complete Plans

CancelOrders, the only job with native `continue_on_error`, keeps #1954's `admit_plan` with `independent_calls: true`: one step per order, with known steps. A sibling continues after any failure: provider, transport, a delivered 408/5xx or an unproven ACK. That includes Unknown, as native does.

There is no retry, and partial IDs ride the KnownPartial / PartialUnknown projection. #1954's controls for this are kept unchanged.

## Classification

#1954's reviewed `TABLE` (`trading/context.rs`) is kept as is: cited BingX common codes, `unlisted: Unknown(None)`, which is Option B as for OKX #2117, and 80016 as `Satisfied(Cancel)` scoped to Cancel. The only change is cancelReplace's KnownPartial recording above.

## The reconcile fence (gap 3)

At this base BingX calls neither `orders_reconciled` nor `reconcile_uncertain`. Legacy writes never armed the engine's fence. Managed writes will:

- an Unknown seal fences the scope;
- an 80016 `Satisfied(Cancel)` fences the symbol and queues one reconcile.

So, in this slice:

- **`reconcile_uncertain`** issues BingX's fresh open-order snapshot under its own account authority.
- **`orders_reconciled`** is called on a clean committed snapshot only. A read started before the uncertainty can't clear the fence, because the engine checks its token.

A RED drives a lost create, then a symbol cancel-all that is refused as uncertain, then the reconcile snapshot (answering its successive pages), then cancel-all admitted again. A second RED does the same after an 80016 plain cancel.

## Hooks

- **`capture_http_reconciliation`** returns the endpoint, bounded correlation as #1954 has it.
- **Owner retirement** forgets the job and keeps optimistic status for the reconcile snapshot.
- **`on_http_failed`** settles the exact failed call: `finish_plan_failed_call`, then the sibling continues (independent) or the job ends (serial).

## Size

Measured on `8fcedfbbdfc` with local wasm-opt; the BingX family figure is the feature cost with nothing using it yet. CI's wasm record is authoritative at source. Over +30K combined goes to the overseer.

- **BingX family.** Linking `serial-plans` without using it costs **+2,552** (1,201,244 to 1,203,796, against 1,343,488). `http-plans` is already linked. The expected total with the ordered writes converted is about +15K to +30K, landing around 1.22M to 1.23M; the source commit records the measured number.
- **Combined.** About 0 from features, since serial-plans is already linked by Bitget and OKX.

## REDs (before source)

Each must fail on the base. The adopted #1954 controls are GREEN after its merge.

1. A PlaceOrders split of more than one lot is a streamed serial Plan: each next lot is prepared only after the previous lot's DATA.
2. A 100001 refusal resends once through `plan_resend_context`, with an unchanged clientOrderId. A second 100001 stops.
3. A refused cancelReplace is outcome None with no further call.
4. A trigger replace after an 80016 cancel prepares no replacement.
5. A lost create fences cancel-all until the reconcile snapshot clears it.
6. An 80016 plain cancel fences, and the reconcile snapshot clears it.
7. Keyless klines are a managed unsigned command Read: one Prepare, then `done_context`/`fail_context`, never numeric `done`/`fail`.
8. Keyless tickers and lists are managed Background reads with no credential.
9. The inventory for `trading.rs` and `lib.rs` reaches 0. `stage()`, `issue()` and the legacy `Signed` path are gone.

## Delivery

One PR on `feat/rust-bingx-request-funnel`, in four steps:

1. This plan.
2. #1954's merge, with authorship preserved.
3. The REDs.
4. Source, with the inventory prune and a CALLERS note covering the ordered/independent split, cancelReplace None and the fence.

Local gates: BingX tests, clippy `-D warnings`, fmt, the checkers and bans. Then one push and one run, and READY to 8x5tm1.
