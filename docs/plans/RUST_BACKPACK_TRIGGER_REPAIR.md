# Backpack trigger repair on the landed repair owner

Status: local consumer source is applied under hcmq3f's exact source review.
WOO seed #2198 landed as adb6555aeac. The actual #2202 settlement correction
(a71a90faf30) is plain-merged in 2006e0cf0dd; no engine source is owned here.
Existing-API and financial/control plan review is CLEAR (fe7lgm/hcmq3f).
Publication waits for the final source/fixture/map review and correctness gates.

## Ownership and scope

Consumer owner codex-mxb4mq; shared-core owner claude-fe7lgm. Native coauthor
codex-ih35js owns the distinct Phemex cancel-only consumer. Ask fe7 for exact
existing-API fit and hcmq3f for venue custody/financial controls. No consumer
edits engine. Open-PR title searches for Backpack and repair were empty at this
checkpoint; no checked-out Backpack repair consumer branch was found. The
Phemex late-repair branch is a separate live consumer, not ours to modify.

Enable existing repair-mutations only in Backpack's engine dependency. Adopt the
one remaining raw internal repair sender, currently account.rs signed helper
fingerprint34b6, after a current scanner proves it is the sole intended sender.
Keep all foreground Command/Plan writes, account GET chains, histories, public
seeds, sockets, installed construction and crypto implementation outside scope.
Remove only proven orphaned raw repair branches/helpers after the direct cutover.
Do not remove a conservative scanner escape merely because no physical sender
uses it; distinguish fingerprints from actual requests.

Expected own production change: roughly350–600 lines across a focused repair
owner and account/trading dispatch, plus additive controls and narrowly reviewed
fixture release changes. Feature-link cost is unmeasured, not a zero-byte claim.
Use one local composed combined measurement and CI's Backpack family record;
only a specifically required A/B pair justifies extra local family builds.
No Bybit/Bitget builds unless separately authorized generic source is added.
Current budgets hold; the standing5% venue allowance is applied from the CI
family number if needed, with no combined-budget exception.

## Native discovery and financial declaration

account_item's Orders branch decodes a row, builds trigger_repair only when
trigger_price exists and trigger_by is absent, then requires successful native
order mapping before storing its candidate. The mapper uses native market
precision, trigger_quantity or base_quantity, Market/Limit, Bid/Ask, trigger
price and limit price, reduceOnly, postOnly and timeInForce. Keep those stable
case values and sign the same wire through the installed PRIMARY recipe.

Discovery is accepted account Orders HTTP only. It includes foreign/UI/restart
orders; no originating local Place is required. Private frames update Store and
fill/rollback fences but do not mint repair authority. Closed, cancelled or
contradictory rows must not create a new placement. Preserve the account read's
stale token/publication rules and next/repair ordering. Financial and required
send safety take precedence over copying native races.

Existing native references in backpack/tests/trading.rs:
- missing_trigger_by_is_repaired_and_does_not_reopen_a_replacement_filled_before_response
- trigger_limit_repairs_preserve_post_only_time_in_force_and_reduce_only
- amend_does_not_replace_after_an_ambiguous_cancel_ack and the preceding
  additional-fill/remainder controls, as safety references rather than a new
  foreground-amend migration.

## Existing API composition

1. Keep the genuine SelectedRead account Orders Context and its exact token.
   Replace its accepted whole publication call with
   cx.commit_repair_snapshot(read.context(), [token], publish). This performs
   the same Store token commit inside the exact live Background DATA callback;
   None is no accepted repair receipt. A failure, stale token, held/invalid
   snapshot or later timer cannot mint. Native account publication and skipped
   row handling still happen under live DATA.
2. After publication, before finish_http_context, select the native-ready
   installed PRIMARY independently of the selected GET identity. Require the
   same account readiness gate already used by account GETs plus private mode.
   The read may be Secondary: it proves account discovery, never write rights.
   Construct the real Primary Credential from the installed identity, not raw
   options, a synthesized KeyRef, or the Read credential.
