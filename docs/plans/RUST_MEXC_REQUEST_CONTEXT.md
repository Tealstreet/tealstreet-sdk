# MEXC RequestContext inventory and adoption plan

Planning checkpoint, 2026-10-23; lane codex-ih35js, overseer claude-ddi2il.
Pinned landing: `7641650e7b1fc54c55593880ac16299171f60a90`.
Read with [RUST_REQUEST_CONTEXT.md](RUST_REQUEST_CONTEXT.md), the caller/audit/quality
ledgers and the nearest venue CLAUDE.md. This checkpoint changes documentation only.

Exact inventory: **42 path/fingerprint entries, 45 occurrences** across
7 files. Open PR file lists were checked before writing.
Source/test exclusions: Account source/tests #1786; history source/tests #1793.

## Native contracts and dependency order

A single unsigned `FetchOrderBook` is an available first implementation after the
Crypto.com slice. Files are `src/lib.rs`, `src/depth_query.rs` and
`tests/port_lows.rs`, `tests/auth_results.rs` and their
`tests/support/mod.rs` prepared-action collector (plus new lifecycle tests), not claimed in the inspected
PRs. Preserve required known market and captured contractSize, limit default100
and validation1..1000, unsigned encoded native symbol, exact native version string,
sorted levels, scaled base amounts, cumulative base totals and atomic refusal.
Preserve the shared rate-limit pause and bounded retry hints; no automatic resend
or auth evidence from a public failure. Keep subscription depth state independent.
Issue read authority before validation so immediate failures also finish through
the Context path; preserve typed setup/timeout failures instead of Network text.

Private REST currently signs inline: sorted encoded GET query or exact raw POST
body, key+integer Request-Time+payload, Recv-Window clamp1..20000. Coordinate HMAC
adoption with `codex-5c57jl`; preserve primary credential trimming versus secondary
preservation, broker bytes and GET-only secondary selection. WS login is its own
key-plus-integer-stamp signature. This lane does not modify signer/core code.

Wait for #1786 before account source/tests and #1793 before history source/tests.
Snapshot orders join regular/plan/stop lists with native revisions and no
successful subset commit. Account settings and identity need read authority;
POST semantic reads must remain reads. Histories keep shared owner across native
windows, pagination and accepted-page proofs; native fee sign stays unchanged.
Private history uses primary credentials directly, independently of account GET
secondary selection. Identity validates the selected reader but keeps the
primary-key fingerprint namespace. Preserve newest-first90day-minus1ms windows,
1000-window/page bounds and amend's at-most-five200ms post-cancel rechecks.

Owner-confirmed additional fix is a separate PR after #1786: an initial plan-list
parsed native credential refusal must emit typed StartupFailed and stop retries,
matching initial MODE. Transient public/private startup retry policy remains
unchanged. Six focused existing startup/failed-plan-snapshot controls passed at
this pin; that does not claim the remaining refusal gap is fixed. Add its RED
regression separately before source changes once the files are released.

Trading requires shared plan admission for batch fallback, paced singles,
preflight+mutation and cancel/replace. Move 450ms (and applicable1500ms) gaps to
actual-release pacing; parked provider/route plus fast ACKs must not bunch sends.
Preserve native volume units, plan/stop statuses, accepted IDs, no-ID cancel effects
and unknown prior sends. Remove local outcome/pacing only after shared adoption.

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

