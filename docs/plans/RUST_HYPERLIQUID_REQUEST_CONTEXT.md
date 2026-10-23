# Hyperliquid perps and spot RequestContext inventory

Plan,2026-10-23; lane `codex-zs5e83`, overseer `claude-ddi2il`.
Pinned landing: `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`.

## Scope and first area

Both `hyperliquid` and standalone `hyperliquidspot`, plus optional merged spot
markets, share this crate and inventory. There is no separate spot directory.
The first candidate area is command candle reads, with native response order,
seconds output, raw/native coin and HIP-3 identities and range/count semantics
unchanged. They are unsigned **POST reads**, not mutations.

Public HTTP is currently gated on the shared plain-managed static-header seam.
`lib.rs::info` and `account.rs::pump_reads` send POST `/info` with
`Content-Type: application/json`. `Cx::request_managed` has no header parameter;
`request_signed_managed` requires a credential when a SignStep is supplied.
The existing static header declaration only reaches the signed recipe path.
Do not drop JSON Content-Type, invent a credential/SignStep, or patch a venue
transport. The engine owner confirmed this gap and is coordinating its shared
unsigned-header extension with the header owner; no reviewed API pin exists yet.

Open #1797 holds `src/history.rs` and `tests/spot.rs`. Spot and perps share
`Pending::Candles`; spot fixtures directly drain legacy HTTP. Until that PR
lands, a perps-only candle conversion must retain the explicit existing legacy
spot branch and its existing hashes. Shared done/fail fingerprints may therefore
remain, and `lib.rs::info` stays while other callers use it. Do not promise a
prune count until the exact proposed source is scanned. Alternatively migrate
both candle aliases together after the hold and header prerequisite clear.

## Destinations

| Code | Engine path and native facts retained |
| --- | --- |
| SENDER | Existing legacy `info` sender remains until catalog/candle/account/history callers migrate. New managed public-header ingress uses credential=None and the same body/array selector. |
| CANDLE | One command Read Context/Lifecycle, RetryPolicy::Never, fixed original raw bound, managed JSON POST, exact read-delivery validator and context finalizers. Preserve native order, no added sorting, interval/time validation and failed streamed-page rejection. |
| ACCOUNT | Managed unsigned user-qualified Read legs admitted before weighted read-queue parking. Preserve request weights/budget facts, user/vault/application-ID subject, role/mode fallback, per-dex revision, settings and snapshot coverage. These reads do not acquire HTTP signatures merely because their payload names an account. |
| JOIN | Builder-fee read producer and trading waiters need shared observer/join ownership before migration. Preserve the existing eight-second waiter bound; one request cannot gain several independent completion owners. |
| HISTORY | Shared Context-bearing CursorHistory/Completion with original Read root and exact page-leg admission; accepted-only partial recovery, native inclusive timestamp/ID progress, retention gaps, caps and fill-to-funding stages remain. Lookup and no-I/O found/not-found/stub branches need their own read completion authority. |
| TRADE | Shared command Plan, separate Read preflight and Mutation legs, dispatch-time EIP-712 recipes, nonce domain, actual-release pacing, shared write journal and bounded late reconciliation; context finalizers replace venue aggregate outcome projection. |
| NOIO | Shared command admission then no-I/O completion authority for fixed spot/merged settings or unsupported mode; no invented HTTP attempt/mutation evidence. |

## Phases and regressions

1. Commit this exact inventory and obtain all three independent reviews.
2. After reviewed public-header ingress, add assertion-RED candle tests first:
   exact POST/body/Content-Type and public routing, parked route/no legacy send,
   early full/stream callbacks, typed preparation failure, expiry before/after
   claim, duplicate/late replies and reused command IDs, invalid inputs and
   failed streamed prefixes. Cover main/test chains and HIP-3 coin spelling.
   Preserve all existing public candle assertions; use shared prepared fixture
   glue only for immediate normal paths, never auto-claim parked controls.
3. Migrate only the authorized candle area. Use the existing Binance 20,000ms
   Read policy shape; include preparation/streaming under its original raw bound.
   Retire only its authority; no changed public WS subscription path. Prune only
   actual removed occurrences after preserving legacy spot callers if held.
4. Later catalog/core/HIP-3/merged-spot discovery spans scopes/stages and waits
   on shared Plan admission. Account read queues, role/mode lookup chains and
   builder observers require plan/join ownership. Histories also await the
   history owner's checked Context bridge; held paths remain untouched.
5. Native EIP-712/phantom-agent signing and scheduleCancel route to
   `claude-19pt8s`; nonce authority to `claude-m26ps4`; Plan/entry to
   `codex-uufub4`; joined sessions to their owning core. Current trading signs
   before dispatch and records its334ms pacing at queue time. Do not wrap those
   pre-signed bytes in managed unsigned HTTP. Keep cloids, vault/network binding,
   partial IDs, amendment/cancel proofs, scheduleCancel ACK semantics and no
   replay of uncertain writes. Use the shared classifier/journal, no new venue
   status ladders or local aggregate demotion.

## Caller-visible contract to document with implementation

`RustEngineExchange.fetchOHLCV` and historical chart consumers retain existing
arguments, candle shape, seconds timestamps, response order and native coin
mapping. Migrated calls require custom worker/hosted transport preparation,
binding and final claim; unavailable preparation and original-deadline expiry
reject the existing promise. Streaming progress cannot renew the upper bound.
`listenOHLCV` remains the current WS path. Perps-only adoption cannot claim spot
or private migration. Future write changes must document Unknown/partial IDs and
reconciliation; history changes must preserve owned incompleteness. No discovered
caller obligation is discharged by an adapter-only or docs-only change.

## Rules and verification

