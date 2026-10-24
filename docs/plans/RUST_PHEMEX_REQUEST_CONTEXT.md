# Phemex RequestContext inventory and adoption plan

Planning checkpoint, 2026-10-23; lane codex-ih35js, overseer claude-ddi2il.
Pinned landing: `7641650e7b1fc54c55593880ac16299171f60a90`.
Read with [RUST_REQUEST_CONTEXT.md](RUST_REQUEST_CONTEXT.md), the caller/audit/quality
ledgers and the nearest venue CLAUDE.md. This checkpoint changes documentation only.

Exact inventory: **47 path/fingerprint entries, 51 occurrences** across
6 files. Open PR file lists were checked before writing.
Source/test exclusions: Account source/tests #1786; history source/tests #1795.

## Native contracts and dependency order

The initial candidate is one candle command, but the common `lib.rs::request`
currently signs even public products/tickers/fullbook/candles when credentials
exist. Preserve this optional authentication and routing, rather than silently
making a credentialed public read anonymous. GET normally selects secondary;
active-order recovery and client-ID lookup force primary egress. Signed migration
must first coordinate with the HMAC signer lane `codex-5c57jl` and the shared
credential-selection owner. Preserve percent-decoded query (literal plus remains
plus), path/expiry/body canonical bytes, rounded protocol expiry and separate
WS key-plus-expiry auth. No `sign_http` token does not mean host signing.

Account reads join USDT and USDC with per-currency retention, freshness and
settings provenance; activeList recovery is independent native order proof.
Wait for #1786 row reporting before `account.rs`/`tests/account.rs`. Non-income
history waits for #1795 `history.rs`/`tests/history.rs`. Existing income ownership
still needs Context authority and signed managed page requests; owner adoption
alone does not migrate HTTP. Preserve bounded ranges/offsets, native-unit mapping,
strict malformed/oversize history refusal and identity from USDT userID.
Income has accepted-page recovery on a later failed page: retain that contract
with its incomplete marker, unlike strict non-income snapshot rejection. Preserve
seven-day splitting, USDT/USDC stages and duplicate-occurrence row identities.

Trading roots/child legs and partial IDs move to the shared journal. Preserve
one-symbol mode changes, native posSide, strict cancel/replace fill fences and
per-symbol repair. Actual `signed_until` must consume shared protocol-expiry to
raw-release projection, never `Credentials::expiry(cx.now)`; parked signing,
positive/negative calibration and raw-deadline precedence are mandatory controls.
The venue also issues commandless `cancel_late -> late_next -> dispatch(None,...)`
mutations. They require explicit shared background-mutation owner authority;
neither command W nor background-read B represents them. Ask the shared owner
before conversion; never invent a numeric command or call a cancel a read.

Unsigned public-only candles may be a small subsequent slice only when the
credentialed path and fixture behavior remain explicit. No generic helper switch
that breaks all startup/secondary drivers. Follow with identity and simple signed
read/write paths once the signer APIs land; paired account reads, multi-currency
history, preflights and late repair remain gated on shared plan/owner APIs.

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

### `venues/phemex/src/account.rs` (3 entries / 3 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `35203e2192556d58fa9706bd22bddc7b92206b18106905b5af0b39aefd365d1c` | 1 | `self . request ( cx , Method :: Get , "/g-accounts/accountPositions" , & format ! ( "currency={name}" ) , None , Pending :: Account ( request ) , Some ( ArrayAt :: Path ( & [ "data" , "positions" ] ) ) , )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `db7b6dc88f90361d5d57c041003af076c94d051c673d00cec046198eb9441f98` | 1 | `self . request ( cx , Method :: Get , "/g-orders/activeList" , & query , None , Pending :: Orders ( OrdersRead { symbol , generation } ) , None , )` | B/S, R/S, H/S or W/S by native pending; joined reads require shared plan/owner |
| `ee694d9fd31f8bcf8dd1202488b242081189214dd143f90fc6bb10d6fd6fe886` | 1 | `format ! ( "Phemex {} account read: {error}" , CURRENCIES [ request . currency ] )` | F (preserve native logic/assertion) |

