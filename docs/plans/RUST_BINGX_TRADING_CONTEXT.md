# BingX ordinary trading on RequestContext

Caller owner: codex-uufub4. Recipe, credential and staging-removal review:
claude-4yjlw0. Native audit parent:
`53a906fd934db2194e3b183fb219f5f9f84c8c4a`.

The plan and compiling original controls were committed before the runtime.
The implementation starts at actual landing a828ede7ebb and composes frozen
#1939 c1512d147e9 for independent cancel batches. Source review and exact-head CI
remain merge gates; this branch is not an actual landing or a zero-site claim.

## Scope and native inventory

At the pinned parent BingX has 39 manifest fingerprints/43 lexical occurrences.
Ordinary trading contributes seven fingerprints/seven occurrences in trading.rs,
plus shared issue/resend/response/failure helpers in lib.rs. A shared fingerprint
may remain while another caller still uses it. Record exact manifest changes at
implementation time; do not equate branch cutover with an invented prune count.

Adopt every ordinary trading command handled by Trading::command, including
PlaceOrders, CancelOrders, CancelSymbolOrders, AmendOrder, SetLeverage,
SetMarginType and SetPositionMode. Preserve the existing complete serial native
Step list, planner validation, split-lot semantics, conditional/protection fields,
precision and client identity constraints. No fallback into the legacy pipeline
for any adopted command.

| Native call | Method/path | Evidence and intent |
| --- | --- | --- |
| Placement and bracket legs | POST /openApi/swap/v2/trade/order | Place; checked native order ID and original client identity |
| Individual cancellation | DELETE /openApi/swap/v2/trade/order | Cancel; original Amend semantics for cancel-before-recreate |
| Cancel symbol | DELETE /openApi/swap/v2/trade/allOpenOrders | Cancel; checked failures and native affected-row scope |
| Quantity amendment | POST /openApi/swap/v1/trade/amend | Amend; confirm exact ID, symbol and quantity |
| Native price replacement | POST /openApi/swap/v1/trade/cancelReplace | Amend; confirm cancellation and fresh accepted replacement ID |
| Leverage | POST /openApi/swap/v2/trade/leverage | Setting; exact BOTH/LONG/SHORT and clamped leverage |
| Margin | POST /openApi/swap/v2/trade/marginType | Setting; exact symbol and mode |
| Position mode | POST /openApi/swap/v1/positionSide/dual | Setting; existing open-position/order refusal |

Metadata #1884/#1747 and account/session #1889 are excluded. Public/account reads,
history #1848, identity and sockets are separate slices. The generic lib.rs
staging funnel remains until its actual other callers are empty; 4yjlw0 owns the
remove-before-master obligation and reviews its exact eventual deletion.

## Genuine entry and complete admission

1. Identify a supported command without entropy, publication, job insertion,
   refresh, progress or other opaque work. In its genuine command callback enter
   begin_preflight(WriteAfterReads), capturing the exact primary Credential used
   by the existing recipe. A real write credential is mandatory; None is not a
   valid WriteAfterReads workaround. Rejected/Revoked returns directly with no
   numeric completion fallback or command-ID issuance alias.
2. Keep native validation and complete Step planning under that retained
   Preflight. A duplicate or validation refusal uses shared typed authority,
   never numeric cx.fail. A real empty native job uses done_empty_plan. Keyless
   synchronous refusals/no-ops instead obtain genuine initial terminal-only
   Mutation Context authority with credential None before planner entropy or
   opaque work, and never enter a legacy send path or Context conversion.
3. Admit the whole serial Plan once before optimistic Store effects, receipts,
   settings_pending or first physical work. Each native Step declares one serial
   call: physical timeout 20,000ms, at most two physical attempts, FreshAttempt,
   DefinitiveRefusal retry policy, no fallback or local delay. Eight calls admit
   320s; eight first acknowledgements at 19s fit 152s, and eight proven-refusal
   re-signs plus acknowledgements at 19s intervals fit 304s. The root is the
   checked complete sum, never an arbitrary 20s whole-command bound.
4. Retain the opaque Plan, exact Slot/Context and native correlation data. Issue
   only the slot made available by the engine. Declare intent narrowing before
   the registered attempt through shared Plan constructors; no method-inferred
   rights or unrestricted Amend-to-Cancel conversion. Empty plans never fabricate
   an accepted result or admit required slots that are silently skipped.
5. Account/version/native observation guard data stays native adapter data.
   Account refresh/reconciliation runs under its own actual authority and may not
   borrow a retired Plan, mint fresh entry custody or renew a root. Any future
   queued retained-preflight wake or conditional terminal requires its reviewed
   shared seam, not a local timer/callback authority substitute.

