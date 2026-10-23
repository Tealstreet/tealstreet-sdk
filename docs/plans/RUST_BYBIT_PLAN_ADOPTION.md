# Ordinary Bybit Plan adoption

Child of #1873 (shared Plan core, `docs/plans/RUST_LIVE_PLAN_ADMISSION.md`),
lane claude-dazzy3 for codex-uufub4. Scope comes from the owner proposal
(ordinary Linear, Inverse, Spot and Demo trading). Hyro, RefreshMarkets groups,
history, options and identity stay untouched. Base: reviewed core `0316a7c0bae`,
re-stacked as the core advances, with the actual landing integrated before the
final proof. The preserved checkpoint `5d30986a65e` is reference only; whole
files are never restored from it.

## Current state (inventory at `da09968ed1d`)

Paths are `sdk-rs/venues/bybit/src/`.

- **Dispatch.** Every ordinary write goes `lib.rs` `on_command` → `trading.rs`
  `on_trade_command` (preflight gates, then plan match) → `advance` (serial
  steps; one request per step) → `lib.rs` `send`. `send` is the single physical
  chokepoint: legacy `cx.request` / `cx.sign_http`, no declared timeout (the host
  default is 20 s). Terminal is `trading.rs` `cx.fail`/`cx.done` (legacy numeric).
- **Mode preflight.** One signed `/v5/position/list` read per unresolved symbol,
  for PlaceOrders and AmendOrder only. A mode version change re-runs the whole
  command with no cap. The only version bump is a successful SetPositionMode.
- **ACK decoding.** The `wire.rs` DTOs are lenient serde structs: they accept
  positional arrays and missing fields, and `BatchStatus.code` defaults to 0.
  Create, amend, leverage and trading-stop success responses have no echo or
  target check. Demo local rows publish before any validation, matched to inputs
  by position.
- **Resend.** `lib.rs` `resendable` matches text ("error sign", "recv_window",
  `"retcode":10004`) in raw success bodies and transport error text, and
  re-signs once for every signed request, writes included. Code 10002 is never
  matched by number.
- **Classification.** `outcome_unknown`, `http_unknown`, envelope checks, the
  10001 batch fallback, 110043/110025 not-modified, Trading-Terms codes, and
  the `not_modified` / `already_gone` text, which applies to any code, status or
  transport text.
- **Defect rows still present** (`RUST_ERROR_CLASSIFICATION.md`):
  - #3: create-batch undecodable or with no list → success with zero IDs, no
    count check.
  - #4: a short or missing cancel status list marks every unmatched order
    cancelled.
  - #7: text-matched write resend, including after transport failure.
  - #16: a failure drops accepted IDs (no partial).
- **Legacy-site ratchet.** `trading.rs` has 18 sites, all in scope (one lexical
  false positive, `inputs.request`). `lib.rs` has 14, of which 4 are trading
  (the `send` chokepoint and the category-disabled failure). The hyro, options,
  identity and refresh sites are out of scope.

## Original REDs (committed with this plan, before production edits)

From the checkpoint tree, `tests/trading_variants.rs` gains the target-behaviour
tail and `tests/partial_stops.rs` gains two tests. At this head 36 fail and 84
pass. Every failure is a runtime assertion; none is a compile or fixture error.

