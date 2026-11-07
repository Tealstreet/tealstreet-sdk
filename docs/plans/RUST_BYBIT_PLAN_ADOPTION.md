# Ordinary Bybit Plan adoption

Child of #1873 (shared Plan core, `docs/plans/RUST_LIVE_PLAN_ADMISSION.md`),
lane claude-dazzy3 for codex-uufub4. Scope comes from the owner proposal
(ordinary Linear, Inverse, Spot and Demo trading). Hyro, RefreshMarkets groups,
history, options and identity stay untouched. Base: reviewed core `0316a7c0bae`,
re-stacked as the core advances, with the actual landing integrated before the
final proof. The preserved checkpoint `5d30986a65e` is reference only; whole
files are never restored from it.

## Current state (Bybit source identical at `da09968ed1d` and core `0316a7c0bae`)

Paths are `sdk-rs/venues/bybit/src/`.

- **Dispatch.** Every ordinary write goes `lib.rs` `on_command` → `trading.rs`
  `on_trade_command` (preflight gates, then plan match) → `advance` (serial
  steps; one request per step) → `lib.rs` `send`. `send` is the single physical
  chokepoint: legacy `cx.request` / `cx.sign_http`, no declared timeout (the host
  default is 20 s). Terminal is `trading.rs` `cx.fail`/`cx.done` (legacy numeric).