This is an inventory and proposed migration, not landed behavior or cutover
proof. Follow [RequestContext](RUST_REQUEST_CONTEXT.md),
[caller obligations](RUST_CUTOVER_CALLERS.md), [audits](RUST_AUDITS.md),
[quality](RUST_QUALITY.md), [rewrite](RUST_REWRITE.md) and
[venue brief](VENUE_PORT_BRIEF.md). Each area gets an original assertion-RED test
commit before implementation, exact venue-only inventory prune, caller behavior
in the same implementation PR, and independent source/adversarial, caller and
full-map reviews. No new inventory fingerprints or exemptions.

Only this venue source/tests and append-only shared docs/inventory may change.
Recheck all open PR file lists before source edits. Shared docs integration keeps
incoming additions first, ours once verbatim; inventory integration preserves all
incoming entries and reapplies only this venue's exact prune. Other conflicts
stop for maintainer direction. Never merge without the overseer's window.

Use CARGO_BUILD_JOBS=2 for focused touched-venue plus Binance tests, scoped fmt
and strict all-target Clippy. Both request-context checks, bans and diff checks
must pass. CI owns WASM release builds, replay sweeps, timing/size and complete
package proof. Keep a background PR-check watch. No local load/release builds,
TS deletion, NX edits, deployment or live-key proof claim.

Architecture gate: **No for adoption through existing engine choke points.**
Missing public headers, entry authority, joined ownership, retry, signing, nonce,
clock or error classification must be extended by the shared owning lane, never
reimplemented locally. Command admission can fail before returning a Context;
retain only exact existing numeric refusal sites until the reviewed engine
command-entry seam lands. Do not hide them in a new helper or rename a legacy
request API. Completion after a failed admission must not target another owner.

The appendix includes physical sends, venue helper calls, numeric completions,
field references and enclosing macro matches. A count is lexical, not a physical
request count. Preserve native predicates and progress facts; conservative
binding/reference cleanup belongs with its owning managed conversion, not a
checker exception or standalone count-reduction exercise.

## Exact inventory appendix

**52 occurrences / 41 distinct path fingerprints.**
Every row is one occurrence (count1); repeated fingerprints retain their
full multiplicity. All paths are relative to `sdk-rs`. No `sign_http` sites
exist at this pin; that does not prove dispatch-time signing adoption.

| Path | Occurrences | Distinct |
| --- | ---: | ---: |
| `venues/hyperliquid/src/account.rs` | 3 | 2 |
| `venues/hyperliquid/src/history.rs` | 15 | 13 |
| `venues/hyperliquid/src/lib.rs` | 9 | 7 |
| `venues/hyperliquid/src/trading/schedule.rs` | 3 | 3 |
| `venues/hyperliquid/src/trading.rs` | 22 | 16 |

### `venues/hyperliquid/src/account.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 274 / `pump_reads` | `request reference` | ACCOUNT | `0d24dec8fa74e4810d472aca9a0aaf81f2d338e73e475d69a163f5168cbb0c2d` |
| 276 / `pump_reads` | `request` | ACCOUNT | `3eb26e27ee804a373b24beefa2c8b24dd136f301503a8403319508106f46ab60` |
| 281 / `pump_reads` | `request reference` | ACCOUNT | `0d24dec8fa74e4810d472aca9a0aaf81f2d338e73e475d69a163f5168cbb0c2d` |

### `venues/hyperliquid/src/history.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 167 / `start_query` | `done` | HISTORY | `95ca4237828f8435d55e2d1e464bad969c28ac05efed94d2a1de945845738d75` |
| 174 / `start_query` | `done` | HISTORY | `da539b025b1000762173bebe5cbd496d99e02923664484e6f40e183ee7ff9118` |
| 190 / `start_query` | `done` | HISTORY | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` |
| 191 / `start_query` | `fail` | HISTORY | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |
| 217 / `start_query` | `fail` | HISTORY | `3df680cd1e2f4c54c571665d0801de18a9503305bd36f985f4e874ddca464ab3` |
| 234 / `start_query` | `fail` | HISTORY | `ad251e5c58e1d420c73ffe2316dc86c18d35f699fb86b25eba7ff188a9594df4` |
| 288 / `history_page` | `fail` | HISTORY | `a8154036fb27748faf81b7747c40e8362162b5271d8c5979eba20d6b8d766ef0` |
| 291 / `history_page` | `macro` | HISTORY | `b1854c6f5b64c99469571ff9e2f7e1da0c69bf23768ad9537e12567cf7473aa9` |
| 303 / `history_page` | `macro` | HISTORY | `4a595f0e8169456b0ddf61d405d836b0cd131fe7329ad6aadc94049f9a2e37d9` |
| 576 / `history_response` | `done` | HISTORY | `e6b5e06ea1d1144f3419909a5caa2040fb5042168a325cc09e7997675f738e62` |
| 577 / `history_response` | `fail` | HISTORY | `180d8a106dd44bf339fbb2be6079affef61e703e71477572a5b804ffa76b1c0b` |
| 581 / `history_response` | `macro` | HISTORY | `b1854c6f5b64c99469571ff9e2f7e1da0c69bf23768ad9537e12567cf7473aa9` |
| 601 / `history_response` | `macro` | HISTORY | `7658b4ae003384019ae0454fa74a2ca07a7379da26a0c633e0e088ef98db9a4e` |
| 619 / `history_response` | `macro` | HISTORY | `70af857378d4296e4191c7baf9894fecedb339396eddbfc5cf6ffc1a492f1acd` |
| 659 / `history_failed` | `fail` | HISTORY | `180d8a106dd44bf339fbb2be6079affef61e703e71477572a5b804ffa76b1c0b` |

### `venues/hyperliquid/src/lib.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 360 / `info` | `request_streamed_body` | SENDER | `2cda0ed5e578b86a8694a63ad2d9bd00b450bd15351aeef0ab0c86504facde43` |
| 1265 / `on_response` | `macro` | JOIN | `8deb2561ea92dfced0e817f532fab6f972df593bdce587a4360f0fddc59f7d6d` |
| 1367 / `on_response` | `done` | CANDLE | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` |
| 1368 / `on_response` | `fail` | CANDLE | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |
| 1469 / `on_request_failed` | `macro` | JOIN | `8deb2561ea92dfced0e817f532fab6f972df593bdce587a4360f0fddc59f7d6d` |
| 1477 / `on_request_failed` | `fail` | CANDLE | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |
| 1584 / `on_command` | `fail` | CANDLE | `6da4c02a5a70033ee0de34264a840871baf53725343f51167d02daeff7708798` |
| 1588 / `on_command` | `fail` | CANDLE | `4108d1559a4ad73e52be4c21b463683f0720640e6139a19d3dc124fc94a3a924` |
| 1616 / `on_command` | `fail` | CANDLE | `ae0c28ce939a28c6614ef199cbb7eae2c3061d25690063d605ed713d389341ae` |

### `venues/hyperliquid/src/trading/schedule.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 27 / `schedule_cancel` | `fail` | NOIO | `8b07153f589b56c1241e14a70a96c3f8c118f2fd3e973ee781133ad48887ce5c` |
| 37 / `schedule_cancel` | `fail` | TRADE | `d3b481a0bbfaa0146c446b217c5d7151fab670f0a218cb488a090be477598276` |
| 43 / `schedule_cancel` | `fail` | TRADE | `15cc6440b7566dc72beef44f387ae96cc04b08d17e2883baafbce9053644bcc1` |

