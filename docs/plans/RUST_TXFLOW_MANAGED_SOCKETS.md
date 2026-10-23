# TxFlow whole socket family on managed feeds

## Scope and original source

Owner: codex-5c57jl. The overseer approved both api-ws and node ws together;
Bitunix, Phemex, Hyperliquid and OKX are excluded. Fresh branch
feat/rust-txflow-managed-sockets starts at current landing
0418d0a4599bb372a10b853ddb23aa3c3bbc2546. This phase commits only this plan,
the index row and new original assertion controls. No production or inventory
changes until plan/RED and owner reviews are complete. #1970 remains frozen;
#1974 CI and its actual-prerequisite dependency remain separate.

Request owner codex-hcmq3f cleared socket/lib.rs/account.rs ownership before source. The open-PR file audit
shows only TxFlow trading-test plan #1883, with no production socket claimant.
Coordinate lib.rs/account.rs boundaries and recheck that inventory before source.
No edits to engine/host/OKX or any frozen shared seam belong in this adapter.

## Native contract and mapping

There are two sockets, not a public-only partial slice:

- /api-ws is Public. It carries ticker and candle subscriptions. Its JSON omits
  the top-level type field and retains type inside subscription. Ticker coin is
  the native base string with instrumentId; candle coin is the native asset ID.
- /ws is Public under public_data_only, otherwise Private. It multiplexes public
  allAssetCtxs/books with native orderUpdates, clearinghouseState and userAssetData
  wallet subscriptions. Node JSON retains top-level type plus subscription.type.
  There is no login ACK or native snapshot-completion barrier on either socket.
  Do not invent auth, credentials or an authenticated/snapshot timer requirement.

Both transports use connect10s, idle30s, ping10s, jittered retry1s to16s, stable30s.
Native receive refresh precedes JSON decoding: AnyText with ControlPolicy::None
fits both feeds. A malformed current frame still refreshes receive liveness;
late/retired/duplicate ingress is rejected before native decoding. Ping bytes
remain {"method":"ping"}. RISK_RATE_LIMITED remains a native callback result
that retires only its current shared feed and uses shared backoff.

Native subscription pacing is150ms, one serialized frame per slot. Map it to
FeedPolicy.subscription_interval_ms, preserving native parameter bytes and
stable topic order. The existing managed desired-interest transaction owns
capacity, epochs, pacing timers and final release. Do not copy the old VecDeque,
subscription_timer, raw SocketId or Supervisor into another venue helper.

Shared/Disabled public-feed policies suppress api-ws and public-only node ws,
while a Private node feed still accepts native financial traffic. A private node
must omit its public interests under those policies. Book/candle refcounts,
book reset/publication behavior, ticker metadata and account subject/revision
validation remain native. Preserve request builders, exact static JSON/version
headers, PublicCors route, HTTP completion/error hooks, history, signing,
trading and all financial reducers.

## Subscription handoff and native refresh

Existing #1943 supplies all required custody. On a private node open, the native
public context/book interests precede the three private topics. Complete desired
interest convergence, including all150ms paced frames, must precede the native
opening/reconnect account refresh. Use set_socket_interests_then with exact
canonical required private topics and a native Pending intent; execute the
refresh only at the ordinary shared released-interest callback. This establishes
returned host-action handoff, not peer ACK or a later JavaScript event-loop turn.
The callback selects current native inflight/read/revision state when it runs.
No borrowed command, HTTP, Plan or signer activation is introduced.

Native startup account reads are independent and stay at their existing startup
point. Do not blanket-delay startup, public-only or zero-private-obligation work.
A private node open still invokes native account refresh when user() yields no
subscription address; invoke existing guarded account_refresh directly from the
genuine current node Ready in that case. Do not fabricate a private interest or
an empty-required continuation merely to obtain a wake. public_data_only retains
its native no-refresh path; no fake started/inflight state is permitted.
Preserve the native150ms ordering trace as a named original positive before
changing the intentional HTTP position. The existing opening path can issue
refresh HTTP while private frames remain paced; a separate original assertion
must expose that race. The later approved order is the full private node batch,
then actual1943 wakeup, then any native refresh admitted by current read state.