### `venues/mexc/src/account.rs` (9 entries / 9 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `00f59a2dcd349c524ee87990b60f98bbd090dae7d0e7351b59b6f63a08c9e560` | 1 | `cx . fail ( command , "MEXC account reads are rate limited" )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `3a512e8472fbb7eb5034b94f2172f6b72ec7152336d77e97e71f1ba0e041e668` | 1 | `cx . done ( command , serde_json :: to_string ( & ResultRails { buy_leverage : rails . long , sell_leverage : rails . short , margin_type : if rails . isolated { MarginType :: Isolated } else { MarginType :: Cross } , position_mode : if cx . store . hedged ( ) { PositionMode :: Hedged } else { PositionMode :: Oneway } , } ) . unwrap ( ) , )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `4108d1559a4ad73e52be4c21b463683f0720640e6139a19d3dc124fc94a3a924` | 1 | `cx . fail ( command , "Market not found" )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `9957739c32728439a0b5b0cbabfd9b7c77479ca2e08a5c8718213e4ddbcdfc7b` | 1 | `cx . fail ( command , "Invalid MEXC leverage response" )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `9baacdfca9e0cc474c05532c9e1e6137179f1860fb215900cb3875863da3c055` | 1 | `cx . request ( method , url , headers , if method = = Method :: Get { None } else { Some ( payload ) } , pending , )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `9ea183eac0882301887d9c9e446a1252e61945a7839d2dc49235be9fa053891b` | 1 | `matches ! ( request , Request :: Mode )` | F (preserve native logic/assertion) |
| `b44d970931e9cb5f96e525d6b46444a2710921aeb72bce9c635f43c9dd7c8359` | 1 | `cx . fail ( * command , error )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `b7dc68beffbce5993cd986ca76deaba009d76529c9a08dd7cba43c5f61aa74e8` | 1 | `cx . request_streamed ( method , url , headers , ArrayAt :: EnvelopeField ( "data" ) , pending , )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `d7aab74134f949f316d653bc421cdba2bd345f74193e4dfc3af0548610e6df25` | 1 | `cx . fail ( command , "MEXC leverage requires credentials" )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |

### `venues/mexc/src/depth_query.rs` (6 entries / 7 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `0707067c9785c0028c07d0ad94a471aeb9a9315be7d6ed5ceee3f353f822fcf3` | 1 | `cx . fail ( command , error ( "Invalid MEXC market contract size" , ErrorKind :: Internal ) , )` | R (including owned validation) |
| `1c48419d2e8ec54a5b30b712ac91268756fd235c538bcacfdc3227f6383b0de8` | 1 | `cx . fail ( command , error ( "Unknown MEXC market" , ErrorKind :: InvalidInput ) , )` | R (including owned validation) |
| `234fcf69606cefc3d5816bdb500ae93f956e92c1fb5deaeace8f9fd0f4185dce` | 1 | `cx . fail ( command , error ( "MEXC order book limit must be between 1 and 1000" , ErrorKind :: InvalidInput , ) , )` | R (including owned validation) |
| `7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b` | 1 | `cx . done ( command , json )` | R (including owned validation) |
| `73bc67e960a617f9b43feeede1fd54e4edf11045a028006651973a5982f241bf` | 1 | `cx . fail ( command , error ( "MEXC depth requests are rate limited" , ErrorKind :: RateLimited , ) . retry_after_ms ( self . depth_paused_until - cx . now ) , )` | R (including owned validation) |
| `87fd4d841397b006cb8e07e6de819cc3176d93fcdcc16d2247db02b24db8fa12` | 2 | `cx . fail ( command , failure )` | R (including owned validation) |

### `venues/mexc/src/history.rs` (8 entries / 9 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `755a658b5cfaa9ec659ee94f62b5f535e94a4a91391476bda23ef1199af846bf` | 1 | `cx . done ( r . command , data )` | H + R/S (including owned validation) |
| `7d9792a0676bccdf20b899bb81c8f19bdf33402d56972457af968a5c4f3f433c` | 1 | `cx . request_streamed ( Method :: Get , url , headers , at , Pending :: History ( request ) )` | H + R/S (including owned validation) |
| `8a87f4b913bf1811f1e4e95b8a7e773e34308f6e57e0140d64dc57fe9eb068b1` | 1 | `cx . fail ( command , "Invalid MEXC query time range" )` | H + R/S (including owned validation) |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | H + R/S (including owned validation) |
| `d577b299e0f06807b67ea1998b2b5ef6249fdd97a3dca81b338c1b40978d02a5` | 1 | `cx . fail ( command , "MEXC client-order-ID lookup is unsupported" )` | H + R/S (including owned validation) |
| `d5e898e07cc7c2b9f7927c0f9a7f462214e004fdec265ff4f3ed0370a6db1a81` | 2 | `cx . fail ( r . command , error )` | H + R/S (including owned validation) |
| `def51f5007658efde0e27d55b87c86b7dadd77774069426672bc9fa5c009a8fb` | 1 | `cx . fail ( command , "MEXC income supports only kind FUNDING_FEE" )` | H + R/S (including owned validation) |
| `ff15a37413ebcc7d0dac770897169efe8e124ad41528bbde14c92df1ca7ad468` | 1 | `cx . fail ( command , "MEXC query range exceeds 1000 windows" )` | H + R/S (including owned validation) |

### `venues/mexc/src/identity.rs` (2 entries / 2 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | `cx . done ( command , data )` | R/S |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | R/S |

### `venues/mexc/src/lib.rs` (8 entries / 9 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `2cb4c59f38cca507a15b20d1ace303afd27defc1b4fe0aba50dddcbcb1f9c37c` | 1 | `cx . request_streamed ( Method :: Get , format ! ( "{REST}{path}" ) , vec ! [ ] , ArrayAt :: EnvelopeField ( "data" ) , pending , )` | R/B/S/W by native pending; declare semantics per call |
| `3ac4b4a98d56f286f0da2c32ff7d0dbd744a053d6ff751354f2d7d92f34a5f6f` | 1 | `cx . fail ( command , "Unknown MEXC market" )` | R or W; typed Context outcome; native failure wrapper adopted/retired |
| `83af50ee01cdc7c77127d8e096184855c4f146642a857f76c4b5b7d6a0ef77e2` | 1 | `cx . fail ( command , interval . unsupported ( "MEXC" ) )` | R or W; typed Context outcome; native failure wrapper adopted/retired |
| `a1d7eb3cdf7f4e938351efa132ee68dac79fc9e6e26db557c0ffd67282e9ae14` | 1 | `cx . request ( Method :: Get , format ! ( "{REST}{path}" ) , vec ! [ ] , None , pending )` | R/B/S/W by native pending; declare semantics per call |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | `cx . done ( command , data )` | R or W; typed Context outcome; native failure wrapper adopted/retired |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | `cx . fail ( command , error )` | R or W; typed Context outcome; native failure wrapper adopted/retired |
| `dd40ba0518f46ed21ce7a11fae5c0e8e1d5877f4466529272ed959bc0aeb843c` | 1 | `cx . fail ( command , VenueError :: new ( error ) . kind ( ErrorKind :: Network ) . outcome ( Outcome :: NotSent ) , )` | R or W; typed Context outcome; native failure wrapper adopted/retired |
| `e7ec3b5f8c170e9a5887721f2eedda084ecf9f5a5eba8d1e899957c63019e0bb` | 1 | `matches ! ( request . call , trading :: Call :: AmendRead { . . } )` | F (preserve native logic/assertion) |

### `venues/mexc/src/secondary_tests.rs` (2 entries / 2 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `01822703e05fca2ea5984588dabf8d7c3aa8138e93501ae6ead58982e25492e3` | 1 | `assert ! ( request . headers . iter ( ) . any ( \| ( name , value ) \| name . eq_ignore_ascii_case ( "ApiKey" ) & & value = = expected ) , "{:?}" , request )` | F (preserve native logic/assertion) |
| `f758cbc2754c2b39d2f81d56901f6920f66dcc953a7fe7faa9b5c8f2a5b46c90` | 1 | `assert_eq ! ( request . headers , expected_headers )` | F (preserve native logic/assertion) |

### `venues/mexc/src/trading.rs` (7 entries / 7 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` | 1 | `cx . fail ( job . command , error )` | W + S; read preflight R; commandless repair X |
| `4310c56cdb4207e078977e5ecfb03800099cc6b8559b1e4cd4c0ef98cc4de727` | 1 | `cx . done ( job . command , data )` | W + S; read preflight R; commandless repair X |
| `5f4315dcd81d8ae6ab0638ddd9864e3fc4326a5911e4df86004080095feaaa15` | 1 | `cx . fail ( command , not_sent ( "MEXC trading needs an API key" ) . kind ( ErrorKind :: AuthInvalid ) , )` | W + S; read preflight R; commandless repair X |
| `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` | 1 | `cx . fail ( command , not_sent ( error ) )` | W + S; read preflight R; commandless repair X |
| `bb2384c4fe0775f69956644df4cda9966a8f10c980910e36d89210625de5771c` | 1 | `matches ! ( & request . call , Call :: AmendRead { . . } )` | F (preserve native logic/assertion) |
| `c090aa594c62e2e9e6909a7328a9019893aeb82f169ff44b63e01cd3132bc052` | 1 | `cx . fail ( command , not_sent ( error . as_str ( ) ) )` | W + S; read preflight R; commandless repair X |
| `c1caaf5a8609b4dad0bef6ecd21d19b958817f15852a993214e7669494790435` | 1 | `cx . fail ( command , error . partial ( & job . ids , job . total_orders , failed_index ) , )` | W + S; read preflight R; commandless repair X |

