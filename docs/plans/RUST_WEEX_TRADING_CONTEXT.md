# WEEX ordinary trading on RequestContext

Owner: codex-uufub4. Signer/recipe and temporary-funnel removal review:
claude-4yjlw0. Parent: actual landing
`53a906fd934db2194e3b183fb219f5f9f84c8c4a`.

This checkpoint contains the plan and original native tests only. Production
source, construction declarations, request inventory and allowances are unchanged.
The RED tests are intentionally executable and unignored; this is not a green
adoption or merge-ready checkpoint. Independent review precedes implementation.

## Native inventory and scope

At the parent, WEEX has 42 path/fingerprint rows and 51 lexical occurrences.
The new caller assignment covers 41 rows/50 occurrences; one additional lib.rs
`6cf35cca...` staging row remains 4yjlw0's removal obligation. Counts include
source macros/test references and are not numbers of physical requests.

This slice adopts the whole ordinary `trading.rs` pipeline plus only its exact
`lib.rs` pending/send/response/error branches: PlaceOrders, CancelOrders,
CancelSymbolOrders, AmendOrder, SetLeverage, SetMarginType and SetPositionMode.
The existing planner computes a complete serial `VecDeque<Step>` before the
first write. There is no independent Hyro/options pipeline in this family.
Do not introduce a managed/legacy fallback for any adopted command.

| Native operation | Method and path | Declaration / retained codec |
| --- | --- | --- |
| Regular placements, batches of at most five | POST `/capi/v3/batchOrders` | Place; exact caller/generated client IDs, row-to-order correlation |
| Individual/bracket/reducing placement | POST `/capi/v3/order` | Place; exact body and native identity |
| Conditional placement | POST `/capi/v3/algoOrder` | Place; conditional/bracket fields retained |
| Position protection | POST `/capi/v3/placeTpSlOrder` | Place; native protection identity |
| Regular cancellations, batches of at most ten | DELETE `/capi/v3/batchOrders` | Cancel, or original Amend semantics for cancel-then-recreate |
| Algorithm cancellations | DELETE `/capi/v3/algoOrder` | Same semantics; exact native IDs |
| Cancel all normal / algorithm families | DELETE `/capi/v3/allOpenOrders`, `/capi/v3/algoOpenOrders` | Two serial native calls |
| Protection price amendment | POST `/capi/v3/modifyTpSlOrder` | Amend; exact trigger/execution working types |
| Leverage | POST `/capi/v3/account/leverage` | Setting; exact side-specific coupled/isolated payload |
| Margin / position mode | POST `/capi/v3/account/marginType` | Setting; native symbol mode, sorted symbol order |

The source's Step.method/path/body/query remain authoritative, including the
cancel-then-recreate path composed from plan_cancel and plan_orders. Nothing
infers authority from the HTTP method or from an operation name.

`trading.rs` currently contributes four fingerprints/five occurrences; generic
lib.rs sender and callback fingerprints are shared with other callers. Moving
this command branch may leave a shared fingerprint unchanged. Record an exact
whole-manifest comparison after implementation; promise no invented prune count.
Account polling, identity, history, public reads, sockets and metadata #1747 are
separate slices. Preserve their current source byte-for-byte where possible.

## Authority and admission

1. Identify a supported trading command without publishing or issuing work.
   Enter `begin_preflight(WriteAfterReads)` in its genuine callback, before
   planner entropy, optimistic Store changes, command progress, refresh requests
   or any legacy operation. Capture the recipe-selected primary Credential
   exactly; absence declares no send authority. Initial Rejected/Revoked returns
   immediately, without numeric `cx.fail` or an ID-taking issuance alias.
2. Perform native validations and build the entire native Step list under that
   retained Preflight. A validation/credentials/duplicate conflict is either
   genuine typed entry rejection or `fail_preflight`, never a numeric fallback.
   A genuinely empty native job uses `done_empty_plan`, with no dummy request,
   fabricated accepted fact or admitted-but-skipped slot. Preserve empty keyless
   commands where their native behavior is success.