Synchronous shared admission refusal or retirement must not lose a native refresh
obligation. If necessary, keep only a native refresh-due intent like the reviewed
HL mapping; it grants no epoch, timer, send or publication authority. Shared
matching-feed retirement/backoff owns recovery. Fresh Ready registers a new
handoff, and only transfer to the existing native read/inflight owner consumes
that intent, including work already legitimately owned under the native inflight
contract. Proposal success, queued frames and callback entry alone do not consume
it. If account.started is false, retain only the intent: native account_start
keeps its existing refresh and existing poll, and those paths discharge it only
after current native read/inflight custody is established. Do not force another
refresh, spin, or add a timer. The zero-address no-op has no fabricated private
obligation to retain. A refused repair does not cancel still-needed committed work; an
old/superseded wake cannot erase newer due work. No direct-read fallback or venue
retry timer is permitted. Source review must pin this transfer boundary.

## Files and phases

1. Plan/index commit, then new tests/managed_socket.rs original controls against
   the unchanged exact current native source. Test compilation and fixture errors
   are not assertion REDs. Keep all working native wire/financial cases.
2. After request-owner/shared-owner review, lib.rs Connection and socket hooks
   become thin shared adapters, with a native socket_codec.rs if useful. Add one
   Pending handoff arm/hook, disjoint from all REST/history callbacks. Keep
   account reducers unchanged; any native refresh-intent adapter is narrow and
   separately reviewed. Enable the existing managed-sockets Cargo feature.
3. Run the original REDs unchanged, except the explicitly identifiable native
   HTTP-order positive updated to the reviewed handoff contract. Verify full
   paced batches, genuine close/retry/new socket, stale/expired denial, private
   publication and PublicFeedPolicy variants. Existing financial assertions stay.
4. Prune only TxFlow socket fingerprints and sourceRoots entry; run strict native,
   formatting, bans, independent reviews and fresh CPU/memory/size/replay CI.
   Actual dependencies and a train GO precede any merge.

## Original executable matrix

RED targets on unchanged production:

- Duplicate accepted api-ws open emits no second ticker frame.
- Duplicate accepted node open emits no second public/private subscription or
  account refresh and does not clear already published current books.
- Both transports reject an open exactly at connect+10s without timer ingress.
- api-ws ticker and node book data exactly at idle+30s cannot publish without a
  watchdog callback. Identical live payloads prove both negative cases meaningful.
- Private node clearinghouseState exactly at idle+30s cannot replace the last
  accepted financial state; its identical current payload must still publish.
- Private opening/reconnect refresh cannot overtake the required paced private
  subscription batch. Inspect unfiltered returned action order, not WsSend alone.

Working controls:

- Exact api and node JSON; native one-frame/150ms pacing and topic order.
- Current ticker, book and private account values, plus old physical socket inert
  after genuine close/new connect. No HTTP auto-response helper hides ordering.
- Native refresh/startup/zero-obligation trace stays identifiable for comparison.
- Both Shared and Disabled keep private account data while suppressing direct
  public topics; malformed current text still refreshes the native AnyText bound.

Follow-up implementation gates include refused handoff then freed capacity and
fresh reconnect, admitted-work retirement, stale old wake, and partial paced
release denial. These use existing1943 controls/API, not a new socket seam.

## Inventory and validation

At original0418, only venues/txflow/src/lib.rs has socket records:
1dd17767b986aa9f889bb7662deb16e7d6ab86e99a57a3a1f4a731c478f4cc25,
68a3f6f47923eae17af0f8556064bc4caf1fdb6e6a0d2aaf20a07a9c349203c2,
8295b5ad0547ce471b41da25af645715f4cb998ae64d34053dd10eb0b27405b8,
d7f62ba4022a0480394d8d646a0502bf832e0c71ee7a1131c509d0fa1034a12e,
one occurrence each:4fingerprints/4occurrences, plus only its family root.
No inventory mutation occurs in the plan/RED phase. Bitunix remains legacy under
Sam's explicit exception and remove-before-master obligation.

Scoped commands: cargo test -p safe-cex-venue-txflow --test managed_socket;
existing TxFlow public/private supervision, public/account tests; strict
all-target/all-feature Clippy, formatting and bans. Full performance/size gates
belong to CI without changed thresholds, profiles, tapes or harnesses. No native
benchmark on the daily-driver machine and no performance claim from local tests.
No deployment, flag flip or SDK-master merge.

## Executed original controls

