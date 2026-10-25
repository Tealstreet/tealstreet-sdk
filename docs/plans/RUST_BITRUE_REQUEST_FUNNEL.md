# Bitrue request funnel and trading writes

Plan CLEAR for B1 (fe7lgm, `06463c218f6`, pins folded); B2 CLEAR in shape, source blocked on fe7lgm's Q1 core seam. 2026-10-26. Owner: claude-4yjlw0 (HMAC lane, overseer assignment). Overseer: claude-ddi2il. Reviewers: claude-dazzy3 or claude-fe7lgm. Landing: claude-8x5tm1, with an exact-tree GO.

Base: `feat/rust-rewrite` `49a5b54f362`. Area plan: [RUST_BITRUE_REQUEST_CONTEXT.md](RUST_BITRUE_REQUEST_CONTEXT.md). This finishes its HTTP, private and trading areas. Precedents: BingX #2159 (complete and serial Plans, the reconcile snapshot), BloFin #2157 (Preflight FIFO, serial Plans, pauses) and OKX #2117 (CodeTable, Option B).

## Scope (inventory at `49a5b54f362`)

| Manifest | File | Sites | What they are |
| --- | --- | ---: | --- |
| request | `lib.rs` | 5 | The one legacy HTTP funnel `issue()` → `cx.request` + `selection.sign`, plus `pending.request` field matches |
| request | `trading.rs` | 8 | `cx.fail` on a dequeue re-plan, terminal `cx.fail`/`cx.done`, plus 5 `matches!(request.step.effect, ..)` matches |
| request | `account.rs` | 5 | Client-order lookup `cx.done`, the FetchLeverage join `cx.done`/`cx.fail`, plus 2 `matches!(request, ..)` matches |
| signing | `sign.rs` | 4 | The test-only local `signature()` HMAC and its vector test |
| signing | `secondary_tests.rs` | 5 | An oracle over the legacy funnel's method-inferred key choice |
| signing | `sign/dispatch.rs` | 2 | The dispatch sign step itself; stays, as for OKX and BingX |
| signing | `lib.rs` | 1 | The secondary-identity validation message; stays (user-visible text) |
| credential | `lib.rs` | 4 | Funnel-side credential selection |

Targets: request 18 → 0. Signing 12 → 3 (dispatch 2, lib 1). Credential lib.rs drops to whatever the managed selection leaves, measured at source.

The matches are not ten dispatchers (area plan). They leave with the pending representation they read: `sign::Request` stops being a field named `request` and the `matches!` bodies bind the step or call instead. Their assertions are kept.

## Phases

One PR per phase, each RED first, then source with the exact prune.

- **B1, reads and sessions.** The funnel for everything except trading: Markets, account Background reads, listen-key session writes, the client-order lookup and the FetchLeverage join. `issue()` keeps only the trading path, still legacy, so the funnel survives B1.
- **B2, trading writes.** All trading on Preflight roots and Plans. `issue()`, `cx.request`, the legacy `selection.sign` and `local_mutation_policy`'s numeric fallbacks are deleted.

Splitting keeps each size and review scoped: B1 links nothing new; B2 links `serial-plans` and `http-plans` (Bitrue has only `order-locks` today).

## B1: reads and sessions

