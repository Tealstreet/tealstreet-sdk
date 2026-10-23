# WOO RequestContext inventory and adoption plan

Planning checkpoint, 2026-10-23; lane codex-ih35js, overseer claude-ddi2il.
Pinned landing: `7641650e7b1fc54c55593880ac16299171f60a90`.
Read with [RUST_REQUEST_CONTEXT.md](RUST_REQUEST_CONTEXT.md), the caller/audit/quality
ledgers and the nearest venue CLAUDE.md. This checkpoint changes documentation only.

Exact inventory: **43 path/fingerprint entries, 48 occurrences** across
6 files. Open PR file lists were checked before writing.
Source/test exclusions: Account/trading source and account tests #1788; history source/tests #1796.

## Native contracts and dependency order

Single unsigned candles in `src/lib.rs` are a possible later slice: require seeded
market, preserve finite ordered range, limit clamp1..1000, recent/history URL and
array-path selection, seconds output, final timestamp sort and whole-page failure.
There is no automatic candle retry. Historical candles explicitly route unsigned
HTTP to Primary egress at api-pub.woox.io. Preserve that routing without fabricating
a credential/signer; request a shared managed routing seam if existing APIs cannot
express it. Do not silently route historical reads through ordinary public egress.

Discovery uses shared startup Backoff; funding is a separate60s poll. Subscription
depth snapshots are background reads with generation/buffer/reconnect/subscription
ownership and a1s repair delay; preserve those fences and do not treat them as a
standalone command or create a venue retry executor. Fixture drivers must use the
existing prepared-action executor where their requests become managed.

Inline signing selects secondary for GET only; preserve v1 sorted form query/body
plus '|timestamp', v3 timestamp+method+path+body and ordered query, and separate
WS auth. Coordinate HMAC host-signer adoption with `codex-5c57jl`; no `sign_http`
site does not prove migration. Account info/positions coalescing and regular/algo
OrdersJoin require shared plan and multi-owner completion support. Account/trading
source and account tests wait for #1788; history source/tests wait for #1796.

Trade FIFO has public touch-read/build/write/readback and regular/algo cancellation
families. Shared plan/journal must preserve partial/native identity and unknown
outcomes; accepted leverage with exhausted confirmation remains Unknown with no
setting patch or write replay. Keep two250ms confirmation retries via shared
wakeup, not a copied timer. Move ordinary100ms, PUT200ms and algo500ms gaps to
actual transport release, including parked signer/route plus fast-ACK controls.

Histories preserve native fill spelling candidates then legacy fallback,30day
windows, strict page metadata/no-progress/1000page proofs and income budgets.
Current strict financial failure cannot silently become successful accepted-row
partial recovery. Ledger6s queue-time pacing needs shared release ownership.
Shared history owner adoption alone does not make HTTP managed. Joined/coalesced
owners and all multi-request plans remain last until their shared APIs exist.

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

### `venues/woo/src/account.rs` (5 entries / 5 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `84cbbb2145bd157642ac2382cdc442278adcbc91dac618d336f080d5622f4a87` | 1 | `cx . fail ( id , e )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `9454a63760e008e5178db4d51fb15fa87e2d28536fe01084bdde4ccb6a6cf896` | 1 | `cx . fail ( id , "Market not found" )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | `cx . done ( command , data )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `ea0ff1d0bf1689d0410b0bcc1cbbeedc2fa1bec60349440b50760648369edf0e` | 1 | `cx . fail ( id , error )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |

### `venues/woo/src/history.rs` (14 entries / 16 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `1f8872c0591ddabf50451cb7d82e62f559adeb94b58bc3d31831b9b3352b06e7` | 1 | `cx . fail ( r . command , "WOO income range exceeded the complete-history budget" , )` | H + R/S (including owned validation) |
| `2f8b28271a09611b520d4d18caa1a05d050a391dd3631c8e90abb7acc1c4dab1` | 1 | `cx . fail ( r . command , "WOO fills range could not be completed" )` | H + R/S (including owned validation) |
| `40efb5beefbf20c77c9db60432359d349d6f6d6bd9afe604d4db959a08330621` | 1 | `cx . fail ( r . command , "WOO history page made no progress" )` | H + R/S (including owned validation) |
| `4bba92ad9c2f3e562fd37ab07235ee0ca41b7fd4902d0e07de5b932da29d510e` | 1 | `cx . fail ( command , "Invalid WOO history range" )` | H + R/S (including owned validation) |
| `718a235d1dc280b57573b9d05295db77d44b30c4871dafc2f7cc6998ab8c915a` | 1 | `cx . fail ( r . command , "Malformed WOO history page" )` | H + R/S (including owned validation) |
| `755a658b5cfaa9ec659ee94f62b5f535e94a4a91391476bda23ef1199af846bf` | 1 | `cx . done ( r . command , data )` | H + R/S (including owned validation) |
| `aabc8213282a8979a24e0eecad23e21d2ec8e5bf00c6e0226e42b48cd609cb63` | 1 | `cx . fail ( command , "Invalid WOO history symbol, range or limit" )` | H + R/S (including owned validation) |
| `bc7ca0a28973fd7884a57c6493cce102261bbd13ab60a87bfd8b58e5ab51aaaf` | 1 | `cx . fail ( r . command , "Invalid WOO history page metadata" )` | H + R/S (including owned validation) |
| `c7840a8b417295613f5ba4cb19cd3ee6243e6810e609f4408e116d2f401f8668` | 1 | `cx . request_streamed ( Method :: Get , format ! ( "{}{path}?{query}" , self . rest ) , vec ! [ ] , at , Pending :: History ( r ) , )` | H + R/S (including owned validation) |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | H + R/S (including owned validation) |
| `caf176b01da3be3156abf4e4d79bcd63c252f0b4ff5acfdb4d325e61e71accf9` | 1 | `cx . fail ( r . command , "WOO history pagination limit exceeded" )` | H + R/S (including owned validation) |
| `d5e898e07cc7c2b9f7927c0f9a7f462214e004fdec265ff4f3ed0370a6db1a81` | 3 | `cx . fail ( r . command , error )` | H + R/S (including owned validation) |
| `db15e9f8a226656cfdc25258e389cf082535f04165d5e96ecf9cbb0b3f16368b` | 1 | `cx . fail ( r . command , e )` | H + R/S (including owned validation) |
| `e10f2a544139be7b8a9e10a239fce00e30d8b13669f3f4b176743ab1bf2a34c5` | 1 | `cx . fail ( command , "WOO remote client order lookup is unsupported" )` | H + R/S (including owned validation) |

### `venues/woo/src/identity.rs` (2 entries / 2 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | `cx . done ( command , data )` | R/S |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | R/S |

### `venues/woo/src/lib.rs` (12 entries / 13 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `0b9ec85fbc5589f3a0cac32b1feb488b09fa231f7aeba936765bf8c0572c0989` | 1 | `cx . request ( method , url , request . headers , request . body , pending )` | R/B/S/W by native pending; declare semantics per call |
| `0c18723dbaff5bc0acce7eaa0f73db7969c3babab0a1dcdb4c9116b765b4c170` | 1 | `cx . request ( Method :: Get , format ! ( "{}/v1/public/orderbook/{}" , self . rest , native ( m ) ) , vec ! [ ] , None , Pending :: Depth { symbol : symbol . into ( ) , generation : state . generation , } , )` | R/B/S/W by native pending; declare semantics per call |
| `2069ebaec13f93bdfde11fbc0fd13f9a0e06b0f902849e4a0d8eed05f878f2a4` | 1 | `cx . fail ( command , "Invalid WOO candle range or interval" )` | R or W; typed Context outcome; native failure wrapper adopted/retired |
| `2faad4fac4a380c49b5e6910eac8c7c3a333d312dba356a73ada3326d6a170b8` | 1 | `cx . request_streamed ( Method :: Get , url , vec ! [ ] , at , Pending :: Candles { command , rows : Vec :: new ( ) , bad : false , } , )` | R/B/S/W by native pending; declare semantics per call |
| `40fb7a9fd1d4a1b5965aa2ced55e6dfe125cc4d1886acbc07f881e175d813afb` | 1 | `cx . request ( method , format ! ( "{}{}" , self . rest , path ) , vec ! [ ] , None , pending , )` | R/B/S/W by native pending; declare semantics per call |
| `4108d1559a4ad73e52be4c21b463683f0720640e6139a19d3dc124fc94a3a924` | 1 | `cx . fail ( command , "Market not found" )` | R or W; typed Context outcome; native failure wrapper adopted/retired |
| `4b3e70a4ab5b77064618fe31859b216ca144332a0bc6f1054a6e2b358862b00d` | 1 | `cx . request_streamed ( Method :: Get , format ! ( "{}{path}" , self . rest ) , vec ! [ ] , ArrayAt :: Field ( "rows" ) , pending , )` | R/B/S/W by native pending; declare semantics per call |
| `9d866d03338adc49f7203b2f42ff7d5a48a59f8dbbf42caf1bf758e564aca0d3` | 1 | `sign :: request ( key , secret , cx . now , method , path , & params , body )` | S (production inline signer; adopt host recipe) |
| `acfb59dac3369d897be3ddc5508ce82b08a0594f9530a7f9a146f73bac26aefd` | 1 | `cx . request_streamed ( Method :: Get , format ! ( "{}/v1/public/funding_rates" , self . rest ) , vec ! [ ] , ArrayAt :: Field ( "rows" ) , Pending :: Funding { rows : Vec :: new ( ) } , )` | R/B/S/W by native pending; declare semantics per call |
| `b873402e4d755030f8a06f7993712806fec0043cbd91cef3dac65fd83b3a1573` | 1 | `format ! ( "{}{}" , self . rest , request . path )` | F (preserve native logic/assertion) |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | `cx . done ( command , data )` | R or W; typed Context outcome; native failure wrapper adopted/retired |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | `cx . fail ( command , error )` | R or W; typed Context outcome; native failure wrapper adopted/retired |