Plan-first commit:8b36a03daaa. The subsequent new managed_socket.rs controls run
against byte-identical original0418 production, Cargo, engine/hosts and existing
TxFlow tests. Request owner codex-hcmq3f cleared the whole socket region and
confirmed role, pacing, AnyText, node-only account invalidation and zero-address
refresh boundaries. Its #1883 remains docs/trading-test only.

The original new target has20 executed tests:11 assertion RED,9 GREEN,0ignored.
No production changes or inventory pruning accompany this checkpoint.

| Assertion RED | Observed original behavior |
| --- | --- |
| duplicate_api_open_does_not_replay_tickers | Second ticker subscription is emitted. |
| duplicate_node_open_does_not_replay_interests | Second public context subscription is emitted. |
| duplicate_node_open_does_not_clear_published_book | Published bid80 becomes absent. |
| duplicate_node_open_does_not_restart_account_refresh | Completed native reads are restarted by duplicate open. |
| api_open_at_connect_deadline_is_inert_without_watchdog | API subscribe is released at connect+10000ms. |
| node_open_at_connect_deadline_is_inert_without_watchdog | Node subscribe is released at connect+10000ms. |
| api_ticker_at_idle_deadline_cannot_publish_without_watchdog | Expired frame changes65000 to999. |
| node_book_at_idle_deadline_cannot_publish_without_watchdog | Expired frame publishes bid80. |
| private_state_at_idle_deadline_cannot_publish_without_watchdog | Expired financial frame changes contracts2 to3. |
| opening_refresh_waits_for_all_paced_private_subscriptions | HTTP action4 precedes final required subscription action11. |
| reconnect_refresh_waits_for_all_paced_private_subscriptions | HTTP action6 precedes final required subscription action14. |

The9 working controls pin the original refresh-before-private-frame trace;
exact API/node JSON, roles and first-frame/150ms pacing; book/candle wire;
identical live book/ticker/financial payload publication; Shared/Disabled private
traffic; node-only book clearing; old API/node/private ingress denial after real
close/backoff/replacement; and malformed-current-text liveness with exact ping.
HTTP replies are explicit and preserve every original HTTP action in the returned
trace. The two handoff REDs inspect the complete unfiltered host-action sequence.
The original order positive remains named and identifiable for the later approved
contract change.

Existing account27, private_supervision2 and public_supervision9 controls all
PASS (38total,0ignored). Scoped new-target all-feature Clippy with -D warnings,
rustfmt and git diff --check PASS. Initial new-fixture compilation mistakes
(Option<SocketChannel> matching and an unused wire constant) were corrected before
any assertion result was counted. This is an original-RED checkpoint, not a green
PR or CPU/memory/size claim; no full suite or CI dispatch was needed for this
plan/test-only phase. Local logs stay outside Git:
/tmp/codex-txflow-socket-original-controls.log and
/tmp/codex-txflow-socket-native-controls.log.

## Bounded request-owner review

codex-hcmq3f cleared plan8b36a03daaa and original controls b007fe39387 after
checking the full actual0418 diff, unchanged protected files, unfiltered HTTP/WS
traces and owner execution logs. This was source/log inspection, with no
independent test execution or production/performance clearance. It confirmed
native roles, pacing/wire, receive-before-decode, startup independence,
zero-private-obligation refresh and node-only account invalidation. #1883 retains
only its docs/trading-test claim.

The implementation must prove that refresh-due is consumed only when the existing
native read/inflight owner accepts custody. Refused or partial handoff, admitted
work retirement and stale wakes must preserve any newer due obligation. Shared
API/custody review and production verification remain separate gates.


## Bounded shared API/custody plan review

codex-uufub4 cleared the b007/8b36 plan direction on existing1943 without a new
seam,1970 opt-in or auth barrier. Its subsequent independent exact b007 git-archive
execution reproduced11 assertion RED/9 GREEN/0ignored with no setup/compile
failure, including both unfiltered order observations and identical live payload
controls. Receipt:/tmp/codex-txflow-plan-b007-review.log. This is original evidence
and plan/API clearance; production/source/runtime review remains separate.

