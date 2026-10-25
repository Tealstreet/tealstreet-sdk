# WEEX remaining depth and socket lifecycle adoption

Draft only, owner codex-uufub4, native/API reviewer codex-hcmq3f. Inventory pin:
35ae6c6b2548c79f426c3f5c4d66e4e2e349a86a. Actual landing tip at draft time is
29186f5fe95539d77c1eb68da971914aec7e9e49 (#2145, disjoint journal iterators).
#2153 is frozen at88458b9220d; this artifact changes neither its branch nor CI.
The overseer authorizes compiling originals locally on518d while #2153 CI runs.
Before production source, plain-merge actual #2153 landing and measure on the
then-current tip. No rebase, production/Cargo/inventory edit or push is authorized
by this checkpoint. Kraken reviews preempt this work.

## Exact inventory and scope

The pinned request inventory is16 fingerprints/22 occurrences: account8/13,
history2/3, lib6/6. Only TWO are physical senders: public streamed discovery
067aa and the nonstream Depth sender61034. #2153 removes067aa and therefore
projects15/21, not zero. The next runtime sender adoption removes61034 (1/1),
leaving14/20 conservative references unless a genuine ownership/provenance
conversion removes them. They must not be advertised as20 uncustodied sends.

Pinned signing inventory: four singleton rows, one native diagnostic in
account.rs and three trusted-setup fixture lines in secondary_tests.rs. WEEX
production already retains opaque Identity refs and uses shared provider work
for REST and signed upgrade; there is no remaining local primitive to delete.
The scanner currently excludes WEEX sign modules. Two socket singleton rows are
real physical lifecycle debt, and WEEX remains in socket-legacy-families.json.
Credential inventory is three exact read_credentials tokens in lib.rs; they
are an adjacent retained compatibility surface, not a signing-material count.

Complete machine-readable hashes, multiplicities and source token mappings are
bound in /tmp/codex-weex-remaining-inventory.json. All rows are enumerated below;
hash prefixes identify existing rows, not replacement allowances.

| Inventory/file | Fingerprint or term | Count | Existing responsibility |
| --- | --- | ---: | --- |
| request/account.rs | bbe154e63f63 | 2 | matches!(request.kind, Algo), send/item |
| request/account.rs | ad97274a8e87 | 4 | Normal/Algo kind classification |
| request/account.rs | 7b3506f07b5f | 1 | native page query format |
| request/account.rs | f0ba50adb6e0 | 1 | Metadata stream classification |
| request/account.rs | 66e350394c2f | 1 | Balance classification |
| request/account.rs | f647ab9388bb | 1 | Normal order-list classification |
| request/account.rs | 1780f5566f27 | 2 | Config/Metadata classification |
| request/account.rs | 081836d9c775 | 1 | Positions diagnostic classification |
| request/history.rs | bbe154e63f63 | 2 | Algo item/response classification |
| request/history.rs | 6d43c66aaf7a | 1 | Income response classification |
| request/lib.rs | 067aa233d0eb | 1 | streamed Markets/Daily/Book; #2153 owns removal |
| request/lib.rs | 61034a5650ef | 1 | raw nonstream Depth snapshot sender |
| request/lib.rs | 869f03854972 | 1 | HTTP-status native failure router reference |
| request/lib.rs | 93e77dda5893 | 1 | malformed-row native failure router reference |
| request/lib.rs | a40239e59613 | 1 | empty catalogue failure router reference |
| request/lib.rs | 1805af84e51e | 1 | on_request_failed native router reference |
| signing/account.rs | 528411274613 | 1 | unchanged API-key/secret/passphrase diagnostic text |
| signing/secondary_tests.rs | 2791b1cb0b8e | 1 | trusted primary fixture material |
| signing/secondary_tests.rs | 8fde5a2b7840 | 1 | trusted read fixture material |
| signing/secondary_tests.rs | 96590eff7cf7 | 1 | empty read-material negative setup control |
| socket/account.rs | f9aa006cf88e | 1 | raw signed private upgrade connect |
| socket/lib.rs | 24ca710070ba | 1 | raw public connect |
| credential/lib.rs | read_credentials | 3 | public opaque Options/validation bridge; preserve |

## Inventory closure decision required

A truthful zero-legacy-runtime conclusion is possible after the Depth and BOTH
socket adoptions, but deleting all current inventory rows is not automatically
justified. Sixteen request occurrences are macros containing a local Request
variable in a Cx-escaped scope; four are a native venue failure helper, not raw
Cx terminal calls. The four signing rows are diagnostics/test oracles. Preserve
their bodies, error wording and assertions. Never rename Request/fail/secret or
remove financial controls solely to shrink the scanner.

Before claiming final map zero, request exact bounded disposition from the
request/signing checker owner: demonstrate genuine owner-bound terminal and
failure routing for every Task, then use the canonical unchanged scanner. If
legitimate non-legacy residues remain, a separately approved provenance/class-
ification correction is a companion prerequisite; this venue slice cannot add
exemptions, weaken a checker, drop diagnostic text, or hand-delete still-present
hashes. Strict sign-module scanning must separately audit opaque work-builder
imports/calls and test oracles before any reviewed map update. No generic core
or checker change is inferred. Remaining benign rows stay exact until ruled.

## Runtime cutover

1. Adopt only the remaining background Depth snapshots. Retain symbol, native
   book generation, user/refcount and any current triggering socket epoch next
   to an independent Background keyless Read context. Use explicit Primary
   egress, Never/no pacing, and a proposed NEW20s physical observation bound.
   No aggregate walk cap, command root or renewed retry authority. No context
   while a native pause/backoff is deferred. Initial SubscribeOrderBook queues
   its snapshot immediately; reconnect queues connect THEN snapshots BEFORE
   WsOpened. Gap healing likewise issues an independent observation, never work
   waiting behind a newly admitted depth interest. Snapshot authority is the
   genuine Background Context plus native book generation/membership; no current
   epoch or depth-topic handoff is required where native has none.
   Exact DATA/current generation precede codec/status/429/Store/book effects.
   Preserve limit200 query bytes, snapshot+buffer join, sequence-gap healing,
   2000-delta cap, accepted book continuity and native jittered1s..60s backoff.
   On accepted response, publish the joined snapshot/book while custody is
   live, retire its exact Context, THEN call native reconcile_topics. Its depth
   eligibility remains EXACTLY last.is_some() && retry.is_none(). For a new book
   with no last snapshot, depth interest follows acceptance; never subscribe
   early to obtain authority for that same snapshot. Reconnect may retain a
   prior last value, so its normal Opened reconcile can remain eligible before
   the replacement response: do not clear last or add a new freshness gate.
   Response-driven reconcile/subscribe always follows that response's retirement.
   Finite owned failure retires once then native recovery; invalid clocks clean
   only issued custody and create no new timers/HTTP/sockets/diagnostics.
   Unsubscribe/reconnect generations retire only their exact outstanding work.
   Remove orphan generic raw get/DeferredGet portions only after proving all
   remaining callers; preserve #2153 public-seed and candle replay semantics.

2. Adopt BOTH public and private managed socket feeds together. Registration
   enables engine-wide managed mode, so public-only adoption would suppress the
   raw private lifecycle. Register both before either physical connect; partial
   registration failure has no raw fallback. Use connect_socket for public and
   connect_socket_signed with the existing exact Upgrade declaration for private.
   Keep private primary-only selection and trusted provider/signature/header
   bytes; no new credential reselection, login frame, Session or HTTP fallback.
   No shared lifecycle/coexistence seam or production signer rewrite is planned.

3. Proposed policies preserve native supervisor values: connect30s, idle30s, PING5s, reconnect1s..16s, stable30s,
   auth_timeout None, as declared by socket_supervisor and Config::default.
   Private has signed upgrade authentication and no native login/ACK wait:
   propose rolePrivate/AuthMode::None/ControlPolicy::Native/TrafficPolicy::AnyText,
   auth_timeout None. Public uses the matching no-auth native control policy.
   This models readiness, not an authentication grant. Opened performs native
   subscribe/refresh once; Ready must not duplicate. Current epoch is validated
   before parse, notices, ping/pong, account reducers and Store. Malformed text
   retains native liveness. PING/PONG, server ping responses, IDs and private
   account/positions/orders/fill subscription payload/order remain exact.
   Private transport-without-upgrade-headers remains REST-only with its existing
   notice; public suppression causes no public connect/watchdog churn and must
   not suppress eligible private operation.

4. Preserve public desired-topic selection (book/candle10, ticker headroom to100,
   at most90 tickers), sorted topics, batch50 and240 operations/hour behavior.
   Use actual managed interest admission and exact epoch handoffs only for
   post-eligibility topic changes; they do not authorize pre-open/bootstrap/gap
   Depth HTTP. Keep desired versus committed state, wire IDs and hourly quota
   transactional on synchronous/asynchronous refusals. Refused encoding commits
   neither IDs nor quota; successful sorted batch50 encoding preserves the
   native240/hour burst behavior rather than imposing a new one-frame cadence.
   Do not erase refcounts or healthy peers, mark phantom topics, or stop a feed
   merely because a venue proposal was refused. Bound any observation/recovery
   policy explicitly at current finite epoch, with cooldown/exhaustion respected
   by every entry path; store and check timer ID AND actual observation not-before
   before consuming quota. Early/duplicate delivery spends no observation; exact
   equality/late delivery may progress. Preserve native account poll budgets.
   No generic bypass. Removing book A retires only A's own snapshot Context/
   generation and its eligible topic proposal; B's already-issued independent
   Background observation remains usable. A post-snapshot handoff/continuation,
   if genuinely needed for topic work, stays distinct from this HTTP authority;
   capture actual cleanup/rearm traces, never guess continuation IDs. Engine initial
   Ready replay errors have an inherited feed-stop disposition distinct from
   recoverable venue proposals; pin that behavior and obtain a ruling before
   promising broader recovery or changing core.

## Originals and preservation gates

Commit compiling originals before source after plan CLEAR, retaining every
existing WEEX native/financial assertion and event clock. No ignored controls or
setup/compile failures count as RED. Genuine providers and atomic SetTimer/
Prepare/Bind/Claim traces supply all authority and cleanup identifiers.

- Depth: literal native wire, real256-context pressure/refusal/recovery and all-
  credit reuse; held Prepare, Network and physical TimerFired equality; stale
 429 and malformed rows before decoder/rate effects; all3 nonfinite admission/
  returned/preparation failure clocks; exact full Store/readiness immutability,
  exact issued cleanup, independent live sibling and native finite retry once.
- BOTH sockets: signed private upgrade byte/MAC equality at exact dispatch time,
  primary/read distinction, header capability negative, public-before/after-
  private, suppressed-public/private-positive and REST-only startup. First open,
  reconnect, duplicate/stale Opened/Ready/text/close/timer cannot duplicate auth,
  subscription or refresh. Exact wire IDs/ordering, native ping liveness and
  public/private reducer effects remain unchanged.
- Native snapshot/topic ordering: subscribe A+B before public Opened and hold
  their real independent HTTP preparations. Unsubscribe A: only A's captured
  resources retire; B completes exactlyone joined snapshot, publishes while
  live, then retires before response-driven reconciliation. With no prior last,
  neither depth topic is made eligible early. Drive actual public Opened/Ready
  and B's admitted eligible topic update; no new A send/publication. Old A timer,
  preparation and HTTP deliveries are inert; independent Aresubscribe obtains
  new generation/custody. Partial/refcount removal preserves B and private feed.
  Separately reconnect with accepted last values: connect precedes both new
  snapshot preparations, all before WsOpened; keep native retained-last topic
  eligibility. Gap recovery needs no new topic/handoff before its HTTP request.
  Any post-snapshot topic continuation uses actual rebase cleanup/current wake
  traces, with old wakes inert; never assert an HTTP wait behind its own topic.
  Real pending/proposal credit and encoded-frame capacity refusal use valid
  bounded keys and actual current work; no oversized-key proxy. Pin cooldown,
  finite retry budget exhaustion, early/duplicate/exact-equality/late not-before
  timer behavior, resolved timer cancellation and all3 invalid-time timer denials.
  Keep native server ping/PONG/result=false and malformed text in the native
  codec/Other path, never fabricate authentication evidence. Include inherited
  initial-replay fail-closed working control separately.
- A complete action ledger and fixed-time Store.everything snapshots reject all
  effects beyond exact captured cleanup. No blanket Cancel/ClearTimer filters.
  Preserve account reconciliation/page/deadline/settings revisions, trading
  journals/barriers, history roots/prefixes and all #2153 seed controls exactly.

## Size estimate and release gates

Planning estimate only: roughly450..800 production lines across WEEX lib,
account and a focused socket adapter plus additive controls; reducers/sign work
remain unchanged. Managed-sockets is absent from the current standalone WEEX
feature closure. Other adopters suggest around35..65KiB new family linkage;
Depth/adapter code adds an estimated5..10KiB, so reserve40..75KiB family growth.
Combined already links managed sockets through other venues: estimate5..20KiB
incremental combined growth. These are hypotheses, not measured passes.

#2153 composed reference: WEEX1210327/1343488 (133161 bytes headroom), combined
14039272/14155776 (116504 headroom). The estimate fits with margin, but stop for
review if the first measured delta materially exceeds it or the fixed ceilings.
Never raise budgets or tune profiles to make the gate pass. Plain-merge actual
landing tip before the first-source paired WEEX+combined measurements, same
archive command/toolchain/locked inputs, Node/web hashes, JOBS4/CORES4; delete
each scratch target immediately after measurement. Measure Bybit/Bitget only if
an explicitly approved own generic engine change becomes necessary.

After plan/ruling closure: originals, exact source/native/fixture/map review by
hcmq3f, scoped WEEX+strict/fmt/root fast lane/full bans, first-source pair and
composed fit, one freeze push/PR-triggered run, green checks, READY to8x5tm1,
exact-tree GO, then overseer merges. #2153 is handled independently when its
current CI is green. No source/adoption/size/CI claim comes from this draft.


## Cleared ordering and local originals authorization

hcmq3f gives full bounded PLAN/native/API CLEAR to corrected artifact b6a52d5d
with one native-state precision: normal on_close clears last. A retained-last
reconnect control must obtain last from a genuine current snapshot accepted
while waiting for reconnect/open, never inject or preserve pre-close last.
Do not add a new clear-last/freshness gate to connect or snapshot admission.

The overseer then explicitly authorizes local, unpushed compiling originals on
actual518d while #2153 CI remains active, superseding only the earlier
actual2153-before-originals prerequisite. Actual2153 landing still precedes
production source and first-source measurement. Merge review changes forward;
production, old fixtures, maps and Cargo remain unchanged during originals.


## First compiling originals checkpoint

On requested actual518d, the new depth_socket_context target executes19 tests:
13 intended assertion RED /6 native GREEN /0ignored, with no compile or fixture
setup failure. Strict scoped Clippy and formatting pass. An initial fixture-only
copy lint was corrected without changing runtime code; both review-build targets
were deleted. All production, existing fixtures, providers, Cargo and inventories
remain incoming-exact.

The six GREENs pin native snapshot-before-depth-topic ordering, independent
pre-open books, exact primary signed upgrade/no-ACK subscribe bytes, disabled
public/private-positive startup, private sibling PONG survival and inert closed
public ingress. The13 REDs reach real early wire/capacity,20s equality expiry,
stale429, all3 nonfinite admission and returned-failure clocks, and duplicate
public/private Opened effects. The private sibling control directly proves PONG
liveness; it is not a new financial reducer oracle.

Deeper managed-Prepare/release/owned-cleanup/all-credit suffixes are honestly
unexecuted where the first native raw-wire RED stops the test. Real physical
TimerFired/preparation-failure controls, reconnect/gap/retained-last calibration,
transactional interest capacity/quota/burst/not-before controls and complete
source-stage action/resource proofs remain gates. This is a bounded first
originals checkpoint, not full originals/source/readiness clearance.

## Local source qualification and approved native fixture changes

Actual #2153 landed as aa75847b5c9. The first source checkpoint74e5b6d1313
passes the first19 controls; c37b55f1567 removes only the genuine Depth sender,
two socket senders and WEEX's socket-family row. Remaining14F/20O conservative
request references and all signing/credential rows remain unchanged. The
first-source WEEX/combined pair is pinned to bare aa758 versus74e5, separately
from subsequent source corrections and their current-tree measurement.

hcmq3f clears the bounded first19 checkpoint and two exact fixture adaptations.
Seven account stream tests add a local genuine first Opened at existing NOW,
while the original six account reads remain pending. Their prepared opening
actions must contain no new HTTP. Shared setup/reply helpers, every original
payload, financial assertion and clock remain unchanged.

Three public Depth fixtures supersede physical observation clocks only:
`depth_ignores_old_frames_and_heals_gaps_with_buffered_diffs`,
`old_snapshots_and_failures_cannot_replace_a_resubscribed_book`, and
`malformed_depth_is_not_an_authoritative_deletion`. Bootstrap1773295700000 plus
the same relative0..8 offsets replaces their inconsistent epoch-to-zero walk;
replies use each actual current observation. All46 public assertion invocations
and34 raw payload literals remain token-exact/in-order. Other changes drive
real prepared actions at three already-correct Depth clocks and prepare the
four original interest-only assertions at their unchanged event clocks. Shared
response/response_at/request helpers remain byte-exact. Dedicated expiry controls
continue to exercise delayed/equality denial instead of being retimed.

Two further public fixtures use a genuine zero-time bootstrap:
`websocket_candles_allow_two_hours_but_rest_rejects_it` and
`active_tickers_send_incremental_diffs_and_identical_updates_send_nothing`.
The approved `started_at(observation)` helper parameterizes only bootstrap
observations; default `started()` still forwards1773295700000 exactly. Only
these two callers select zero, before their unchanged0..3 commands. Their entire
assertions, payloads and event clocks remain unchanged; every other bootstrap
retains its original clock. This supersedes only these two admission clocks.

Required invalidation is deliberately fail-closed on a refused public interest
clear: native close first invalidates generations and retires every exact held
or released Depth context, then a clearing failure denies public replacement.
It grants no parallel recovery or raw fallback, and must preserve private,
foreign and independent caller work. The saturated512-proposal resource control
must pass before functional closure. Native mixed book-first topic order and
hourly quota recovery require their own exact controls and source review; no
full source or readiness claim follows from the first19 pass.

The expanded local target executes38 controls, retaining the entire original19
prefix. Corrected source passes all38 and all-target strict Clippy/formatting.
An isolated c37 build reproduces both retirement-credit and mixed-wire-order
assertion failures. Three additional nonfinite interest-timer controls reproduce
the lost-cooldown failure before correction: invalid observation consumes only
the timer identity, retaining its epoch/not-before bound until a finite event
rearms it without spending an observation. Persistent512-proposal pressure,
early/duplicate/equality/late wakes, three-observation exhaustion, resolved-demand
recovery, real240/hour release floor, exact65536/65537-byte frames and unchanged
wire IDs after atomic refusal all pass. No source change widens shared authority.

First-source measurements on aa758 are WEEX1210270 ->1266673 (+56403), and
combined14034260 ->14041661 (+7401), matching Node/Web with fixed budgets.
These numbers do not certify the subsequent correction. The complete native
qualification is428PASS/2FAIL/0ignored: two command-only trading fixtures have
not initialized feeds through genuine Engine.start. Their narrow initialization
preview requires reviewer disposition before application; source stays local.

The exact trading initialization preview is approved for
`fill_only_socket_event_during_cancel_blocks_replacement_before_order_update`
and `fill_only_socket_event_before_amend_waits_for_authoritative_order_reconciliation`.
Only these two post-start private-fill fixtures add genuine Engine.start1000
and its actual prepared drain before the unchanged importer. Both already keep
their original first Opened1002; all234 trading assertions,23 raw payloads,
financial events and old command/response clocks remain exact. The initial
catalogue read remains independently unanswered. This introduces neither global
autostart nor a new command-before-start support contract.

A reverse-catalogue ticker control additionally pins native removal ordering.
New subscriptions follow proposed demand; unsubscribe batches follow the previous
committed order, because removed topics are absent from the proposal. On fd8 the
new control reaches a literal ETH/BTC-versus-BTC/ETH unsubscribe assertion failure;
the narrow selector correction passes all39 controls with strict Clippy and fmt.
The prior38 control bodies remain unchanged. This correction changes no IDs,
hourly quota, recovery cadence or custody authority.

## Corrected source freeze qualification

Full native qualification at fd8dd8d7558 is430PASS/0FAIL/0ignored, including
the approved trading initialization. The subsequent unsubscribe correction
966fb89a10a passes all39 controls and strict all-target Clippy/fmt; its previous38
control bodies are unchanged. hcmq3f clears both exact source pins, including
fixture and inventory preservation. The plain integration-tip merge1a02f3aa96f
is automatic and keeps the entire WEEX subtree byte-exact to966fb89a10a. Root
typecheck/lint, the stable-tree root tests, full bans and5 budget controls pass.
The root test run overlapping the aborted legacy-master merge is excluded.

The LOCAL current pair uses bare integration tip b0ee0a9a4d5 versus composed
1a02f3aa96f, with Rust1.99.0, wasm-bindgen0.2.129, Binaryen132, jobs4 and
BINARYEN_CORES4 under the unchanged shipping build commands/profiles:

| Artifact | Bare bytes | Composed bytes | Delta | Fixed budget | Headroom |
| --- | ---: | ---: | ---: | ---: | ---: |
| WEEX | 1212633 | 1274315 | +61682 | 1343488 | 69173 |
| Combined | 13908680 | 13920684 | +12004 | 14155776 | 235092 |

Optimized Node/Web bytes and hashes match for every build. Each of the four
measurement targets is deleted, retaining source archives, logs and hash tables.
This pair supersedes no earlier attribution: aa758->74e remains first-source
evidence only. Combined proof is LOCAL; frozen-head CI supplies the family-size
record. No generic engine/host/crypto/registry, shared Cargo input, signing or
credential inventory differs from the integration tip. Exact-head CI, final
composition/receipt carry and auditor GO remain gates before landing.
