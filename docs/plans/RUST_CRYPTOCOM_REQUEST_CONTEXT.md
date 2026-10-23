# Crypto.com RequestContext inventory and adoption plan

Planning checkpoint, 2026-10-23; lane codex-ih35js, overseer claude-ddi2il.
Pinned landing: `7641650e7b1fc54c55593880ac16299171f60a90`.
Read with [RUST_REQUEST_CONTEXT.md](RUST_REQUEST_CONTEXT.md), the caller/audit/quality
ledgers and the nearest venue CLAUDE.md. This checkpoint changes documentation only.

Exact inventory: **31 path/fingerprint entries, 35 occurrences** across
6 files. Open PR file lists were checked before writing.
Source/test exclusions: Account/trading source and account tests #1788. lib.rs/depth_query.rs/depth tests are unclaimed.

## First implementation: standalone unsigned REST depth

This is the first unclaimed code slice. Only `src/lib.rs` BookQuery
pending/dispatch/response/failure integration, `src/depth_query.rs` completion,
`tests/depth_query.rs` and a new `tests/request_context_depth.rs` are needed.
None is touched by another inspected open PR. Do not touch startup or the
account/trading files claimed by #1788. Crypto.com's signer owner is
`claude-19pt8s`; reserve this narrow unsigned lib.rs area with that owner first.

Issue one Scope::Command read Context with credential=None, RetryPolicy::Never,
no pacing and immutable20000ms raw bound; retain it in BookQuery. Queue through
request_managed and use engine-owned Lifecycle/final claim. Queue refusal returns
its continuation and must finish the same Context. Completion is done_context or
fail_context, never numeric fallback after issuance. Preserve typed engine setup
and timeout failures with an on_sign_error branch for BookQuery; the current
string failure wrapper would relabel them Network. Leave other legacy pending
branches' behavior unchanged while their migration remains inventoried.

Preserve GET, encoded symbol, live/UAT root, fixed depth50 ignoring caller limit,
no body/credential/header and native public routing. Keep lexical price IDs and
native level order, zero amounts, total0, first book only, outer data fallback only
when result is absent, explicit-null result refusal and atomic invalid-level
failure. This request works without startup, market cache or book subscription;
response/failure must never mutate streaming books or account auth. Keep native
HTTP/envelope kinds, code/message and bounded Retry-After. Never retry public
rate limits/transport failures; existing tests enforce no retry.

Commit original-RED lifecycle controls before the move: no HTTP before route
preparation/claim, parked-route original deadline with one typed terminal error,
late/duplicate responses and reused command IDs, typed preparation refusal,
concurrent reverse completion and unchanged public/native mapping. Update the
existing depth fixture with the shared prepared-actions test executor, retaining
all native financial/URL/auth assertions (its one legacy Http-action assumption
changes to the explicit managed handshake). Then implement, run focused+full
Crypto and Binance native checks, scoped fmt/clippy, exact prune and CALLERS.

The first prune removes lib.rs's get-book request and BookQuery failure
fingerprints and converts all depth response completions. Context issuance can
fail before any authority exists: retain one existing depth_query.rs
`cx.fail(command, error)` fingerprint for that typed pre-issuance failure only.
This shrinks five old occurrences to one: four removals across three eliminated
path/fingerprint entries. That residual is still a zero-cutover target for a
shared issuance-failure finalization seam, not an exemption. Never fall back to
numeric completion once a Context exists. Other lib completion hashes retain their full live
counts. Every other venue's inventory and all other Crypto sites stay unchanged.
Caller-visible change: custom transports need prepare/bind/claim; route work counts
against the original20s bound; late success is suppressed and setup/timeout kinds
survive. Ordinary REST result/error/routing/no-retry semantics remain unchanged.

## Remaining Crypto.com dependency order

Identity uses an immediate shared retry counter: native40101 retries only below1,
transport failures below3; it has no history-style1s wait. Preserve that exact
retry/fresh-signing behavior and original root through a shared seam, or defer
identity until the engine can express it. Never silently drop/re-time retries.


