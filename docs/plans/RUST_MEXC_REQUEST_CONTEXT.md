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