| Group | Tests (count) | What fails today |
| --- | --- | --- |
| Text authority | `ordinary_signature_words_*` (3), `ordinary_gateway_already_gone_text_*` | resend or cancellation effects come from words, not codes (#7) |
| Create ACK shape | `managed_single_create_*` (3), `managed_create_batch_*` (3), `managed_native_id_overflow_*` | positional IDs, missing client echo, count mismatch, local publication before validation, proven subset lost (#3, #16) |
| Cancel ACK shape | `managed_cancel_batch_*` (3), `managed_single_cancel_*`, `managed_whole/item_cancel_unresolved_*` (2), `managed_documented_absent_order_codes_*` | missing or short status lists patch the store; already-gone text overrides codes (#4) |
| Refusal and resend authority | `managed_batch_*refusal_*` (4), `managed_single_create_signature_refusal_*` | per-item effects or native IDs still allow fallback or re-sign |
| Amend / TP-SL / no-op | `managed_single_amend_*`, `managed_not_modified_text_*`, `managed_documented_tpsl_noop_code_*`, `managed_tpsl_noop_code_*`, partial-stop (2) | not-modified text seals success; targets are not echo-checked |
| Margin / mode | `managed_margin_*` (2), `managed_mode_read_rejects_positional_rows_*` | malformed or positional reasons and rows are accepted |
| Cancel-all housekeeping | `managed_cancel_all_housekeeping_preserves_live_and_uncaptured_children` | live or uncaptured children are retired |
| Native time controls | `managed_long_plan_*` (2), `managed_mode_redos_*`, `managed_preflight_signature_retry_*` | **compatibility controls.** Today they fail only on the declared `timeout_ms == Some(20_000)`: legacy requests leave the field unset and the host default applies. The timing itself (152 s, 304 s, 133 s, a fresh 20 s retry) already holds. |

Deliberate test rewrite: `tests/dispatch_signer.rs`
`host_failure_eligible_retry_then_provider_refusal_retains_ambiguity` pins a
transport-text resend for an ordinary write. Under this port an ordinary write
never resends on transport text, so that test is rewritten to assert no resend
and an Unknown outcome. Its Hyro twin stays unchanged (see the carve-out below).

## Target design

1. **One `const BYBIT: CodeTable`** (`trading.rs`):
   - unknown: 10000, 10016, 170001, 170007;
   - `RefusedResend`: 10002 (recv window) and 10004 (signature), by code only;
   - `Satisfied(Setting)`: 110043 leverage and 110025 position-mode not
     modified, each with `only: Setting`;
   - `Satisfied(Cancel)`: the documented absent-order codes, `only: Cancel`,
     from the checkpoint's documented-code control; never from text;
   - unlisted codes `Refused(Rejected)`, as today's explicit-retCode refusal;
     Trading Terms only rewrites the message;
   - success `Key::Int(0)`; documented status rows only where cited.
   The `not_modified` / `already_gone` text rules and the transport-text resend
   are deleted for ordinary writes.
2. **Genuine Preflight.** `on_trade_command` runs inside command entry:
   `begin_preflight(WriteAfterReads)` when a mode read is needed, else direct plan
   admission. The mode read uses `preflight_read_context` with a native 20 s
   `FreshAttempt` window and one native signature retry (`classify::read`
   `NativeResend`, never a write grant). A version change redoes the preflight
   under the same owner with no new whole-preflight cap.
3. **Final native Plan.** Once preconditions are current, the existing planner's
   steps become `StepBound`s:
   - serial steps; pacing 1200 ms for create-batch and 1000 ms for cancel-batch;
   - one call per step, each a 20 s `FreshAttempt` with 2 attempts when a
     signature resend is possible;
   - the 10001 single-order fallback declared as `after_refusal`.
   `admit_plan` mints the one immutable bound; there is no 120 s cap. Each slot
   gets `plan_leg_context(_with_intent)`, with intent narrowing before first
   registration: a placement's conflict-cancel → Cancel, trading-stop legs →
   Setting, an amend replacement → Place. Requests keep today's native recipe,
   body and headers.
4. **Strict ACK proofs** (a new `trading/acknowledgement.rs`, written fresh):
   object-only DTOs that reject positional arrays, a missing or null `result`
   and absent IDs.
   - Create: the order ID is present and the client ID echo matches when one was
     sent.
   - Create-batch: the `result.list` / `retExtInfo.list` counts equal the chunk
     size, and each row is matched by client echo, not by position. Rows go
     through `http_write_batch` with the exact expected count.
   - Cancel: each status row echoes its requested ID; a missing or short list is
     Unknown.
   - Amend: the target echo matches.
   - Margin reasons and mode rows: object-shaped only.
   Facts are recorded through `classify_write` + `http_write_fact` (or
   `http_write_batch`) **before** any Store or local publication. A proven
   subset with an unresolved remainder uses #1873's `PartialUnknown`.
   Evidence-only IDs never authorise effects.
5. **Resend.** 10002/10004 refusals take only the `ResendGrant` from
   `http_write_fact`, through `plan_resend_context`, with a fresh signature and
   the original bytes. There is no text or transport resend and no replacement
   or resend after any per-item effect or native ID.
6. **Cancel-all housekeeping** keeps the approved narrow rule: only captured,
   already Closed/Canceled rows, after an exact validated retCode 0, with no ID
   overflow and no malformed or unproven response; never a live or uncaptured
   row. The command stays Unknown, and CALLERS says this proves nothing about
   omitted live orders.
7. **Terminal.** `done_plan` / `fail_plan` (the journal projection) replace the
   numeric `cx.done` / `cx.fail`. Pre-plan validation refusals use the entry
   rejection path.
8. **Ratchet.** Prune only the migrated `trading.rs` / `lib.rs` sites from
   `request-legacy-sites.json`, decrementing per-file counts. Record the
   old and new maps and hashes in the PR body.

**Hyro carve-out.** Hyro writes share `send` and `resend`. They keep the legacy
text resend until Hyro's own adoption, so `resend` stays reachable only for the
Hyro and non-trading signed reads it serves today. Ordinary trading calls bypass
it.

## Phases (one PR, these commits)

1. This plan and the original REDs, independently reviewed.
2. The code table plus the strict ACK module, with no dispatch change yet.
3. Genuine Preflight for the mode read.
4. Plan admission, slots, leg intents, recipes and delivery validation.
5. Fact recording, resend grants, PartialUnknown, cancel-all housekeeping and
   the terminal.
6. Ratchet pruning, CALLERS (final behaviour and NX reconciliation obligations)
   and AUDITS.

Gates: all 36 REDs green; the four native controls green with declared 20 s
windows; the existing ordinary, Hyro and group tests unchanged (rewritten tests
listed); engine + Bybit + Binance focused tests; strict clippy, fmt and bans;
an independent review per phase; fresh CI with replay gates on an integrated
head; no merge without an exclusive window after actual #1873.

## Architecture gate

Is this a per-venue fix of a cross-cutting concern? No. Classification,
resend authority, plan bounds, retry windows and the journal projection are the
shared classifier and Plan core. Bybit supplies a code table, native recipes
and strict native ACK decoding only.