### `venues/phemex/src/history.rs` (14 entries / 17 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `007e1c20fd9ad3fe2cb38a2646b5f0910bcced3498841d2ec9362b0da3b4e4f8` | 1 | `self . request ( cx , Method :: Get , path , & query , None , Pending :: History ( request ) , Some ( at ) , )` | H + R/S (including owned validation) |
| `05967623e28d0acd558da8192dba9a5d1f571a4e57337bd4a354d59c3a6bbdcc` | 1 | `cx . done ( command , r#"{"found":false,"definitive":false}"# . into ( ) )` | H + R/S (including owned validation) |
| `139c5990615d19868461cf9c3fc7c97d895017fced401b219198883e89eeefd1` | 3 | `matches ! ( request . rows , Rows :: Trades ( _ ) )` | F (preserve native logic/assertion) |
| `194dbf421bcb32511aab47afac9c97a26d5ab68ce17ed2741f3a7b5eef5094e1` | 1 | `cx . fail ( request . command , "Malformed or oversized Phemex history response" , )` | H + R/S (including owned validation) |
| `1ca3b53997ab5d41fcef35db137f612b4836198fb1116731d22d513e4cb34e0f` | 1 | `cx . fail ( command , "Too many active Phemex history queries" )` | H + R/S (including owned validation) |
| `5cc3422c65cca92ebe04677ec96d5f5a144bef89e045df63c4dd1150517c8191` | 1 | `cx . fail ( command , "Phemex private history requires credentials" )` | H + R/S (including owned validation) |
| `5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2` | 2 | `cx . fail ( request . command , error )` | H + R/S (including owned validation) |
| `639272e32bb3ce8f877fa8630b1f2feb1aa14970ef429c1ba5b17c19d9227cbf` | 1 | `cx . fail ( command , "Invalid Phemex history time range" )` | H + R/S (including owned validation) |
| `6db5f410d6f00a616c4990ff32d4640e1a8db6f9d560ebe13c303d81dc823d15` | 1 | `cx . fail ( command , "Phemex history requires a known symbol" )` | H + R/S (including owned validation) |
| `7f284f04d3b73fcbb59e861ae7f25e8aadc122a371d2e30d308dc9e01d28c312` | 1 | `cx . fail ( command , "Invalid Phemex client order ID" )` | H + R/S (including owned validation) |
| `a25ed19569db55f160c6e07637c3dfe1a7151e2722da2780fb332e3fe3cab917` | 1 | `format ! ( "symbol={}" , request . native_symbol )` | F (preserve native logic/assertion) |
| `aa71c1d92e34443bf9b99a67bc669e086e750105b16ed6b6d1835d2e7f2c2c0f` | 1 | `cx . done ( request . command , data )` | H + R/S (including owned validation) |
| `e74f33a4a874839f7f8b471e8b36279ed94f190b2780d57e59f1cf14d23869a5` | 1 | `cx . fail ( command , "Unsupported Phemex native history symbol" )` | H + R/S (including owned validation) |
| `f913fd4fec0b8ab4a85e00f3b7439983491def6ad5bc264f606d18cbfc7da52e` | 1 | `format ! ( "{time}:{}" , request . count - 1 )` | F (preserve native logic/assertion) |

### `venues/phemex/src/identity.rs` (3 entries / 3 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `0d4f7749bb0b24c084131b4f15d0e612c9b03378700f1ea465dfa856113e17e9` | 1 | `self . request ( cx , Method :: Get , "/g-accounts/accountPositions" , "currency=USDT" , None , Pending :: Identity ( command ) , None , )` | R/S |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | `cx . done ( command , data )` | R/S |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | R/S |

### `venues/phemex/src/income.rs` (6 entries / 6 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `238884203c8b673bc8d07d3f21f1c808fce23b0710a33064f25eb76aac6fec24` | 1 | `cx . fail ( command , "Phemex account income cannot be filtered by symbol" , )` | H + R/S (including owned validation) |
| `5cc3422c65cca92ebe04677ec96d5f5a144bef89e045df63c4dd1150517c8191` | 1 | `cx . fail ( command , "Phemex private history requires credentials" )` | H + R/S (including owned validation) |
| `891103f78a8eca3e3afa6df1d04856f35db60eaddb365f9e92293271fa46c893` | 1 | `self . request ( cx , Method :: Get , "/api-data/futures/v2/tradeAccountDetail" , & query , None , Pending :: Income ( request ) , Some ( ArrayAt :: Root ) , )` | H + R/S (including owned validation) |
| `b23d2cd6eab1b45e491401cab80ff14bae48a9fa2a910e93f3400a669413b859` | 1 | `cx . fail ( command , "Invalid Phemex income range or limit" )` | H + R/S (including owned validation) |
| `e86a99cf9fac6ceb0ab993b14a362b03d6711e49e03f0c6c8da4b2cbed7e94c9` | 1 | `cx . fail ( command , "Too many active Phemex income queries" )` | H + R/S (including owned validation) |
| `f208d33f03a5291e0ba68dd6417e98dd3fd08398627717ab367d630192c297c2` | 1 | `format ! ( "currency={}&start={:.0}&end={:.0}&offset=0&limit={PAGE}" , CURRENCIES [ request . currency ] , request . from , request . to )` | F (preserve native logic/assertion) |

