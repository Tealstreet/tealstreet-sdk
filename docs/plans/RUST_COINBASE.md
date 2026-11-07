# Coinbase Rust venue port

Branch: `feat/rust-coinbase`, based on `origin/feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process and acceptance: `VENUE_PORT_BRIEF.md`.

Implementation and local acceptance are complete. Final landing and exact-head
CI evidence are recorded on [PR #1325](https://github.com/Tealstreet/tealstreet-sdk/pull/1325).

## Scope and boundaries

One `safe-cex-venue-coinbase` crate serves `coinbase` and `coinbasespot`.
Preserve the TS Advanced Trade INTX compatibility, CFM dated futures plus spot,
and the saved-account spot-only variant. Portfolio permission discovery decides
the authenticated product family. Spot-only must never probe CFM or INTX private
routes. Public-only INTX discovery remains keyless; the spot-only profile uses the documented unauthenticated Advanced Trade market routes. Starbase modules are inert
in TS and remain outside runtime activation; do not guess their unresolved units
or private notification envelopes. Typed records replace the deleted first-port
JSON emulation described by stale Coinbase documentation.

Current availability is narrower than TS's legacy surface: Coinbase's
[October 2026 changelog](https://docs.cdp.coinbase.com/coinbase-app/introduction/changelog)
retires Advanced Trade INTX and its `/intx/*` routes. This port preserves that
legacy adapter's typed compatibility and deterministic tests without claiming live
INTX account access or activating the unfinished Starbase migration. CFM and spot
remain the Advanced Trade products in scope. As in TS, `testnet` uses the same
real Advanced Trade URLs; it does not select a separate sandbox.

## Phases

1. Public data: typed streamed discovery and ticker polling, full-depth sequenced
   books, candle stream/fetch, symbol aliases, subscriptions and socket watchdogs.
   Verify documented examples, gaps, reconnects, malformed snapshots and aliases;
   independent code-correctness review, fix, commit.
2. Private data and native JWT: portfolio selection, balances, positions, settings,
   paginated open orders, user snapshot batches and incremental executions.
   Stale/failed reads preserve newer state. Auth plan below is audited against
   official sources before implementation. Verify both algorithms, REST/user
   auth, permission probes, snapshot races, fill replay and CFM units; review,
   fix, commit.
3. Trading: preflight full batches and caller IDs; Market, Limit and StopLoss
   configurations; native CFM contract quantities; stable-ID spot edits with exact
   provenance; confirmed INTX cancel/replace; per-ID batch cancellation; fixed
   one-way/cross/leverage behavior. No automatic write retries. Verify ambiguous
   responses and cancellation failures; review, fix, commit.
4. Queries: bounded documented history/public-trade reads with typed output,
   pagination and explicit unsupported axes/filters. Never manufacture reported
   PnL by folding fills. Verify limits/cursors/errors; review, fix, commit.
5. Harness and gates: SDK/WASM/native registration for both aliases; freeze the
   TS reference outside production and remove replaced runtime code; documented
   private tapes with independent JWT verification; live two-book public fixture,
   declared TS corrections, identical-output/performance gates; Binance controls;
   bans/fmt/Clippy/native/WASM/host/SDK checks. Review, fix, commit.

After all gates pass, merge latest landing branch, rerun affected gates, push,
open PR into `feat/rust-rewrite`, post independent correctness reviews and fixes,
repeat until clear or diminishing returns. Merge only on exact-head green CI.
Cut a fresh follow-up branch from updated landing. Never stash/rebase/force-push
or touch master. Record measurements and unresolved evidence honestly.

## JWT design and source audit

The signer is owned by the venue, imported once, with no Debug or private export.
Credential input is bounded to 16 KiB per field before parsing. Typed downloaded
JSON accepts `name` and `privateKey`, `private_key`, `keySecret`, `key_secret`,
`secret`; secret-field JSON takes precedence over key-field JSON, as in TS.
Qualified names are exactly `organizations/{org}/apiKeys/{key}`, with nonempty
segments and no whitespace/control characters. Errors never echo input.
Decoded buffers, normalized private strings, parsed JSON secret fields, and
Ed25519 seeds zeroize when dropped.

P-256 SEC1 PEM is decoded and wrapped in a bounded PKCS8 PrivateKeyInfo with
`ecPublicKey`/P-256 OIDs; PKCS8 PEM imports directly. Escaped newlines normalize
before decoding. `safe-cex-crypto::es256::Es256SigningKey` validates the curve and
signs SHA-256 over exact JWS input using RFC 6979. The signature is fixed-width
64-byte `r || s`, never DER. Deterministic Rust ECDSA signatures can differ from
randomized browser ECDSA while both verify; harnesses must verify signed bytes
rather than compare signatures.

Ed25519 accepts standard or URL-safe base64, padded or unpadded, whitespace and
an optional case-insensitive `ed25519:` prefix. Decoded length must be 32 or 64;
64-byte CDP keypairs use their first 32-byte seed, matching TS and the official
Advanced Trade Python SDK. The appended public half is not treated as a second
source of authority or a signing input. Signing uses
`safe-cex-crypto::ed25519::sign` on exact input bytes without prehash.

Typed JOSE headers contain algorithm, `typ: JWT`, exact `kid`, and a fresh nonce
from `safe-cex-crypto::uuid::uuid`, which already uses native `getrandom` or the
browser cryptographic random source. Its TS-compatible alphanumeric nonce need
not match the documentation's example hexadecimal spelling. Empty entropy
output fails closed; no time/counter fallback exists. Private tests inject a
fixed nonce only inside the signer module. Three JWS parts use base64url without
padding. Engine milliseconds are floored to seconds for `nbf`, and `exp` is
exactly 120 seconds later; non-finite, negative or overflowing clocks reject.

REST signs an uppercase method, fixed real venue host `api.coinbase.com`, and
the resolved endpoint pathname, excluding query, fragment, scheme, routing
prefix and body. Relative absolute-path inputs are validated, and double-slash
network paths reject. Sign when dispatching after any pacing, never when a job
is queued. Native requests use the venue's standard `Authorization: Bearer`
header and JSON content type; generic hosts preserve venue credentials while
applying routing. The former TS proxy header alias and copied Binance API-key
header are not venue authentication requirements.

Each private `user` and `heartbeats` subscribe gets a different fresh JWT with
no URI. Subscribe immediately after open, with a fresh token on reconnect;
discard stale socket callbacks and incomplete user snapshot staging. Watchdog
health follows real server heartbeats. JWT expiry does not justify inventing an
application ping, replaying old subscriptions or unsolicited periodic renewals.

Claim compatibility is explicit: preserve active TS ES256 `iss: coinbase-cloud`
without `aud`, and EdDSA `iss: cdp` with `aud: [cdp_service]`. Current official
sources conflict: App documentation warns ECDSA-only, while its newest official
Advanced Trade SDK loads Ed25519 as well as P-256 and uses `iss: cdp` without
`aud` for both. General CDP JWT samples use `cdp`/`cdp_service`; a WebSocket C#
sample still uses `coinbase-cloud`. This port documents the contradiction;
without an account capture it does not label the existing claim spelling a
proven bug or silently switch it.

Independent tests verify both P-256 PEM formats using the p256 verifier and
Ed25519 using the RFC 8032 seed/public-key vector; inspect claims, REST URI,
64-byte signatures, fresh per-message nonces, absent WS URI, credential JSON
precedence, malformed secrets, and clock/path boundaries. Mutation and wrong-key
verification reject. Production signing stays entirely in Rust.

Primary references checked 2026-10-20:

- [Coinbase App API-key authentication](https://docs.cdp.coinbase.com/coinbase-app/authentication-authorization/api-key-authentication):
  qualified names, PEM/newlines, standard Bearer header, 120-second lifetime.
- [Advanced Trade WebSocket authentication](https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/websocket/websocket-authentication):
  WS tokens omit REST method/path; source examples retain conflicting issuers.
- [WebSocket overview](https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/websocket/websocket-overview):
  subscribe within five seconds, one channel/message, new JWT per message.
- [Official Advanced Trade JWT generator](https://github.com/coinbase/coinbase-advanced-py/blob/master/coinbase/jwt_generator.py):
  P-256/Ed25519 key import and raw 32/64-byte Ed25519 handling.
- [Official REST implementation](https://github.com/coinbase/coinbase-advanced-py/blob/master/coinbase/rest/rest_base.py):
  JWT binds method/host/path, while query parameters are passed separately.
- [General CDP JWT authentication](https://docs.cdp.coinbase.com/get-started/authentication/jwt-authentication):
  Ed25519 seed/public-key representation and issuer/audience alternatives.
- [Coinbase App changelog](https://docs.cdp.coinbase.com/coinbase-app/introduction/changelog):
  native client IDs are at most 128 characters (August 2025); public trade sides
  describe the maker (July 2025), inverted for the SDK's taker-side contract.

## Progress

- Fresh branch created; clean landing brief/spec inspected. Independent scope,
  auth-source and public implementation work started. No acceptance gates claimed.

### Public protocol evidence

Read-only live probes on 2026-10-20 resolved conflicting sequence documentation:
`subscriptions:0`, `heartbeats:1`, BTC level2 `2..10`, ETH snapshot `11`,
BTC update `12`, ETH update `13`, subscriptions `14`, BTC/ETH candles `15,16`,
subscriptions `17`, BTC/ETH trades `18,19`, subscriptions `20`, then interleaved
book/trade messages `21..34`. The current AsyncAPI connection-wide rule matches
this observation; the overview's per-product prose does not. Track every valid
connection envelope, and invalidate books on any gap.

The legacy INTX instruments endpoint returned 310 instruments and zero
`TRADING` perpetuals. INTX book subscriptions returned empty snapshots, so the
live public performance fixture uses the supported spot profile and active
BTC-USD/ETH-USD books. Authenticated INTX/CFM/spot coverage uses independent
payload tapes; no private live-auth claim is made without credentials.

Supporting frame change: an optional spot currency table retains total,
available and reserved amounts independently. Legacy aggregate/collateral frames
are unchanged. Engine49 tests and JS wire7 tests pass, including transfer,
reserved holdings, absent-versus-empty and special currency property names.

### Phase 1 checkpoint

Public data and the native signer foundation are implemented. Independent
code-correctness review found sequence-domain, pagination, failed-read metadata,
CFM multiplier, alias snapshot, daily seeding and socket recovery defects. Those
findings are fixed and rechecked. Verification: 16 public tests plus 5 native
signer tests pass; strict venue Clippy passes; bans pass. Native signer cases
independently verify P-256/Ed25519 signatures and reject altered claims.
Full workspace, WASM, SDK and performance acceptance remain later gates.

### Phase 2 checkpoint

Portfolio selection, private JWT subscriptions, balances, positions, settings,
50-row user snapshots, active-status REST pagination, execution deltas and stale
read guards are implemented. Independent review's five P2 findings are fixed and
rechecked: newer user snapshots fence old REST scans (including removals), native
edit proofs require explicit spot metadata and no trigger, first live fills are
retained across placement races, missing price provenance cannot invent an
execution price, and enqueue failures release reconciliation state.

Verification: 40 venue tests (8 module, 17 public, 15 account) and strict targeted
Clippy pass; engine 49 tests and JS frame 7 tests pass. The currency extension's
presence flag preserves independent absent/empty collateral and spot holdings.
Future trading/query hooks have explicit temporary lint expectations, removed
when their following phase consumes them. No replay or live private gate claimed.

### Phase 3 checkpoint

Native placement, exact caller IDs, confirmed INTX cancellation before replacement,
proof-checked spot edits, per-ID cancellation and fixed account settings are wired.
Placement amounts retain venue units (CFM contracts); store orders retain base units.
The optional engine account identity exposes only the selected portfolio.

Independent code-correctness reviews found and fixed queued cancel-all omitting
earlier accepted jobs, late create acknowledgements resurrecting terminal orders,
and closing fills releasing reserved exposure before causal position reconciliation.
Grouped execution reservations now cover overlapping own and external orders,
retain ambiguous writes, and require fresh post-event position evidence before
release. This is a bounded client-side reduce constraint; the API has no native
reduce-only flag and cannot make external concurrent trading atomic.

Verification: 67 venue tests pass (8 module, 16 account, 17 public, 26 trading),
including the race regressions; strict targeted Clippy passes. Known TS corrections
include per-ID cancellation outcomes, confirmed cancel/replace, placement caller
IDs/order prices, and explicit fixed-setting limitations. Replay gates remain open.

### Phase 4 checkpoint

Typed streamed fills, terminal order history and public trades now implement the
SDK JSON query shapes. Walks cap at 50 pages, 1000 returned rows and 16 concurrent
reads; duplicate IDs require consistent economics, UTC ranges filter inclusively
after widened exclusive API boundaries, and later failures reject the whole read.
CFM query amounts stay in native contracts while quote notional uses contract size.
Public trade sides invert the documented maker side to the SDK's taker convention.
Unavailable income and exact client-ID lookup reject explicitly without inferring
an empty ledger or definitive absence.

Independent source/test review is clear after checking quote-size provenance,
pagination, proof tokens, timestamp bounds, aliases and public trade side. All
84 venue tests and strict targeted Clippy pass, including 13 new query cases.
Integration and full acceptance gates remain open.

### Host routing checkpoint

Coinbase's native standard Bearer header now survives ordinary authenticated
proxy routing through a generic host `headerAliases` policy; direct/HOW traffic
keeps the original header. Aliasing precedes proxy authorization and conflicts
fail before network I/O. Independent transport review is clear. All 187 JS
transport tests and 31 native-host tests pass; existing 48 routing-policy cases
remain unchanged alongside four Coinbase cases. Repository typecheck (44 tasks)
and lint (28 tasks) pass. No final acceptance claim yet.

### Integration checkpoint

Both SDK registries and WASM aliases now select the native crate. Replaced active
TS modules moved to a test-only reference; inert Starbase contracts remain.
Native credential import, private portfolio identity, caller IDs, economic order
fields, unsigned public startup and independently verified JWTs pass five real
WASM tests. Nonfinite optional command numbers reject before JSON can erase a
bound or protection. All 27 repository test tasks pass (6958 safe-cex tests in
each of its two tasks), and the generic harness passes 229 tests. Independent
reviews fixed exact query-head/alias matching in the documented-input oracle;
14 declared TS corrections retain strict native request/signature comparisons.

The 20-second live capture has 666 messages and both full-depth spot books; its
eight-second fixture retains 201 messages and all 925 products. Three-run public
gate passes: CPU TS714.4ms/Rust272.3ms; steady CPU403.2/57.2ms; retained memory
17.9/14.1MB including 12.0MB native linear memory. Coinbase-only WASM952808bytes
is below the1258291byte budget. Both legacy INTX private algorithm tapes pass.
CFM revealed recurring polling contention with its initial order scan; budgeted
polling and timer-driven regression are in progress. Spot variants, Binance
controls, combined WASM tests, latest landing merge and PR gates remain pending.

History pagination is venue-bounded rather than transactional: Coinbase documents
the fills sort cursor as unstable under concurrent activity. Conflicting repeated
IDs reject, but no client can claim a server snapshot across external account
changes. Crypto audit found no newly unused first-port helper from this port;
the shared ES256 and Ed25519 modules are both used directly.

### Paced startup correction

Live recording fixed deferred book subscriptions, current `online` product
status/disabled flags, and spot's zero funding-time sentinel. Private CFM replay
exposed recurring reads starving its initial eleven-page order scan. Recurring
account reads now run every five seconds, CFM product polling every five seconds,
and CFM fallback order scans at least ten seconds apart. Initial order scans
suppress background polls but retry transient failures under backoff; debounced
postwrite repair remains two seconds and public spot/INTX polling remains one.
Two timer-driven ninety-second regressions verify startup within twelve seconds,
initial 503 recovery, three-per-second pacing and sustained bounded scan progress.
Independent review is clear; all 86 venue tests and strict Clippy pass.

Main Coinbase's spot tape now proves the real 403 futures-permission fallback;
the saved spot alias has no futures probe. Four private variants passed before
these corrections (both legacy INTX and both saved-spot algorithms); the full
eight-profile rerun is in progress. No final gate claim yet.

CFM startup now completes and its account/command replay matches. A final ticker
correction keeps funding time zero for dated futures even when the shared API
schema includes incidental perpetual metadata; INTX retains its original
unknown-metadata sentinel. The strengthened CFM regression and all 86 venue tests
pass; independent review and strict Clippy are clear. Both main spot tapes pass.

### Landing integration checkpoint

Merged landing commit922b5fdc89, preserving its Bybit/KuCoin aliases, allocator
and optional native-order frame tail. Conflicts were reviewed in separate SDK,
WASM and harness scopes; engine currency balances/account identity survive the
automatic merge. Newly introduced Bybit-only native options reject for Coinbase.
CFM native quantity metadata now survives both worker and main-thread frame
application; the sixth real WASM case proves two contracts become0.02base and
three native placement contracts become0.03base while retaining native extras.
All six SDK/WASM cases, SDK typecheck/lint, 55 engine tests,86 venue tests and
272 harness tests pass. The post-merge eight-private/public/control gates are
running before PR publication.

### Local acceptance gates passed

- Bans, workspace formatting, strict workspace Clippy and strict WASM Clippy pass.
- Native workspace:1458 tests across129 suites; Coinbase86, engine55 included.
- Generic harness272; combined SDK/WASM81; browser/Workers/native JS host47.
- Repository typecheck44 tasks, lint28 tasks, test27 tasks pass; each safe-cex
  test task passes6977 cases. Six real Coinbase WASM cases include CFM units
  through worker and main-thread decoding and independent JWT verification.
- All eight product/algorithm private replays pass; the default private command
  also passes on the combined build. Documented payloads, not live account keys.
- Twenty-second public capture, eight-second fixture:925 markets,201 replayed
  frames, two full-depth books. Three-run post-merge output is identical.
  Median TS/Rust CPU827.6/316.4ms, steady459.1/74.3ms, retained32.3/14.2MB
  (Rust includes11.8MB linear memory). Coinbase-only WASM959506bytes against
  the1258291byte budget; combined build and both web/Node bindings pass.
- Binance public control passes three runs:CPU179.3/79.2ms,
  steady93.0/24.1ms, retained19.4/3.3MB. Its generated private control passes.

Fourteen TS corrections are enumerated in the input-only oracle; all native
request, signature, economic, caller-ID and command comparisons remain active.
The PR targets only `feat/rust-rewrite`; posted correctness reviews and exact-head
green CI are the remaining landing gates. No live private-authentication claim;
the retired INTX routes and inactive Starbase migration are documented above.

PR [#1325](https://github.com/Tealstreet/tealstreet-sdk/pull/1325) targets
`feat/rust-rewrite`. Fresh account/auth, trading/history and public/integration
code-correctness reviews are running against the published port. Findings and
fixes will be posted on the PR; merge requires green CI at its exact head.

### Published PR correctness review, round one

Three fresh reviewers found six actionable issues: REST scans advanced fill
emission baselines and discarded deduplication history; user orders mixed quote
leaves with base fills without denomination proof; recovered placement IDs did
not resolve ambiguous reservations; history sent an undocumented sort enum;
stream intervals reused REST's smaller whitelist; and daily candles did not
backfill after reconnect. Fixes and focused interleaving/reconnect regressions
are in progress, followed by affected gates and another independent review.

CI's five secret-scan findings were inspected locally with the same gitleaks
8.30.1 version. Three are literal `organizations/test/apiKeys/test` test names;
two match PEM delimiter labels/invalid `AAAA` inputs rather than key material.
Only their exact historical commit/path/rule/line fingerprints are ignored;
the default detection rules and future source scans remain enabled.

### Review repair checkpoint

All six round-one findings now have fixes and focused regressions. Delivery
baselines survive REST scans/removals; typed REST sizing or native placement
metadata proves spot user quantities before normalization/editing. Unknown
zero-remainder terminal updates apply immediately without inventing sizing
provenance. Bounded detail reads retry transient failures and isolate socket
generations. Identity recovery is independent of positive fills, merges external
execution fences, and retires known spot reservations. History uses the API's
default creation-time order. Streaming intervals retain TS's wider set while
REST keeps its documented granularities. Daily candles backfill reconnects,
including subscriptions added offline, and compare cumulative bucket volume in
both arrival orders. Quote/base sizing is the fifteenth declared TS correction.

Strict targeted Clippy/formatting and all103 venue tests pass; harness274 pass.
Independent public/history re-review is clear. A fresh reviewer is checking the
account/trading repairs; latest Aster landing integration and affected combined
acceptance gates remain pending. CI's frozen reference entry-point classification
failure is fixed, with three passing targeted checks (one web-only skip).

### Integrated review checkpoint

Merged landing d9aa597fd3, retaining Aster and Coinbase options, routing/deadlines,
account identity and full currency balances. The shared model follows landing's
nested Balance shape; the frame retains explicit optional-collateral flags and
all four currency columns. Independent Rust, SDK/host and harness merge reviews
are clear. Merge repairs removed duplicate `canSign` and protocol imports and
preserved native Coinbase currencies in explicit replay projections.

Further account/trading reviews fixed zero/partial terminal recovery after lost
ACKs, no-client-ID terminal updates resolved by later ACKs, deferred rows overtaken
by REST sizing evidence, and live-fill eligibility while an initial snapshot
waits for sizing. Receipt-time eligibility prevents historical fills becoming
live after delayed reads; failed initial baselines reconnect. Existing quote
provenance outranks duplicate-client ACKs. Stale sizing responses cannot apply
authentication failures/backoff to a replacement socket. Focused final review
finds no remaining actionable P1/P2. All113 Coinbase tests and strict targeted
Clippy/formatting pass. Shared engine60, harness284, host53 and SDK integration230
pass; root typecheck44/lint28 and uncached test27 tasks pass. Integrated acceptance
replays, final WASM builds and exact-head CI are still pending.

### Post-review acceptance rerun

At ce16b82d4d all eight private variants pass, with15 commands per profile and
both ES256/EdDSA. Three-run public replay is identical:201 frames,925 markets,
925 tickers,two full-depth books. Median TS/Rust main CPU844.3/317.8ms,
steady470.9/78.1ms, process972.3/401.3ms, retained32.5/14.2MB (Rust includes11.8MB
linear memory). Coinbase-only WASM1011067bytes is below1258291bytes. Native
workspace1596 tests across135 suites, bans/format/native+WASM Clippy and root
44 typecheck/28 lint/27 uncached test tasks pass; safe-cex7009 tests in each of
its two tasks. Correctness findings/fixes and clear re-review are posted on PR1325.

Kraken landing7da56dc8a5 arrived during these replays. Integrating its registration
changes before final combined-build controls and exact-head CI; Coinbase venue,
crypto and engine source stay unchanged by this merge.


### Final local acceptance

Latest Kraken registration merge is reviewed clear. All mandatory venue gates
pass; no outstanding code-correctness P1/P2 remains. Coinbase venue/engine/crypto
behavior is unchanged by this registration merge. Bans, formatting, native/WASM
Clippy,113 venue tests and the added native WASM host compilation pass. Shared
workspace acceptance checkpoint:1596 tests/135 suites; the latest CI also checks
the newly landed Kraken crate. Latest harness296, real combined SDK/WASM82 and
JS host53 pass; root typecheck44/lint28/uncached tests27 tasks pass, including
7009 safe-cex cases in each test task.

All eight product/algorithm private replays pass, and the default Coinbase
private replay passes again on the8070228byte combined build. Binance controls
also pass: public873 frames/917 markets/783 tickers/three books, three runs,
TS/Rust main185.4/89.5ms, steady98.8/26.1ms, retained19.6/3.4MB; generated private
control passes. Coinbase public and selected-size metrics are recorded above.
Fifteen explicit TS corrections remain bounded by documented-input oracles;
request, signature, economic and caller-ID checks remain enabled. Independent
review reports/findings/fixes are posted on PR1325. Merge requires all checks at
its exact final head to pass. No live private authentication or retired INTX
availability is claimed; inactive Starbase remains outside activation.

### Paradex shared-transport integration

Integrated landing1f9e9a5206 after the preceding PR head passed all30 CI checks.
Retained both venue registrations and explicit binary transport opt-in; Coinbase
public/private sockets continue through text channels. Corrected an automatic
merge omission in the single-venue secret-field cfg. Independent Rust/native,
SDK/host and harness code-correctness reviews are clear. Harness337, host57,
targeted SDK270, SDK typecheck/lint, selected WASM Clippy and113 venue tests
plus shared engine tests pass. Full integrated Clippy, rebuilt acceptance
replays and exact-head CI are rerunning; their final evidence is posted on PR1325.

### Phemex landing integration

All31 CI checks passed at5cd4c97a09. Landing45d7a3fb98 then arrived during
the merge preflight and created registration conflicts; GitHub rejected the merge.
Integrated Phemex and its opt-in catalog parsing/UTF-8 byte-input host work,
retaining Coinbase credentials, portfolio identity and text-channel behavior.
Independent Rust review found a new host test double missing account_id; it now
returns None. Native Coinbase113, engine65 and selected WASM host6 tests pass;
strict selected/combined WASM Clippy, bans/formatting and harness359 pass.
SDK82, host/socket/wire52 and mock Workers9 pass, with SDK typecheck/lint;
independent SDK/host review is clear. The selected WASM is990638bytes.
Rebuilt public/all8private replays, combined
controls and exact-head CI rerun; final measurements/results are posted on PR1325.

### Port-review lows plan (2026-10-21)

1. Reverify spot total/free/used and explicit OHLCV lower bounds against native
   mappings and documented fields. Preserve correct behavior where TS differs.
2. Keep CFM summary unrealized-PnL/equity aggregation unchanged without verified
   portfolio/spot accounting evidence; record the unresolved risk precisely.
3. Update audit/caller notes and independent review. Re-run relevant existing
   regressions; avoid runtime changes where current behavior is already correct.
   A documentation-only family PR is exempt from the code merge hold.

- Revalidation is documentation-only. `account.rs::Request::SpotBalance` uses
  total_balance_crypto, available_to_trade_crypto, and their difference for
  total/free/used. The existing holding regression proves a total2/free1 row
  retains total2; copying TS available-as-total would lose held funds.
- `public.rs::FetchCandles` maps explicit from_ms to native start (seconds),
  independently of defaulted/explicit limit; it validates the range and uses
  limit only as the fallback window length and native count. The frozen TS
  implementation defaults limit before testing !limit, making its from branch
  unreachable. Preserving an explicit lower bound is the intended contract.
- CFM uses balance_summary.unrealized_pnl; TS zero is not proof of correct equity
  accounting. Keep this source pending verified CFM/spot accounting and consumer
  evidence. This family makes no runtime or caller-contract change.
- Primary field/parameter references:
  https://docs.cdp.coinbase.com/api-reference/advanced-trade-api/rest-api/portfolios/get-portfolio-breakdown
  https://docs.cdp.coinbase.com/api-reference/advanced-trade-api/rest-api/public/get-public-product-candles
- Re-ran existing invalid-holding and public-candle regressions: both pass.
  Explicit-from disposition is source inspection, not a claim of dedicated
  lower-bound regression coverage. Independent read-only review found no blockers;
  diff whitespace check passes. Full CI follows the documentation-only PR.