- **Markets.**
  - With keys: `SelectedRead` with `PrimaryRead { Catalog }` (m26ps4's role), as today.
  - Keyless: an unsigned Background Read `http_context_with_egress(.., Egress::Primary)`, keeping the explicit Primary route the funnel sets with `route_request` today. It is never default Public (area plan).
  - Ordered `Content-Type` JSON and `X-Referer: Tealstreet` stay through `request_managed_with_headers` and the recipe's unsigned headers.
- **Account reads** (snapshot, orders, positions, balance): Background `SelectedRead` with `ReadRole::Read`, so the read key is used when one is configured, by role, never by method.
- **Native retries kept.**
  - A Network or Timeout failure on a GET retries up to 3 times under the same root through `SelectedRead::retry(ReadFailure)`. The engine bounds it by the root deadline; the 3-attempt count stays the venue's native counter.
  - Markets keeps no retry (its startup retry stays).
- **-1022.** One re-sign through the same `retry`, only when `codes::resend` proves it from a delivered response (the classifier's status gate, area plan rows 8/22). Never on text or an arbitrary status.
- **Listen key.**
  - POST and keepalive PUT become Session mutations (`SessionAuth`), as the area plan requires.
  - Native retries the POST up to 3 times on a lost response. A Session write gets one attempt per operation, so each native retry is a fresh `SessionAuth` operation context from the same session owner and generation, never a resend under the old attempt (D1, CLEAR). Pins:
    - The next operation opens only after the previous one is finished from a released Network or Timeout failure, so two listen-key POSTs are never in flight.
    - A late response to a finished earlier POST is refused; no key is adopted from it.
    - The 3-count stays the venue counter, and a 4th is never sent.
  - The keepalive PUT keeps native's retry behaviour exactly.
- **Client-order lookup.** The command's entry Read Context is retained with its query. It completes `done_context` with `{found:true, order}` or `{found:false, definitive:false}`, unchanged, from the store or the Orders read.
- **FetchLeverage join** (D2, CLEAR: no group core is needed).
  - The one physical snapshot GET is a Background `SelectedRead` with no command owner, and it publishes to the Store. It is never one owner's Context whose DATA the siblings consume, which would be shared response authority.
  - Each joined command keeps its own entry Read Context in `leverage_queries`. It completes `done_context` from the published Store state, or `fail_context` if that read fails or is refused.
  - An expired owner refuses only itself; each owner carries its own deadline and generation, by the retention the WS FIFO reads use (#1830).
- **Delivery.** Each response validates its exact Context before Store effects (`validate_http_context` / read-root delivery, as Brackets does today).

## B2: trading writes

- **Entry and FIFO** (BloFin #2157 shape).
  - Each trading command opens `begin_preflight(WriteAfterReads, read: the Read selection, write: Primary)` before validation. A validation refusal is `fail_preflight` (NotSent).
  - Native's FIFO (`queue`, `busy`) parks each queued command's own Preflight. The front is woken by `request_preflight_ready` and re-planned at its wake against the current Store, so a dequeue re-plan refusal is `fail_preflight`.
  - SetMarginType and hedged SetPositionMode stay local typed Unsupported refusals, and one-way stays `done_local_setting`. These already use reviewed local-refusal authority; only `local_mutation_policy` is renamed if it stays.
- **Ordered jobs on a streamed serial Plan.**
  - Place (with protections), Amend and SetLeverage. Calls: Inspect GET (Read), Cancel (Mutation), Verify GET (Read), Place (Mutation), Leverage (Mutation).
  - `SerialCall { Mutation, 20 s, attempts: 2 }` for writes, since -1022 is the one `RefusedResend`, through `plan_resend_context`. Reads are `SerialCall { Read, 20 s }`.
  - Each next call comes from the previous call's DATA (`continue_serial_plan`).
  - Replacements keep "requires final cancellation proof". The amend replacement is prepared only after a Verify that proves the cancel, and the execution-progress abort (`filled_native` or a closed target) ends the job before the next call.
- **Native read retries inside a job (D3).** Inspect, Verify and census GETs keep native's 3 retries on Network or Timeout, with no caller-visible change. The serial-Plan rule is per call: a released Read failure may `continue_serial_plan[_after]` to a fresh Read call (the `Stage::Active { failed: true }` branch), and that retry's own released failure may continue again. The venue keeps the 3-attempt counter and chains Read continuations; a 4th is never sent. If an original proves the chain refused, this goes back to fe7lgm.
- **Read calls sign with Primary (recorded caller-visible key change).** Every serial-Plan call signs with the Plan's single admitted credential, Primary for a write Plan. So Inspect, Verify and census GETs, which today use the read key by method, sign with Primary. That is safer. `secondary_tests.rs` is rewritten to pin it.
- **Cancel batches** (CancelOrders and CancelSymbolOrders) keep native independent continuation. Like BingX: a complete Plan with `independent_calls`, one step per target, with no retry.
  - Native skips a child step at dispatch time whenever its Store row is terminal by any route (parent cancel, the stream), including after a sibling's failure. A complete Plan declares its steps up front.
  - **Q1 (fe7lgm owns the core seam, a separate small core PR with paired Bybit, Bitget and combined sizes):** `retire_unissued_plan_call(plan, slot)`.
    - Allowed only on a complete Plan with `independent_calls`, only on the schedule's cursor slot (order preserved), only while that call is unissued (no call, credit not Issued), and only for a live, unexpired, uncancelled owner from a Plan callback.
    - It records no journal effect for that leg, neither Accepted nor Unknown. It releases that slot's reserved attempt credits and advances the cursor, so `done_plan` can complete.
    - It cannot touch an issued call, a serial or Read Plan, or a non-cursor slot.
    - B2 originals are written against this shape now; B2 source waits for the seam to land.
  - Native's `canceledOrderIds` still lists a skipped child it sees as Canceled. That is venue text and never changes kind or outcome.
  - The aggregated message (`canceledOrderIds`, `failedOrders`) stays as native's text. Outcome and kind come from the journal: an Unknown leg is PartialUnknown, replacing `cancel_unknown`.
- **Write facts.**
  - `codes::BITRUE` gains Bitrue's documented pre-execution refusal codes, each cited (API docs URL and revision), plus a unit test that no exact row sits in a range. `unlisted` stays `Unknown(None)` (Option B), so an unlisted refusal never authorizes the next call.
  - Malformed 2xx, a missing order ID and a contradictory ACK are `http_write_unproven` (Unknown). They are never outcome None (area plan rows 8/22).
  - Accepted IDs ride the journal on failure (row 21). This replaces `effective`, `cancel_unknown` and the `trade_sign_failed`/`trade_lost` outcome edits, which are deleted.
- **Protective census.**
  - After a protective placement, native holds the terminal until a fresh complete order census (conditionals included) certifies each protected symbol. A failed census makes the terminal Unknown.
  - **D4 (CLEAR):** the census runs as the job's final serial Read calls (open-order pages, Primary-signed per D3), with the D3 chained retry. `done_plan` follows only a clean census.
  - A failed census terminal is the journal projection: accepted placement IDs preserved (row 21), outcome Unknown, never NotSent or None. It uses `fail_plan[_noted]` with an Unknown error, not a venue-built result.
  - Each census call has the 20 s per-call bound, and there is no aggregate deadline (matching native).
  - The alternative, a Plan held open across a Background census, is rejected: it would leave a root with no call in flight and no bound.
- **Reconcile.**
  - Unknown seals and protective uncertainty clear through `reconcile_uncertain`, which issues the open-order snapshot as `PrimaryRead { PostUnknownOrderRead }` (fe7lgm's P1, as in #2159 and #2157). `cx.orders_reconciled` is called on a clean reconcile snapshot only.
  - `unresolved_protection` stays the venue's protective fence, since an amend is not an engine placement scope (native).
- **Retirement.** `on_http_owner_retired_preflight` plus `capture_http_reconciliation` (endpoint correlation). Creating and Canceling rows are restored at the next callback (#2157). There is exactly one terminal.

## Signing

- The `sign::signature` vector moves to `tests/` as an independent oracle (the checker scans `src/` only). `secondary_tests.rs` is rewritten over managed selection: role-based (`Read` for account reads, `Mutation` for writes), never method-inferred.
- `sign.rs` keeps only `Request` (renamed off `request` with B1).

## Original controls (RED first, per phase)

B1:

1. Keyed and keyless Markets: one managed Prepare with Primary routing, the native static headers, and a `PrimaryRead Catalog` selection when keyed.
2. Account reads use the read key with one configured. A Network failure retries 3 times, then fails; a 4th is never sent.
   - Listen key: never two POSTs in flight; a late response to a finished POST is refused; a 4th is never sent; keepalive retries unchanged (D1).
3. -1022 on a delivered read re-signs once. A second -1022, or -1022 text at an unproven status, does not.
4. Listen POST and keepalive PUT are Session mutations; a lost POST retries natively (D1).
5. The client lookup completes through its Context in both result shapes; a late or expired completion is refused.
6. FetchLeverage: two joined owners on one Background snapshot. Expire the owner that joined first; the later owner still completes, and each owner gets exactly one terminal.

B2:

7. Entry refusal, FIFO parking, a dequeue re-plan refusal and an expired parked command (#2157's set).
8. Amend streams Inspect → Cancel → Verify → Place, and a Verify that does not prove the cancel prepares no replacement. Inspect and Verify chain native's 3 Read retries, never a 4th, and sign with Primary (D3).
9. An execution-progress abort ends the job before the next call.
10. -1022 on a write resends once with the same client order ID; a malformed 2xx is Unknown, never None.
11. A cancel batch continues past an Unknown leg and skips a closed child (Q1), closed either by the parent ACK or by the stream. A skip of the last step still completes `done_plan`. The aggregated text stays, and the outcome is PartialUnknown.
12. A failure after accepted IDs keeps them.
13. Protective census: clean → done; failed → Unknown with the accepted placement IDs (D4).
14. A lost create fences cancel-all until the Primary-signed reconcile snapshot clears it.
15. Retirement or a 503 restores rows and gives one terminal; the next queued command proceeds.
16. The inventory: request 18 → 0, signing 12 → 3.

## Size

Measured at each phase's source on the actual landing tip. CI records the family; the combined number is local and composed. B1 links nothing new. B2 links `serial-plans` and `http-plans`; expect the first-link cost (#2157 saw about +90K family for BloFin) against Bitrue's 1,343,488 budget, and a small combined delta. Over +30K combined goes to the overseer.

## Decisions for review

Resolved by fe7lgm's review:

- **D1, CLEAR with pins:** each listen-key POST retry is a fresh Session operation.
- **D2, CLEAR:** one Background snapshot; per-owner Contexts complete from the Store.
- **D3, corrected:** native's three Read retries are chained per call. The Primary signing of in-job GETs is recorded.
- **D4, CLEAR with pins:** the census is the final serial Read calls, and its failure projects Unknown with the IDs.
- **Q1:** fe7lgm's core seam, `retire_unissued_plan_call`; B2 source waits for it.