## Planning checkpoint verification

Exact source/manifest/table equality and local links verified; both request-context
checks pass (13 adversarial controls). Independent source/adversarial, caller and
full-map reviews are clean after correcting fixture paths, native retry/routing
and shared dispatcher authority. Architecture gate No for all four plans.
No production source, test or inventory changes are claimed by this checkpoint.

## Next unclaimed slice: single public candle read

Implementation branch starts from landing `fa5d5fb90c4`. Only `src/lib.rs`,
the candle fixture in `tests/public.rs`, a new `tests/request_context_candles.rs`,
the exact inventory and append-only docs are needed. Current open source/test
claims remain account #1786 and history #1793; none of these files is edited.
Recheck file lists before implementation. This slice does not change signing,
headers, startup/discovery, subscription repair or the deferred startup-auth fix.

Retain one issued command Read Context in `Pending::Candles`, credential=None,
RetryPolicy::Never and original20000ms bound. Issue it before native interval/
market validation so those failures also retain Context authority. The single
unsigned ordinary GET moves directly to request_managed, avoiding a blanket
change to the common legacy `get` helper that serves other consumers. Complete
through done_context/fail_context; returned queue refusals keep the same owner.
Only an issuance failure before any Context exists may use one exact existing
`cx.fail(command,error)` fingerprint. Its eventual-zero command-entry custody
seam is owned by codex-uufub4; no new fingerprint/exemption or post-issuance fallback.