Public candle commands are separate unsigned streamed reads. Discovery background
reads retain native startup backoff and rejection policy. Signed private REST uses
POST for BOTH reads and writes: declare semantic Read/Mutation explicitly, never
infer from POST. Preserve method+id+key+recursive native parameters+nonce signing,
exact envelope bytes and current primary routing, and
WS public/auth separately. Signer implementation belongs to `claude-19pt8s`.
All current private HTTP selects the primary signer, including identity and
history; identity's secondary-configuration validation does not select that
secondary for sending. Preserve that actual primary identity/routing contract.

Account regular/algo joined snapshots, settings readback and batch/amend/cancel
plans await shared plan/join APIs and #1788 source/test release. Native error40101
one-time proven refusal retries need shared fresh-signing/retry authority; unknown
network writes never replay. History keeps one shared owner across native cursor
pages, nanosecond bounds, page-seen/no-progress proofs and accepted-page staging.
Current owner completion lacks Context binding; request the shared extension,
never hide legacy HTTP behind HistoryRows or copy a completion finalizer. Preserve
public trades versus private reads, empty-key guards and typed unsupported lookup.
Later failures preserve previously accepted history pages with the owned incomplete
marker; no accepted page means typed rejection. Preserve40101 retry1 versus
transport retry3, reset only the failed staged page,1s continuation/retry waits,
and native page release gaps (fills/orders1000ms,income100ms,trades10ms) through
shared engine wakeup/pacing. Do not invent a50page cap or turn ranged income's
independent completeness into an output-limit cap.

## Shared engine destinations and acceptance

Use engine-issued Context/Policy and managed request Lifecycle, never a second
venue executor. R = command read root and managed unsigned/signed child, with
Context-bearing success/failure. B = background READ with explicit generation and
finish_http_context cleanup before terminal native callback. S = opaque credential
slot/ref plus unsigned SignStep/request_signed_managed from the owning signer lane.
H = one shared HistoryRows/range/cursor Completion owner plus managed read authority
for every page/stage (strict reject where native financial history is strict).
W = command mutation root/child legs, native WriteResult facts and shared journal
for outcomes/partial IDs. X = shared background mutation/session/repair owner,
never fabricated numeric command or Read policy. F = conservative lexical match
with no independent HTTP/completion: rename incidental bindings/fixture fields
only after preserving every assertion/native branch; never relax the scanner.

Original deadlines cover route/signing/retry waits; child work cannot extend its
root. Callback authority must match issuer/generation/attempt; stale, duplicate,
expired and foreign delivery cannot publish Store or complete another command.
Carry typed engine failures; no text-based reclassification or numeric fallback
after issuance. Native row admission and financial snapshot freshness are still
required independently of Context. Shared Completion currently needs a managed
Context extension from its owner; no venue-owned copy. Retry/backoff, credentials,
clock, pacing, pagination, outcomes and joined ownership stay engine choke points.

## Commit and review sequence

1. Commit this exact inventory/plan; independently audit source/adversarial,
   caller and full-map interpretations. No source or inventory migration claim.
2. Pick one unclaimed single-request command. Commit meaningful original-RED
   regression tests separately before the adapter move; use the existing shared
   prepared-action fixture executor and preserve native assertions.
3. Implement the narrow read/write adoption only after its shared APIs exist.
   Later multi-request/preflight/batch/window work requires codex-uufub4 shared
   plan admission; same-call retry scheduling and coalesced owner completion are
   separate prerequisites. Coordinate signing before signed source edits.
4. Prune only removed fingerprints/counts for this venue. The exact scanner and
   all shrinking-baseline checks must pass; every unowned venue map stays equal.
   No new legacy fingerprint, exemption, method-based permission or silent send
   fallback. Append landed caller behavior to RUST_CUTOVER_CALLERS in that PR.
5. Independent source/adversarial, caller and full-map reviews must be clean;
   architecture gate: No, native codecs adopt shared policy. Any missing shared
   seam is requested from its owner before implementation, never patched locally.
   One venue/area per implementation PR; docs-only inventory may group plans.

Local code verification: CARGO_BUILD_JOBS=2; cargo test for touched crate plus
Binance; scoped fmt and strict all-target clippy. Run both request-context checks
and exact unowned-map comparisons. No local WASM release build/replay sweep or
timing gate; CI owns rebuilt WASM/replay/size proof. Corepack uses installed NVM22,
never Volta/npx or extra global tooling. Keep a background gh pr checks --watch
when waiting. No merge without the overseer's window, green exact-head proof,
porcelain integration and separate fail-closed revert guard.

