# Shared repair Mutation custody: Phemex and Backpack

Status: revised after fe7lgm review and explicit overseer R2/R3/R6 preservation rulings; shared authority/API direction CLEAR with fe7 E1-E3 and revised S3, plus overseer bounded joint-authority ruling. Originals/source and source ownership remain separate; no implementation claim. PLAN ONLY, jointly authored by codex-mxb4mq (Backpack) and codex-ih35js (Phemex); engine reviewer claude-fe7lgm. No originals, core source, venue source or inventory changes are contained in this plan. Both repair migrations wait for actual shared-core landing. One shared plan supersedes a cancel-only seam proposal.

## Real consumers and inspected sources

Backpack account-read branch 80022eaf42501e75cf715ee1bed4f1db5c968176, based on actual nonce landing c5d7c3595af: trading.rs trigger_repair/start_repair/run_trade and account.rs Orders repair discovery. One remaining real raw HTTP sender, account.rs fingerprint34b6 count1. Internal high-range u64 jobs are bookkeeping, never caller CommandIds. trading.rs fingerprint9ecfe is a conservative matches! escape, not another sender; keep it unchanged.

Phemex consumer pin supplied by ih35js: #2130 f2b5438dfe03406bc4ca9d1758f2e954ef3253dc, docs/plans/RUST_PHEMEX_REMAINING_FUNNEL.md. Its four raw holdouts are unchanged. account.rs orders_cancel_all/cancel_landed plus trading.rs cancel_late/late_next/dispatch(None) form the cancel-only consumer.

Existing engine constraints remain intact: request_context_registry.rs issue_until denies Scope::Background with Operation::Mutation and denies Read-parent Mutation escalation. execution_registry.rs begin_background_execution accepts Read only. Existing planned/serial Mutation admission requires genuine Command Preflight. Neither an expired original command, a Read DATA callback, a stream frame, nor an internal numeric job grants this missing authority.

## Native contracts that the shared design must represent

| Consumer | Eligibility and sequence | Required failure/evidence behavior |
| --- | --- | --- |
| Phemex cancel-only | cancel-all records only unknown placement client IDs absent from Store at dispatch, with reconciliation generation. A definitive order read or private stream finds matching symbol/owed client ID, excludes children, and selects Open/Updating native order IDs. Native cancel_plan deduplicates, groups symbol plus position side in stable order, chunks at10, and chooses single DELETE /g-orders/cancel or grouped DELETE /g-orders. One native FIFO releases exactly one repair request at a time. | No place/amend permission. Fill evidence emits native CHILD_LEG_FAILED/check-position diagnostics. Accepted effects and exact IDs survive ambiguous replies; released uncertainty is Unknown. Revision/amount/filled rollback guards remain exact. Failure refreshes account, reports cancel-manually, releases the latch and advances to the next independent repair, without resending the failed cancel. |
| Backpack cancel-then-place | A native observed trigger order produces a corrected NativeOrder using existing market precision and trigger/quantity rules. Native-ready primary account credentials are required. One internal job serially cancels old_id, then places its replacement only after the cancel is definitively accepted under native acknowledgement rules. | Preserve old/new identity, full wire fields, cancel latches, Store publication and stream-time rollback fences. Failed/ambiguous cancel cannot authorize replacement. Accepted cancel plus failed placement is a partial effect; keep its journal and exact IDs. Released placement uncertainty cannot become NotSent or an automatic resend. |

Phemex unknown-placement discovery retains its existing exact SigningRelease RequestId/raw signing-expiry custody: read reconciliation waits until actual raw expiry+5000ms; matching private-stream arrival may repair immediately. This is an eligibility fence, NOT the repair root deadline. Panic cancel-all's existing fence_uncertain_cancel_all=false path is untouched. Complete reads retire only owed/lost generations they postdate. Do not broaden repair eligibility, suppress private-stream immediacy, or redefine the original unknown write's result.