Preserve native-symbol path, interval spelling, floor-to-seconds from/to bounds,
missing limit retaining every row and explicit tail trimming (including zero),
equal-length column validation, native row order/OHLCV and seconds timestamps.
There is no automatic candle retry. Keep generic HTTP status handling, HTTP429
depth/account pause side effects and native envelope errors in their current
response path; moving the decoder ahead of them would change behavior. Current
response admission must fence those effects too. Public failures supply no
account-auth evidence. Retain native no-I/O unsupported interval/unknown-market
messages; shared typed route/transport/expiry errors retain their kinds and the
explicit no-mutation read outcome, with no text/deadline inference.

The existing1000-row candle fixture uses the shared prepared-actions executor at
its original0ms command time; keep its payload/result assertions and the shared
test support untouched. New original-RED tests cover prepare/bind/final claim,
exact query/native output, explicit-limit tail behavior, malformed column failure,
typed setup and released/parked expiry, late/duplicate callbacks and command ID
reuse. Include HTTP429 pause/auth isolation and immediate validation/no-HTTP.
Commit these failing tests separately before changing the adapter.

The exact prune removes unsupported-interval, unknown-market and candle-success
fingerprints, and reduces the generic lib.rs failure count2 to1 for pre-issuance
failure:45 to41 occurrences and42 to39 entries. Common HTTP helper sites remain
inventoried because their other callers remain legacy. Every other path map is
unchanged; do not hide or rename the common helpers to claim removal.