### `venues/woo/src/secondary_tests.rs` (3 entries / 3 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `7b0edc17e5ac2dddeedfd7026b7c19de39bc700dcbe5c7739a280d5f6bf3f636` | 1 | `sign :: request ( expected , secret , 1 7 0 0 0 0 0 0 0 0 0 0 0 . 0 , request . method , "/v1/secondary-test" , & [ ] , None , )` | F (preserve independent signer oracle) |
| `a84ee060601aa067113ce080e8dd5708fbcbc075d13919975086093a8948e7ba` | 1 | `assert ! ( request . headers . iter ( ) . any ( \| ( name , value ) \| name . eq_ignore_ascii_case ( "x-api-key" ) & & value = = expected ) , "{:?}" , request )` | F (preserve native logic/assertion) |
| `e6f6ad3d3db6bf3dd992108083e8f38927000f8342199b32390375f0d0868f4b` | 1 | `assert_eq ! ( request . headers , signed . headers )` | F (preserve native logic/assertion) |

### `venues/woo/src/trading.rs` (7 entries / 9 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` | 2 | `cx . fail ( job . command , error )` | W + S; read preflight R; commandless repair X |
| `1808c6730705b3d4a97b1d043697c38c0e4e2221733f0cb80dbd7e955603d8f6` | 1 | `cx . request ( Method :: Get , format ! ( "{}/v1/public/orderbook/{}" , self . rest , native ( m ) ) , vec ! [ ] , None , Pending :: Trade ( Request { command , step } ) , )` | W + S; read preflight R; commandless repair X |
| `3acd8a170840b44b40182db4042138229bf16307af5ca8e03cbde593c57da688` | 1 | `cx . fail ( command , not_sent ( "WOO trading requires credentials" ) . kind ( ErrorKind :: AuthInvalid ) , )` | W + S; read preflight R; commandless repair X |
| `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` | 1 | `cx . fail ( command , not_sent ( error ) )` | W + S; read preflight R; commandless repair X |
| `c52807ee63a9bb738f999e17d032e614d141aaaee089518137ae225d5b9d74b9` | 1 | `cx . done ( job . command , json ( & job . ids ) . unwrap ( ) )` | W + S; read preflight R; commandless repair X |
| `c5fae1ffb504c6d10a80b2f0b165afeefb2883628d719ac3e38843c8dfd9a997` | 1 | `matches ! ( request . step , Step :: Write { . . } )` | F (preserve native logic/assertion) |
| `e2320c299f38679fd3276c424119a5c7c40dd69406aa809dc2a39e66fa2d4c01` | 2 | `matches ! ( request . step , Step :: Mode { . . } \| Step :: Leverage { . . } )` | F (preserve native logic/assertion) |

## Planning checkpoint verification

Exact source/manifest/table equality and local links verified; both request-context
checks pass (13 adversarial controls). Independent source/adversarial, caller and
full-map reviews are clean after correcting fixture paths, native retry/routing
and shared dispatcher authority. Architecture gate No for all four plans.
No production source, test or inventory changes are claimed by this checkpoint.