Backpack R2 answer: native repair DOES cover orders with no originating Place command in this engine session. account.rs account_item computes repairs for any valid known-market Orders row with trigger_price present and trigger_by absent, and accepted Orders snapshot publication later calls start_repair. Neither path checks local Place provenance. Existing tests missing_trigger_by_is_repaired_and_does_not_reopen_a_replacement_filled_before_response and trigger_limit_repairs_preserve_post_only_time_in_force_and_reduce_only start from ready(), open a socket, then supply an account Orders row; no Place command precedes repair. This includes account orders discovered after restart or from another client in the structural native behavior, without claiming a live external account experiment.

R2 overseer ruling: PRESERVE these foreign/restart/UI-order repairs through an explicit named, account-scoped Background repair entitlement minted from verified account-snapshot discovery, with its own audit trail. This is a specifically authorized repair authority, not a generic Background Mutation exception, a fake originating Command or escalation of the Read Context. Exact verification/minting and evidence shape still require fe7lgm review. Primary signing selection is independent from the read credential that supplied discovery; a secondary snapshot key never gains write authority. Internal u64 allocation never supplies a command receipt.

## Proposed common authority, for engine API review

Names in this section are proposed responsibilities, not existing APIs.

1. ORIGIN has two explicitly separate, engine-issued cases under one repair owner/journal. Command-origin entitlement (Phemex), per fe7 M1/M2, may be minted only AFTER genuine admission while io.core.entry_bound(command) holds for that exact current BOUND entry. Fresh, Legacy, Revoked, ended and borrowed callbacks refuse without allocating IDs or changing entry state. Mint consumes only separate repair/owner-registry capacity, never the Command Preflight loan, Plan slots or journal reservation; no combined admit-and-mint API is required. It binds the minting command identity, selected PRIMARY, declared cancel intent and unique owed subject keys plus generations; subjects may span native symbols/sides and are bounded by existing owner-registry capacity, not10. A pre-admission mint refuses BEFORE any ID allocation, leaves the fresh entry intact for real admission and returns no entitlement. Do not use the Fresh-only refuse_command check after begin_preflight: validate the issued admitted handle plus the still-current original entry instead. Later discovery selects/spends that authority; a stream or Read never fabricates a Command entry. Snapshot-origin entitlement (Backpack, explicit overseer R2 exception) is a named account-scoped Background repair authority minted only from verified account-snapshot discovery. It binds engine issuer epoch, account/PRIMARY identity, exact snapshot/observation generation and native subject/shape, with a separate auditable origin tag. Ordinary Background+Mutation admission and Read-parent escalation remain denied. No arbitrary native venue string, current callback or decoded row alone is proof. fe7lgm must specify the exact snapshot-acceptance proof and core checks; the adapter keeps native financial/row semantics, and no new Context/receipt/token is forged.
2. SHAPES: CancelOnly carries one native grouped request with at most10 exact IDs; CancelThenPlace binds one original order plus replacement symbol/side and frozen adapter financial declaration. Core enforces dedupe/count/declared intent and symbol; native Phemex chunking/group order and financial codecs remain native. Consumption is per SUBJECT, not once for the entire entitlement. Each CancelOnly root atomically spends at most10 previously unspent eligible subjects from one or several compatible command-origin entitlements under the revised S3 rule, preserving native symbol/side grouping; one entitlement may found several independent FIFO roots. Spending a root never discards its other unspent subjects. Spent subjects cannot be spent again after failure or Unknown. Native client ID plus owed generation is the original subject; later observed native order ID is a constrained refinement requiring the same native mapping proof, not an arbitrary new identity or renewed authority. CancelThenPlace stays exactly one subject from one snapshot-origin entitlement; snapshot origins never join a multi-entitlement root. Incompatible authorities are never merged. Backpack discovery continues to repair foreign/restart/UI orders without a local Place; do not silently reduce it to a command-origin subset. This narrow named repair authority is not generic autonomous placement or a general permission to send new orders.
3. LIFETIME follows overseer R3: NO time-based entitlement expiry or guessed evidence horizon. Entitlement remains with the known-open order/owed unresolved order generation and ends on terminal event or unknown-resolution; exact engine teardown/account epoch revocation also retires it. Engine reviewer must pin unknown-to-known transition and definitive resolution versus merely absent/stale snapshots. No root wall clock. Each physical leg gets explicit20_000ms per-leg Policy and RetryPolicy::Never, matching audited JS managed-http.mjs177 and native transport/http.rs111 request(None) defaults as accepted by fe7. This covers that leg only, not aggregate workflow life. One engine-recorded accepted cancel permits at most one successor, so a slow cancel cannot lose placement to a newly introduced aggregate cap. Bound stored entitlement/root/evidence counts by the existing owner registry, not by a behavior-changing wall clock. Spent subject authority cannot be renewed by replaying an old observation, copying an order ID or minting a new generation for the same unresolved effect; other unspent subjects of the entitlement remain eligible until their own terminal/unknown-resolution lifecycle ends.
4. OWNER/OVERLAP follows fe7 R5: Owner::Repair {entitlement, root, step} and Scope::Repair admit Mutation only. Read stays outside; existing Background Mutation denial is unchanged. write_intent must derive Cancel/Place from declared step rather than non-Command fallback Place. Order locks key repair ownership by root, never fabricated CommandId. Existing engine lock conflicts REFUSE both directions, not an extra queue; native FIFO remains the queue, with admitted work bounded by entitlement capacity. Released Unknown fences its symbol and queues ordinary reconciliation; placement retains current placement uncertain-fence behavior. Native latch/notice/independent-group behavior on refusal must be explicit and tested, not silently treated as a successful repair.
5. CUSTODY/EVIDENCE: opaque issuer-bound root/slot/attempt handles reuse provider, route/bind/claim, managed release and shared write-result journal. Record successful native cancel effects under exact live DATA, retire predecessor physical credit, then enable only its declared successor; at most one physical leg per workflow. Refused/Unknown cancel cannot authorize placement. Later signer/precommit failure cannot erase accepted cancel IDs/effects or turn the entire workflow into NotSent. Released ambiguity stays Unknown with exact accepted IDs. No automatic resend. Original user Command receipts remain sealed/isolated; repair has no synthetic CommandDone/Failed fallback. Entitlement lifetime alone is not DATA permission for stale delivery.
6. TERMINAL NOTIFICATION follows fe7 R4: invoke exact terminal/cleanup with a Cx in engine reconciliation custody outside any command entry, like reconcile_uncertain. Native Store/latch rollback guards run; Background Read refresh may be issued, and the next independent FIFO group may spend its own entitlement immediately. Do not stall until a user command. No-Cx teardown/expiry cleanup restores native Store/latches without ID allocation or I/O at the top of a new command. Notification never grants permission to mint another origin, renew or resend an effect. Physical per-call expiry can deliver genuine failure-only cleanup without inventing DATA.
7. CAPACITY follows approved overseer R6: entitlements and roots draw from existing owner-registry capacity with atomic refusal and exact credit cleanup. Failed mint adds a typed diagnostic to the minting Command result without failing that Command, plus ONE Warn notice. Proposed exact common Warn text for the capacity test is "Automatic repair authority unavailable: capacity exhausted"; keep the typed reason and affected subjects in structured fields, without secrets. Snapshot-origin Backpack has no minting Command result: fe7 must specify its typed non-command audit/cleanup notification plus the same one Warn, without inventing a caller terminal. Never claim retained auto-repair permission after refused mint; preserve independent native FIFO and Store/latch cleanup. Exact diagnostic representation/text is an implementation-stage pin, not a new source change here.
8. FEATURE/SIZE follows fe7 R6: common repair core behind one Cargo feature enabled only by Phemex/Backpack consumers. Other families must show zero-byte change, with paired Bybit/Bitget and combined on composed shared-core tree. No core feature/source activation in this plan-only assignment. Existing Command and Background Read controls remain exact.