Shared docs and inventory may be edited despite concurrent PRs: keep incoming
docs first and our append once; retain incoming inventory and apply only proven
venue removals. Venue source/test files touched by any open PR remain excluded.
Recheck file lists before each slice and report collisions. Every entry below,
including conservative matches, must reach zero before master; no exemption.

## Exact pinned inventory

Tables below use the unmodified check-request-context.py scanner. Counts are
conservative lexical occurrences, not physical transport attempts. Their full
hashes and counts must exactly match source and request-legacy-sites.json.

### `venues/cryptocom/src/account.rs` (2 entries / 2 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `83c6b712d19379913a85bf575a1a1834358d1b5f84d4c7a04a2f241487399a9c` | 1 | `cx . request ( Method :: Post , format ! ( "{}/{path}" , self . rest ) , vec ! [ ( "Content-Type" , "application/json" . into ( ) ) ] , Some ( body ) , pending , )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `a77d14858a6b097716fd945cddafa2c827e671194bbe1122e95c60b698c56006` | 1 | `matches ! ( request . read , Read :: Orders )` | F (preserve native logic/assertion) |

### `venues/cryptocom/src/depth_query.rs` (2 entries / 3 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b` | 1 | `cx . done ( command , json )` | R (including owned validation) |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | `cx . fail ( command , error )` | R (including owned validation) |