Reviewed #1808 head `9b639939e378ee3d582c06b24fd068c095eded0b` supplies error
conversion and reviewed #1816 head `ff6a9f8255111b228817d0ae1368e297d2e28ee3`
supplies typed post-release delivery. Stack exactly on these authorized pins and
re-integrate actual landing before merge. Gates: full MEXC+Binance native suites,
scoped fmt/strict all-target Clippy, both ratchets and three independent reviews;
CI owns actual-WASM/replay/size/timing proof. CALLERS records prepared-transport
requirements, original-budget expiry/late suppression and typed causes without
changing candle payloads or native retry/status/pause policy. Architecture No:
the venue supplies native semantics and consumes shared lifecycle/error custody.

### Candle implementation checkpoint

Plan commit `1eac68ce71f` precedes original regression commit `e3545f4edd9`:
seven lifecycle controls fail against the legacy adapter; immediate native
validation remains green. Caller review added transport/native-error controls
and a public fixture proving account pause precedence without account source or
shared-support changes. The adapter consumes shared managed HTTP and typed
callbacks; no engine, signer, timer or classification implementation is added.
The precise manifest delta is confined to lib.rs, 9 to5 occurrences /8 to5
entries, giving venue totals41 occurrences/39 entries. The one retained numeric
failure occurs only when root Context issuance fails. Eventual zero still needs
the genuine shared command-entry custody seam; other native paths stay listed.

Native checkpoint: full MEXC+Binance suites pass,443 tests across39 suites,
including9 candle lifecycle controls; scoped fmt and strict all-target Clippy
pass. Both request-context checks pass with13 adversarial controls. Independent
source/adversarial and full-map reviews are clean (architecture No); caller
review's additional controls are included. CI supplies the remaining WASM/replay/
size/timing gates after publication; no merge permission is implied.

## Genuine candle command-entry follow-up

Phase one #1834 landed at `bc91dc15d705f7d388ab3df46fa8c3ffb9395822`.
This follow-up consumes actual #1830 `0001c78eeb2` through
`Cx::admit_http_command(Policy)` inside the genuine FetchCandles callback,
before validation or I/O. Keep the same unsigned Read policy, original20s
budget and Never retry. Both Rejected and Revoked return without an adapter
terminal: the engine owns initial rejection publication and grants no custody
on Revoked. Delete Scope and the last candle pre-Context numeric failure.

Before the adapter move, separately commit two original-source RED controls:
nonfinite command time emits one InvalidInput initial rejection with no write
outcome/no I/O/auth evidence, then a valid reused ID succeeds; the 257th root
emits one Internal rejection without stealing any of the existing256 owners,
and the first parked read still prepares, claims and completes. Preserve all
nine existing lifecycle controls and native fixtures unchanged.

Prune only the now-removed generic lib.rs numeric-failure fingerprint, reducing
MEXC41 to40 occurrences and39 to38 entries. All other venue/path maps remain
exact. Append the initial read error contract to CALLERS and audit/quality
ledgers. Account #1786 and history #1793 remain excluded. Full MEXC+Binance
native tests, scoped fmt/strict Clippy, both ratchets and independent source,
caller and full-map reviews precede readiness. Architecture No: this adapter
consumes shared entry ownership rather than duplicating it. Actual #1820
integration and fresh CI will be required after the current merge freeze.


### Genuine-entry verification checkpoint

Plan `6cb0759fd81` precedes separate original-source RED tests `3f6cb5416ac`;
both tests fail on the old pre-Context NotSent outcome and pass after genuine
entry adoption. All11 candle lifecycle/admission controls pass. Full MEXC+Binance
native suites pass 446 tests across40 targets; scoped fmt, strict all-target
Clippy and both ratchets (13 controls) pass. Exact inventory is40 occurrences /
38 entries; all other maps and the nine earlier lifecycle controls are unchanged.
Independent source/adversarial, caller and full-map reviews are clean, architecture
No. This checkpoint is on actual #1830 and phase-one landingbc91; actual #1820
integration and fresh CI remain required before the overseer's merge window.

## Initial plan-list credential refusal