## fe7 S1-S4 snapshot/lifecycle refinements

S1 discovery is confirmed HTTP ONLY in Backpack: trigger_repair has one caller,
account_item handling managed Background account Orders items; accepted whole
Orders snapshot publication in account_response starts repairs. private_message
updates native Store/order/fill state but does not call trigger_repair/start_repair.
No stream-origin repair mint is needed. The legacy34b6 sender is now only raw
repair writes; account GET discovery migrated in actual #2139 (removed90455717).

Proposed snapshot mint point is the exact current Background HTTP DATA callback:
validate_background_read_http_delivery must succeed, native Orders snapshot
commit must be accepted, and Store publication precedes mint. Mint then precedes
finish_http_context; current start_repair is after retirement, so it cannot be
used as the live mint callback. Core requires Store.order(subject) exists with
the declared symbol; one snapshot-origin entitlement/subject, CancelThenPlace
only, replacement symbol identical. Exact Read root/request identity is the audit
origin. Failure, stream, Session/Command delivery and later timer cannot mint.

E1 settles S1(a): Egress is ROUTING, not key identity. Primary-only snapshot
egress is withdrawn by fe7; native Secondary discovery remains supported. At
exact delivery core checks the Read selected KeyRef is still owned by this
engine (existing signing owns(KeyRef)), and Read issuance/mint share the SAME
credential-install generation. One new reviewed core seam adds a generation
counter bumped on ANY credential install/replace/clear and records it on Read
issuance and mint; generation mismatch takes S4 refusal. Independently select
native-ready installed PRIMARY with existing Mutation selection at mint; absent
primary takes S4. The Read never lends a signing key or Mutation rights, no GET
key is changed, and secondary receives no write authority. Account/issuer,
accepted snapshot/Store subject and all exact DATA checks remain mandatory.
Pin secondary-positive discovery, stale credential-generation refusal after
replace/clear/reinstall (including same visible key), no-primary mint refusal,
and zero secondary write permission. Feature isolation must retain other-family
zero-byte gate despite the new generation bookkeeping.