3. For each eligible published row, call mint_snapshot_repair(receipt, primary,
   Subject{key: old native order ID,generation: issued observation generation},
   symbol). Use the actual observation's stamp/identity; no guessed command ID.
   Call the engine mint and handle each typed refusal once; do not silently
   pre-filter Stale/NotOwned/StoreMissing/NoPrimary/Duplicate/Capacity. Engine
   verifies current receipt/revision, selected read-key ownership, installed
   primary and duplicate/capacity rules. Installation is structurally
   frozen in the engine; do not add a credential-generation counter.
4. Retire the exact account Read before starting any physical repair. Minted
   Entitlement+Subject travel with the frozen NativeOrder. Retain an unspent
   entitlement only in bounded native repair state. On a fresh accepted snapshot
   an unspent admission refusal may be reconsidered under that actual current
   observation; no immediate retry loop and no physical retry of a spent root.
5. admit_repair_replace(&entitlement,&subject,Binding{order_id,symbol,side})
   supplies the opaque RepairRoot and cancel Context. The existing shared owner
   enforces locks, primary binding, Mutation20s/Never and exact spend. It is a
   distinct owner, never a fake Command, Plan or Background Mutation. Internal
   numeric bookkeeping must not produce command progress or terminals.
6. Use request_signed_managed with that exact Context and the existing Backpack
   recipe/native DELETE body and receive window. Record the exact cancel
   WriteResult through the shared fact API only after native acknowledgement
   validation. Only engine-recorded Accepted permits repair_successor(&root).
   Call repair_successor INSIDE that same accepted cancel DATA delivery after
   recording Accepted; at callback exit the core ends a root without a successor.
   No timer or later callback may revive it. The shared call retires the cancel
   credit before allocating the one placement Context. Queue exactly the frozen native POST through the same installed
   primary recipe, at most once. No aggregate deadline; each physical leg has
   its own20s bound. Old-order Cancelled is expected and cannot revoke an already
   admitted root's one valid successor.
7. Purely decode the acknowledgement with NO Store effects or diagnostics, then
   record its exact http_write_fact/result. After that recorded fact, require
   validate_http_write_effects before Store reducers or diagnostics. Finish the
   Context last, after any accepted-cancel successor issuance in that delivery.
   Use the typed write-housekeeping/failure APIs
   for exact expired-owned cleanup; do not require live DATA to drop a failed
   leg. Precommit refusal is NotSent for that leg only. Released loss, timeout
   or ambiguous decode is Unknown, never automatic resend or whole-root
   NotSent after an accepted cancel. Preserve known order IDs and native
   stream-time rollback fences.
8. on_repair_notice matches the exact root and consumes its native job/latches
   once outside Command entry. MintRefused is typed audit plus the shared Warn;
   do not warn twice or fabricate a caller terminal. Settled retains per-subject
   cancel/place facts and refreshes account under genuine independent Background
   Read custody when needed. A notice does not itself grant another write.

## Subject and root lifetime

No guessed wall-clock entitlement expiry. Repeated snapshots cannot remint a
live, spent-unresolved or Unknown order. Unspent subjects retire only on the
Store terminal proof, or Retire::CompleteRead with a genuinely complete,
postdating accepted receipt. Partial/held/stale absence is not a proof. Invoke
retirement while the receipt remains current. Terminal stream updates may retire
an unspent subject through Retire::Terminal, without minting anything.

Once spent, root lifecycle governs. Cancellation of the old order is not subject
revocation that suppresses replacement. The placement result ends the root;
Unknown stays represented and fences/reconciles under the shared contract.
Credential replacement is impossible after start; future install-model changes
belong to the core owner, not this consumer. Local teardown/owner cleanup may
allocate no new IDs and cannot reissue retired work.

## Financial safety policy