3. Opt WEEX into engine `http-plans` in the actual adoption commit, then admit one
   complete serial Plan. One native Step becomes one serial single-call bound:
   20,000ms, one physical attempt, no fallback, no deliberate wait. The root is
   the checked serial sum, not a fixed 20s whole-job limit. Eight batches admit
   160s and can acknowledge at 19s intervals (152s total). Capacity/ID/journal
   refusal precedes any optimistic mutation or first request.
4. Retain the Plan, declared Slots and native Step/correlation data in the job.
   Only engine-owned availability/custody selects a slot. Declare any per-leg
   intent before its registered attempt, using the shared Plan constructor.
   Never add an unrestricted Amend-to-Cancel narrowing. An absent cancel must
   not authorize a non-position-bound replacement.
5. Preserve optimistic status updates, bracket ownership, order/settings
   revisions, client-ID progress receipts and exact native rollback rules,
   after whole-plan admission and at the same native dispatch points. Progress
   supplies identities, not release, DATA, slot settlement or success authority.
6. `request_signed_managed` uses the existing family `dispatch_signing::Http`
   recipe and its exact primary identity, sorted query/body bytes and headers.
   Secondary read credentials never sign a write. No caller reconstructs key
   selection, signature material, timestamps, nonce or signer state. No change
   to the recipe's behavior without 4yjlw0 reviewing the hunk first.

The incoming recipe does not create a retry budget. WEEX writes keep
`RetryPolicy::Never`, attempts=1; signature refusals, transport loss and uncertain
responses cannot re-sign/resend a write. There is no local retry timer.

## Delivery, evidence and completion

- Validate exact Plan/leg/current delivery before decoding or Store changes.
  Pending carries opaque Plan/Slot/Context plus native Call metadata; it grants
  no authority by numeric command ID. Typed preparation/provider/HTTP failures
  retain their original kind and use the shared owner/journal terminal path.
- Separate strict native ACK decoding from Store reducers. Record each proven
  native identity/effect through `http_write_fact` or `http_write_batch` before
  native Store publication. Batch cardinality, client/native IDs, duplicate rows,
  malformed arrays and protective ACKs must be checked, not guessed.