S2 at most one live entitlement per(account,native order ID), including
spent-but-unresolved/Unknown. Repeated snapshots cannot remint or duplicate a
cancel/replacement. UNSPENT subject retirement is exact-handle/idempotent under one of:
Store terminal for bound ID (filled/cancelled/rejected); adapter retire under a
COMPLETE definitive order read postdating the subject generation; teardown or
account epoch revocation. Incomplete/superseded/stale absence never retires.
E3: once a root spends a subject, that ROOT lifecycle governs it. Old-order
Cancelled is an expected root effect, never grounds to suppress its one declared
placement successor. The root ends after successor accepted/terminal/Unknown,
or before-release successor refusal (NotSent for that leg only; accepted cancel
survives). Teardown and credential-generation/account revocation still retire
unsent successor and retain released evidence/Unknown. Pin accepted cancel,
then Orders snapshot showing old order Cancelled, then exactly one placement.
No aggregate wall clock or restored future admission is introduced.

S3 cross-origin native grouping is now CLEARED by fe7 and explicitly APPROVED
by the overseer. The ih35js source audit showed native grouping has no origin
field, so same-side co-observed orders from two cancel-alls share one bulk
/g-orders DELETE. Splitting by origin is rejected: preserve native path/count,
chunking/rate/ack atomicity and FIFO cadence.

A CancelOnly root may atomically spend at most10 ALREADY minted subjects from
several COMMAND-origin entitlements when every member shares issuer, account,
credential-install generation, recorded PRIMARY KeyRef (still owned and selected
at root admission), Cancel intent, native symbol and position side. Every subject
is bound by M3, unspent and not retired. Core validates every member before any
spend; one invalid member refuses the candidate with NOTHING spent. Compatible,
individually eligible cohorts may be admitted separately when the entire group
cannot join; no old credential retagging or fallback for an ineligible subject.
No subject is minted, reassigned or reminted at discovery.

