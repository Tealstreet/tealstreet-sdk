# Lighter RequestContext inventory and migration plan

Plan, 2026-10-23; lane `codex-zs5e83`, overseer `claude-ddi2il`.
Base: `origin/feat/rust-rewrite` at `7641650e7b1`.
Applies to `lighter` and `lighter-robinhood`, mainnet and testnet perpetuals.
This document records proposed work, not landed behavior or cutover proof.

## Authority, scope and dependencies

Follow [the request contract](RUST_REQUEST_CONTEXT.md),
[caller obligations](RUST_CUTOVER_CALLERS.md), [audits](RUST_AUDITS.md),
[quality rules](RUST_QUALITY.md), [rewrite](RUST_REWRITE.md) and
[venue brief](VENUE_PORT_BRIEF.md). Venue HTTP/reducer changes stay in
`sdk-rs/venues/lighter`; engine changes belong to their owning lanes.
The owner requires the entire legacy inventory to reach zero before master;
deferred entries below are dependencies, never exemptions.

Before editing, inspected all file lists of the 21 open PRs into landing.
No open PR touches Lighter Rust source/tests at this pin. Other assigned venues
collide: GRVT `src/account.rs` and `tests/account.rs` (#1785–#1788), Paradex
`src/account.rs`, `src/trading.rs`, `tests/account.rs` (#1787), Hyperliquid
`src/history.rs`, `tests/spot.rs` (#1797). Leave those source/test paths alone
until their PRs land; recheck open PR file lists before each area.

Overseer correction permits shared-by-design ledgers and inventory edits.
For integration conflicts, keep both doc additions, incoming first and ours once
verbatim. Preserve every incoming inventory entry and reapply only this venue's
exact prune, then verify the ratchet. Do not edit the checker to authorize growth.
Other conflicts stop for maintainer direction. No merge without an overseer window.

## Current inventory

The ratchet finds **51 lexical occurrences / 47 distinct path fingerprints**:
15 HTTP calls (7 `request`, 8 `request_streamed`), 33 completions
(8 `done`, 25 `fail`), and 3 conservative enclosing-macro matches.
There are no `sign_http` or `request_streamed_body` entries in this venue.
The appendix lists every fingerprint, occurrence and destination; line numbers
are pinned to the base above, not intended as stable post-migration locations.

| File under `sdk-rs/venues/lighter/src` | Occurrences | Distinct fingerprints |
| --- | ---: | ---: |
| `account.rs` | 5 | 5 |
| `history.rs` | 10 | 10 |
| `identity.rs` | 3 | 3 |
| `lib.rs` | 10 | 8 |
| `limits.rs` | 6 | 6 |
| `trading.rs` | 17 | 15 |

Destination codes used in the appendix:

| Code | Engine path and native facts retained |
| --- | --- |
| CANDLE | Single-request command/background Read Context via `Cx::http_context`, `request_managed`, `stream_response` and the engine Lifecycle. Command replies use `done_context`/`fail_context`; background retires through `finish_http_context`. Preserve interval/range validation, generation/revision fences, sorted candle shaping and streaming validation. |
| PUBLIC | Independent single-request background Read Context/Lifecycle for ticker and depth seeds. Preserve existing in-flight flags, startup/backoff, book generation and source revisions. Retire success and error contexts; no command completion authority. |
| CACHED | No-I/O command Read completion authority and context finalizers for cached leverage/tier and fixed one-way mode. No invented HTTP attempt or mutation evidence. |
| CATALOG | Managed background Read plan for `orderBookDetails` then optional `orderBooks` fallback. Preserve validated catalog staging; shared plan admission prerequisite before multi-request migration. |
| IDENTITY | Managed command/background Read plan across lookup candidates. Distinct per-attempt legs under one owner/deadline; retain exact account selectors and forbid API-key-index fallback for public identity. Shared plan admission prerequisite. |
| SNAPSHOT | Single background managed Read with existing account row admission/coverage and native snapshot serial/revision fences. The trading reduce-only confirmation can observe this read: audit joined ownership before moving this shared producer. Preserve failed-scope exposure, settings and balance. |
| ORDERS | Managed authenticated Read plan for native all-market/market fallback reconciliation, existing row coverage and terminal/revision fences. Shared plan admission and dispatch-time auth prerequisites; no copied retry or row-admission algorithm. |
| BACKFILL | Managed authenticated background Read plan plus existing native execution dedupe and coverage. Preserve reconnect windows, native paging facts and one fill per execution; shared plan admission and auth prerequisite. |
| HISTORY | Keep existing `CursorHistory`/`HistoryRows`/`Completion` ownership. Add the shared Context-bearing completion/publication path and managed Read legs with `validate_read_http_delivery` before command-owned publication. Preserve accepted-only partial pages, strict auth/cursor/budget refusals and trade/funding stages. Shared plan/history authority prerequisite. |
| LIMITS | Managed authenticated background Read; separate ordinary refresh ownership from command preflight waiters. Use shared plan/coalesced ownership before moving its command-dependent use; retain tier cache/cadence. |
| TIER | Managed command Mutation Context/Lifecycle for `changeAccountTier`, native result facts through `http_write_result`, and context finalizers. Bound late-response codec and dispatch-time auth are prerequisites. Post-ACK limits refresh remains background, not a second command result. |
| TRADE | One managed command plan, separate Read preflight/nonce legs and Mutation submission legs, shared write journal and context finalizers. Native ACK/refusal/IDs are codec facts, never a venue outcome aggregator. Shared plan admission, native signer/nonce recipe and bounded late reconciliation prerequisites. |
| MACRO | Conservative lexical match, not an extra HTTP/completion call. Remove by renaming the enclosing local `request` binding as part of that area's managed conversion, preserving the predicate exactly. No checker exception. |

## Phase 1: single-request candle area

First implementation PR changes only `lib.rs`, focused Lighter tests/support,
this plan, caller/audit/quality appendices when warranted, and the exact manifest
prune. It removes four occurrences: `lib.rs:386`, `644`, `1221`, `1222`.
The exact `cx.fail(c, error)` expression at `404` remains only for refusal
before authority exists: `http_context` can fail without returning a Context.
Expected venue total **51 -> 47**, `lib.rs` **10 -> 6**; all other source maps
remain identical. Removing that last fallback requires the shared engine
pre-authority failure seam, requested from `codex-uufub4`, not a renamed local
completion helper. It is still counted and must reach zero before master.
Market discovery, ticker/depth and private paths stay intact.

1. Commit failing regression tests separately, before production changes.
   Assert candle work emits managed preparation rather than legacy HTTP; route
   parking emits no physical request; callbacks before claim publish neither
   rows nor success; release followed by valid streaming returns sorted data.
   Assert preparation denial, timeout, late/duplicate callback and command-ID
   reuse settle once without touching a later command. Assert typed preparation
   failures retain their `VenueError` fields, including kind/outcome. Keep native
   invalid interval/range behavior and failed streamed-prefix rejection.
   Add background denial/timeout cleanup and subsequent poll recovery controls.
2. Store command/background Context authority with `Pending::Candles` and issue
   explicit Read policy (`RetryPolicy::Never`, public credential intent).
   Use a single `Policy { timeout_ms: 20_000, ... }` read context, matching the
   landed Binance read adapter (no separate root/child needed for one HTTP).
   The host's configured request timeout may shorten its original raw bound;
   do not add timeout recomputation or a venue timeout/retry framework.
   Preflight failures must use
   the issued command completion authority even when no request is queued.
   If authority issuance itself fails, preserve the one exact numeric refusal
   above and prove that no managed owner/request/resource was created.
3. Queue through `request_managed`; attach the existing array selector with
   `stream_response`. Propagate queue refusal through the existing public-error
   cleanup, retire background handles on every terminal path (including stale
   generation/revision early returns), and use generation authority on command
   finalization. Override the default string-only signing/preparation error hook
   for candles so typed errors reach `fail_context` unchanged.
   `validate_read_http_delivery` applies to
   command read publication; background reads retain engine delivery admission
   plus native generation/revision checks, not a fake command owner.
4. Adapt candle test drivers through the existing shared
   `safe_cex_engine::test_support::prepared_actions`; preserve all old assertions.
   Keep mixed legacy startup handling while only candles migrate. Add parked
   preparation tests directly rather than auto-claiming them in the helper.
   Existing long-delay resubscription tests become expiry controls under managed
   deadlines; also deliver old responses/failures within the original 20s bound
   after resubscription to continue exercising the native generation fence.
5. Prune only removed Lighter fingerprints/counts; run both request-context
   checks. Append CALLERS landed behavior in the implementation PR, then obtain
   independent source/adversarial, caller and full-map reviews and fix findings.

Preserve recovered-stream candle dominance, unsubscribe/resubscribe fencing,
subscribers, requested time bounds/count, all aliases and public-feed modes.
The architecture-gate answer is **No**: this area adopts an existing engine
choke point. If review finds a missing shared concern, report it to its owner
instead of implementing a per-venue substitute.

## Later venue areas, one small PR each

After phase 1: independent ticker/depth seeds (`PUBLIC`), then no-I/O completion
sites (`CACHED`). Audit the private snapshot's command observers before treating
it as independent. Catalog/identity candidate fallbacks, history stages, orders,
backfill, limits/preflight and trading wait for `codex-uufub4`'s shared plan
admission. Joined/coalesced producers stay last and must be reported explicitly.
Ask `codex-cxxyqr` for the shared history Context-bearing completion path:
current `Completion` stores a numeric command and calls legacy finalizers, so
simply wrapping the existing HTTP site cannot make history fully managed.

Lighter has no inventoried `sign_http` call, but native Schnorr/Poseidon auth,
nonce allocation and transaction signing currently happen inside the venue.
Do not move pre-signed expiring bytes into `request_managed` and call that a
signer migration. Coordinate recipe/nonce ownership with `codex-uufub4` and
the signing lane before private work; keep all native vectors and slot identity.
Ordinary header-bearing legacy auth cannot be silently dropped: unsigned
`request_managed` has no arbitrary header argument. Report any missing shared
recipe seam with concrete signatures and affected sites, not an exemption.

## Caller effects and evidence boundary

Phase 1 preserves candle result shape/units/order and subscription semantics.
It changes the transport contract for Lighter candle requests: custom transports
need prepare/bind/claim support, and unavailable preparation or expiry fails the
command instead of using the legacy sender. For background subscription work,
denial/timeout retires the context and clears only the matching generation's
`in_flight`, emits no command result and allows later fallback polling.
Retired responses cannot finish a new command or emit a stale candle.
The implementation's CALLERS appendix must
name `RustEngineExchange.fetchOHLCV` / `listenOHLCV`, worker/hosted HTTP adapters
and chart consumers; document actual landed behavior, not this plan's status.
The planned 20s raw upper bound includes route preparation and cannot be renewed
by streaming progress; document that deadline alongside transport readiness.
No NX file edits, deployment, live-key proof or TS-venue deletion is claimed.

Later write PRs must document original-deadline Unknown, partial IDs and typed
native refusal behavior through the existing caller error contract; history PRs
must preserve command-owned incomplete markers and accepted-page recovery.
These are separate deliverables, not implied by finishing public candles.

## Verification and merge gates

Plan-only proof: recompute exact source fingerprints with the existing scanner;
assert all six path maps equal the manifest; run
`python3 sdk-rs/scripts/check-request-context.py` and
`python3 sdk-rs/scripts/check-request-context.test.py`, and `git diff --check`.

Implementation proof from `sdk-rs`, with `CARGO_BUILD_JOBS=2`:
`cargo test -p safe-cex-venue-lighter -p safe-cex-venue-binance`, scoped
`cargo fmt -p safe-cex-venue-lighter --check`, and
`cargo clippy -p safe-cex-venue-lighter --all-targets -- -D warnings`, plus both
ratchet checks and relevant bans. Run via the installed toolchain and nvm Node22
Corepack, never the Volta Yarn shim. CI owns WASM release builds, replay sweeps,
timing/size and complete repository gates. No local load test or release build.

Post independent findings and architecture answer in the PR, fix then review
again, push and start background `gh pr checks <n> --watch`. Ask for a merge
window only with green CI and complete proof. Run the separate fail-closed
revert guard before porcelain merge; do not merge autonomously.

## Exact site appendix

Each row below is one distinct path fingerprint. Repeated line numbers retain
their full counted multiplicity. Macro lines are listed by their native predicate.

### `account.rs`

| Full fingerprint | Lines | Count | Kind | Destination |
| --- | --- | ---: | --- | --- |
| `c8f9a0172b2ce950ab3034688232375d595c32b2c2aeb379c9b371e8e3f4a0fa` | 672 | 1 | `request_streamed` | IDENTITY |
| `72b8b7cc43c1ad222fb9344f94bd9cd33b65cd264e0d380557b1f2813e496b59` | 755 | 1 | `request_streamed` | SNAPSHOT |
| `d32ae768422b5856ac109309d4d810d40edfb507300a9926583cdef8336166ec` | 795 | 1 | `request_streamed` | ORDERS |
| `a06eefea5de1bc52140036fd44389e113e0fa21b2855ebc2381fdfc470ebdea0` | 2087 | 1 | `fail` | TIER |
| `cf890186cf11dfee2fff367f85521f3f12679567ec4fbbf21089f9dc28b87187` | 2445 | 1 | `request_streamed` | BACKFILL |

### `history.rs`

| Full fingerprint | Lines | Count | Kind | Destination |
| --- | --- | ---: | --- | --- |
| `bde2b4607b65d0c5a80f7cc74527b97cf172bf2f01c6a15af05a3d59cccf9083` | 247 | 1 | `fail` | HISTORY |
| `43eef047be9375351e4b6c48ab87b405865d076d1f8b8f01a416086ebe7c6972` | 302 | 1 | `fail` | HISTORY |
| `79e87d62ec17f7bf661325e15041d94257db847779850b97696a982d07699860` | 330 | 1 | `fail` | HISTORY |
| `be82ef9cf88debeece0fea3eca405694b32c7c83cc5256bc463fb0ec3fed4841` | 334 | 1 | `fail` | HISTORY |
| `bc8873d3293d9ef3e4011d84e43669c3922f9f9ce78df72ea42a8040419389e5` | 341 | 1 | `fail` | HISTORY |
| `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 362 | 1 | `done` | HISTORY |
| `05267b6f1fa97972a9adb12d43d7cc145356c749dcaf345decbc6b914335d18e` | 364 | 1 | `fail` | HISTORY |
| `7d9792a0676bccdf20b899bb81c8f19bdf33402d56972457af968a5c4f3f433c` | 447 | 1 | `request_streamed` | HISTORY |
| `5b3fa60c35b90d7d54c1c78a6208717da15fe27f3581b790ac0bc0f5cc640ca1` | 506 | 1 | `macro` | MACRO / HISTORY |
| `2cb4373904a9bd645f5f03689ee4a667afacb9de41ec48ddb42d1b05efe59004` | 781 | 1 | `macro` | MACRO / HISTORY |

### `identity.rs`

| Full fingerprint | Lines | Count | Kind | Destination |
| --- | --- | ---: | --- | --- |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 66 | 1 | `done` | IDENTITY |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 69 | 1 | `fail` | IDENTITY |
| `7b3812a7db557b003fb7e0703f4a783dbb56807d3a225c32d012c38243013485` | 147 | 1 | `request` | IDENTITY |

### `lib.rs`

| Full fingerprint | Lines | Count | Kind | Destination |
| --- | --- | ---: | --- | --- |
| `1edfac341aacb74280c04d466785c3f6d76a4871da14449c5712b67a351a608e` | 244 | 1 | `request_streamed` | CATALOG |
| `2bbebb7465e83aac90aeda0b54e32b996272c805baa3f70ed2fafc69a8e73268` | 289 | 1 | `request_streamed` | PUBLIC |
| `18cd2d97de7df286e747348057b54c6924eb3e65bc7df288229e919c4a447f11` | 320 | 1 | `request` | PUBLIC |
| `9c626141896323bf40dfa1082ef3fcf4edc0bd527e4d2bd0ce8e65cc6054b231` | 386 | 1 | `request_streamed` | CANDLE |
| `f06dce80ebd266653d22bf24f105c411c21c1de8a011cd3d6fe688214267e373` | 404, 644, 1222 | 3 | `fail` | CANDLE |
| `6cb9a1e96c67a077e2d60646f5ef1786c7274adad1b681cbcddadb9317b35947` | 1221 | 1 | `done` | CANDLE |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1459 | 1 | `done` | CACHED |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1460 | 1 | `fail` | CACHED |

### `limits.rs`

| Full fingerprint | Lines | Count | Kind | Destination |
| --- | --- | ---: | --- | --- |
| `fef0765d66051b91cf0f7abc366da1b0610f672c386bf9cf4580a46b5c24391d` | 150 | 1 | `request` | LIMITS |
| `417fc0471a429c56d55eaad2b64bb864c71d9c9b5598f513e022b2a7d8b1417c` | 175 | 1 | `done` | CACHED |
| `26fc6022675dbea66e5c0e605347b6e0bdfc680e7a2744ff5621ba94df81790f` | 191 | 1 | `request` | TIER |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 202 | 1 | `fail` | TIER |
| `0b656f6ecb232b6e58dda360591ebef081f74c048b07c021b2589b269fb21488` | 223 | 1 | `done` | TIER |
| `68ea64422786bdc642d572cc7c40cd278680587c99e433f09070c826e1967dad` | 229 | 1 | `fail` | TIER |

### `trading.rs`

| Full fingerprint | Lines | Count | Kind | Destination |
| --- | --- | ---: | --- | --- |
| `70b134c16687e60d0b82d98b3c7a949f8b205710ebbd9dfb3cfe9798c93cb8c0` | 180 | 1 | `fail` | TRADE |
| `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` | 189, 210, 283 | 3 | `fail` | TRADE |
| `a6b6e8a4b78910cf9fb6e685e8f4360f0a5926caf5a3e8a1fdbcb726feaf7863` | 199 | 1 | `fail` | TRADE |
| `f3dda987438ebe48dd9d889f51ce4861effe2295b27284ca1078f76e98d1d191` | 206 | 1 | `fail` | TRADE |
| `74b21c9b3c437839bff40aeb220ccceb04d2ae9f5387d912b1e45552e404fcf2` | 243 | 1 | `fail` | TRADE |
| `588cf8ac30fce8487799c5876953de687fb902d6a4f1f5215e63068126e21e77` | 250 | 1 | `fail` | TRADE |
| `f6b6f1b6ea96ac6f57f8d33ea26017b3e021f32d01711134210681a272ab66ba` | 254 | 1 | `fail` | TRADE |
| `7a759169f4762e27c3dc28eb74de3aca1d844dc924af69f19adfebe7d51471be` | 322 | 1 | `fail` | CACHED |
| `0b656f6ecb232b6e58dda360591ebef081f74c048b07c021b2589b269fb21488` | 329 | 1 | `done` | CACHED |
| `604f28d91adfdb4cd0caac06047abd9569f9a6c248b698a784032256d7cf8047` | 378 | 1 | `done` | TRADE |
| `192c4cfe04acb541fc2e95e0c5051b2cb96999301a50f397f0ea164366e7cef3` | 469 | 1 | `request` | TRADE |
| `8bc784b5c45d9fdaf371df54a954f2e1e8049b372ce5ac4d6e7b3d75c1917b37` | 482 | 1 | `request` | TRADE |
| `5f02ce1e69bf29b2fd757ec1080b380c2ceb6dbac585e1f2e62426aeafe8085f` | 565 | 1 | `request` | TRADE |
| `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` | 1041 | 1 | `fail` | TRADE |
| `0bc3151ddc1e8889eb3a752d5431f3ab5643750a38409c18c5e239d38dcfc4b2` | 667 | 1 | `macro` | MACRO / TRADE |


## Phase 1 implementation checkpoint

Plan commit `239631e2523`; regression commit `61fa794a815`. The unchanged
venue passed both existing candle ordering tests, then all seven new regression
functions failed at the explicit legacy-HTTP/no-final-claim assertion. After
migration all seven pass without relaxing that assertion. The existing two
ordering tests adopt only the shared prepared-actions fixture; their original
assertions, clocks and payloads remain intact.

Source/adversarial plan review corrected the initial five-removal estimate:
Context issuance can refuse before authority exists, so exactly one old numeric
failure remains; expected and actual prune are four occurrences,51->47. The
shared-engine owner acknowledges this dependency. Caller review added explicit
background failure/poll recovery, and full-map review recomputed every appendix
fingerprint and multiplicity. History and nonce owners acknowledged their
separate prerequisites. No later-area implementation or merge is implied.

Final local phase-1 checks pass: all Lighter and Binance unit/integration tests,
strict all-target Lighter Clippy, scoped formatting, full bans, the exact source
inventory check,13 ratchet controls and `git diff --check`. Independent
source/adversarial, caller and full-map implementation reviews report no blocking
findings; the caller review's validator wording is corrected to distinguish
command authority from background engine/native admission. Full-map review
verified every original candle-review assertion/clock/payload is retained.
Architecture gate: No. CI replay/size/complete-package proof and the overseer's
merge window remain separate gates; this checkpoint is not a merge approval.

## Phase 1b: engine command-entry admission (2026-10-23)

Base `6741c1c6609` contains actual candle #1807 and engine command-entry #1830
(`0001c78eeb2`). This follow-up replaces only the remaining candle issuance
refusal, not the deferred inventory families in the appendix. Current Lighter
inventory is 47 occurrences (`lib.rs`: 6); removing the one exact
`cx.fail(c, error)` fingerprint makes those totals 46 and 5 respectively. Every
other venue fingerprint and every other Lighter path remains incoming verbatim.

The complete callback audit finds two `fetch_candles` callers: genuine
`on_command(FetchCandles)` passes `Some(command)`, and the poll timer passes
`None`. Before admission the helper only reads subscription generation/revision;
it queues no work, publishes nothing and consumes no resource. Command reads
will call the landed `Cx::admit_http_command(Policy)` once, without an ID argument.
On either `Rejected` (already emitted one typed denial) or `Revoked` (no current
entry authority), return without a local terminal, request or fallback. The
background caller continues using `http_context(Scope::Background, policy)`;
capacity refusal still clears only its matching generation's in-flight flag.
Both branches retain the same public Read policy, 20-second timeout, no
credential and `RetryPolicy::Never`. Native input validation, query bytes,
streaming, Context terminals, revision fences and WebSocket behavior are intact.
No wrapper, numeric completion, local capacity policy or retry is introduced.

Files: `src/lib.rs`, `tests/request_context_candles.rs`, this plan, the exact
Lighter-only manifest prune and caller/audit/quality appendices. Fresh all-open
PR inspection (`--limit 200`) finds #1866 holds only Lighter `src/trading.rs`;
leave it untouched. No other open PR holds this slice's source or test file.

Commit this plan and obtain independent source/adversarial, caller and full-map
reviews before production edits. Commit original-RED controls separately: full
256-owner admission returns the engine's exact typed Read capacity error (no
financial outcome), before invalid native parameters, with exactly one terminal
and no I/O/timer allocation. Existing owners must still release and complete;
a live duplicate command must produce no new request or terminal and must not
revoke the original. Exercise both aliases and networks, preserve all original
seven candle controls, and prove a freed slot accepts the refused command.
Then adopt shared command entry, prune just that one fingerprint and retain all
incoming entries/prunes. Run focused candles, all Lighter+Binance native tests,
strict touched-crate clippy/fmt, bans and both request-context checks; obtain
three clean implementation reviews and open a separate draft PR with CI watch.

Caller-visible refusal changes from the adapter's generic lifecycle-unavailable
message to the engine's typed `Engine request capacity reached` / Internal Read
denial, with no financial outcome. Successful and native validation replies keep
their existing wire/result shape. Duplicate live command IDs are refused by the
shared entry owner without affecting the original command. Private trading,
auth/nonce, history and multi-request paths remain dependent on their owning
shared seams. Architecture gate: this is a consumer of the shared admission
choke point, not a per-venue admission implementation.

Plan-review correction: initial admission can also reject a nonfinite raw clock
or invalid computed deadline. The shared denial is `Invalid request context`,
`InvalidInput`, outcome `None`, replacing the same generic adapter error. Add a
one-terminal/no-I/O control for NaN and both infinities on a genuine command
callback, plus a later finite-clock admission to prove no retained owner.
This is shared policy validation; native candle parameter validation is unchanged.

Implementation receipt: original production source at `6741c1c6609` with corrected
controls in `d1e6ab36d6c` produces two assertion failures (capacity message and
invalid-clock kind), with all eight compatibility controls passing. The first
RED commit `3c99a9d2957` also rejected the engine's existing duplicate diagnostic;
`d1e6ab36d6c` explicitly preserves that diagnostic rather than treating it as an
admission bug. No production source was committed before these controls.

## Cached Read completion slice on actual 732cf8 (2026-10-23)

Base `732cf8b0a6e5f5dbe783c62af9f14e7c32ee8fe0`; branch
`feat/rust-lighter-cached-read-context`. The current full venue inventory is
46 occurrences: account5/history10/identity3/lib5/limits6/trading17. The original
full inventory appendix remains historical; incoming prunes are retained.

Selection: migrate the two synchronous cached reads together. `FetchLeverage`
(lib.rs) reads only Store settings and serializes equal buy/sell leverage;
missing settings fail with `Lighter leverage is not loaded`. The extension
`FetchAccountTier` (limits.rs) returns only its existing tier cache, including
`null` before a successful limits refresh. Neither command fetches metadata,
refreshes limits, authenticates, signs, allocates transport or joins another read.
Native cache cadence, tier labels/normalization and all writes remain unchanged.

This is the largest currently unblocked coherent slice found in the fresh
open-PR file map. Hyperliquid spot candles could prune6 but #1877 holds the
existing test that explicitly requires merged spot to remain legacy. Paradex
candles could prune6 but retain klines-to-TradingView fallback and polling, which
require a separate shared multi-attempt authority audit (the existing Read retry
API is not absent); identity/private paths also wait on shared
session/signing cores. Lighter identity/catalog have native multi-attempt fallback;
its independent ticker/depth public pair prunes2, GRVT catalog prunes1, and the
already planned unsigned Paradex public trades consumer prunes1. GRVT session
regions belong to claude-55ao7t. Paradex signing is untouched while Sam decides
whether to use remote Stark payloadSigner option C. Lighter trading.rs is held by
#1866, and Hyperliquid sign/trading by #1852/#1866; no held source/test is edited.

### Implementation and exact prune

Before reading or serializing either cache, admit the genuine current callback
using existing `Cx::admit_http_command(Policy)` with Read/no credential,
20,000ms/Never/no pacing. This existing engine API explicitly supports a no-HTTP
initial Read root. Rejected already emitted its one typed failure; Revoked
returns without touching another owner. Never use a numeric fallback or invent a
request, delivery, timer, mutation fact, completion wrapper or venue capacity rule.
Consume the root immediately with `done_context`/`fail_context`; handle returned
revocation without unwrap or a second result. Put admission directly in each
existing command branch; preserve the native result/error codec verbatim.

Only lib.rs, limits.rs, new tests/request_context_cached_reads.rs, this plan,
CALLERS/AUDITS/QUALITY appendices and the shared inventory are intended paths.
The exact three removed occurrences are lib.rs `cx.done(command, data)` and
`cx.fail(command, error)`, plus limits.rs `cx.done(command, tier)`. Expected
inventory46→43, lib5→3 and limits6→5; every other Lighter and foreign mapping
remains byte-equivalent. Do not prune the identical `fail(command,error)` in
limits.rs's SetAccountTier branch. Generate counts from the actual scanner.

### Original REDs and verification

Commit assertion-failing tests before source. Test both command entry points via
the real Engine, both aliases and networks. With256 managed candle owners parked,
valid cached leverage/missing settings/tier must each refuse once with the shared
Internal capacity error and no financial outcome; a refused ID can be used after
freeing a peer slot. Invalid NaN/±Infinity clocks must give the shared InvalidInput
entry error, then succeed at finite time. These assertions must fail on original
source, which currently bypasses admission and returns cache data/native missing
setting error instead. No tests merely inspect source spelling or mirror the move.

Compatibility controls preserve finite-time leverage JSON, missing-setting text,
unknown tier null and repeated immediate completions without leaking the256-owner
bound; prove no HTTP/managed HTTP/signing/socket/timer or Store publication from
cached reads. Preserve duplicate-live-command diagnostic/original candle ownership,
including cross-entrypoint duplicate IDs. Retain existing account_limits cached
standard-tier and SetAccountTier tests unchanged. Use existing shared test_support
and native support fixtures, with no generic transport fork.

Obtain independent source/adversarial, caller and full-map plan reviews before
source and repeat after implementation. Run focused new tests, full Lighter and
Binance native tests with CARGO_BUILD_JOBS=2, strict touched-crate Clippy/scoped fmt,
bans and both request-context checks. CI runs WASM/replays/size/timing. Push a draft
PR, keep a background CI watch, then request a train slot when green and reviews
are CLEAN; separate fresh full-map guard before a head-matched merge.

Caller-visible: finite valid caches retain method signatures and native output.
Cached calls now share bounded command admission: capacity/invalid clock failures
carry shared typed Read metadata and cannot consume another owner; no financial
outcome or transport support is required for these synchronous reads. No caller
outside the SDK needs a signature change. Architecture gate: No; this consumes
existing engine entry and completion authority, adding no cross-cutting mechanism.

### Cached Read original RED and implementation receipt

Plan2a093966518, original RED ffef6eaf580. Eight focused tests run against the
unchanged actual732cf8 production: five intended assertion failures and three
compatibility passes. Present leverage/tier incorrectly succeed at full capacity;
missing leverage exposes its native unclassified error instead of the capacity
refusal; both cached success branches ignore invalid clocks. After consuming
shared admission and Context terminals directly, all eight controls pass unchanged.
The test uses real Engine callbacks and genuine parked candle peers, never mocks
Context authority. Unknown tier remains null; existing loaded-tier/write controls
remain intact in account_limits.rs. Scanner-generated delta is exactly lib c5a…1
and c873…1 plus limits417fc047…1; no other fingerprints are changed. Production
uses only existing shared APIs, no new helper, transport, retry or signing path.

Cached slice verification: 504 Lighter+Binance native tests across48 result groups,
all8 focused controls, strict Lighter all-target/all-feature Clippy, scoped fmt,
bans and both request-context checks (14 controls) pass. Genuine integration of
actual #1871/#1872 at e233cb05dfb preserves their source and prunes; three shared
doc conflicts keep complete incoming first plus exact611/868/777-byte own tails.
No Rust runtime dependency of the tested Lighter/Binance closure changed during
integration. Only the same8 own paths differ from the new landing. Fresh CI and
independent implementation reviews are separate final gates; no WASM/replay or
whole-venue closure is claimed by native evidence.

## Public ticker/depth seed Context slice on actual db32 (2026-10-23)

Fresh branch `feat/rust-lighter-public-seed-context` starts at actual #1900
`db32da5314cce685e3ff2e17d51e26bfb94c8093`. Full current inventory43:
account5/history10/identity3/lib3/limits5/trading17. The public ticker and order-book
seed pair removes the next two independent physical send sites; catalog fallback
retains the one remaining lib site. Public identity/catalog retain native multiple
attempts; private reads have signer/join prerequisites. Hyperliquid spot's six
sites remain blocked by the #1877 legacy-spot fixture. GRVT login belongs to #1905.

Before source, recheck every open PR file list. #1866 holds Lighter trading.rs;
#1877 now also holds tests/request_context_candles.rs. Neither may be edited.
If either held fixture necessarily needs adaptation, stop this slice and report
that collision rather than weakening it or cloning its assertions elsewhere.
Intended own files: lib.rs, new tests/request_context_public_seeds.rs, existing
public.rs/public-review.rs/support only where prepared drains are needed, this
plan and CALLERS/AUDITS/QUALITY appendices, exact shared inventory prune. Other
native fixtures may be listed by the independent driver audit before source;
no assertion changes or unrelated production edits are authorized by this list.

### Native facts and managed design

`exchangeStats` is unsigned GET with empty headers/body and the exact six native
ArrayAt envelope alternatives. It snapshots ticker_revision, stages native Stats
rows (retaining skipped-row diagnostics/partial valid peers), then updates only
markets whose WebSocket source revision is no newer. Preserve ticker_in_flight,
existing startup Backoff, stream subscriptions and native field mapping.

`orderBookOrders?market_id={id}&limit=100` is unsigned GET with empty headers/body,
a complete response (no array selector). Existing depth generation/revision,
in_flight, rest_retry_at and per-book silence/gap rules remain native. Preserve
strict RestDepth/Depth validation and last_received/nonce/book_changed behavior;
failed or stale reads cannot reset another subscription's latch or publish a book.

Add mandatory Context ownership to the two existing Pending variants. Before
setting a new in-flight latch, issue a separate shared Background Read Context:
no credential,20s,Never,no pacing. Use request_managed, then attach the original
stream selector only for tickers. This is a single logical read per seed; no
command completion owner, fake mutation fact, joined execution, local Plan or
copied retry policy is introduced. Admission/queue failure uses existing native
failure cleanup; never emit a command terminal or touch another Context.

On ticker item admission use the existing shared live-Context validation before
native staging; engine managed delivery already fences physical release. Before
publication validate the current Context, retire through finish_http_context,
then preserve native ticker source or depth generation/revision checks. Every
terminal path retires its Context, including malformed envelopes/prefixes,
preparation/queue/transport/expiry failures and stale-generation early returns.
Depth cleanup mutates only its matching generation. Ticker cleanup releases its
single in-flight latch and existing startup Backoff; Context refusal must not
strand either seed. Expired handles may retire without acquiring send rights.
No new retry timer, budget, outcome classifier or metadata policy is added.

### Original REDs, exact map and gates

Commit assertion-failing tests first: both aliases/networks emit public Prepare
with exact native GET/header/body bytes; neither early response nor streamed rows
before final claim publish. Successful release preserves native ticker partial
rows/source dominance and depth mapping. Preparation denial, original deadline
while parked/claimed, and failed prefixes publish nothing, allocate no command
terminal and release only the matching seed latch for native retry. Prove late
old depth response/failure cannot release a resubscribed replacement; stale
revision cannot replace fresh WS depth/ticker data. Test Context capacity refusal
against unrelated parked command owners, then free a slot and recover through
native timer/backoff. Keep existing native assertions through prepared_actions.

Expected exact removals: lib fingerprint
2bbebb7465e83aac90aeda0b54e32b996272c805baa3f70ed2fafc69a8e73268 once
(ticker request_streamed), and
18cd2d97de7df286e747348057b54c6924eb3e65bc7df288229e919c4a447f11 once
(depth request). Inventory43→41, lib3→1; all other Lighter/foreign maps unchanged.
Never prune the remaining catalog sender or rehash unrelated legacy sites.

Run full Lighter+Binance native tests CARGO_BUILD_JOBS=2, strict touched-crate
Clippy/scoped fmt, bans and both current request checks. Obtain three independent
plan reviews before source and source/adversarial/caller/full-map reviews after
implementation. CI supplies WASM/replay/timing/size evidence with a background
watch. Request a train slot only when green; fresh separate full-map guard before
head-matched merge. No whole-venue zero, private/signing or transport-package proof
is claimed by this public pair.

Caller-visible: custom worker/hosted transports must support prepare/bind/claim
for these background GETs. Preparation shares the original20s bound with response
processing. Successful data/cadence and SDK call signatures stay native; parked
or expired work cannot mutate Store. Failure remains background recovery, never
a correlated user's command result. Architecture gate No: existing Context and
HTTP lifecycle APIs carry policy; the venue retains only protocol/cache facts.

### Public-seed slice narrowed to depth only before source

The deeper independent fixture audit found a concrete #1877 hold collision for
ticker adoption: request_context_candles.rs's background-generation recovery test
selects the first SetTimer at NOW+140000. Its poll issues ticker before candle;
a managed ticker adds the same20s deadline first and the held test would expire
ticker rather than the intended candle owner. No deadline change, operation
reorder or duplicate test can honestly remove that collision. Ticker adoption
waits until the held fixture can be disambiguated by its owner or after landing.
No ticker source/test is changed by this PR.

Proceed with **depth only**, the next unblocked physical read. All design,
original-RED and validation requirements above apply to depth; ticker requirements
remain a deferred plan rather than this implementation claim. Exact prune is
only18cd2d97de7df286e747348057b54c6924eb3e65bc7df288229e919c4a447f11
once:43→42/lib3→2; ticker2bbebb and catalog1edfac remain exact. The held candle
fixture does not subscribe to order books, so depth adds no timer to its poll.
Confirm this with all three plan reviews before source. Shared unheld test drivers
may receive prepared drains with actual event clocks, retaining all assertions.

Depth driver audit also adds unheld tests/public_supervision.rs. Its suppressed-
feed test currently counts exactly one tracked timer and300 total SetTimer
operations after subscribing a depth read. Managed read deadlines/recovery now
add legitimate timers. Retain every timer and action; explicitly distinguish and
assert the20s Context deadlines/recovery from the original2s poll count, keeping
all original no-WS-connect/no-ping/no-reconnect-supervision guarantees. Never drop
unknown timers, manufacture completion or alter runtime order/deadlines to keep
an old total. Public/public-review direct depth drains use prepared_actions at
the actual event clock; shared support only where required. Ticker-only drivers,
held candle/trading tests and production ticker logic remain unchanged.

### Depth original RED and native implementation receipt

Plan8b700d2ffc8 narrowed at caf6e4cf1b3, driver correction c7a1d352046; all three
independent plan reviews CLEAN/architectureNo. Original RED362e8edb77f runs seven
compiled assertion failures on actualdb32 legacy dispatch. Test correction
6311e52cfe0 pins the existing native testnet REST host (testnet-api.lighter.xyz);
the original managed-preparation/capacity RED assertions are unchanged. This is
a fixture correction, not a new endpoint change. All seven controls now pass.

Production changes only Pending::Depth, its single sender and its terminal
cleanup. Issuance before the in-flight latch uses Background Read20s/Never/noauth;
queue failure retires the exact returned handle. Success captures liveness and
retires before native generation/revision early returns or unchanged RestDepth
mapping; global failures retire before existing matching-generation cooldown.
No command result, retry loop, row framework, signing or native ticker change.
Scanner removes only18cd… once:43→42/lib3→2; catalog/ticker sites stay exact.

Only public.rs and public-review.rs need driver adaptation: shared prepared_actions
at each existing event clock exposes released depth HTTP while preserving every
native assertion, including the old-failure no-new-request check. The actual
six supervision tests pass unchanged for depth-only, so no timer assertion edit,
shared support change or held fixture edit is needed. All33 focused tests across
public/public-review/public_supervision/new depth controls pass. Dedicated tests
cover exact four-deployment wire, preclaim fencing, parked/released original
expiry and recovery, route/transport/malformed cleanup, resubscribed generation,
WS revision precedence, capacity peer isolation and300 successful retirements.
Full native/strict/ratchet/CI and independent source reviews remain final gates.

Depth verification on source81ac295d25a:511 Lighter+Binance native tests across49
result groups PASS, including all7 new controls, all8 cached-read controls and
unchanged held candle tests. Strict all-target/all-feature Lighter Clippy, scoped
fmt, bans and both current request checks/15 controls PASS. Ticker/catalog,
private/account/signing/trading and shared engine source stay unchanged; only9
own paths differ from actualdb32. Independent implementation reviews and broad
CI remain separate gates. No local WASM release/replay/timing sweep was run.

### 2026-10-24: ticker hold released after depth landing

Base actual1912 `5ce4b13fb7edadcf188a49f435663091e9596807`, including actual1877.
The public-seed plan above already maps and specifies ticker ownership; resume
only its ticker half. Fresh open-PR file lists show no Lighter lib/test collision
except the unrelated trading.rs nonce lane. Current inventory42/lib2 becomes41/1
by pruning only ticker fingerprint
`2bbebb7465e83aac90aeda0b54e32b996272c805baa3f70ed2fafc69a8e73268`:1.
Catalog `1edfac` remains counted; its primary/fallback relationship is separate.

Add mandatory Context to Pending::Tickers. Before setting ticker_in_flight,
issue Background Read/no credential/20s/Never/no pacing. Refusal goes through
existing startup Backoff without a stranded latch or command terminal. Replace
only its request_streamed with request_managed followed by the exact same
stream_response Envelope aliases and empty headers/body. Validate live ownership
before item staging. Before native response publication, capture liveness and
retire the Context once; failures retire then clear the latch and use existing
startup.failed(false). Preserve native usable partial rows (failed && rows.empty
is the existing rejection predicate), per-symbol ticker_sources/revision dominance,
all field conversions and the original poll/backoff cadence. Malformed streamed
envelopes discard staged prefixes through the existing stream-error cleanup.
No new row framework, retry budget, native fallback or local authority seam.

Original REDs before production: all four deployments Prepare exact GET bytes;
unclaimed complete/streamed responses publish nothing; parked/released20s deadline
and typed route/transport failure retire the seed and permit native recovery;
malformed envelopes/prefixes do not escape; usable partial rows retain native
semantics; newer WS rows dominate only their symbol; Context capacity admission
cannot strand the latch or retire unrelated candle owners; repeat300 successful
cycles leave no Background owner leak. No user command terminal/financial outcome.

The previously held candle fixture is now editable. Its timeout branch currently
fires only the first NOW+140000 SetTimer, which becomes ticker's timer when both
seeds are managed. Change only host driving to deliver every captured timer due
at that same deadline, as a real host does; preserve its exact time and every
candle generation/recovery assertion. Run this driver-only correction against
legacy ticker source first (GREEN). Preparation selection is already URL-scoped
to candles. Add no production ordering/deadline workaround or duplicate test.
Existing ticker-native fixture drivers may use shared prepared_actions at their
original event clocks, retaining every wire, value, failure and revision assertion.
Only drivers that actually encounter ticker Prepare may change; no trade/signing
source, engine or held files. Document each required driver correction.

Files: lib.rs, new tests/request_context_tickers.rs, affected existing public/
startup fixture drivers and shared test support, request_context_candles.rs host
clock driver, exact manifest key and append-only plan/CALLERS/AUDITS/QUALITY.
Original tests must compile and fail on actual legacy dispatch assertions. Run
Lighter+Binance native (CARGO_BUILD_JOBS=2), strict touched-crate Clippy, scopedfmt,
bans and both inventory checks/15controls; three independent plan then implementation
reviews, architecture No (existing shared Context/managed HTTP), draft/background
CI at120s and serialized train slot only when green. No catalog/private/signing
or whole-family-zero claim.

Ticker execution receipt: original six compiled admission assertions are RED at
85eec6c0675; all six pass on managed source. Driver-only24a7c15952e first passes
all10 candle Context controls with legacy ticker source. Full native verification
identified five fixture targets still relying on immediate ticker transport:
public-review and public_feed now use shared prepared drains at their original
clocks; shared support restores the startup ticker before trading protection
assertions. Two depth-capacity controls first settle the genuine public ticker
before filling all256 command-owner slots, preserving the original depth/free-slot
and WS-revision assertions. The supervision driver completes empty ticker reads
and counts actual TimerFired deliveries (exact300 polls), rather than counting
created-but-cancelled Context deadline timers as fires. All original supervision
assertions and timestamps remain. The five affected targets now pass; production
trading/account, catalog and engine code are unchanged. Full final native/strict,
legacy-driver compatibility, ratchet and independent review gates follow.