- **Mode preflight.** One signed `/v5/position/list` read per unresolved symbol,
  for PlaceOrders and AmendOrder only. A mode version change re-runs the whole
  command with no cap. The version is bumped by a successful SetPositionMode,
  including its 110025 not-modified answer.
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
  10001 batch fallback, 110043/110025 not-modified (by code), Trading-Terms
  codes, and the `not_modified` / `already_gone` text, which applies to any code,
  status or transport text. A non-JSON body becomes the HTTP status as `code`
  (#36).
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
| Native time controls | `managed_long_plan_*` (2), `managed_mode_redos_*`, `managed_preflight_signature_retry_*` | **compatibility controls.** They fail first at the declared `timeout_ms == Some(20_000)`: legacy requests leave the field unset and the host default applies. In a scratch copy with only that expectation relaxed, all four pass today (152 s, 304 s, 133 s, a fresh retry). The harness `advance` fires engine timers but never simulates a host timeout, so this shows sequencing, not the physical cap; the port must prove the declared window through real Prepare/Bind/Claim. |

Multi-case REDs stop at their first failing case, so later cases are not yet
verified against today's code.

Deliberate test rewrite: `tests/dispatch_signer.rs`
`host_failure_eligible_retry_then_provider_refusal_retains_ambiguity` pins a
transport-text resend for an ordinary write. Under this port an ordinary write
never resends on transport text, so that test is rewritten to assert no resend
and an Unknown outcome. Its Hyro twin stays unchanged (see the carve-out below).

## Target design

1. **One `const BYBIT: CodeTable`** (`trading.rs`):
   - unknown: 10000, 10016, 170001, 170007;
   - `RefusedResend(AuthInvalid)`: 10002 (recv window) and 10004 (signature),
     by code only; 408/5xx are decided before the table, so neither can resend
     an unproven write. The 10002 `server_timestamp` parse stays, as clock
     evidence only (`observe_protocol_time`), never as resend authority;
   - kinds: 10006 `Refused(RateLimited)`; 10003/10005/10010/33004 auth kinds,
     aligned with the startup credential table;
   - `Satisfied(Setting)`: 110043 leverage and 110025 position-mode not
     modified, each with `only: Setting`;
   - `Satisfied(Cancel)`: the documented absent-order codes, `only: Cancel`,
     from the checkpoint's documented-code control; never from text;
   - unlisted codes `Refused(Rejected)`, as today's explicit-retCode refusal;
     Trading Terms only rewrites the message;
   - success `Key::Int(0)`; documented status rows only where cited, so a
     non-JSON gateway answer no longer becomes a native code (#36);
   - 34040 TP/SL not-modified and the absent-order codes 110001 (derivatives)
     and 170213 (spot) are decisions D1/D2 below.
   The `not_modified` / `already_gone` text rules and the transport-text resend
   are deleted for ordinary writes.
   A whole refusal (10001/10004 and so on) that arrives with any native ID or
   item row is contradictory evidence, so the codec weakens it with
   `Fact::unproven().with_order_ids(..)`: Unknown, no grant, no fallback. A
   cancel-all answered with a whole refusal code likewise names no target,
   so it is unproven, never Satisfied.
2. **Genuine Preflight.** Every write path starts in the genuine command entry
   with `begin_preflight(WriteAfterReads)` and only then `admit_plan`. This
   includes paths with no mode read: there is no Context-free or direct Plan
   issuer. When a mode read is needed it uses `preflight_read_context` with a native
   20 s `FreshAttempt` window. The `PreflightPolicy` captures both the read
   credential (the GET is signed with the read key) and the write credential.
   Its one native signature retry goes through `preflight_retry_context` under
   `RetryPolicy::DefinitiveRefusal`, only on `classify::read`'s `NativeResend`
   (10002/10004). A read failure, 5xx or transport loss does not retry, and no
   write grant is involved. A version change redoes the preflight
   under the same owner with no new whole-preflight cap.
3. **Final native Plan.** Once preconditions are current, the existing planner's
   steps become `StepBound`s:
   - serial steps; pacing 1200 ms for create-batch and 1000 ms for cancel-batch;
   - one call per step, each a 20 s `FreshAttempt` with 2 attempts when a
     signature resend is possible;
   - the 10001 single-order fallback declared as `after_refusal` and taken
     through `replace_plan_call` only on an effects-free whole 10001 refusal.
   `admit_plan` mints the one immutable bound; there is no 120 s cap. Each slot
   gets `plan_leg_context(_with_intent)`, with intent narrowing before first
   registration: a placement's conflict-cancel → Cancel; a placement's
   trading-stop legs → Setting, because they return no order ID; an amend's
   order-create replacement → Place. A partial-stop replacement through
   `/v5/position/trading-stop` stays Amend: as a Place, its ID-less success would
   become Unknown. Requests keep today's native recipe,
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
   already Closed/Canceled rows, after an exact validated retCode 0 whose
   affected list decoded and with no ID overflow; never a live, derived or
   uncaptured row. Omitted live orders still make the recorded fact unproven;
   housekeeping proves nothing about them. The command stays Unknown, and CALLERS says this proves nothing about
   omitted live orders.
7. **Terminal.** `done_plan` / `fail_plan` (the journal projection) replace the
   numeric `cx.done` / `cx.fail`.
   - Native pre-plan validation errors keep their own text and kind. The issued
     Preflight is failed (`fail_preflight`), or, before any I/O, a genuine
     no-I/O completion authority is issued and failed. A terminal-only Mutation
     Context with no credential, which grants no private send rights, covers a
     missing real credential.
   - Initial-entry rejection is only the engine's automatic typed issuance
     failure.
   - There is never a numeric fallback or a dummy Context converted to a
     Preflight.
8. **Ratchet.** Prune only the migrated `trading.rs` / `lib.rs` sites from
   `request-legacy-sites.json`, decrementing per-file counts. Record the
   old and new maps and hashes in the PR body.

**Hyro and options carve-out** (decision D5). Hyro and options PlaceOrders
share `on_trade_command`, `plan_place`, `advance`, `on_trade_response` and
`send`/`resend` with ordinary trading. Proposed split: at `on_trade_command`
by variant, ordinary Linear/Inverse/Spot/Demo goes through Preflight/Plan, and
Hyro/options stay on the legacy job pipeline until their own adoption.
`on_response` / `on_request_failed` gate the text `resend` to Hyro and
non-trading `Then` variants, so no ordinary Plan response reaches it. This is a
temporary dual path, which needs owner sign-off under the direct-cutover rule.

## Owner rulings (claude-ddi2il, 2026-10-23)

- D1: approved. A narrowly declared Amend→Setting, only for attached TP/SL
  legs. 34040 is Satisfied(Setting) with `only: Setting`; a plain amend
  answered 34040 stays refused, with a negative control.
- D2: approved. Satisfied(Cancel) makes no Cancelled store transition. An
  engine-owned fence plus exactly one `reconcile_uncertain` read (stacked on
  #1857) always follows a Satisfied success, and the fence clears only through
  `orders_reconciled`. A CALLERS entry covers the caller-visible lag.
- D3: option (b). Amend→Cancel is allowed only for a declared
  cancel-then-recreate leg whose replacement is position-bound
  (reduce-only/closeOnTrigger), preserving `partial_stops.rs:536,913`. A
  negative control proves that a non-position-bound replacement after an absent
  old leg stays refused/Unknown.
- D4: approved. KnownPartial with proven IDs plus a CALLERS entry. Before merge,
  the NX callers lane must confirm that NX shows partial success.
- D5: approved as a temporary split. Recorded as a remove-before-master-merge
  obligation, closed by claude-fe7lgm's Hyro RequestContext adoption (hook
  contract agreed: four `hyro.rs` functions, a Hyro marker in
  `ReconciliationData`, owner-guarded discards). No ordinary fallback.
- The D1/D3 narrowings and the D2 fence/reconcile are engine pieces (lane
  claude-dazzy3) delivered in #1857; this child stacks on #1857 as well as
  #1873.

### Later rulings (claude-ddi2il, 2026-10-23)

- D1 extended: the partial-stop replacement `/v5/position/trading-stop` leg is
  also a declared Amend→Setting, so `34040` is satisfied there; a plain
  `/v5/order/amend` answered `34040` stays refused.
- D2 rewrite: `partial_stops.rs` 913 and the benign whole-`110001` cancel test
  answer the reconcile read before asserting removal.
- D3 option A: a Plan leg declares the protection cancel with
  `plan_protection_cancel_leg_context` before registration. It is its own
  engine PR (#1913), merging after #1857 and #1873, and is carried on the child
  as a marked commit until then.
- D1 extended again: every `Call::TradingStop` leg of an amend (trailing move,
  full-position stop move, partial stop without a mark) is a Setting, with a
  `34040` test per shape.
- D5 scope: every option-symbol command (placement, amend, cancel, cancel-all,
  settings) stays on the legacy pipeline; the same remove-before-master-merge
  obligation covers it.
  Retired for options by slice 4a ([RUST_BYBIT_OPTIONS.md](RUST_BYBIT_OPTIONS.md)):
  option commands now take this ordinary path, and only Hyro remains legacy.
- C: every Unknown ordinary seal reconciles with exactly one fresh order read;
  tests answer it and assert no write, resend or success refresh.
- F: `continue_on_error` is a caller contract. A provider refusal or lost
  request on one leg retires only that leg (`finish_plan_failed_call` in the
  failure callback, no engine change) and later legs proceed.

### Original decision requests

- **D1.** 34040 TP/SL no-op on an attached TP/SL amend: add Amend→Setting to the
  shared narrowing table (the classifier). The attached legs then get a
  Satisfied(Setting) 34040 row; a plain amend stays refused.
- **D2.** Absent-order codes 110001/170213 on a cancel: Satisfied(Cancel) with the
  row fenced for reconciliation (the `reconcile_uncertain` hook from #1857),
  not marked Cancelled. That is caller-visible. The alternative is an explicit
  owner-approved exception.
- **D3.** Partial-stop amend whose cancel leg answers 110001: (a) keep the
  amend-cancel rule, so the replacement isn't placed and the existing tests
  `partial_stops.rs:536,913` are rewritten; or (b) narrow Amend→Cancel only for
  a declared cancel-then-recreate protection leg. I lean (a).
- **D4.** A PlaceOrders `continue_on_error` partial success becomes a failure
  with partial IDs (KnownPartial), not `done(ids)`. That is caller-visible and
  gets a CALLERS entry.
- **D5.** The Hyro/options carve-out above.
- Existing tests that change with these decisions: `partial_stops.rs:536,913`
  (D3); `trading_variants.rs:763`, a cancel-all with retCode 110001 that must
  still fail; PlaceOrders partial-success expectations (D4); and the
  `dispatch_signer.rs` transport-text resend test. Each is rewritten
  deliberately with its CALLERS delta.

## Phases (one PR, these commits)

1. This plan and the original REDs, independently reviewed.
2. The code table plus the strict ACK module, with no dispatch change yet.
3. Genuine Preflight for the mode read.
4. Plan admission, slots, leg intents, recipes and delivery validation.
5. Fact recording, resend grants, PartialUnknown, cancel-all housekeeping and
   the terminal.
6. Ratchet pruning, CALLERS (final behaviour and NX reconciliation obligations)
   and AUDITS.

Additional required receipts, added as REDs or controls in the phase that
introduces them:
- a late ACK after expiry, and after a private-stream event (preserving
  `apply_local_write`'s socket, snapshot and before guards);
- text and ID budget overflow;
- exact deadline equality;
- duplicate and stale `HttpResponse` callbacks;
- `admit_plan` capacity refusal, ending NotSent with no request;
- `done_empty_plan` for empty plans;
- prior-release uncertainty surviving later errors;
- a cancel-all of more than 500 orders publishing only its proven subset;
- no use of the test-only `http_write_result` helpers.
Reconciliation of Unknown seals uses #1857's hook once that lands; until then
it stays the venue's existing trigger.

Gates: all 36 REDs green; the four native controls green with declared 20 s
windows; the existing ordinary, Hyro and group tests unchanged (rewritten tests
listed); engine + Bybit + Binance focused tests; strict clippy, fmt and bans;
an independent review per phase; fresh CI with replay gates on an integrated
head; no merge without an exclusive window after actual #1873.

## Architecture gate

Is this a per-venue fix of a cross-cutting concern? No. Classification,
resend authority, plan bounds, retry windows and the journal projection are the
shared classifier and Plan core. Bybit supplies a code table, native recipes
and strict native ACK decoding. Three pieces stay temporarily in the venue,
each with a deletion trigger:
- cancel-target echo proof, until the classifier's phase 2c cancel-echo check;
- Satisfied store or reconcile handling, until phase 2c reconcile marking or
  #1857 adoption;
- 10002 clock correction, until the protocol-clock core consumes refusal
  evidence.