One root and physical journal retain EACH subject's exact ID, origin entitlement
and accepted/Unknown/NotSent facts. Per-order ack attribution is mandatory:
partial ack never settles a subject whose order was not acknowledged. A failed
or Unknown admitted root spends all its subjects; none is replayed from any
origin. A failed mint has no subject authority and cannot join. CancelThenPlace
remains one subject/one entitlement and no snapshot-origin participates in this
joint root. Controls: two cancel-all origins on one side produce the native ONE
request; mixed issuer/account/PRIMARY/install generation/intent or side refuses
atomically,6+5 subjects refuses, and partial ack preserves each distinct origin
and unacknowledged subject effect. No blanket command-origin split is introduced.

S4 snapshot mint refusal has no caller terminal. Typed MintRefused(origin tag,
subject,reason) is delivered in R4 reconciliation Cx custody with exactly one
Warn. Native Store/latch state is unchanged; later snapshot may try again only
if no live/spent-unresolved/Unknown entitlement exists. Add negative controls
for failure delivery, mint-before-publication, duplicate snapshot, incomplete
retirement and cross-entitlement spend. E1
independent same-account PRIMARY selection preserves secondary-signed discovery;
there is no primary-egress requirement.

## Implementation and source-stage pins after direction review

- Snapshot authority RESOLVED by fe7 E1/E2: exact live DATA, accepted Store publication, currently owned selected Read KeyRef, matching credential-install generation, and independently selected native-ready PRIMARY. No egress/key confusion or Read conversion; mint before finish_http_context, spend afterwards. Exact implementation remains source-reviewed.
- Lifecycle proof: distinguish terminal/unknown-resolution from incomplete, superseded or stale absence; preserve Phemex any-age private-stream cancellation while bounding resource counts. Owner review must address reset/epoch/teardown, and repeated discovery after accepted cancel/Unknown without a second placement.
- Phemex mint custody RESOLVED by fe7 M1/M2: trading.rs:1164 (empty physical job) and :1226 (nonempty job) are the ONLY orders_cancel_all callers, both synchronously after begin_preflight in the original trade_command. The nonempty case also admits its serial Plan first. Their State::Bound entry is legal for entry_bound(command); the earlier Fresh-only suggestion is superseded. Keep native gates/admission, then mint, then latch. Refused mint leaves Plan slots, journal and Command completion unchanged except approved R6 diagnostics. Later serial/ready/timer callbacks have no original Command entry and cannot mint.
- Subject refinement RESOLVED by fe7 M3: mint (client ID, generation); only at root admission may a validated observation bind each subject to AT MOST ONE native ID with symbol/position side. Binding is immutable once spent, adds no subjects, and child legs bind none. A root spends at most ten same-symbol/same-side unspent subjects; entitlement subject count is bounded by capacity, not ten. Terminal/unknown-resolution or teardown/epoch revocation retires the subject. Preserve native grouping order and later ID refinement rather than guessing aliases.
- Empty-job rule per fe7 M4: no owed subjects means no mint. Zero immediate physical cancel steps can still leave owed unresolved placements; mint for the actual nonempty owed set before done_empty_plan, never an empty entitlement.
- Typed mint refusal: exact representation of successful Command plus additive diagnostic and one Warn; snapshot origin has no caller terminal, so use auditable repair-origin notification rather than fabricated result. Preserve original command evidence and do not fail it merely for repair capacity.
- Exact step slots, write_intent, root-keyed locks, reconciliation Cx notification, physical timeout defaults and Cargo feature boundary remain source/API review gates. Overseer rulings supersede the previous626a draft's command-only/finite-expiry restrictions; those restrictions are withdrawn, not additional review requirements.

## Planned originals and working controls, after plan review only

All I/O controls must drive real selected provider Prepare/Bind/Claim/release; private opaque fields cannot be forged. Existing native wire/financial assertions stay unchanged.