Use fe7's conservative condition, without remainder math. Purely decode a
cancel acknowledgement that binds the old ID and definitive cancelled outcome.
Record Accepted, validate effects, then compare the snapshot filled quantity
against both acknowledged cumulative fills and the current native stream/Store
fence before issuing a successor. Automatic replacement additionally requires a proven explicitly present finite
ZERO snapshot/cumulative fill baseline for the full frozen quantity. A decoded
default-zero or absent executedQuantity is not proof. Snapshot amount5/filled2
with unchanged cancelAckfilled2 must NOT replace5. Nonzero or unknown initial
fill, any increased fill evidence, contradictory quantity, Filled acknowledgement
or independent terminal event suppresses replacement. Preserve a per-repair
highest observed finite acknowledged/stream cumulative fill and an unknown-proof
flag even if the Store row disappears or rolls back; lower later values never
erase evidence. Replacement requires complete finite non-decreasing ack
cumulative evidence, not a missing/default-zero field. Cancel identity/status
may still prove an accepted cancel independently of insufficient replacement
fill proof. The accepted cancel remains Accepted, while an unissued placement
is SubjectFact.place=None (NOT ISSUED); settlement reconciles and emits one
manual-action diagnostic. The root's own
expected Cancelled Store transition alone does not suppress its successor.

An actually issued successor refused before release instead yields
Some(Effect::NotSent), distinct from place=None when repair_successor was never
called. Do not manufacture a NotSent fact or issue a Context solely to change
presentation. Pin both notice shapes independently.

No guessed remainder quantity, no reopening a fully filled order, no request
retry or silent loss of the accepted cancel. Stable no-new-fill cases keep exact
existing mapper/wire values. Pin every conservative suppression with native
wire/financial working controls and record it in CALLERS. Remainder replacement
would be a separate money-safety decision; it is excluded from this slice.

## Originals and qualification

Working controls first, then meaningful assertion REDs on actual tip:
- Real primary and secondary-signed account discovery, different raw material,
  installed-provider signature proof; no public-only/no-primary mutation.
- Foreign/restart trigger rows, native Market/Limit, reduceOnly, postOnly,
  GTC/IOC/FOK, exact precision and triggerBy; no raw or fake Command path.
- Stale/rejected/partial snapshot, mint before publication, failure delivery,
  duplicate snapshots/IDs and missing Store subject: no unauthorized write.
- Genuine cancel Prepare/Bind/Claim and frozen exact wire; accepted cancel opens
  exactly one placement with independent per-call bound. Empty/mismatched,
  nondefinitive, malformed, failed, expired or Unknown cancel never places.
- Zero-fill snapshot plus stable complete zero-fill ack preserves exact native
  wire. Initial partial fill with no later delta, missing/nonfinite fill proof,
  decreasing ack, retained high-water evidence despite Store disappearance,
  additional fills, full fill before cancel reply, old-order terminal snapshot
  between legs and replacement stream fill before placement response: no excess
  quantity, stale rollback or reopening. Pin accepted cancel plus placement
  preparation refusal/timeout as a partial outcome. Suppressed replacement pins
  cancelAccepted+placeNone and one manual diagnostic; an issued/precommit-refused
  successor separately pins cancelAccepted+Some(NotSent).
- Duplicate/stale/foreign events and NaN/+Inf/-Inf: exact cleanup only, no new
  work or financial publication. Full Store plus complete action ledgers; exact
  cleanup timers/preparations, independent peer/cadence survival.
- Genuine255 peers+predecessor sole-credit successor proof, full256 refusal,
  root lock conflict both ways, terminal/complete retirement and full reuse.
- Root/subject notices settle once; simultaneous real commands retain separate
  evidence. No command terminal from a repair, no failed write retry.
- Existing account, trading, wire, provider and socket controls retain meaningful
  assertions and clocks unless a reviewed safety improvement supersedes one.
  Record each intentional change in CALLERS. Fixtures use real release helpers.

Venue-only plan/originals/source may share one PR under current policy. Keep WOO
source publication separate; after WOO is moving, commit this plan with its own
README row on the consumer branch. Exact API/financial review precedes source;
scoped/native/strict/fmt/bans, exact scanner prune, frozen source review and the
venue replay remain correctness gates. Under the current speed-mode ruling,
eligible venue-only work uses one reviewer CLEAR and a guarded owner merge;
CPU/certification and auditor GO are not gates for this removal-only slice.
CI is the family-size record. No local combined measurement is required while
combined headroom exceeds 200 KiB.