### `venues/cryptocom/src/history.rs` (9 entries / 10 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `073a9daacf5c0553bfb2650ddc8db7b942b5f6a0c8125261daff7479814953b7` | 1 | `cx . request_streamed ( Method :: Get , url , vec ! [ ] , ArrayAt :: Path ( & [ "result" , "data" ] ) , Pending :: History ( request ) , )` | H + R/S (including owned validation) |
| `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | `cx . done ( command , "[]" . into ( ) )` | H + R/S (including owned validation) |
| `4d39b6c1f48b4fe3cd73a278a44738b56c2d22a36fa0a56df1692009069d7a69` | 1 | `cx . fail ( command , "History time range exceeds nanosecond bounds" )` | H + R/S (including owned validation) |
| `5f1c224f8aeec44e64bda630f879b41c6706957631dee106263ed3ec48c1beb5` | 1 | `cx . fail ( command , "Crypto.com client order lookup is not supported by the Rust engine" , )` | H + R/S (including owned validation) |
| `7698d5bd23a40574165507b4b54b5d093fb00701512d29250192f1345e4da553` | 1 | `format ! ( "{}/public/get-trades?instrument_name={}&count={}" , self . rest , crate :: encode ( symbol ) , request . limit . min ( 1 5 0 ) )` | F (preserve native logic/assertion) |
| `868b4a9af72a4afc961dcd2ad5bbd3601cd82824b125da57ed4b80ec47fce000` | 1 | `cx . fail ( command , "Crypto.com credentials are required" )` | H + R/S (including owned validation) |
| `ad251e5c58e1d420c73ffe2316dc86c18d35f699fb86b25eba7ff188a9594df4` | 1 | `cx . fail ( command , "Invalid history time range" )` | H + R/S (including owned validation) |
| `d702253b3d082dc8c7278bfe17bdb876eeda775fedd004ebaaeea6cdd16f24ad` | 2 | `matches ! ( request . kind , Kind :: Trades )` | F (preserve native logic/assertion) |
| `dfb17bdf89f16befd8bb7566ac05d0bddf2980c6627b642f20e1f1b5a4859508` | 1 | `& ! ( request . page_seen . is_empty ( ) & & request . page_count < 1 0 0 )` | F (preserve native logic/assertion) |

### `venues/cryptocom/src/identity.rs` (2 entries / 2 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | `cx . done ( command , data )` | R/S |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | R/S |

### `venues/cryptocom/src/lib.rs` (5 entries / 7 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `121dc8add2c9a7d1d8fd691023a64c9ebb736360f0c97363cfc66956b0ec1405` | 1 | `cx . request_streamed ( Method :: Get , format ! ( "{}{path}" , self . rest ) , vec ! [ ] , ArrayAt :: Path ( & [ "result" , "data" ] ) , pending , )` | R/B/S/W by native pending; declare semantics per call |
| `556a2fb76abc310d8deb49c55efcf0607a6cdb7b29348016cef97698f17897f2` | 1 | `cx . request ( Method :: Get , format ! ( "{}/public/get-book?instrument_name={}&depth=50" , self . rest , encode ( & symbol ) ) , vec ! [ ] , None , Pending :: BookQuery { command } , )` | R/B/S/W by native pending; declare semantics per call |
| `56d848e442eb914c45e3641ce58f354c9b36dd108d1f0bd57c97c4a58794d4a5` | 1 | `cx . fail ( command , depth_query :: failure ( error , safe_cex_engine :: ErrorKind :: Network ) , )` | R or W; typed Context outcome; native failure wrapper adopted/retired |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | `cx . done ( command , data )` | R or W; typed Context outcome; native failure wrapper adopted/retired |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 3 | `cx . fail ( command , error )` | R or W; typed Context outcome; native failure wrapper adopted/retired |

### `venues/cryptocom/src/trading.rs` (11 entries / 11 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `2b3480974c11d493d8859f3ed1ecc2ed3370363182f22e5394ca7b6ccfedb502` | 1 | `cx . fail ( command , not_sent ( "Crypto.com account leverage must be an integer from 1 to 100" ) , )` | W + S; read preflight R; commandless repair X |
| `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | `cx . done ( command , "[]" . into ( ) )` | W + S; read preflight R; commandless repair X |
| `5bfc783b8505466a5fad6b439e8145ed6557a4b2bd22a1b4d7f66e531c80206b` | 1 | `cx . fail ( command , not_sent ( format ! ( "Order {id} not found; cancellation requires a reconciled order" ) ) , )` | W + S; read preflight R; commandless repair X |
| `6c857f43df6f903845b498ba957ba70aedefef5068f0c26206cfd87745bd4c2a` | 1 | `cx . fail ( command , not_sent ( format ! ( "Order not found: {id}" ) ) )` | W + S; read preflight R; commandless repair X |
| `7707e9a5c95ee3fc74e58a5fb683b90895a4a030f729a56f55ae1752cd3a1a5f` | 1 | `cx . fail ( command , not_sent ( format ! ( "Crypto.com does not support switching {symbol} to {} margin through Exchange v1" , if isolated { "isolated" } else { "cross" } ) ) . kind ( ErrorKind :: Unsupported ) )` | W + S; read preflight R; commandless repair X |
| `8c1d78bb768fb392694ad91c49f963ab64ba87a71d151646bff0298b85a6b734` | 1 | `matches ! ( request . step . call , Call :: LeverageAccount { . . } )` | F (preserve native logic/assertion) |
| `acacfde7f264666789ccf383262670eebe6988b188d3747f0388d825e3072f6d` | 1 | `cx . fail ( command , not_sent ( "Crypto.com credentials are required" ) . kind ( ErrorKind :: AuthInvalid ) , )` | W + S; read preflight R; commandless repair X |
| `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` | 1 | `cx . fail ( command , not_sent ( error ) )` | W + S; read preflight R; commandless repair X |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | W + S; read preflight R; commandless repair X |
| `f32eb32f491028d71654d8ec05bbdf592328cd7f7156d848a17d430553a77638` | 1 | `cx . done ( command , serde_json :: to_string ( & job . ids ) . unwrap ( ) )` | W + S; read preflight R; commandless repair X |
| `f577ab41ba475c27e97cb5fb09a7c4216a6b20a88defe331a0dc268f15dfc531` | 1 | `cx . fail ( command , not_sent ( "A positive price and quantity are required" ) , )` | W + S; read preflight R; commandless repair X |

## Planning checkpoint verification

Exact source/manifest/table equality and local links verified; both request-context
checks pass (13 adversarial controls). Independent source/adversarial, caller and
full-map reviews are clean after correcting fixture paths, native retry/routing
and shared dispatcher authority. Architecture gate No for all four plans.
No production source, test or inventory changes are claimed by this checkpoint.