For nonempty private keys, atomically register the complete desired node batch
and exact canonical required private keys. The released handoff waits for every
paced batch and reads current native generation/inflight/revisions at callback.
Required source controls include zero-address no artificial work; callback-time
inflight changes; capacity refusal then freed capacity and genuine reconnect;
admitted continuation retirement; old stale wake while newer work waits;
Shared/Disabled private financial publication; and partial150ms batch release
without early HTTP. Startup reads remain independent. Both the native-owner
transfer boundary and the pre-start startup/poll ownership rule above are
implementation gates, not a new authority or venue scheduling mechanism.


## Reachable zero-address and pre-start controls

Native private Options::validate requires a valid application_id, and user()
selects vault or application_id. A genuinely admitted private setup therefore
cannot currently reach user()==None. codex-uufub4 accepted testing the reachable
boundaries without bypassing validation or fabricating callback custody. The
existing defensive zero-address Ready branch remains a source-preservation
obligation; no executed reachability claim is made for it.

Three added working controls on unchanged original production now pass:

- missing_private_address_rejects_setup_without_artificial_socket_or_account_work:
  private setup emits CONFIG_INVALID before any metadata, HTTP, socket, signing
  or timer action; no financial state is manufactured.
- genuine_private_open_before_native_account_start_keeps_refresh_noop: an actual
  issued Private node opens before the native25ms startup transition. Opening
  emits the native public context frame without refresh HTTP; a financial frame
  remains a no-op while account.started is false. The existing startup timer
  alone issues one state read. After its response and full paced batch completion,
  no duplicate state/orders/fills refresh is introduced.
- public_only_without_address_has_no_private_refresh_obligation: valid public-only
  setup opens/drains the public context subscription without private topics,
  account HTTP or financial publication.

The expanded target executes23tests:11 original assertion RED/12 GREEN/0ignored.
All original RED observations remain; compilation succeeds. Scoped all-feature
new-target Clippy -D warnings, rustfmt and git diff --check pass. Production,
Cargo, inventory, engine/hosts and all pre-existing tests remain unchanged.
The independent archived b007 result above stays pinned to its original20tests;
this expanded23-test execution is the owner receipt, not relabeled independent
proof. Local log:/tmp/codex-txflow-socket-zero-address-controls.log. Production
refusal/retirement/stale-wake transfer controls and runtime review remain gates.

## Implementation contract decisions

The overseer authorized shared desired/coalesced subscriptions after the original
FIFO controls exposed the difference: the observable subscription contract is the
final desired set at150ms pacing, rather than a transcript of every transient
command. Coalescing removes unnecessary frames/rate-limit risk; the old4096-flip
close guarded unbounded FIFO growth and is unnecessary with bounded desired state.
The original FIFO/backlog tests remain identifiable at8d130e81837. Their explicit
updates prove final burst convergence, heartbeat and unsubscribe after an already
returned subscription; the rate-limit fixture no longer requires an extra empty
queue timer after the final frame. No venue queue, repair-loop, one-shot workaround
or churn counter is introduced.

The overseer separately authorized inverting only the opposite-order original
positive original_native_refresh_precedes_paced_private_batch (b007/8d) into
released_private_batch_precedes_opening_refresh, because the approved1943 handoff
requires all private frames before opening HTTP. The other11 original GREEN
control assertions remain unchanged; the11 original RED assertions are green.
Additional real-engine controls cover partial paced release/final frame versus
actual wake ingress, native poll consumption before a delayed wake, admitted
retirement and old wake while new work waits, and actual proposal-capacity refusal
followed by drained pressure and real reconnect. The pressure fixture delegates
to native TxFlow and fills the existing bounded shared proposal pool; it does not
fabricate native started/inflight, epochs or financial reducer state.

Connection retains only FeedId, native open state and a refresh-due intent. Shared
feeds own identity/epoch, expiry, retry and150ms pacing. The only account reducer
file addition clears the intent after native state/orders/fills ownership exists;
all native HTTP builders, row preparation, publication and fences are preserved.
Only TxFlow's4socket fingerprints/4occurrences and its legacy family root are
removed. #1970/#1974 remain frozen and are not modified by this work.

Pre-integration owner checkpoint: scoped native143 tests across12targets
PASS,0ignored; all-target/all-feature Clippy -D warnings and package formatting
PASS. Bans exposed incoming Aster/BingX provenance pruning after a fresh fetch;
current landing must be genuinely integrated, with no foreign inventory waiver.
Fresh post-integration checks and independent source/runtime reviews follow.