## Recipe identity and proof-gated retry

Use sign::dispatch::Http unchanged, with the exact primary identity selected by
Credentials::identity for mutations. A configured secondary read key must not
sign a POST/DELETE mutation. Preserve Params' sorted HMAC query, POST JSON native
number/boolean types, header order, timestamp/signature formats and native URL.
Never reproduce key selection or provider state in callers.

The parent Pending::Signed branch re-signs once when code=100001 OR message text
contains "signature verification failed", before the trading codec checks HTTP
status. It therefore re-sends uncertain 408/5xx responses. This slice removes
that legacy mutation behavior: only a whole, engine-classified code-specific
pre-execution refusal can mint the shared grant. Use classify_write / its
WriteFact and plan_resend_context; the engine validates the latest exact attempt,
consumes the one grant and enforces the admitted attempt/root caps. Preserve the
same logical Step and original clientOrderId; only physical timestamp/signature
change. Do not use read retry evidence, plan_retry_context, text matching or a
local resent counter as write authority. A signature refusal does not settle the
call as DATA before the granted resend.

Transport loss, HTTP408/5xx, unknown code, malformed ACK, text-only resemblance,
expired grants and second signature refusals cannot obtain another write attempt.
No generic native write retries, extra timer or third physical request. Any
recipe/re-sign behavioral hunk goes to 4yjlw0 before alteration.

## Shared evidence and completion

Validate exact Plan/leg/latest delivery before native decoding or Store changes.
Use one const BingX CodeTable reviewed by the classifier owner. Its whole-refusal
100001 row supplies retry proof; no local status/outcome ladder or message retry
matching survives in the adopted branch. Unknown/system codes remain uncertain.
Classify HTTP status and actual venue code separately, never put a status in
venue_code. The existing legacy rejection table is audit input, not authority.

Split strict ACK decoding from Store reducers. Record all accepted native IDs,
Satisfied outcomes and known partial/uncertain effects in the one Plan journal
through http_write_fact/http_write_batch before publication. Preserve exact
amendment ID/symbol/quantity checks, contradictory/empty-ID rejection, cancellation
failed lists and native cancelReplace's two-stage proof. A cancelReplace error
may follow a successful cancellation; a whole code refusal must not erase that
effect. Its codec must expose proven cancellation/uncertainty to the classifier
rather than treating all envelope failures as execution-free.

For ordinary cancel code80016, review an intent-specific Satisfied(Cancel) row.
Satisfied is not proof of a Cancelled Store transition: use the shared engine
fence/reconciliation path, with caller behavior recorded where needed. An absent
old order never authorizes a non-position-bound replacement. Trigger replacement
keeps its original no-absent-success rule and freshness constraints; no generic
Amend-to-Cancel exception. New accepted effects/IDs survive any later provider,
claim, transport, malformed-response, timeout or replacement-guard failure.

Remove Job.ids/error/effective/replaces_cancelled as settlement authority; retain
only codec/correlation/freshness data. CancelOrders explicitly sets native
continue_on_error. Preserve its separate sibling calls after provider/transport
failure through exact finish_plan_failed_call and after ordinary classified
responses through exact DATA settlement; do not weaken native continuation
assertions or retry a failed call. Other native jobs stop on their first failure.
Overseer ruling preserves independent continue_on_error siblings for delivered
408/5xx as well. The current core auto-aborts before a native callback, so this
requires the separate reviewed PlanPolicy.independent_calls prerequisite after
#1932: Mutation-only, default false, immutable at admission. The engine records
Unknown before typed on_http_failed; the handler settles the exact failed call
and advances siblings, with no retry. If the handler does not settle, the wrapper
auto-seals once. Non-continue_on_error commands retain auto-abort. This is an
explicit shared prerequisite, never an adapter timer, hidden fallback or assertion
rewrite. Preserve partial IDs through
the shared KnownPartial / PartialUnknown journal projection. Complete a finished
schedule with done_plan, failures with fail_plan/fail_preflight, and clean up
exactly once on owner retirement. No numeric done/fail or local settlement ledger.

Implement bounded capture_http_reconciliation before mutation release. Initially
late ACKs remain nonpublishing, as in WEEX/Binance: an immutable late view cannot
safely update native observation/freshness state together with Store. Any later
native late reducer requires a reviewed freshness contract. Capture is nonsecret
native correlation only, never credentials, outbound recipes or a scheduler.
Late ACKs cannot complete retired commands, reopen work or reach ordinary Store
publication; no claim is made that they initiate a reconciliation read. Typed preparation/signing/failure causes remain typed and retain
journal evidence even if the native callback is no longer publishable.