Re-verified after #1786 on landing `e37a80b8c4445e2de45abec7fa3f494ee0561888`,
with actual #1854 engine `e2b03d5592a`. Initial MODE already terminates startup
for the existing parsed native credential refusals. The initial Plan snapshot
still falls through to generic retry after MODE succeeds. This separate fix
extends that same refusal path only to Plan reads before the first complete
orders snapshot. Successful and later account refresh behavior stays native.

Use the existing auth_rejection helper and typed StartupFailed publication,
including its native code/message and primary-invalid-key deactivation policy.
Keep terminal account state from resuming HTTP through a later poll or private
login. Do not change markets/tickers startup, transient private failures, bare
HTTP401/403 responses, ordinary rate limiting or unrelated snapshot scopes.
There is no signer, RequestContext or error-table migration in this slice.

Before production edits, separately commit original-source RED controls for
initial Plan AuthInvalid/AuthExpired/AuthPermission refusals, exactly one typed
startup terminal, no successful orders initialization and no resumed account
HTTP after sibling callbacks or private login. Keep negative controls for
transient initial Plan failures and post-startup/scoped refusals. Then run full
MEXC+Binance native tests, scoped fmt/strict Clippy, both ratchets and independent
source/adversarial, caller and full-map reviews. Inventory remains unchanged.
The account implementation, its account tests, narrow lib.rs stream-error
routing and append-only ledgers are needed; history #1793 stays excluded.
Complete error envelopes without a data list use the same refusal check through
on_stream_error; a successful envelope without its expected list remains a
retryable malformed snapshot. Terminal state discards late snapshot pages and
blocks both existing and newly requested asset refreshes without rearming polls. Architecture No: reuse existing native refusal facts and
engine startup authority rather than adding another policy implementation.

### Initial Plan refusal verification

Plan d015560d56a precedes separate original-source RED tests d759c610dda
(two failures, two negative controls passing). The implemented fix passes all
44 account tests and full MEXC+Binance native suites:456 tests across40 targets.
Scoped formatting, strict all-target Clippy and both integrated request-context
checks pass, including14 scanner controls. Independent source/adversarial, caller
and full-map reviews are clean, architecture No. Actual #1786 and #1854 are
integrated, and all incoming Crypto.com #1818 work is preserved. Inventory is
identical to incoming; only native initial-Plan refusal behavior changes.

## Standalone public book command slice

Base landing367601fdf7a30bed7aa5442546466fd1337fb781, newer than the requested
cd3dc6a13fe. The overseer authorizes this branch to stack the reviewed green
#1872 headaa40d4ce87ea70b0cc8ec64e4d7e2127bfc21968 despite its own lib.rs claim;
porcelain merge6bc38aa2345 includes it. Actual #1872 landing must be integrated
before final proof. Shared docs retain incoming first and each appendix once.
HMAC owner claude-4yjlw0 confirms no MEXC/Phemex/WOO source reservation or reviewed
private recipe; private adoption waits for the shared engine read helper after
#1817. This unsigned public read needs no signer or credential plumbing.

Current MEXC inventory is37 occurrences /36 entries after actual #1793 history
landing. This slice removes8 occurrences /7 entries, leaving29 occurrences /
29 entries. Every other venue/path map stays incoming-exact. Exact removals:

| Path | Fingerprint | Count | Shared destination |
| --- | --- | --- | --- |
| venues/mexc/src/depth_query.rs | 0707067c9785c0028c07d0ad94a471aeb9a9315be7d6ed5ceee3f353f822fcf3 | 1 | Issued Context preflight refusal |
| venues/mexc/src/depth_query.rs | 1c48419d2e8ec54a5b30b712ac91268756fd235c538bcacfdc3227f6383b0de8 | 1 | Issued Context preflight refusal |
| venues/mexc/src/depth_query.rs | 234fcf69606cefc3d5816bdb500ae93f956e92c1fb5deaeace8f9fd0f4185dce | 1 | Issued Context preflight refusal |
| venues/mexc/src/depth_query.rs | 7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b | 1 | Context success terminal |
| venues/mexc/src/depth_query.rs | 73bc67e960a617f9b43feeede1fd54e4edf11045a028006651973a5982f241bf | 1 | Issued Context preflight refusal |
| venues/mexc/src/depth_query.rs | 87fd4d841397b006cb8e07e6de819cc3176d93fcdcc16d2247db02b24db8fa12 | 2 | Context native/decode failure terminal |
| venues/mexc/src/lib.rs | dd40ba0518f46ed21ce7a11fae5c0e8e1d5877f4466529272ed959bc0aeb843c | 1 | Typed Context HTTP failure terminal |