Core owner codex-uufub4 confirms the pre-issuance residual is covered by its
planned genuine command-entry custody API, `Cx::admit_http_command(Policy)`: no
numeric command argument, engine-owned exactly-once Rejected publication, and
Revoked outside/repeated/already-owned use. Reviewed source/PR is not yet pinned;
keep the exact existing pre-issuance fingerprint until that seam actually lands.

## Standalone-depth implementation audit

The full native suite also exercises standalone depth against an active book in
`tests/public.rs`; its existing isolation fixture adopts the same shared prepared
executor at its original clocks. All result, live-book, ticker and account
assertions remain unchanged. This adds that unclaimed fixture to the slice's
file list; it does not move discovery/subscription requests onto managed HTTP.
The five original-RED controls are committed separately at `af049f62be4`.
Reviewed #1808 head `9b639939e378ee3d582c06b24fd068c095eded0b` supplies shared
refusal conversion, integrated by porcelain merge under overseer authorization.
The released-expiry control exposed a shared string-failure loss of Timeout;
the error-core owner is supplying typed post-release delivery. No venue clock or
string-based reclassification is introduced to bypass that regression.

## Green native depth checkpoint

The error-core owner confirmed frozen reviewed #1816 head
`ff6a9f8255111b228817d0ae1368e297d2e28ee3`; its exact source was integrated by
porcelain merge. BookQuery now consumes `on_http_failed`'s typed cause and
declares only the existing read NotSent outcome. All five original-RED controls
pass unchanged, including the previously blocked late-response/reused-ID path.
The complete Crypto.com/Binance native run passes 408 tests across 37 suites;
scoped formatting and strict all-target Clippy pass. Both request-context checks
pass, including 13 adversarial controls. Inventory shrinks35 to31 occurrences
and31 to28 entries; every unowned map remains identical. Final independent
source/adversarial, caller and full-map reviews are clean, architecture No.
Actual landing integration, fresh CI/raw/size/timing proof and the overseer's
merge window remain separate requirements; no merge/deployment is claimed.

### Canonical command-entry follow-up plan

Actual #1808 landing `73d04380dd9` is integrated. Consume only reviewed composed
#1830 `66cb32e89588e60bd736d3ff121f5ce52bfc4f3b` (already includes reviewed
#1816/#1838); genuine ingress uses `admit_http_command(Policy)` without a numeric
argument. Both Rejected and Revoked return without adapter completion; Rejected
already published the one engine failure. Remove the numeric command parameter
from the public-depth helper and its caller, and delete the last pre-Context
`cx.fail` and obsolete Scope/CommandId imports. No source edits to shared APIs.

Before this move, separately commit original-RED controls for nonfinite command
time and the 257th owner after filling the existing 256-owner bound. Each must
emit one typed InvalidInput/Internal initial read rejection, no write outcome,
no transport/sign work or credential evidence; existing owners remain usable.
The invalid-time command ID can be reused through a later valid Context. Keep
all prior five lifecycle controls and every native depth assertion unchanged.
Then prune only the now-empty depth_query.rs inventory map, making Crypto totals
30 occurrences/27 entries (35/31 at the original plan pin). Original raw budget,
public route/no-retry and read transport/expiry behavior remain unchanged.

Canonical-entry implementation: the two separately committed controls at
`c307c1f1c01` both failed on the old pre-Context NotSent fallback. The adapter now
returns on either shared command-entry error without a numeric finalizer, and
its helper/caller carry no numeric command argument. The last depth_query.rs
manifest entry is removed; Crypto totals30 occurrences/27 entries. Independent
source/adversarial, caller and full-map reviews are clean, architecture No.
Scoped fmt, strict all-target Clippy and both ratchets (13 controls) pass; full
native suites and new-head CI remain the final proof for the dependency change.

Final canonical-entry native receipt: Crypto.com+Binance411 tests across38
suites pass, including the two initial-admission regressions and original five
lifecycle controls. No failed tests. All three follow-up reviews, strict
all-target Clippy, scoped fmt and both ratchets are clean. Future venue slices
keep seam adoption in a fresh follow-up when possible; this already-completed
slice retains the authorized canonical stack and awaits actual seam landing.
MEXC phase one does not fold in this new command-entry adoption.