## Original controls

Proposed file: sdk-rs/venues/bingx/tests/managed_trading_plan.rs. It uses actual
Bingx through tests/support::engine, distinct primary/secondary credentials, real
market state and the shared prepared_actions host Prepare/Bind/Claim driver.
The existing tests/trading.rs includes a synthetic/source-included venue and is
not reused as managed production proof. The raw-action Prepare discriminator
runs before host preparation.

Reproduced assertion REDs on unchanged parent 53a906fd934:

- ordinary_write_requires_managed_preparation_before_physical_http: zero Prepare
  versus one required; no legacy physical bypass before host preparation.
- eight_serial_calls_keep_each_native_twenty_second_attempt: packet timeout None
  versus Some(20000), while eventual eight-call native success fits 152s.
- each_proven_signature_resend_has_a_fresh_window_within_the_complete_plan:
  resent timeout None versus Some(20000); original client/body identity retained,
  fresh stamps, eight resends plus ACKs fit 304s inside the 320s declared root.
- released_first_attempt_expires_unknown_without_a_generic_retry: parent emits no
  enforced native write deadline; desired exactly one Timeout/Unknown and no
  resend or late terminal.
- gateway_100001_does_not_grant_a_write_resend: 503 +100001 currently resends.
- timeout_status_100001_does_not_grant_a_write_resend: 408 +100001 currently resends.
- signature_text_without_a_refusal_code_does_not_grant_a_write_resend: code80001
  plus signature text currently resends; desired Unknown, no new physical write.

Reproduced compatibility GREEN controls on the same unchanged parent:

- Exact independent HMAC/POST/header oracle uses primary while secondary exists.
- One genuine 100001 re-sign preserves clientOrderId and never has a third attempt.
- Transport failure is never resent.
- Accepted prefix ID survives a later uncertain write as Unknown partial evidence.
- Accepted prefix ID survives a later proven code80014 refusal as KnownPartial.
- Invalid later order prevents every earlier native write.
- Eight serial native successes at 19s intervals retain all native IDs in order.

The prefix controls are compatibility controls because the parent already uses
partial_ids. Do not mislabel them as original failures. Reproduce each exact
assertion, keep logs outside Git, and never count compile errors/ignored tests as
REDs. The 80014 refusal table row needs independent code-table review before
production; it is a native compatibility fixture, not inferred HTTP proof.

From sdk-rs, run the new integration target, then scoped strict Clippy/fmt and
request/signing checks. Intentionally unignored REDs make this checkpoint
non-green until implementation; never claim merge readiness for it. Proposed
commands:

```
cargo test -p safe-cex-venue-bingx --test managed_trading_plan
cargo clippy -p safe-cex-venue-bingx --test managed_trading_plan -- -D warnings
cargo fmt --all -- --check
python3 scripts/check-request-context.py
```

Implementation gates additionally cover strict ACK/cardinality/mixed-result
validation, cancellation and replacement uncertainty, setting side/mode guards,
capacity/entry refusal before effects, provider/claim refusal, exact generation
reuse, no retry after grant/root expiry, pre-codec deadline denial, restricted
late reconciliation and retirement cleanup once. Obtain independent plan review
before source; later source/caller/map review and full CI use the then-current
actual engine landing, without fixture/budget/allowance edits. Publish a draft
only under the normal authorized PR flow.


## Original execution receipt

The actual native integration target on unchanged 53a906fd934 compiled and ran:
7 assertion REDs, 7 compatibility GREEN, 0 ignored. Raw log remains outside Git.
The failures are the seven listed above (Prepare bypass, missing physical
windows/deadline, and unsafe signature-text/status resends); no compile failure
or existing prefix behavior was relabeled as an adoption RED. Production source,
signer code, manifest, fixtures and allowances are unchanged at this checkpoint.
Independent plan/source-control review is required before implementation.


The independence prerequisite must reproduce the six ruled negatives/positives:
auto-seal follows the original failed call even if the callback issued a sibling;
a declining callback seals once and retires the suffix; captured unproven facts
cannot be overwritten or converted into a resend; final Unknown keeps IDs and
requests one reconciliation per seal; Read independence is refused at admission;
and duplicate/late delivered failures invoke neither callback nor reconciliation.

## Classifier-owner review conditions

The classifier owner reviewed the proposed rows. Before production, cite the
official meanings of 80014 and 80016 and choose the former's refusal kind from
that evidence. The row shapes are:

- Parsed numeric 100001: RefusedResend(AuthInvalid), with no intent restriction.
  Unproven HTTP status takes precedence over this row; text without the numeric
  code grants nothing. Shared grant issuance enforces one resend. HTTP status
  rows must be documented 4xx codes other than 408.
- 80014: Refused(InvalidInput or Rejected as documented), Place only. KnownPartial
  is the journal projection of proven earlier items plus a refusal, never a row
  verdict. Decode mixed batch items into http_write_batch; a whole-envelope
  refusal carrying accepted IDs is unproven evidence, not a clean refusal.
- 80016: Satisfied(Cancel), Cancel only, conditional on documented absent-order
  meaning. It cannot cover wrong-symbol or invalid-parameter failures.

Satisfied(Cancel) requires BingX to implement reconcile_uncertain with a fresh
owner and a complete, wholly admitted order read. Only that read calls
orders_reconciled; an older read must not clear the fence. The absent response
never publishes Cancelled. This is required caller adoption, not a claim that
the shared hook alone clears native state. Test classify::assert_table(&BINGX),
status/code separation, scope negatives and the fresh-owner fence behavior.


## Runtime and native evidence

Every ordinary command now enters genuine Preflight and admits its complete
native serial Step list before physical preparation. The unchanged family Http
recipe signs every mutation with its primary identity. The journal records each
native request separately; BingX's native adapter sends one order per request,
so the caller's batch prefix consists of per-leg facts. There is no aggregate
Failed fact replacing successful earlier items. A future wire batch endpoint
must use the shared per-item batch classifier rather than this single-call codec.

The official [Swap V2 common-code documentation](https://bingx-api.github.io/docs/#/en-us/swapV2/base-info.html)
serves 80014 as Invalid parameter and 80016 as Order does not exist in its
[documentation asset](https://bingx-api.github.io/docs/static/js/app.4dcba784df17c52faa47.js).
The official [error-code reference](https://raw.githubusercontent.com/BingX-API/api-ai-skills/main/skills/references/error-codes.md)
describes gateway signature refusal 100001, routing refusal 100400, rate refusal
100410 and insufficient placement margin 101204. The table preserves these
native diagnostics; 80012 and unlisted codes remain uncertain. No HTTP status
rows are declared. HTTP 408/5xx proof takes precedence over parsed native codes.

Ordinary CancelOrders alone preserves native continue_on_error by opting into
independent_calls. Typed provider/transport/gateway failure settles only the
original call in on_http_failed and advances the sibling without retry. All
other native jobs retain stop-on-error. The final journal projects Unknown or
KnownPartial with the proven prefix; native_ids is output ordering only, never
an outcome or settlement ledger. A replacement's effect-bearing IDs or confirmed
cancellation weaken any refusal to Unknown and cannot mint a resend.

Satisfied(Cancel) never publishes Cancelled. The shared hook creates a fresh
Background managed order read even when an earlier order read is pending. A
forced read advances the native revision; only its wholly admitted complete
snapshot calls orders_reconciled. Older responses/failures cannot clear its
reservation, overwrite its Store projection or restart an already applied read.
Malformed, expired and partial snapshots do not clear the uncertainty fence.
ACK objects keep one deserializer depth budget and reject tuple-shaped aliases,
contradictory failed cancellation lists and incomplete order identities.

At the implementation parent the original target ran 10 assertion REDs and
7 compatibility controls, all unignored. Review controls additionally cover
stale snapshot ordering, replacement identity evidence, rejected and duplicate
entry effects, contradictory absent cancellation, and bounded nested decoding.
The exact shrink is trading.rs 7 fingerprints/7 occurrences to 2/2: five removed.
The two remaining conservative fingerprints are native data-field references,
not physical request or numeric completion calls. Other BingX paths and shared
staging funnels keep their exact inventory until their own callers migrate.

## Post-beta: cancel-all late-landing protection

Keep the default uncertain cancel-all fence for beta. An Unknown create can
land after DELETE /openApi/swap/v2/trade/allOpenOrders, so that DELETE alone
cannot justify reporting every order cancelled. The final maintainer and
auditor ruling requires a fresh complete Primary read before another cancel-all.

The queued follow-up is automatic late-landing cancellation for placements
owed by a genuine cancel-all. It must preserve the exact client identities and
generations, cancel only proven matching live orders, retain per-order outcomes,
and keep unrelated orders safe. Prove the late-arrival race and failure cleanup
before considering a cancel-all fence opt-out; no write retry or fake Command
authority is permitted by this plan note.