Genuine admit_http_command issues Read/Public20s/Never before preflight; the
public adapter accepts no numeric owner argument. Rejected is already terminal,
Revoked grants nothing, and neither falls back to numeric completion. Store the
Context with the captured contract size and limit in Pending::BookQuery. Use one
unsigned request_managed GET to the native encoded contract depth URL. Preserve
limit1..1000/default100, market/scale validation, sorting, base amounts/totals,
exact u64 version string, zero quantities and native snapshot error metadata.
The original status/code510/Retry-After codec is reused, not expanded or replaced
by another status ladder. Preserve depth+account pause only for live responses;
no retry, live-book/account commit or credential evidence is introduced.

Typed setup/network/released-deadline errors complete the Context with the native
public-read NotSent declaration. Initial engine refusal has no write outcome.
Parked/released expiry, old preparation, duplicate reply and ID reuse must not
publish stale success or apply a stale429 pause. Native error/decode controls
remain independent of Context liveness. No signer, shared executor, new timer,
classification policy, numeric lookup or exemption is added.

Commit original-source REDs before production changes: preparation/release,
parked and released expiry, typed timeout, stale callbacks/ID reuse, genuine
initial rejection and late429 non-interference. Preserve native book regression
payloads/assertions in port_lows/auth_results/candle cross-controls, adapting
only prepared transport and forward response clocks. The old no-timer assertion
becomes an explicit single engine20s deadline with no retry timer or socket send.
Then change depth_query.rs/lib.rs, prune exactly the table above, append CALLERS/
AUDITS/QUALITY and obtain source/adversarial, caller and full-map reviews.
Run full MEXC+Binance suites, scoped fmt/strict Clippy and both ratchets; CI and
an overseer window are required. Architecture No: native codec adopts existing
shared command custody, managed lifecycle and typed completion. History #1793
is actual and its remaining paths stay untouched; account #1872 is stack-only.

### Standalone book verification checkpoint

All7 original RED controls pass with the managed adapter. Full MEXC+Binance
suites pass472 tests across41 targets; scoped fmt/strict all-target Clippy pass.
Independent reviews preserve native totals-before-truncation and restore the
original no-timer refusal assertions alongside managed-dispatch exclusions.
Only the queued request expects the engine deadline. Public/native fixtures
retain result/auth checks and use forward callback clocks. The exact scanner
shows29 occurrences /29 entries, an8-occurrence/7-entry reduction. Incoming
landing integration and both ratchets precede CI/readiness; no merge window is
claimed. Actual #1872 integration remains required before final merge proof.

Frozen actual landing9aef6bca90b is genuinely integrated atb5d484c45ff. Runtime,
MEXC/Binance tests and dependency trees are byte-identical to the472-test/strict/
fmt checkpoint; both integrated ratchets pass (14 controls), and every incoming
map is preserved except the exact8-occurrence/7-entry book prune. Three
independent reviews are clean, architecture No. The reviewed #1872 source stays
exact apart from the explicitly authorized BookQuery integration; its actual
landing and the overseer's window remain required before merge.

### Managed book integration after startup landing

Actual startup #1872 e233cb05dfb63d7575382bed823c7538a5826b98 and subsequent
Phemex #1878 7b6b4a2d029052c08fc3801a763a613b6a7ab9d4 are genuinely integrated.
The startup dependency is resolved: only the eleven managed-book paths differ
from incoming, with account.rs and its account tests incoming-exact. The native
book source, fixtures and dependency trees retain the reviewed green behavior.
Incoming inventory stays intact except the proven eight MEXC occurrences/seven
entries; source-equivalent integration review and fresh-head CI are recorded in
the PR. No merge without an overseer window.