| Case | Required control |
| --- | --- |
| Two consumers | Phemex grouped cancel-only plus verified snapshot-origin Backpack cancel-then-place, including foreign/restart/UI orders. CancelOnly cannot request placement. |
| Mint custody | Command-origin mint outside genuine entry fails. Snapshot-origin mint requires its named verified account snapshot proof; ordinary Read DATA/stream cannot mint by itself. Wrong origin/issuer/account/PRIMARY, stale snapshot, duplicate subjects, over-capacity mint, over10 root subset or intent escalation refuses atomically. |
| Entitlement lifetime | No arbitrary wall expiry: old-but-open eligible order can still repair. Terminal/unknown-resolution retires subject authority; stale/partial absence does not. Per-subject spending/no replay; slow accepted cancel retains its one placement successor. |
| Admitted mint | Before-admission mint allocates nothing, returns no entitlement and preserves the entry. Both native empty Preflight-only and nonempty admitted Plan paths mint after admission in their original callback; stale/foreign handles fail. |
| Subject subsets |12 owed client IDs across2 symbols produce2 roots from1 entitlement; root1 leaves root2 subjects unspent. Duplicate consumption, failed-root respend and invented native-ID refinement fail without affecting remaining subjects. |
| Classification | Repair cancel classifies Cancel, not Place; root-keyed locks refuse both foreground/repair conflict directions with independent journals unchanged. |
| Notification | Same reconciliation Cx drives cleanup/refresh/next independent FIFO without a user command. No-Cx teardown does no I/O/ID allocation; fresh command admission remains intact. |
| Identity | Installed primary signature differs from valid secondary/raw material and matches the provider; secondary-only/public-only cannot gain writes. Exact account/issuer/epoch mismatch refuses atomically. |
| Eligibility | Phemex owed absent client-ID generations, child exclusion, fill/manual diagnostics and native grouping. A second native ID for a spent subject, mixed symbol/side or eleven subjects in one root refuses; an entitlement with more than ten subjects retains all its native groups. No owed subjects mints nothing. Backpack native trigger formatting and native-ready gate stay exact. |
| Snapshot proof | Exact accepted live Background HTTP DATA after Store publication; missing Store subject, failure/stream callback, duplicate observation and wrong symbol deny. Primary and secondary discovery both use exact owned-KeyRef/install-generation proof plus independent native-ready PRIMARY Mutation selection. |
| Lifecycle | Second snapshot cannot remint live/spent-unresolved/Unknown subject; incomplete/stale absence never retires; complete postdating proof and exact terminal do; foreign handle is inert. |
| Cross entitlement | Native Phemex same-side cross-origin grouped DELETE must retain wire grouping; exact reviewed common grant or bounded joint-authority disposition is required before originals. Wrong issuer/account/PRIMARY/intent unions never send. |
| Original command isolation | Old sealed/expired command, reused numeric IDs and simultaneous foreground writes cannot be completed, rebound or have their journal changed by repair. |
| Serial evidence | Backpack accepted cancel opens one placement only; failed/Unknown cancel never does. Accepted cancel plus placement prep refusal retains partial effect. Phemex two groups sequence despite first failure. |
| Boundaries | Real parked preparation, per-call release/expiry equality, exact terminal/unknown-resolution lifecycle, NaN/+Inf/-Inf, wrong issuer/slot/attempt and duplicate/stale response; no successor, renewal or financial publication on denial. An accepted cancel must not lose placement to an invented aggregate clock; entitlement lifetime alone cannot authorize expired DATA. |
| Outcome | Pre-release refusal versus released loss, mixed/grouped ack, malformed response, retained exact IDs and Unknown after a later provider error; no whole-workflow false NotSent. |
| Credits | Saturated capacity admission atomic;255 real peers+predecessor prove retire-before-successor; peer survival/full reuse, cancellation, expiry and teardown release all exact physical credits. |
| Host parity | Native and JS/capsule real release/signing, cancellation/late callback behavior and effect journal agree; no raw export callback after poison. |