### `venues/phemex/src/lib.rs` (17 entries / 18 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `1b0b8cbc9ce44a55975ea8f3e025691cecfa45581828e95eb50ccc2f2d18e4f1` | 1 | `self . fail ( cx , p , error )` | R/B/H/W/X terminal dispatch by native pending; background cleanup before callback |
| `1d1591136af8d3c7b891a725138b1fe1e9ef3e5d401e18f610b77d9866b877e3` | 1 | `self . fail ( cx , bad , "malformed public response row" )` | R/B/H/W/X terminal dispatch by native pending; background cleanup before callback |
| `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 | `cx . request ( method , url , headers , body , pending )` | R/B/S/W/X by native pending; declare semantics per call |
| `3707a3e4c331d8aa244ffa1fdfff00869707d624a6ed97afed2f783400c15f54` | 1 | `self . fail ( cx , p , & format ! ( "Phemex {} {}" , e . code . unwrap_or ( - 1 ) , e . msg ) )` | R/B/H/W/X terminal dispatch by native pending; background cleanup before callback |
| `3d411e9715932bdc90a7a565f468fcb4804f1d8ef3c91c122626f77b2d90df2b` | 1 | `self . request ( cx , Method :: Get , "/public/products" , "" , None , Pending :: Markets ( vec ! [ ] , false ) , Some ( ArrayAt :: PathOmitLists ( & [ "data" , "perpProductsV2" ] ) ) , )` | R/B/S/W/X by native pending; declare semantics per call |
| `415cde8bbb79aec0ffd251892d668f5fea56c30150ff07e0a3a1e8d1ebb85a04` | 1 | `self . fail ( cx , p , & format ! ( "HTTP {}: {}" , r . status , r . body ) )` | R/B/H/W/X terminal dispatch by native pending; background cleanup before callback |
| `4dbce16a94437affd078f0756fdc5bf35899434ebde3ba73925201568fb05469` | 1 | `self . request ( cx , Method :: Get , "/md/v2/fullbook" , & format ! ( "symbol={}&limit=1000" , safe_cex_engine :: url :: encode ( self . native_symbol ( symbol ) ) ) , None , Pending :: Depth { symbol : symbol . into ( ) , generation , } , None , )` | R/B/S/W/X by native pending; declare semantics per call |
| `5993f99450d63ae5eb78cf2899ffc8efaaa9fd47dd066fcd1ac85a4cb8d937eb` | 1 | `self . request ( cx , Method :: Get , "/md/v3/ticker/24hr/all" , "" , None , Pending :: Tickers ( vec ! [ ] , false ) , Some ( ArrayAt :: Field ( "result" ) ) , )` | R/B/S/W/X by native pending; declare semantics per call |
| `60f3aecbbb9e98af4226ed2769775e3b26ba4af804afa0def383aad527bddd1b` | 1 | `cx . fail ( command , interval . unsupported ( "Phemex" ) )` | R/B/H/W/X terminal dispatch by native pending; background cleanup before callback |
| `6d16631ed954d85c787eae05eab608a09c8ce735e57abce7ab9ccfe2f305d968` | 1 | `cx . fail ( command , "Phemex requests are rate limited; candle read was not sent" , )` | R/B/H/W/X terminal dispatch by native pending; background cleanup before callback |
| `7b15b10003401f5b84eb1992913534cdc7868ab658ed9210e32d7230da28218c` | 1 | `self . request ( cx , Method :: Get , "/exchange/public/md/v2/kline/list" , & format ! ( "symbol={}&resolution={n}&from={:.0}&to={:.0}" , safe_cex_engine :: url :: encode ( self . native_symbol ( & symbol ) ) , ( from / 1 0 0 0 . ) . floor ( ) , ( to / 1 0 0 0 . ) . ceil ( ) ) , None , Pending :: Candles { command , rows : vec ! [ ] , invalid : false , } , Some ( ArrayAt :: Path ( & [ "data" , "rows" ] ) ) , )` | R/B/S/W/X by native pending; declare semantics per call |
| `7df0ba98dfdb7e3d532ca88e6cf5e2d67158db5c27a0486c22ce18d7493f5f07` | 1 | `matches ! ( & pending , Pending :: History ( request ) if request . is_lookup ( ) )` | F (preserve native logic/assertion) |
| `8e74b914b9159816adc7eb353d1c728eaea647260ac64fac31e4b967a6dab017` | 1 | `format ! ( r#"{{"id":{},"method":"{method}","params":{params}}}"# , self . public . request )` | F (preserve native logic/assertion) |
| `9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d` | 1 | `cx . done ( command , serde_json :: to_string ( & rows ) . unwrap ( ) )` | R/B/H/W/X terminal dispatch by native pending; background cleanup before callback |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | R/B/H/W/X terminal dispatch by native pending; background cleanup before callback |
| `f9764ec2239f809efce38c8d387de2eb9e144668ccf4e1d4193e1859c5e5e05f` | 2 | `self . public . request` | F (preserve native logic/assertion) |
| `fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6` | 1 | `cx . request_streamed_body ( method , url , headers , body , at , pending )` | R/B/S/W/X by native pending; declare semantics per call |

### `venues/phemex/src/trading.rs` (4 entries / 4 occurrences)

| Full fingerprint | Count | Current expression | Destination |
| --- | --- | --- | --- |
| `06d49adf719c2e5ba5712d013a8bd485f49a8559eab9fbd74b8265789e9e3c9c` | 1 | `matches ! ( request . call , Call :: Place { . . } )` | F (preserve native logic/assertion) |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | W + S; read preflight R; commandless repair X |
| `ce2317fc0bfce16cd6c1a0f3eaaa03828eef8d6a89d120fcf4fefb51a8d6b7fd` | 1 | `self . request ( cx , step . method , step . path , & step . query , None , Pending :: Trade ( Request { command , call : step . call , revision , signed_until : crate :: sign :: Credentials :: expiry ( cx . now ) as f64 * 1 0 0 0 . , } ) , None , )` | W + S; read preflight R; commandless repair X |
| `db4da6f92276f049047ceb4f85c06083f4c683531dd3d76b0a5ba44d9e2d5cb7` | 1 | `cx . done ( id , serde_json :: to_string ( & job . ids ) . unwrap ( ) )` | W + S; read preflight R; commandless repair X |

## Planning checkpoint verification

Exact source/manifest/table equality and local links verified; both request-context
checks pass (13 adversarial controls). Independent source/adversarial, caller and
full-map reviews are clean after correcting fixture paths, native retry/routing
and shared dispatcher authority. Architecture gate No for all four plans.
No production source, test or inventory changes are claimed by this checkpoint.

## Unsigned candle command slice

Pinned landing7c5e706e410 after Crypto.com #1818. Current source/test claims
exclude history #1790/#1795/#1796/#1797; HMAC owner claude-4yjlw0 confirms lib.rs is clear.
Unsigned candles are selected only when BOTH constructed primary and secondary
credential containers are absent. `public_data_only` alone is not that test:
optional credentials still select the exact legacy signed request branch.
The HMAC owner will coordinate before later Phemex edits; its signed migration
waits for shared credential selection and the engine read helper after #1817.

Add a separate Context-bearing public candle Pending continuation and a small
native adapter module. In the genuine FetchCandles callback, unsigned selection
calls engine admit_http_command with Read/Public,20s,Never before preflight or
transport. Rejected is already published; Revoked grants nothing. No numeric
owner argument/fallback exists in the public adapter. The original signed arm,
credentials/signing bytes, native query and legacy numeric terminals remain
exactly available for the credentialed branch; no common request helper switch.

Managed streamed delivery uses the existing data.rows selector, CandleRow
mapping, sorted output, status/native failure behavior and rate-limit pause.
Route/signing setup and transport/deadline failure carry the engine's typed
error to the Context terminal; public network/expiry remains a declared read
NotSent, with no credential evidence. Late/duplicate/expired replies cannot
finish another owner or update public/account caches. There is no new retry,
local timer, status ladder, signing recipe or cancellation authority.

Before production changes, commit original-source RED tests for preparation,
original20s expiry (parked and released), typed setup/network failures, stale
callbacks/ID reuse and genuine initial rejection. Keep original native query,
limit/from/daily boundary assertions; adapt their transport drain to the shared
prepared fixture and repair the existing backward response clock without
weakening assertions. Primary/secondary/dual credential controls must pass on
original production and retain the unsigned/signed selection boundary.

Files: lib.rs, new public_candles.rs, public.rs transport fixture,
request_context_candles.rs, and append-only plan/CALLERS/AUDITS/QUALITY ledgers.
Every legacy fingerprint remains necessary for credentialed candles or other
families: this partial unsigned adoption has zero inventory delta, no exemption
and no new legacy sites. The credentialed cutover will remove them after the
shared signer/selection prerequisites exist. Both ratchets must match incoming
exactly. Full Phemex+Binance native tests, scoped fmt/strict Clippy and independent
source/adversarial, caller and full-map reviews are required before CI/readiness.
Architecture No: native codecs consume shared command/lifecycle authority.

### Unsigned candle verification checkpoint

Six separately committed lifecycle controls fail on original production while
credentialed primary/secondary/dual compatibility passes. The managed path
passes all9 new controls plus14 existing public tests, including a
native error/malformed-row/missing-list comparison against the signed path and
late429 suppression. Full Phemex+Binance suites pass475 tests across38 targets;
scoped fmt and strict all-target Clippy pass. Independent source/adversarial,
caller and full-map reviews are clean, architecture No. The exact scanner and
manifest remain unchanged; integration of paused landing258c880656f and both
ratchets (14 controls) pass. Exact-head CI precedes merge readiness. Credentialed migration remains explicitly outstanding.