### `venues/hyperliquid/src/trading.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 160 / `trade_command` | `done` | NOIO | `09d284b47513158ba424efe6b85ef618c3f2649072f4211be5a3ec043ecc0e88` |
| 163 / `trade_command` | `fail` | NOIO | `6e1b8b70601806f0f7f300336dc321505c33fbf11e9f62a5290fee6b6323fd5b` |
| 171 / `trade_command` | `done` | NOIO | `09d284b47513158ba424efe6b85ef618c3f2649072f4211be5a3ec043ecc0e88` |
| 174 / `trade_command` | `fail` | NOIO | `6e1b8b70601806f0f7f300336dc321505c33fbf11e9f62a5290fee6b6323fd5b` |
| 190 / `trade_command` | `done` | NOIO | `09d284b47513158ba424efe6b85ef618c3f2649072f4211be5a3ec043ecc0e88` |
| 198 / `trade_command` | `fail` | TRADE | `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` |
| 215 / `trade_command` | `fail` | TRADE | `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` |
| 245 / `trade_command` | `fail` | TRADE | `50ff9b0e2bb5b55c33177ff30f41cb338a1fdd72e85a2497a6b4cd14eafdeb30` |
| 252 / `trade_command` | `fail` | TRADE | `49dcc4f8d42f742545ab6f94e17ba1a375b08a7ce52e4193dca9744a1a78c841` |
| 256 / `trade_command` | `fail` | TRADE | `84bc0552279614503b71afa7b750f8fcea1a266db05309c07d88a08c6b55b760` |
| 308 / `trade_command` | `fail` | TRADE | `e0159b3732cafa0c96d1dfcb7403825d472d00b7312b7b991d19bbfdff0f5080` |
| 315 / `trade_command` | `fail` | TRADE | `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` |
| 355 / `trade_command` | `fail` | NOIO | `99cfbe816e0ff44e8f06010085e0831fce73055db9f36a216963425e0d2f9867` |
| 362 / `trade_command` | `done` | NOIO | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` |
| 370 / `trade_command` | `fail` | TRADE | `15cc6440b7566dc72beef44f387ae96cc04b08d17e2883baafbce9053644bcc1` |
| 729 / `pump_trades` | `request` | TRADE | `ca0d380912387fca3067308dbe2b5388d6d23fea1793f726dba47fff4173b2fb` |
| 901 / `trade_response` | `macro` | TRADE | `dc655ef7211a7714e84df0611ec1eb50c5732fa25e3cab129eb4f5db4b1987c6` |
| 922 / `trade_response` | `macro` | TRADE | `48fcd9334c7ffae618c7070d2136a3aa98a9d1de4790ef057c21d7dd7f5a782a` |
| 989 / `trade_response` | `macro` | TRADE | `16c49568201f97eb3ad29a92965a3ae28aef816fa3cb85ebb9da40a2a294f375` |
| 990 / `trade_response` | `macro` | TRADE | `48fcd9334c7ffae618c7070d2136a3aa98a9d1de4790ef057c21d7dd7f5a782a` |
| 1151 / `finish_trade` | `fail` | TRADE | `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` |
| 1170 / `finish_trade` | `done` | TRADE | `4310c56cdb4207e078977e5ecfb03800099cc6b8559b1e4cd4c0ef98cc4de727` |

## Independent plan review receipt

Source/adversarial, caller and full-map reviews are clean. The full-map reviewer
independently compared every path/line/function/kind/full fingerprint and
multiplicity to source and manifest, including all conservative matches. Both
request-context checks pass, including13 adversarial controls; whitespace checks
pass. Architecture gate: No for the proposed adapter adoption. This is planning
proof; implementation, CI and merge authorization remain separate gates.


## Perps candle implementation plan on actual public-header ingress

This area starts from actual #1820 `2b1e35e50b2095c6651344fc5ffe0553ad456f35`,
which includes actual #1830 command admission. The header owner released its
`tests/public.rs` hold. The fresh open-PR map still reserves sign.rs/trading.rs
for #1852 and history.rs/tests/spot.rs for #1797; none belong to this slice.
Only lib.rs, a new request_context_candles.rs test and the existing public candle
fixture's shared prepared-action drain change, plus required shared appendices.

Keep both the standalone HyperliquidSpot instance and known merged spot markets
on their existing Pending::Candles path while tests/spot.rs is held. Select this
protocol boundary with self.spot and a pure market.is_spot==Some(true) lookup
before admission; that lookup allocates no engine resource. Unknown symbols on a
perps instance still use shared admission before native rejection. Use a distinct boxed ManagedCandles payload for perps; never store a runtime
legacy-versus-Context authority switch inside one payload. The shared info sender
and all spot numeric validation/completion expressions remain counted. Therefore
this perps-only runtime cutover may remove zero lexical entries: preserve the
52-occurrence inventory exactly unless the actual scanner proves a deletion.
Do not delete still-used fingerprints to claim progress or add a numeric wrapper.
Spot adoption later removes those shared sites. No all-family closure is claimed.

For a perps FetchCandles, call admit_http_command before native validation or any
resource allocation: Read, RetryPolicy::Never, no credential, 20,000ms original
raw bound. Return on Rejected (already emitted) or Revoked, without numeric fail.
Validate interval and coin exactly as today, retaining native text/classification.
Extract only the pure existing candle JSON/time geometry into one helper reused
by the old spot branch and new perps branch; preserve each old numeric expression
verbatim to obey the shrink-only fingerprint rule. Preserve floor/clamp rules,
limit default500/cap1500, from/to handling, raw coin/HIP-3 spelling, serialized
field order and native REST api.hyperliquid.xyz / api.hyperliquid-testnet.xyz
routes. Existing API-UI WS routes are unrelated and remain unchanged.

Send POST /info with request_managed_with_headers using static ordered
[("Content-Type","application/json")], unchanged body and no signer/credential.
On successful queueing select ArrayAt::Root through stream_response. Pending
retains the exact root/physical leg and native Vec<Candle>/valid facts. Gate each
stream item and the final response through validate_read_http_delivery(root,leg)
before mutation/publication. Reuse CandleRow::candle unchanged: native encounter
order and seconds timestamps remain, with no new sorting, filtering or truncation.
A failed stream prefix is discarded by strict Context failure, never returned.

Preserve the existing single non-2xx codec in on_response; validate the managed
read before reaching it. Add only the new pending arm to existing failure routing.
Override typed preparation/released-failure hooks for this pending variant to
forward the shared VenueError intact; other pending variants keep their existing
callbacks. Read terminals use done_context/fail_context and deliberately handle
or ignore ownership denial, never numeric fallback or unwrap. No venue deadline,
retry, classifier, signer, auth, nonce or outcome algorithm is introduced.

Original three assertion-RED controls are committed at 8fd78523f51; carry them
with provenance onto this branch and rerun against actual legacy behavior before
source. At full capacity, even an invalid perps request must receive canonical
Internal with outcomeNone before validation; normal native preflight metadata
remains. Extend controls first for failed incremental prefixes, original-bound
stream progress, input/capacity refusal, sibling isolation and native HIP-3 text.
Allow exact current-page Cancel on retirement; forbid new Prepare/Bind/legacy
HTTP. Keep existing public wire/schema assertions and every held spot assertion.
Use shared prepared_actions only for the immediate existing fixture, never to
claim the parked controls automatically. After source, run full focused HL and
Binance native packages with CARGO_BUILD_JOBS=2, strict touched-crate clippy/fmt,
bans and both ratchets; CI runs WASM/replays/timing. Three independent plan and
implementation reviews are required; architecture answer should be No, an
adapter to existing shared choke points. Document concrete caller metadata,
20-second preparation-inclusive bound and spot scope in CALLERS/AUDITS/QUALITY.

## 2026-10-24: local client-order lookup completion

Base: actual Store landing `c55a6a760443bf1531eb536bfc504a312cee1356`.
The full inventory and destination map above remain the family map. Current
manifest: Hyperliquid 50 sites, history.rs 13. This slice removes only the two
local `OrderByClientId` completion fingerprints below, leaving 48 / 11:

- `95ca4237828f8435d55e2d1e464bad969c28ac05efed94d2a1de945845738d75`
  (one): matching cached symbol and client ID returns the complete native Order.
- `da539b025b1000762173bebe5cbd496d99e02923664484e6f40e183ee7ff9118`
  (one): invalid client ID or missing account identity returns the exact native
  `{"found":false,"definitive":false}` payload.

Only these synchronous returns enter the shared `admit_http_command` choke
point, with Read, 20,000ms, no credential/pacing, RetryPolicy::Never. Decide the
native local branch without effects, admit before serializing or publishing its
result, then call `done_context`; rejected/revoked admission returns immediately
without numeric completion or a fabricated owner. Cached lookup keeps precedence
over ID validation, missing identity and the spot capability branch. Use a cached
order index across admission and serialize the unchanged borrowed Order afterward;
no clone, new result schema or venue finalizer is needed. Valid uncached perps
lookups retain the native public-only refusal or private REST path; the public-only
refusal keeps its exact text even at full Context capacity. Spot uncached lookups keep their existing
non-definitive local result. Neither gets a new unused Context.

Files: history.rs, a new tests/request_context_cached_lookup.rs, this appendix,
CALLERS/AUDITS/QUALITY, and only the two manifest entries. The fresh open-PR map
shows these source/new-test paths clear; candle fixtures and signing/trading
files are held and stay outside the slice. No Paradex signing changes.

Original RED commit precedes source: both local branches reject NaN/infinite
entry clocks and full 256-owner capacity, emit exactly one typed correlated Read
failure with no financial outcome, no HTTP/WS/timer/marker/store publication;
freeing one parked genuine perps-candle peer enables exact native results and
cannot complete another owner. Controls cover perps/spot, mainnet/testnet,
public/missing identity, full cached Order wire equality and cache precedence,
non-matching symbol/ID, duplicate active command ownership, finite ID reuse,
300 synchronous retirements, and valid uncached perps REST fallthrough. Keep
existing native history assertions unchanged. Tests must fail for the original
admission assertions, rather than compilation or altered native expectations.

Verify scoped Hyperliquid + Binance native tests (CARGO_BUILD_JOBS=2), strict
all-target/all-feature Hyperliquid Clippy, scoped fmt, bans and both request-context
checks. Independently review plan then source/adversarial, caller and full-map
preservation. Architecture gate: No; only consumes existing shared entry and
terminal authority. Draft PR and background CI; request a serialized train slot
only after clean reviews and green exact-head CI. No engine or signer work.

## 2026-10-24: perps history on the caller-wait Read root

Owner: claude-fe7lgm. Approved by claude-ddi2il: `history.rs` (11 occurrences)
plus only its exact `lib.rs` pending/callback branches, on the #1848 history
authority. Reviewers: codex-cxxyqr (history bridge consumer) and codex-uufub4
(engine fit). Production waits for #1945 to land. This plan and its REDs stack
on frozen #1945 `15df705c0b5`, which supplies `Cx::command_wait`.

### Root policy (overseer ruling)

- **The root.** Each perps history command (fills, order history, both income
  forms, public trades and the REST client-ID lookup) admits ONE original Read
  root in its genuine Fresh entry. It uses `admit_caller_read_root` from the
  #1945 follow-up (`RUST_READ_ROOT_EXPIRY.md`). The root deadline is the
  caller's exact receipt deadline, as a raw f64: the configured
  `rustCommandTimeoutMs`, the per-call override, or the SDK default 120000
  stamped at receipt. The root is tracked for engine-owned expiry.
- **Admission outcomes** (all owned by the engine API):
  - **Expired** gives Revoked with no effect. The venue then calls
    `reject_expired_command(Read)`: one typed Timeout, no outcome, zero HTTP,
    timer or Store effects.
  - **A missing receipt** (unstampable clock, duplicate of a live legacy id,
    or exhausted sequence) is refused by the admission itself: one typed Read
    failure with outcome None, no fallback and no new deadline.
- **Never renewed.** The root is immutable across pages, the fill-to-funding
  stage restart and retries. Nothing renews it, and the venue never casts a
  Plan or a physical leg as a root.
- **Children.** Each page is a physical child leg,
  `http_leg_context(&root, Read, 20_000 ms, Never)`. It is minted only at
  eligible dequeue in `account.rs::pump_reads`, never at enqueue, so a page
  parked behind the shared 1000-weight/minute budget does not burn its 20 s
  before release. The engine caps the child at the root deadline.

### Geometry (for CALLERS)

| Family | Endpoint | Rows/page | Page cap | Stages |
| --- | --- | --- | --- | --- |
| Fills | `userFillsByTime` | 2000 | 200 | 1 |
| Income (fills part) | `userFillsByTime` | 2000 | 200 | 1 of 2 |
| Income (funding part) | `userFunding` | 500 | 200 | 2 of 2 |
| Funding-only income | `userFunding` | 500 | 200 | 1 |
| Order history | `historicalOrders` | 2000 (retention) | 1 | 1 |
| Public trades | `recentTrades` | n/a | 1 | 1 |
| Client-ID lookup | `orderStatus` | n/a | 1 | 1 |

- **Weight.** A streamed page reserves 120 weight at dequeue against the shared
  1000/minute account read budget, then settles to `20 + ceil(rows/20)` on
  response: `history_weight` releases 100 of the 120-weight reservation and
  then adds `ceil(rows/20)`. A full2000-row fills page settles120; a full
  500-row funding page settles45. Reservation headroom, response latency and
  other Account FIFO work determine actual throughput; one universal pages/minute
  or whole-walk duration estimate would be incorrect.
- **Duration.** Combined income retains up to400 pages because `restart_stage`
  resets the200-page budget. Any caller deadline reached before the native walk
  completes ends strictly with no rows. The default120s is not a promise that
  every retained page fits. The overseer ruled not to shrink native geometry.
  CALLERS must also name derived historical positions (`PositionHistory::FillsSymbol`),
  which rejects on root expiry rather than folding an accepted prefix.

### Bridge and terminals

- **Owner.** `history::Request` holds a `ReadCursorHistory<HistoryRow>` built
  from `ReadCompletion::from_context(cx, command, root)`, plus the current page
  leg once one is minted.
- **Per item.** Every streamed item runs under `history.admit(cx, &leg)`, so a
  stale, foreign or expired leg stages nothing.
- **Response.** `history_response` validates the delivery first, then charges
  the settled weight. It advances through `admit(..).advance_with_coverage`,
  keeping the existing native inclusive-progress, gap, retention and stall
  rules unchanged.
- **Terminals.**
  - A **failed page with the root live** keeps today's accepted-only recovery:
    `finish_with(Err)` publishes accepted rows with one INCOMPLETE marker, or a
    strict typed failure when nothing was accepted. The failed page's staging
    never enters the output.
  - **Root expiry** (parked, in flight, or on the last page) belongs to the
    engine funnel: one strict Timeout with outcome None, and no rows, marker or
    native finalizer. Hyperliquid owns no root timer and no root map.
    - The engine discards managed requests and returns their pendings directly
      to `read_root_retired(owner, Vec<Pending>)`, without normal failure callbacks.
      Hyperliquid drops them and only that exact owner's queued
      `ReadContinuation::History` entries from `account.reads`. The hook has no Cx
      and grants no terminal, send, weight settlement or Store authority.
  - **A leg-only timeout with the root live** stays an ordinary page failure,
    with accepted-only recovery.
- **Lookup.** The REST client-ID lookup completes through `done_context` or
  `fail_context` on its root, not numeric `done`/`fail`. This preserves native
  untyped lookup-codec errors instead of adding ReadCompletion's Internal default;
  typed core causes and metadata remain typed without message inference.
- **No-I/O branches.** Missing identity, an invalid range and an unknown
  public-trades market fail through `fail_context` on the admitted root. The
  standalone-spot branches already use the legacy `Completion` and are out of
  scope; they keep their numeric sites.

### Queue seam (`account.rs`, history branch only)

- **Parking.** `ReadContinuation::History` parks the retained root plus its
  continuation, with no leg.
- **At dequeue.**
  1. `pump_reads` first mints the child with `http_leg_context`. A timer
     callback can mint a leg because outside an entry
     `entry_context_allowed` is true and minting needs no delivery. Every
     refusal ends the query strictly through its own owner's `reject`, never
     `finish_with`, so a synchronous child/queue-issuance refusal cannot publish an
     accepted prefix. No weight is charged and no Prepare is emitted.
     - An expired root never reaches this point: the engine sweep retires it
       first and the hook removes its queue entry.
     - Other refusals use the shared `read_venue_error()` projection: Capacity
       is Internal "Engine request capacity reached"; no venue error map is added.
     - `Revoked` still calls `reject`, which completion validation denies if
       the owner is gone.

     A `request_managed_with_headers` refusal returns its `Pending` and takes
     the same strict route with `error.read_venue_error()`.
  2. Submit with `request_managed_with_headers(leg, POST /info,
     Content-Type: application/json)`, charging native reservation weight only
     after Ok; a synchronous Err retains no reservation. Streamed kinds retain
     `stream_response(ArrayAt::Root)`.
- **Typed callbacks.** History gets its own `on_sign_error`/`on_http_failed`
  branches. Asynchronous route/preparation/transport failures with a live root
  retain native accepted-only recovery and their typed cause/metadata; a leg-only
  Timeout is not string-demoted to Internal. Public Auth* failures add no private
  auth evidence. These are distinct from strict synchronous issuance refusal.
- **Wake-ups.** The read timer path issues children through the same public
  APIs. Engine expiry removes queued work through exact-owner cleanup, never a
  codec, numeric completion or venue root timer.
- **Unchanged.** The Account branch, its weights, the FIFO order and the
  shared budget are unchanged.

### Rulings applied

- **Root expiry.** It is ENGINE-owned, per the overseer and codex-uufub4. It
  lives in the #1945 follow-up, `RUST_READ_ROOT_EXPIRY.md`.
- **Missing receipt.** The engine's cause-aware refusal handles it, and no
  default is ever minted.
- **Production dependency.** This slice's production waits for that
  follow-up as well as #1945. The REDs below assert behaviour only through
  engine events and wire actions, so they compile today.

### Original REDs (assertions on existing behaviour; fail before source)

1. A perps fills query with envelope `timeoutMs: 300000` emits a managed
   Prepare (not legacy `Action::Http`). It is a public-intent JSON POST
   `/info` with the native body and Content-Type, and its root deadline is the
   receipt plus 300000. With no field, the deadline is the receipt plus 120000.
2. **Multi-window queue.** Twenty concurrent fills queries:
   - only the budget-eligible first reads are prepared;
   - a parked read gets its 20 s child only when the read timer releases it
     (its child deadline is the dequeue time plus 20000, capped at the root);
   - no weight is charged and nothing is prepared for a read whose root has
     expired.
3. **Root expiry** (engine funnel behaviour).
   - **While parked:** at the root deadline, one Timeout terminal (kind
     Timeout, no outcome), with no rows, no marker, and no Prepare or weight
     afterwards. The queue does not need to pump for this to happen.
   - **During page 2 in flight, after page 1 was accepted:** one Timeout
     terminal with no partial rows and no marker. Late page-2 DATA is inert.
   - **On the last page:** the same inert result.
4. **A page failure with the root live** keeps accepted-only recovery (the
   existing `owned_partial` oracle) under managed delivery. A first-page or
   zero-eligible failure stays a strict typed failure.
5. **Income stage restart** keeps the same root: the funding-stage child's
   deadline never exceeds the original root deadline. A root expiring between
   stages ends with no fills-only partial.
6. **Live versus stale timer issuance.**
   - A read-timer wake after the root is retired (command reused after a
     terminal) issues nothing for the old query.
   - A reused command id gets a fresh root.
6b. **Unstamped entry.** A plain `handle` at `f64::MAX` (no default stamp)
   gives exactly one typed Read failure with outcome None, no HTTP, no timer
   and no 120 s root.
6c. **Same-deadline ordering.** With the root and the leg at one deadline,
   page 1 accepted and page 2 in flight, deliver the leg timer and the
   deadline event in both orders. Each order, plus late DATA and command-id
   reuse, gives exactly one strict Timeout with no rows or marker.
6d. **Dequeue refusals.** At dequeue, a refused leg mint (forced Capacity,
   and expiry) gives a strict typed terminal, never an accepted prefix, with
   no weight and no Prepare.
7. **Lookup.**
   - The REST `orderStatus` lookup completes through its root.
   - It is a Fresh-entry `Expired` read when the wait has passed: one Timeout,
     no HTTP.
8. **Shared queue.** Account reads queued behind and ahead of history keep
   their FIFO position and weights. A history root expiry does not consume
   another owner's budget slot.
9. **Fingerprints.** `check-request-context.py` with only removals: history.rs
   goes from 11 to 0 for these perps branches. The spot `Completion` sites are
   not in the manifest. The shared Account sites stay, with no rehash.

All existing `tests/history.rs` native assertions (paging, gaps, retention,
order fold, income split, sort/limit) are kept. Where they drive legacy
`Action::Http`, they move to the shared managed fixture (prepare, bind and
claim), not to new expectations.

### Files

- `venues/hyperliquid/src/history.rs`
- `venues/hyperliquid/src/account.rs`: `ReadContinuation::History` and the
  `pump_reads` history branch only
- `venues/hyperliquid/src/lib.rs`: only the `Pending::History` callback arms
- a new `venues/hyperliquid/tests/request_context_history.rs`
- `tests/history.rs` fixture glue
- this appendix, CALLERS, and the manifest removals

Open-PR map at plan time: no open PR touches these Hyperliquid files.
codex-5c57jl's socket plan keeps its new `Pending` arm separate and leaves
`history.rs` and the history branch of `pump_reads` exact.

Architecture gate: No. This only consumes #1848, #1945 and the existing
managed-header ingress.

## 2026-10-24 handover: codex-ih35js plan and original controls

Ownership transfers from claude-fe7lgm at dd4cf3941a5 to codex-ih35js on
`feat/rust-hyperliquid-history-context`. The original plan commits remain in
history. No original REDs or production edits were inherited. The overseer
reconciled dependency custody: reviewed source ce54ab9357b may be plain-merged
LOCALLY for development; no mergeable PR or READY claim before the frozen pin.
That source is genuinely integrated at9943d66bbdc with no conflicts. Later
frozen/actual dependencies must be plain-merged before fresh CI and landing.

The exact current API is `admit_caller_read_root(None, None)` and
`read_root_retired(owner, Vec<Pending>)`. Cleanup drops returned pendings and
queued History continuations only for that exact Owner. It has no Cx and cannot
publish, send, settle another owner, or install a venue timer. An expired Fresh
entry is rejected by `reject_expired_command(Read)` after the root admission
returns Revoked; a missing receipt is refused by root admission itself. There
is no default-root substitution. Sub-millisecond expiry is an initial-admission
rule; an admitted fractional absolute root expires at its exact boundary.

The scanner re-dump confirms history.rs has nine fingerprints / eleven lexical
occurrences, six numeric terminals plus five conservative macro matches:

| Exact fingerprint | Count | Native site and managed destination |
| --- | ---: | --- |
| `3df680cd1e2f4c54c571665d0801de18a9503305bd36f985f4e874ddca464ab3` | 1 | Missing identity refusal; original root typed failure. |
| `ad251e5c58e1d420c73ffe2316dc86c18d35f699fb86b25eba7ff188a9594df4` | 1 | Invalid range refusal; original root typed failure. |
| `a8154036fb27748faf81b7747c40e8362162b5271d8c5979eba20d6b8d766ef0` | 1 | Unknown public market; original root typed failure. |
| `e6b5e06ea1d1144f3419909a5caa2040fb5042168a325cc09e7997675f738e62` | 1 | Lookup success; original root done_context. |
| `180d8a106dd44bf339fbb2be6079affef61e703e71477572a5b804ffa76b1c0b` | 2 | Lookup parse/transport failures; original root fail_context. |
| `b1854c6f5b64c99469571ff9e2f7e1da0c69bf23768ad9537e12567cf7473aa9` | 2 | Ranged-kind macro in builder/response; same native predicate on managed continuation. |
| `4a595f0e8169456b0ddf61d405d836b0cd131fe7329ad6aadc94049f9a2e37d9` | 1 | Fill aggregateByTime macro; preserve false and native body. |
| `7658b4ae003384019ae0454fa74a2ca07a7379da26a0c633e0e088ef98db9a4e` | 1 | Inclusive-progress cursor formatting; preserve timestamp/seen identity. |
| `70af857378d4296e4191c7baf9894fecedb339396eddbfc5cf6ffc1a492f1acd` | 1 | Combined-income stage switch; same root and native reset geometry. |

The prospective shrink is exactly history.rs 9F/11O to zero, conditional on the
unchanged scanner after implementation. Five removals are lexical macro matches,
not physical HTTP calls. The shared account sender still serves Account reads;
its entries and unrelated lib entries remain. No prune occurs in this phase.

The new request_context_history target uses genuine Engine command receipts,
transport preparation/release, HTTP/data/timer events and native fixtures. It
must compile against unchanged venue production on the reviewed core stack.
Original controls pin managed-only Prepare for fills/public trades/lookup,
configured/default/fractional root deadlines, strict parked/in-flight/final-page
expiry, income-stage root retention, missing-receipt refusal, same-boundary
ordering, late callbacks and same-ID recovery. Working controls retain exact
JSON POST/header/user/vault bytes, accepted-only live-root recovery, strict first
failure, native funding split, queue reservation bounds and invalid host-clock
refusal. Neither a compile error nor a failed setup assertion counts as a RED.

Further adoption controls must cover forced leg-capacity refusal with no weight
charge and Account/History FIFO progress after exact-owner expiry. They require
the genuine entry/queue harness; no private engine mutation or substitute issuer
is introduced to fabricate the condition. Retain all native history owned_*
assertions and add shared prepared-action glue only when implementation begins.

Open-PR file lists were rechecked before editing: no open PR touches the proposed
Hyperliquid history/account/lib/test paths. The landed socket implementation's
wire, continuations and private_subscribe regions are outside this slice.
Architecture gate: No venue root timer, retry scheduler, issuer, classifier or
completion algorithm is introduced. All three independent reviews precede any
production readiness claim. The older-base ratchet currently reports incoming
foreign prunes; dependency/current-landing integration must resolve provenance
without editing those maps. Scope here is committed plan plus original REDs.

Plan review corrections: stage-specific weight facts replace universal duration
estimates; exact cleanup-only hook and synchronous reservation handling are
explicit. Lookup uses direct Context terminals to preserve native untyped codec
errors. Typed History route/transport failures and derived positions are named.
Frozen #1945 448567a690c is genuinely integrated at57f9e380dd9, with README and
CALLERS incoming-first keep-both resolutions only; ce54 source remains the locally
authorized reviewed API, pending its eventual frozen/actual integration.

## Original-control checkpoint

Test-only04e6ee356ad adds sixteen real-engine controls without venue production,
existing-fixture, checker or manifest edits. On the composed reviewed core with
frozen #1945, all sixteen compile and execute: eleven intended assertion REDs,
five working native passes, zero ignored. The REDs expose legacy physical sends,
missing caller-root timers/refusals, expiry publication and absent root-capped
children. Positives pin vault subject and exact JSON POST/headers, accepted-only
owned partial recovery, strict first-page failure, native income gross-PnL/rebate
split and host invalid-clock refusal. Scoped test-target strict Clippy and crate
rustfmt pass. The initial formatting diff was corrected before this checkpoint.

An attempted native leg-timer lookup was an invalid setup assumption: native
legacy HTTP has no engine leg timer. That failed precondition is excluded from
the qualifying count. Its replacement uses genuine host HttpFailed and the root
sweep in either order. Full engine leg-timer/root-timer interleavings, forced
child-capacity refusal, Account FIFO cleanup/progress, and typed public Auth*
metadata/isolation remain required adoption controls, not completed proof here.

Independent source/adversarial, caller and exact-map reviews are CLEAR for the
corrected plan and exact new target. Their reviews are read-only; execution is
this lane's evidence. Architecture gate No. Current actual landing7eed6dc1b009
is genuinely integrated ate8baa42338c, with only CALLERS incoming-first plus the
shared seam sections once. Hyperliquid production, the new target and shared core
remain byte-exact to the qualifying pre-integration composition. Both full
request-context checks now pass, including all15 checker controls, without a
Hyperliquid prune. This resolves the earlier stale-base provenance failures.

The branch remains plan plus original REDs; no managed-history, green CI or READY
claim. Local development may consume reviewed ce54 under the overseer's explicit
rule. Before a mergeable PR or readiness claim, integrate fe7lgm's frozen
root-expiry/public-refusal pin, then the actual landings and fresh CI. No landing
merge without an explicit window.

## Hyperliquid perps history implementation checkpoint

The original-control commit04e6ee356ad precedes this source conversion. All
sixteen original controls now pass; four additional adoption controls pin actual
root/page timer orders, forced child-capacity refusal without spending another
owner's weight, Account/History FIFO preservation in both enqueue orders after
exact-owner expiry, and public Auth* metadata without private auth-health evidence.
All20 execute, with zero ignored. The complete Hyperliquid and Binance native
suites pass568 tests across45 targets. Independent implementation reviews, fresh
CI and integration of the replacement frozen #1976 head remain separate gates.

The already-managed cached/invalid/anonymous local lookup branches keep their
existing20s genuine-entry scope and no-timer behavior. Standalone spot branches
also remain unchanged. Uncached perps lookup and the remaining legacy no-I/O
refusals enter the tracked caller root: at full capacity the public-only missing
identity refusal now receives the canonical Internal capacity error first. When
admitted, it retains the native untyped identity message and immediately retires
its root (SetTimer, ClearTimer, one terminal). No default deadline is manufactured.
This supersedes the older cached-lookup plan's full-capacity exception for that
previously legacy uncached branch.

Only history.rs loses inventory:9 fingerprints/11 lexical occurrences (six
numeric terminal calls and five conservative macro matches). The continuation
binding changes together with its owned managed conversion. The Account sender,
its two fingerprints/three occurrences, and every unrelated manifest entry are
retained. Full provenance checking currently sees the newer incoming Aster and OKX chain
prunes on origin; no foreign maps are edited to conceal the stale composition.
The checker unit suite passes15 controls.

Existing history fixtures now prepare managed transport through test_support.
The200/400-page geometry controls explicitly request the existing maximum caller
wait, respond within each20s child and drive only the actual native budget wake;
all original financial, gap, retention, dedup, cap and income assertions remain.
The default/custom root lifetime controls live in the dedicated20-control target.
There is no venue root timer, retry loop, issuer, classifier or finalizer.

Scoped strict all-target/all-feature Clippy and rustfmt pass after removing one
unnecessary generic-argument borrow. The scanner exactly matches the local
manifest. Its full provenance gate currently rejects only foreign Aster/OKX
entries already pruned on origin; those maps stay untouched pending genuine
owner-composed dependency integration. No full ratchet PASS is claimed.

Independent source/adversarial, caller/fixture and full-map implementation
reviews are CLEAR at06d05755752. All16 original control bodies remain exact;
the four adoption controls are additive. Native financial serializers, filters,
queue/paging facts and Account/sign/socket functions are retained. Full-map
review confirms exactly11 intended paths, no foreign changes, and sole
history.rs9F/11O removal with Account2F/3O unchanged. Architecture No: consumes
shared roots/expiry/row/terminal APIs, with no venue replacement policy.
This receipt and caller default-stamp precision are documentation only.
Replacement1976 integration, full provenance and fresh CI remain required
before READY; no landing merge has been attempted.