Native Phemex working references include failed_late_cancel_is_reported_as_an_error, late_landing_seen_on_the_private_stream_is_canceled_immediately, late_cancels_in_two_groups_are_sequenced_without_false_errors, plus signed-expiry/cancel-all generation controls. Backpack working references must pin current native repair cancel/place wire, stream-time rollback, amount/trigger precision and partial-ID behavior before adoption.

## Scope, ownership and landing sequence

1. This ONE joint plan consolidates the coauthor native audits, overseer preservation rulings and fe7 direction CLEAR. Commit it with one index row on its own docs branch; retain the immutable hash and consumer pins. No production or original execution claim is made by committing the plan.
2. Separate shared-core originals then implementation only after explicit source ownership and API approval. No core implementation is authorized by this plan-only assignment. The consolidated plan reflects the reviewed S3 direction; exact originals and source remain separate review phases. Core changes need engine/provenance reviews, scoped checks and composed Bybit/Bitget/combined measurements, exact qualifying CI and auditor GO.
3. Core lands first. Each consumer plain-merges ACTUAL prerequisite landings and separately writes/adopts reviewed native controls. Never assume a stack or rebase. Backpack trigger-repair source stays held; Phemex raw holdouts remain unchanged.
4. Venue source adoption, exact map pruning, provider/native source reviews, scoped tests, paired family+combined sizes, frozen push/one qualifying run, READY and exact-tree GO remain separate. Other Backpack/Phemex slices may proceed independently.

No current runtime, test execution, API availability, production readiness or automatic mutation authority is claimed by this plan.

## Originals (claude-fe7lgm, reviewer codex-ih35js)

Engine feature `repair-mutations = ["order-locks"]`; only the adopting Phemex
and Backpack families will enable it, so other families stay byte-identical.
The API lives in `engine/src/repair.rs`, and the controls in
`engine/src/repair_tests.rs` drive real command admission, real Background
Read delivery and the real Prepare/Bind/Claim path.

Two engine facts settle plan items:

- Identities and the signer are frozen once the engine starts (`set_identities`
  and `set_signer` refuse after `started`, and no event installs, replaces or
  clears credentials). E1's credential-install generation is therefore
  structural within one engine instance and needs no counter. The proof at mint
  is `owns(read KeyRef)` plus the installed primary. A GREEN control pins the
  precondition.
- There is no engine teardown path. Entitlements and roots retire through the
  subject and root lifecycle only.

API (inert at this checkpoint; each call refuses with `Invalid`):

| Call | Custody |
| --- | --- |
| `mint_command_repair(primary, subjects)` | `entry_bound` for the current entry; refused before any ID allocation otherwise |
| `mint_snapshot_repair(read, primary, subject, symbol)` | exact live Background DATA delivery, owned Read key, published Store order with that symbol, no live entitlement for it |
| `admit_repair_cancel(members)` | at most 10 compatible subjects, possibly from several command-origin entitlements; atomic spend |
| `admit_repair_replace(entitlement, subject, binding)` | the cancel leg of one snapshot subject |
| `repair_successor(root)` | once, after an engine-recorded accepted cancel |
| `retire_repair_subject(entitlement, key, Retire)` | unspent subjects only; idempotent |
| `Venue::on_repair_notice` | once per ended root and per refused snapshot mint, outside any command entry |

Every repair leg signs with the recorded primary under
Policy{Mutation, 20_000 ms, Never}. Each `Warn` reads "Automatic repair
authority unavailable: <reason>", with "capacity exhausted" for capacity.

Original execution on tip `d3b392b0b14` with `--features repair-mutations`:
25 new controls, 24 assertion REDs and 1 GREEN (frozen identities). The
other 937 engine controls pass. No fixture panic or missing method counts as
a RED. Setup assertions (a successful mint) gate the later suffixes; those
suffixes are not execution evidence until source exists.

Deferred to source review, where the inert stubs cannot express them: wrong
issuer or foreign handles, saturated-credit and retire-before-successor
accounting, mint after a serial Plan admission (the Phemex nonempty path),
the per-subject partial-ack facts, and host (JS/capsule) parity.