## Existing API review

fe7lgm reviewed scratch28e91961 against actual74a0: existing API fit CLEAR,
no engine source. This revision incorporates P1 typed mint refusal, P2 same-DATA
successor timing, P3 pure-decode/fact/effects ordering and P4 conservative
no-new-fill replacement condition. P5 lifetime and P6 secondary-read versus
primary-write pins remain. Native/financial original review is the next gate;
no Backpack source or executed originals are claimed by this plan.

Narrow hcm plan corrections: initial partial/unknown fill cannot prove the frozen
full quantity safe, so automatic replacement requires explicit finite zero-fill
proof with retained cumulative high-water evidence. A never-issued successor is
place=None, whereas a real issued/precommit-refused leg is Some(NotSent).
These are financial/control-plan requirements, not source execution claims.

## Current composition

Originals start on actualfff73624e857 after rechecking open Backpack PRs (none).
The shared core and foreground Backpack remain untouched in the original stage.
No engine source is authorized. This plan and the originals/source ship together
in one consumer PR after exact review and the current correctness gates.

## Missing installed Primary disposition

The exact API disposition from fe7 is local typed NoPrimary when no installed
Primary identity exists. There is no mint call and the discovery/read key is
never passed as a write credential, even to provoke refusal. Emit one Warn per
accepted snapshot, not per row, with the core repair.unavailable wording. With
an installed Primary, always call the core with its genuine Credential and
retain every core authority/refusal check. Secondary-only controls must pin zero
mint calls, zero repair preparations/HTTP and exactly one local diagnostic.

## Implemented safety and qualification

Replacement requires explicit finite zero cumulative fill in the accepted raw
snapshot and cancel acknowledgement. Unknown, partial, decreasing or later
positive evidence is sticky across accepted REST rows, private frames and Store
rollback. Raw row evidence is independent of trigger eligibility and preserved
when a malformed row retains an older Store copy. A matching unlisted-symbol
frame cannot leave a held successor authorized.

Held Prepare and Bind work retires by exact Context; late preparation and old
claims cannot release it. Before release this is Some(NotSent); an already
released successor remains Unknown. Suppression without issuing a successor is
place=None, preserving the actual accepted cancellation. These intentionally
safer choices replace native full-quantity repair after insufficient fill proof.

A malformed accepted placement row keeps the old Store identity and creates no
replacement row. The accepted placement still invalidates older Orders tokens
before reconciliation. Its Accepted write fact is never changed to NotSent.

The 22 original controls remain unchanged. Three additive initial nonfinite-row
controls were corrected after inspecting their historical failures: the native
strict decoder already rejected those rows before any cancel. They now pin
no row, cancel or place rather than requiring a nonexistent cancel. The initial
28-test log was 3 native GREEN, 22 meaningful assertion RED and 3 missing-cancel
helper failures; it was not 25 meaningful unsafe-placement REDs.

On the actual #2202 composition the 21 lifecycle controls pass, including both
Background-delivery settlement controls that were assertion RED before that
core fix, 255-peer single-credit turnover, all-256-credit reuse, both command/
repair lock directions, raw REST proof and held/released effect distinctions.
The original 28 controls pass on the locally adopted source. Full composed
native, strict/fmt/bans and CI/replay results remain separate final evidence.

## Composed correctness checkpoint

The b37013c4c1d landing tip is merged (0f4d893ad31), including the actual
#2202 repair-settlement fix and #2203 affected-venue CI path. The merged
Backpack suite passed 427 tests across 28 targets, with no failures and one
existing ignore. The reviewed test-only module relocation then passed all
nine unit tests; all-target strict Clippy and workspace formatting passed.
The relocation keeps both private module parents and every signing/header
assertion, and moves no production sender outside the scanner. The raw
account request entry and its three raw credential-selection sites are
removed; the conservative trading macro entry remains. Test inventories and
signing/socket manifests receive no allowances. Exact final bans and frozen
PR correctness/replay results remain separate gates.
