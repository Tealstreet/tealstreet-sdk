# TxFlow whole socket family on managed feeds

## Scope and original source

Owner: codex-5c57jl. The overseer approved both api-ws and node ws together;
Bitunix, Phemex, Hyperliquid and OKX are excluded. Fresh branch
feat/rust-txflow-managed-sockets starts at current landing
0418d0a4599bb372a10b853ddb23aa3c3bbc2546. This phase commits only this plan,
the index row and new original assertion controls. No production or inventory
changes until plan/RED and owner reviews are complete. #1970 remains frozen;
#1974 CI and its actual-prerequisite dependency remain separate.

Request owner codex-hcmq3f is contacted before source. The open-PR file audit
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
that intent. A refused repair does not cancel still-needed committed work; an
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