- One const venue `CodeTable` feeds the shared classifier. Do not carry forward
  local HTTP status ladders, message retry matching or first-error outcome
  aggregation. In particular, code `40015` is a system error, not proof of a
  refusal: [WEEX's error-code reference](https://www.weex.com/help/articles/52122274060569).
  That reference identifies `40006` as an invalid access key. Use a reviewed
  code-specific pre-execution refusal row for the new refusal control; do not
  treat the old fixture text labeling `40015` as insufficient balance as proof.
  All undocumented/system outcomes remain uncertain. Classifier-owner review of
  the actual table is a production gate.
- Accepted prefix IDs survive a later provider failure, proven refusal,
  transport loss, malformed response, timeout or replacement guard failure.
  KnownPartial is the shared journal projection (bounded partial IDs with no
  Unknown/Rejected/NotSent outcome); an unresolved released remainder is Unknown
  with those IDs. Retire every unissued remainder via the shared Plan terminal.
- Preserve native serial continue-on-error after proven refusals where declared.
  Generic unproven writes abort the owner under the landed Plan contract; they
  never issue the second cancel family or another placement. Record this caller
  behavior change rather than keeping the old native continuation as a bypass.
- The fill/revision guard before cancel-then-recreate remains exact. A revoked
  replacement fails the Plan with its journal evidence; it never silently
  skips an admitted required write. Conditional successful suffix retirement,
  if actually needed, requires the separately reviewed #1894 declaration.
- Successful complete schedules use `done_plan`; errors use `fail_plan`.
  Remove local `Job.ids/error/effective` as settlement authority, keeping only
  native codec/freshness/job data required by the adapter. No venue ledger.
- Post-command account refresh remains background work with its own actual
  authority; it cannot borrow the retired Plan or extend a root. Preserve the
  existing refresh/freshness contract and coordinate the separate account slice.
- Implement `capture_http_reconciliation` and the restricted `reconcile_http`
  native ACK codec before any managed mutation can release. Bounded nonsecret
  Call/row correlation only; no credentials, outbound recipe or venue scheduler.
  A late ACK cannot complete a retired command, reopen work or publish through
  an ordinary response callback.
- Use `on_http_owner_retired` for exact job cleanup, after all native failure
  handling has finished. No forgotten command, capacity credit or optimistic
  row survives a rejected prepare/sign/claim or terminal owner retirement.

## Original native controls and validation

New file: `sdk-rs/venues/weex/tests/managed_trading_plan.rs`. It constructs the
real WeexFutures via the trusted local provider, seeds real market/settings,
issues genuine command events and uses the shared host Prepare/Bind/Claim
driver. The independent first control also inspects raw actions *before* host
preparation; compatibility controls alone are not claimed as managed proof.

On unchanged parent source, **six assertion REDs / three GREEN controls**:

| Control | Parent observation / required result |
| --- | --- |
| `ordinary_write_requires_managed_preparation_before_physical_http` | Prepare count 0, required 1; no legacy physical bypass |
| `eight_serial_calls_keep_each_native_twenty_second_attempt` | packet timeout None, required Some(20000); eventual 152s success |
| `released_write_expires_once_with_unknown_and_late_ack_cannot_complete` | no terminal from emitted deadline timers; required exactly one Timeout/Unknown and no late terminal |
| `accepted_prefix_is_preserved_when_the_next_write_is_uncertain` | missing accepted IDs 9000..9004; required Unknown with bounded IDs |
| `accepted_prefix_is_known_partial_after_a_proven_later_refusal` | missing accepted IDs 9000..9004; required known partial projection |
| `a_system_error_code_does_not_prove_the_write_was_refused` | Rejected, required Unknown for native 40015 |
| `compatibility_primary_recipe_and_no_transport_retry` | GREEN: exact independent HMAC/header oracle, primary with distinct secondary configured, no resend |
| `compatibility_long_serial_success_preserves_native_ids` | GREEN: eight serial five-order lots, 152s, all 40 native IDs in command order |
| `compatibility_invalid_later_order_refuses_before_any_write` | GREEN: invalid later order prevents first write |

Command: `cargo test -p safe-cex-venue-weex --test managed_trading_plan` from
`sdk-rs`. Tests are unignored and intentionally fail at this plan checkpoint.
No compile error, timeout or filtered test is counted as an original RED.
External logs stay local and uncommitted.

Implementation must additionally prove capacity/entry refusal before optimistic
effects; malformed/cardinality/correlation ACKs; settings side/mode compatibility;
replacement fill races; exact no-retry transport/status/signature failures;
preparation/claim/provider refusal before release; generation reuse; late ACK
restricted reconciliation; pre-codec expiry fencing; and exact cleanup once.
Keep incoming tests unchanged unless an explicit caller behavior decision is
recorded. New native controls and existing focused trading tests must pass,
followed by scoped strict Clippy/fmt, request/signing checkers and independent
source/caller/map review. Full family CI/WASM/replays follow the implementation
on the then-current actual landing; no budget/fixture/allowance changes.

## Commit and review sequence

1. Commit this plan and the reproduced original controls; independent review.
2. After review, implement shared Preflight/Plan ownership and strict ACK/fact
   recording in the one ordinary path, with existing dispatch recipes unchanged.
3. Add the remaining release/reconciliation and race controls, run scoped checks,
   refresh the exact ratchet and CALLERS evidence, and obtain source/caller review.
4. Publish/update a draft PR when authorized; fresh actual integration and full
   CI precede any request for a venue merge window. BingX ordinary trading is
   the next independently planned caller slice, not a hidden part of this one.

The generic legacy staging sender stays while account/history callers remain.
Delete it only when genuinely empty, with 4yjlw0's exact-map review. No local
engine API, fabricated Context, wrapped numeric completion or ratchet exception
is needed or permitted for this Plan adoption.
