# Orderly family Rust port

Branch: `feat/rust-orderly`, created from fetched `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and gates: `VENUE_PORT_BRIEF.md` from landing.
Full auto; no master, stash, rebase or force-push. PR targets `feat/rust-rewrite`.

## Scope and design

One `safe-cex-venue-orderly` crate under `sdk-rs/venues/orderly` with a typed
broker option. Registered aliases: kodiakfi, mode, salsadex, tealstreet-dex,
what, woofi. All share the existing EVM REST/WS hosts and Tealstreet broker
UUID; regular order tags respectively TEALKODIAK, TEALMODE, TEALSALSA,
TEALSTREET, TEALWHAT, TEALSTREET. Testnet selects its own hosts and broker UUID.
No TS method transcription or generic JSON values. Borrowed typed payloads,
streamed large REST arrays, sans-I/O state machines, ED25519 from crypto.

## Phases

1. Public data: crate, borrowed wire types, market/ticker seeds, stream routing,
   socket watchdog/reconnect, refcounted candles/books and snapshot buffering,
   FetchCandles, native documented-payload tests. Audit official timestamp and
   book continuity contract rather than invent sequence numbers.
2. Private data: exact base58-seed ED25519 signing, startup validation,
   margin defaults/account leverage, positions + balance, normal/algo orders,
   authenticated WS, read reconciliation, stale guards and per-execution fills.
   Preserve simultaneous Cross/Isolated identity through optional compatible
   frame data; ensure failed reads never clear last-good state.
3. Trading: full-batch preflight, caller IDs verbatim on one native order,
   broker attribution, normal and positional TP/SL/brackets with sibling repair,
   strict stable-ID native amend, normal/algo bulk cancellation without repair,
   per-symbol margin/leverage, fixed OneWay. No automatic write retries.
4. Queries: typed documented history/trade/candle endpoints in engine JSON
   result shapes, bounded paging/ranges and honest unsupported surfaces.
5. Integration and gates: wasm and SDK aliases/options, recording/replay config,
   strict signature/body matching private tapes with all broker/testnet variants,
   live read-only 20-second public recording for two books, fixture, correctness
   and CPU/memory gates; Binance and all existing venue regressions; bans, fmt,
   clippy, native/wasm tests, SDK typecheck/lint/tests. Remove provably unused
   replaced helpers and resolve TS removal vs replay oracle without losing gates.

Every phase: plan audit, implement, meaningful checks, independent sub-agent
review, fix and recheck, commit this progress. Deep planning only for actual
Orderly risks: dual-margin identity, REST-proven amend, positional sibling jobs,
and timestamp-only books. Genuine TS bugs are declared in gate KNOWN_TS_FIXES,
commit messages and PR, with independent expected results.

After all gates: fetch/merge landing, revalidate, push, PR, local adversarial
sub-agent reviews posted to PR, fix/review until clear or diminishing returns,
wait every required CI green, merge to landing, cut clean follow-up branch.
Report PR, exact correctness/performance numbers, TS fixes, unfinished work.

## Progress

- Planning: brief/spec and nearest docs read; six aliases and tags confirmed.
- Independent account, trading and harness plan audits in progress.
- Early audit: store keys positions by ID, but frame currently reconstructs IDs;
  optional identity extension required for simultaneous margin modes.

### Public phase

Implemented public crate and typed broker options, streamed market/ticker
seeds, four aggregate streams, refcounted early subscriptions, candle reads
and stream, heartbeat and reconnect, full-depth snapshot buffering with exact
integer predecessor timestamps and generation-correlated snapshot requests.

Official current docs omit the incremental topic; a read-only live probe
confirmed `@orderbookupdate` contains `data.prevTs`. Snapshot `data.ts` is the
same book clock. First buffered update bridges `prevTs <= snapshot.ts < ts`,
then predecessor must equal the last applied timestamp. Gaps/malformed frames
hide the stale book and resnapshot. No floating-point sequence conversion.

Public live sockets reject invented identifiers with normal close1000. The
official js-sdk `packages/net/src/ws/ws.ts` uses COMMON_ID
`OqdphuyCtYWxwzhxyLLjOWNdFP7sQt8RPWzmb5xY`; this identifier works keylessly.
Documented WS `event:request` orderbook snapshots returned full depth (372
bids/356 asks in probe), avoiding TS's private signed REST snapshot in public
mode. Source: https://github.com/OrderlyNetwork/js-sdk/blob/main/packages/net/src/ws/ws.ts
and https://github.com/OrderlyNetwork/documentation-public/blob/main/build-on-omnichain/websocket-api/public/request-orderbook.mdx .

Independent phase review found and fixed startup subscription loss, quiet-book
snapshot starvation, malformed update retaining a stale book, missing candle
timestamps/OHLC bounds, unsupported intervals, and snapshot symbol mismatch.
Re-review found no phase blockers; also tightened null timestamps and missing
depth data. Native tests: 11 passing; fmt/clippy/bans passed. SDK wiring and
record/replay performance checks remain in the integration phase. Genuine TS
book continuity/public snapshot fixes will be declared in gate/PR.

### Private phase

Implemented normalized base58 ED25519 credentials with zeroized seeds and
URL-safe padded signatures over exact REST/WS bytes; validation before private
bootstrap, margin defaults, per-mode leverage caches, positions/collateral and
paged normal/algo orders; authenticated subscriptions, heartbeat, reconnect,
periodic 2s positions and every-fourth-tick orders. Failed reads preserve state.
Private sequences/IDs remain exact integers; executions deduplicate independently
from order-state updates. Fills invalidate pending positions/collateral reads.

Optional appended position-ID references preserve simultaneous Cross/Isolated
identity across frames without changing legacy venue payloads. Reader, types,
FrameApplier and store/spec docs updated; transferred/legacy frame tests pass.

Independent review found/fixed dual status/algoStatus decoding, documented
root lifecycle and average-price aliases, REST freshness stamps, fill-vs-held
positions, root terminal-before-any-child-vs-held-algo-snapshot, and leverage
cache mutation before snapshot validation. Parent tombstones filter subsequent
REST/WS children. Re-review found no remaining phase blockers.

Verification: 22 venue tests (2 signing, 9 account, 11 public); engine44;
host26; crate fmt/Clippy and bans; safe-cex typecheck/lint all pass. SDK constructor
wiring, end-to-end WASM account tests, strict independent signing tape audit,
and account deactivation signal integration remain for the gates phase.

### Trading plan audit

Use sequenced typed jobs with complete batch preflight before writes. Preserve
caller IDs on exactly one regular native order; reject conditional/bracket/split
IDs and duplicate IDs as a batch. All writes carry explicit mode and correct
broker UUID/order tag; plain TIF maps to native LIMIT/POST_ONLY/IOC/FOK.
Native amendments require complete REST proof and one precision-aligned real
change, with EDIT_SENT success and stable ID. Positional replacement cancels
its parent directly and posts one pair preserving only the opposite sibling;
child cancellation groups selected siblings by root before rebuilding survivors.
Bulk normal+algo cancellation attempts both independently and never repairs.
Margin selector writes affect defaults only; leverage targets selected mode;
position mode is fixed OneWay. No write retries; retain partial accepted IDs
if later sequenced steps fail. Review after native command/error/race tests.

### Trading phase

Implemented fully preflighted typed sequenced jobs, exact bounded quantity
splits, verbatim ASCII client IDs on one regular order, broker attribution,
native TIFs, strict REST-proven stable-ID amend, dual-mode settings and
per-market leverage bounds. Positional replacement preserves only the opposite
sibling and waits for old-parent terminal state or a fresh complete snapshot.
New pairs wait for all authoritative children before another protective job.
Uncertain writes retain scoped fences until a fresh successful snapshot;
pre-write snapshots cannot release them. Bulk cancellation preempts queued
placements and pair waits, attempts both families, and never repairs siblings.
No write retries. Partial acceptance reports exact accepted IDs.

Independent review rounds found and fixed terminal-before-ACK resurrection,
unconfirmed-pair duplication, duplicate bracket scopes, global Cross leverage
clamping and Isolated fallback mutation, uncertain Pair response loss, and
bulk cancellation arriving before a Pair ACK. Final review has no blockers.
Tests also cover stale snapshots failing to release uncertain-write fences and
fresh reads restoring progress. Verification: 38 venue tests (16 trading),
crate fmt/Clippy and bans pass. TS differences (TIF, split precision, child/root
response shapes, sibling repair, cumulative fills and reconciliation) will be
explicit KNOWN_TS_FIXES with independent tape oracles in the integration phase.

### Queries plan audit

Implement documented rare reads for fills, ordinary+algo completed orders,
settled funding, and public market trades. Typed streamed rows, bounded page
continuations, exact IDs, strict range/symbol/order filters and deduplication.
Failed or malformed pages fail the whole command without changing account state.
Use direct order-trades endpoint for order-specific fills so latest500 account
trade retention does not hide an older order. Funding exposes settled Pay as
negative and Receive as positive; other income types fail explicitly. Match
Binance JSON shapes, including seconds execution/income/public timestamps and
millisecond historical createdTime/updatedTime. Public rows without native IDs
use a deterministic content key plus identical-row occurrence discriminator.

### Queries phase

Implemented streamed fills (including direct order-trades lookup), ordinary and
nested algo terminal history, settled funding and public trades. Order history
omits the API's COMPLETED filter so rejected orders remain visible; Open rows
are filtered locally. Output units follow Binance. Bounds: 50 pages, 500 rows
per page, 1,000 results, algo depth8 and 1,000 nodes per root. Malformed/missing
rows, repeated pages and failed later reads fail the complete command. Public
trade IDs preserve identical same-time executions using occurrence suffixes.
Funding starts are paced across concurrent commands and rechecked when delayed
host timers fire; Pay is negative and Accrued rows are excluded.

Independent review found/fixed delayed-timer funding bursts, nested bracket
traversal, root status overwriting explicit child status, and opening bracket
entries accidentally defaulting reduce-only. Final re-review has no blockers.
Verification: all17 query tests and crate Clippy pass. SDK/harness integration
is underway; its first public gate exposed broker-suffixed symbols and the
obsolete aggregate funding topic, to be corrected before counting gates green.

### Integration plan audit

Ship all six aliases through production registries and a default WASM loader;
retain the former implementation only in __tests__/reference for independent
replay. Conditional package exports must keep Node filesystem glue outside the
browser graph. Preserve applicationId, tick cadence, broker UUID/tags, account
identity and startup rejection/disposal lifecycle. Honor selected REST routing
at the host boundary; never silently send routed credentials directly.
Use strict independent ED25519 verification and exact ordered bodies in tapes;
correlate public snapshot responses by taped semantic request, symbol, socket
generation and opaque-ID occurrence. Run actual live two-book20s recording,
all alias/testnet private gates, existing venue regressions, and full checks.

### Integration checkpoint before landing merge

WIP: production six-alias registry cutover, test-only legacy replay reference,
conditional WASM loader, explicit deactivation event, scoped host routing,
strict signed private tapes and guarded independent TS-fix oracles are present.
Broker suffixes now preserve native identity. Funding moved to dedicated
90-topic sockets after live evidence of the venue's100-topic/socket limit.
Public shard tests16 and query tests17 pass; harness96 and transport10 pass.
SDK alias/private/lifecycle WASM tests15 pass; typecheck/lint pass. Full SDK
suite had6567passes and six expected registry-characterization failures;
those six snapshots were updated and targeted49tests now pass.

No final gate has passed yet. Remaining: verify new funding topology on a fresh
20s recording, finish native/main/private handshake watchdog and main-topic
capacity checks, propagate request abort through the actual host so deferred
proxy authorization cannot send after disposal, preflight unsupported socket
routes, rerun private gate with current query build/oracles, fit combined WASM
within existing3MiB budget, run all gates/regressions/CI and adversarial reviews.
Current public fixture is an intermediate recording and must be replaced.
Unused hmac_sha512 helper removed; spec-required P5a signing primitives retained.
At this checkpoint the legacy HOW adapter could not preserve signed bytes;
raw custom transport/proxy routes preserved them. The landing transport refresh
below supersedes this temporary HOW limitation.

### Landing refresh (c5bc2c4adf)

Merged the landing branch after the quota interruption, preserving Bitget and
Hyperliquid, strict harness request/write accounting, and optional per-venue
WASM builds. Orderly now participates in those feature builds; the landing
per-venue budget replaces the obsolete aggregate-size workaround above.
The new shared client-order lookup query fails explicitly for Orderly.
Locked native checks of the Orderly crate and complete WASM host pass.
Remaining integration and lifecycle gates above remain open.

### Lifecycle review checkpoint

Independent reviews cleared initial/reconnect handshake deadlines on main and
private sockets, duplicate-open guards, the 97-topic main socket capacity,
and host cancellation across authorization/fetch/stream consumption. Unsupported
socket routes now fail synchronously during SDK startup, including testnet.
Verification: 65 native Orderly tests, Orderly Clippy, full-workspace format,
WASM host Clippy, 19 host tests and 23 SDK transport tests pass. The complete
host suite needs the combined build (its Binance Workers test correctly rejects
an Orderly-only binary). The single-venue WASM is770,916bytes below1,258,291.
Fresh20s public tape captured637socket frames and two full books without topic
errors; first gate has only legacy zero-opening-price NaN percentage diffs.
Strict guarded private oracles are being updated for landing optional fields
and newly enforced request-set accounting; no replay gate counted green yet.

### Public and private replay checkpoint

Fresh20s fixture now passes all three public replay runs:639inputs,144markets,
144tickers,two full books. Median main CPU TS179.1ms/Rust117.6ms, steady
TS140.3ms/Rust82.0ms, retained TS2.0MB/Rust1.4MB (Rust linear0.7MB).
Private base replay passes with33inputs,two markets,14commands; all broker
mainnet/testnet variants are running before final completion.175harness tests
pass. Added declared TS fixes for zero-opening-price NaN percentages and
explicit open-order pagination, plus independent exact read accounting for
legacy query stubs. Account comparison now canonicalizes object keys while
preserving every row/value and array order.

Full SDK suite:6644tests/459files passed. SDK real Orderly WASM:15passed.
Review fixes: shared startup promise prevents duplicate/orphaned hosts;
disposal while loading rejects; unsupported Orderly history ID/status filters
reject before reads.48focused SDK tests, typecheck/lint and87metadata/account
construction tests pass. Built-in HOW capability was disabled at this
checkpoint; the landing transport refresh below restores it using raw RPC.
Test drivers for Binance/Bybit/Bitget explicitly fail on unexpected Deactivate
notices, retaining exhaustive checks after the shared lifecycle addition.
Remaining: combined host/SDK WASM tests, workspace checks, all broker variants,
mandatory Binance replay regressions, final landing refresh, PR reviews and CI.

All14broker/network private gates now pass. Combined host32tests and SDK
WASM40tests pass. Follow-up independent review closed prompt disposal of a
never-settling WASM loader (49focused SDK tests plus typecheck/lint pass).
Landing advanced tofc18c55874 with shared transport routing and Bitrue;
merge that structure before final workspace/Binance regression checks.

### Shared transport integration (fc18c55874)

The landing branch now owns engine REST/HOW/socket routing and native routing
plans. Orderly uses that shared transport; its provisional adapter is removed.
The shared HTTP path guards Orderly method, routed URL, body, venue headers and
request signal before dispatch, while allowing intermediary authorization
headers. Raw HOW preserves decimal lexemes and IDs in requests and responses,
so all six brokers advertise tunnel support again. Deterministic wire tests
cover all six aliases and testnet. Direct/custom/proxy, cancellation, mutation
rejection and native policy regressions use the shared transport boundary.
SDK startup retains memoization, default loading, prompt disposal and explicit
deactivation alongside the landing branch's secondary-key routing preflight.

Merged transport verification:316SDK/transport/metadata/account-construction
tests pass, including actual SDK rejection before engine construction when a
custom provider requires upgrade headers on an incapable host. Full repository
typecheck44tasks and lint28tasks pass; follow-up safe-cex typecheck/lint after
the header-capability guard also pass. No Orderly-specific transport remains.

Merged socket protocol integration adds Public main/funding roles and Private
account role. Main/funding sockets have stable market/funding-N identities;
SocketContext owns its feed string so dynamic shards need no leaked strings or
arbitrary shard cap. Native selectors retain the same string key semantics.
Thirty seconds of heartbeat-only traffic requests recovery independently for
the affected feed; malformed/empty/unknown aggregate payloads do not refresh
usable-data liveness. Three independent shard/recovery regressions pass.
Full native workspace:888tests passed; Orderly68passed. Post-merge harness181
and host23tests pass; shared SDK/transport316targeted tests and whole-repo
44typecheck/28lint tasks pass. Final WASM builds/replays follow this merge.

### Final local gates (2df28a2ec9)

All required local gates pass on the merged landing structure:

- Bans, workspace fmt, strict workspace/all-target Clippy and WASM host Clippy.
- Native workspace888tests; Orderly68tests (including malformed/empty/unknown
  payloads failing to postpone protocol recovery).
- SDK6885tests/466files; combined SDK WASM44tests; host41tests.
- Full repository44typecheck and28lint tasks.
- Harness181tests; Orderly private14broker/network gates, each14commands.
- Public three-run gate:639inputs,144markets,144tickers,two full books.
  Median main CPU TS156.4ms/Rust104.8ms (1.49x), steady122.7ms/72.1ms
  (1.70x), process196.0ms/168.4ms, retained2.0MB/1.4MB (1.47x).
  Rust linear0.7MB. Venue WASM772899bytes within1258291budget.
- Binance public:873inputs,917markets,783tickers,three books; main
  TS252.4ms/Rust130.3ms, retained4.3MB/2.5MB. Private14inputs passes.

The public fixture was recorded live for20seconds with BTCUSDC/ETHUSDC. No
live private credentials were supplied; documented-payload private tapes are
strictly signed/verified, with exact requests, ordered writes and protocol
checks. No local gate is waived. Next: PR into feat/rust-rewrite, post local
adversarial reviews, fix findings, wait for green CI, merge and branch afresh.

### PR1318 correctness review, round1

Three independent reviews were posted as PR comments. Six findings reopen
verification: cross-account browser initialization, main-subscription rejection,
incomplete live rows, native STOP/nested bracket mapping, finite/limit sibling
repair, and queued repairs surviving bulk cancellation.

The browser loader now shares one module-level initialization promise across
accounts and later starts; failed initialization clears only that module cache.
Three new loader regressions plus transport suite41tests, package typecheck
and lint pass. Independent re-review repeated the browser-artifact reproduction:
one initializer invocation and existing engines remain usable. Public/private
regressions reproduced the other findings before their implementation fixes.

Public review fixes: rejected main subscribe/unsubscribe acknowledgements now
invalidate books and reconnect with recovery enabled. Snapshot-request errors
still use per-book retry. Live rows require present finite topic-specific
fields before updating prices or feed health; valid zero values clear old
prices, while incomplete rows preserve them and cannot suppress recovery.
Removed the obsolete aggregate funding handler. The independent harness now
records subscription rejection as failure instead of trusting sent topics.
Three public cases failed before fixes; public23tests and harness182tests now
pass. Independent code-correctness re-review reports no remaining findings.

CI exposed a clean-checkout import failure: the engine package export resolved
directly to generated, ignored WASM glue. Committed conditional Node/browser
loader entries now defer their literal generated imports until engine startup;
the SDK's shared initialization promise still covers import and web init.
With generated artifacts temporarily absent, all10affected CI files plus the
loader regression pass (11files/468tests). Artifacts restored; real SDK WASM44
tests, package typecheck/lint and independent code-correctness review pass.

### Private correctness reviews, rounds2–4

Native STOP and bounded nested BRACKET trees now reconcile all actionable
orders without blocking initial account readiness. Exact child lifecycle and
root identity are retained; sparse entry/leaf terminal events cannot close
live siblings, and a nested container cannot finish its ancestor root.
Sibling repair requires explicit full-position provenance, preserved across
partial child updates and POST-ACK ordering. Finite TP_SL, LIMIT, contradictory
mode/type, and attached bracket protection reject partial repair before any
write; whole-parent cancellation remains available. Bulk cancellation also
supersedes affected queued cancel/amend jobs that could recreate protection,
while unrelated symbols retain their queued commands.

Initial regressions failed before these fixes. Final native82tests and strict
Clippy pass. Independent read-only code-correctness reviews found and closed
two follow-up provenance/lifecycle cases; round4 reports no remaining material
findings. Public and integration re-reviews are also clear. Full acceptance
replays and CI must run on the refreshed landing branch before merge.

### Post-review acceptance (cb132db8b1, landing6945ee83b4)

All three independent code-correctness re-reviews are clear, including the
landing conflict resolutions. The incoming Bitget frame mock now implements
the position-ID accessor and asserts the symbol fallback; its regression failed
before the fixture fix. No production economics or assertions were weakened.

Refreshed acceptance results:

- Bans, workspace fmt, workspace/all-target Clippy and WASM host Clippy pass.
- Native workspace1083tests, including82Orderly tests, pass.
- Orderly single-venue WASM783885bytes within1258291budget.
- Public three-run gate:639inputs,144markets,144tickers,two books, identical
  output. Median main CPU TS180.1ms/Rust118.3ms(1.52x), steady138.2/80.6ms
  (1.71x), process216.3/181.2ms. Retained2.0/1.4MB(1.46x), linear0.7MB.
- All14broker/network private gates pass, each14commands, with exact signing,
  requests, write order and socket protocol checks.
- Combined WASM build passes; SDK WASM46tests, host42tests and harness188tests
  pass. The initial harness run had one20ms routing timeout during concurrent
  checks; the isolated complete rerun passes.
- Binance public873inputs,917markets,783tickers,three books; CPU main
  TS232.3/Rust123.2ms, steady135.3/56.2ms, retained4.3/2.5MB. Private14inputs
  passes. Landing is still6945ee83b4 at the final fetch.
- Whole-repository typecheck44tasks and lint28tasks pass; package lint passes
  again after the Bitget fixture edit.
- Full SDK default run:6933passed/3failed across472files. The Bitget mock was
  fixed; the metadata sweep passes on rerun. HYRO's two dynamic imports exceed
  their default5s timeout locally; the isolated10-test file passes with a30s
  CLI-only timeout(17.91s total). No source timeout was changed. All6936test
  cases are covered by the full run plus these reruns; default-budget CI is
  still required to pass before merge.

Fourteen declared TS corrections remain independently guarded in the replay
oracle. Public fixture is the committed live20s recording; private validation
uses documented deterministic test payloads, not live credentials. Required
remaining work: publish refreshed results, wait for all CI checks green, merge
PR1318 into feat/rust-rewrite and cut a fresh branch from the updated landing.

### Lighter landing refresh (1f08347267)

Merged landing31ec401f60 after PR1319 landed during this PR's CI wait.
Independent code-correctness review clears the feature/constructor/option and
harness unions. Orderly venue, engine and shared host behavior are unchanged.
Updated workspace native1167tests, bans/fmt/Clippy, WASM Clippy and repository
44typecheck/28lint tasks pass. The repeated20ms Orderly routing test failure
was queued socket opens consuming its routing deadline; the test now awaits
those opens before starting routing assertions, retaining both negative20ms
timeouts and every subscription/generation check. Full harness204tests pass.

The focused SDK run confirms Orderly/Lighter/Hyperliquid/client-ID behavior;
cold loader/metadata imports still hit local time budgets under contention.
The loader's isolated3tests pass with a CLI30s timeout; metadata's four other
checks pass while its explicit60s constructor sweep times out. Previous code
headcb132db8b1 passed both general CI runs with default budgets. Final merged
head CI remains mandatory. Rebuilt WASM/replay results will be posted on PR1318
before merge; no later source change is planned beyond a concrete CI finding.

### Port-review lows: history candles and trailing display

Bounded `FetchCandles` uses Orderly's native
[`/v1/tv/kline_history`](https://orderly.network/docs/build-on-omnichain/restful-api/public/get-kline-history),
with seconds in `from`/`to` and validated parallel OHLCV arrays. Unbounded latest
reads keep `/v1/kline`. Results are sorted ascending, filtered at exact caller
millisecond bounds, and limited to the newest requested bars. Malformed columns,
duplicate timestamps or invalid OHLCV fail the whole read; `no_data` is empty.
This deliberately improves on TS, which ignores bounds entirely.

Trailing-stop display uses native extreme price with callback value (preferred)
or rate, including REST snake_case and stream camelCase fields. BUY adds the
callback; SELL subtracts it, matching the
[official SDK](https://github.com/OrderlyNetwork/js-sdk/blob/main/packages/utils/src/order.ts).
An explicit positive trigger wins; inactive/unpriced trails display zero.
TS always subtracts a callback value and ignores rate, so its BUY/rate display
is intentionally corrected. Placement/cancellation economics are unchanged.
Uncached cancel IDs still reject: safe cancellation requires symbol, normal vs
algo identity and sibling provenance. Finished IDs intentionally succeed locally.
