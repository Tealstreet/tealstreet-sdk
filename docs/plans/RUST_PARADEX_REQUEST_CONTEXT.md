# Paradex RequestContext inventory and migration plan

Plan,2026-10-23; lane `codex-zs5e83`, overseer `claude-ddi2il`.
Pinned landing: `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`.

## Scope, holds and first area

Paradex perpetuals on production/testnet. Open #1787 holds `src/account.rs`,
`src/trading.rs` and `tests/account.rs` (the row/freshness series #1785-#1788
is integrating sequentially). Keep those source/test paths untouched until their
PRs clear. Shared ledgers/inventory remain permitted append/prune surfaces.

The first unblocked area is the independent public REST book query, changing
only `src/lib.rs`, `src/depth_query.rs`, `tests/depth_query.rs`, a new focused
request-context test file and shared ledgers/inventory. It is an unsigned GET
with no catalog, credentials, JWT, subscription or native trading quote work.
No required header is lost by the existing plain managed API.

## Destinations

| Code | Engine path and native facts retained |
| --- | --- |
| BOOK | Single command Read Context/Lifecycle, public intent and RetryPolicy::Never; exact read-delivery validation, context finalizers and unchanged request-owned depth codec. |
| CONFIG | Background public Read for system/config, validated chain config and credential setup. Held account source; audit consumers before moving. |
| SENDER | Shared authenticated managed dispatch ingress, operation class from originating work, original body/array selector/native headers and routing. Existing legacy sender remains until callers migrate. |
| AUTH | Shared JWT producing/refresh ownership and dispatch-time Starknet recipe; queued command/background/socket observers retain original owner bounds. |
| SNAPSHOT | Managed background Reads, existing cycle/boot chain, revision/time/coverage and row admission; no failed snapshot becomes empty success. |
| IDENTITY | Authenticated command Read `/account`, Context-bound companion diagnostic and finalization, separate from financial polling cycles; shared JWT dependency. |
| HISTORY | Shared checked Context-bearing CursorHistory/Completion and page-plan legs; preserve strict client-ID/auth/cursor refusal, accepted-only recovery, incomplete markers, discovery/lookup fallback and public-trades shaping. No-I/O branches also require completion authority. |
| PUBLIC | Single background market/ticker Reads, existing item parsing, startup Backoff, native ticker revision and seed policies; independent scheduler admission remains shared. |
| CANDLE | One command/background Read Plan over documented klines and TradingView fallback. Preserve3s polling, existing interval handling, native bounds, trade recovery and generation/revision fences. |
| TRADE | Shared command Plan with optional capped-order quote Reads, rails Reads and signed Mutation legs; shared journal, native result facts, bounded reconciliation and context finalization. |
| LEVERAGE | Authenticated Read context, exact setting publication authority and finalization; held trading source. |
| NOIO | Shared admitted completion authority for fixed CROSS/one-way and unsupported command/setting refusals; no invented request or mutation evidence. |

## Phase 1: independent REST book query

1. Commit original assertion-RED tests before production changes. Cover no
   legacy send while preparation is parked, exact prod/testnet URL and raw-symbol
   encoding, public route and unchanged caller depth, early response rejection,
   typed preparation refusal, original-deadline expiry before/after claim,
   duplicate/late replies and command-ID reuse, independent concurrent reads,
   and visible refusal when initial authority capacity is exhausted.
2. Issue a single `Policy` Read context with20,000ms timeout, no credential,
   pacing or automatic retry, matching the existing Binance read adapter.
   Capture the original bound before preparation; configured host timeouts may
   shorten it. Store Context in BookQuery pending. Use `request_managed` GET and
   validate exact read delivery before native result completion.
3. Keep the depth codec's current server order, duplicates, zero amounts,
   quote-product totals, strict malformed/overflow rejection and bounded native
   non-auth/error/rate metadata. Pass typed preparation errors intact through
   `on_sign_error`; do not add status ladders or classifiers. The existing
   response codec remains native data validation, not send/outcome policy.
4. Use context done/fail in success, native failure, transport and dispatch
   failure paths. Put initial issuance in `depth_query.rs`, retaining exactly
   one existing `cx.fail(command, error)` there if no Context was returned.
   Adding a numeric fallback in lib would grow a different path fingerprint.
   The shared command-entry seam may later remove it; never invent a handle or
   numeric fallback after authority exists.
5. This area owns six occurrences. Expected prune with that retained issuance
   refusal is **62->57**: `lib.rs` **3->0**, `depth_query.rs` **3->1**.
   All other path maps remain identical. If the reviewed shared entry API lands
   first, re-audit exact source/manifest rather than guessing a different count.
6. Adapt the existing eight book-query fixture tests with shared
   `prepared_actions`, preserving every assertion/body/clock/error contract.
   Keep parked/denial controls explicit. Run full Paradex+Binance tests, scoped
   Clippy/fmt, bans and both request-context checks; obtain all three independent
   reviews and document the architecture answer before a draft implementation PR.

No live SBE/JSON book sequencing, account/JWT state or capped-order trading
quote path changes. A REST snapshot never becomes a live book update.

## Later areas and shared prerequisites

Independent public market/ticker seed migration follows after command books.
Candles have a genuine two-endpoint fallback, including command and polling
owners; do not label them single-request. They wait for `codex-uufub4`'s Plan
admission and shared observer authority where required. Private account refresh
queues up to256 requests behind singleflight JWT; commands, background cycles
and socket auth share that producer. Shared joined/JWT ownership must preserve
waiter bounds instead of giving every observer a renewed auth deadline.

Starknet JWT/native order recipes route to `claude-19pt8s`, with credential/nonce
prerequisites owned by `claude-m26ps4`. Preserve direct auth routing, Starknet
account/subkey/network domains, protocol stamps, partner/delegated headers and
order-body signatures at dispatch. Plain managed ingress cannot carry the
private header bundle; do not queue pre-signed expiring bytes as unsigned work.
History waits for the checked Context-bearing completion bridge and Plan, keeping
one root across fills/lookup/fallback stages. Classification uses the engine
owner's shared seam; do not introduce new per-venue status/code ladders.

## Caller-visible contract to document with implementation

`RustEngineExchange.fetchOrderBook` retains its signature, raw-symbol URL,
default20/explicit-unclamped depth and result shape. Custom worker/hosted HTTP
transports must implement preparation/binding/final claim. Preparation refusal
and original-deadline expiry reject the existing snapshot promise; retired
callbacks cannot finish a reused command. Preserve current non-auth typed/native
and bounded429 error metadata, with no automatic query retry. The caller ledger
found no default direct NX/CLI/MCP snapshot producer; ordinary depth listeners
and the separate capped-order quote path are not migrated by this area.
No chart/listener migration, private connectivity or deployment claim.

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

**62 occurrences / 53 distinct path fingerprints.**
Every row is one occurrence (count1); repeated fingerprints retain their
full multiplicity. All paths are relative to `sdk-rs`. No `sign_http` sites
exist at this pin; that does not prove dispatch-time signing adoption.

| Path | Occurrences | Distinct |
| --- | ---: | ---: |
| `venues/paradex/src/account.rs` | 10 | 10 |
| `venues/paradex/src/depth_query.rs` | 3 | 2 |
| `venues/paradex/src/history.rs` | 14 | 10 |
| `venues/paradex/src/identity.rs` | 3 | 3 |
| `venues/paradex/src/lib.rs` | 3 | 2 |
| `venues/paradex/src/public.rs` | 8 | 8 |
| `venues/paradex/src/trading.rs` | 21 | 18 |

### `venues/paradex/src/account.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 367 / `fetch_config` | `request` | CONFIG | `e473c92d9c3a15cbdf8752a673c64963c9ef531ee4a7e47ae60d1abe5e32659e` |
| 449 / `dispatch` | `macro` | SENDER | `c8154248a24f58ded15a175a8afd344f331635f51e564a526d510748797ff231` |
| 451 / `dispatch` | `request_streamed_body` | SENDER | `3205be4fd57aee0c1cacd8fdd9fa48f9d000e9bbb2b7f53409ca39c64f0edc4a` |
| 460 / `dispatch` | `request` | SENDER | `388c9f66c40d488788f2f4a9808088e22a0425615dfd2818c9d63d58675392d1` |
| 491 / `refresh` | `request` | AUTH | `0bc0f05583257432e512c356c6a5b41046e2a70d3cfe6eb8190238c9514faba2` |
| 980 / `margins` | `request` | SNAPSHOT | `ac42e25ca2de115b1eb59cc4913e6fd26670c4dfb47f17ed56fc0d1f34272fd9` |
| 1001 / `positions` | `request` | SNAPSHOT | `85a7a04ae14d4df7bd123dd4837791701bbb02dc615d4e9f24060a2161f7ba3e` |
| 1021 / `balance` | `request` | SNAPSHOT | `395b8568f2efae46398fc76c6b422807900a928734749cdd4f8bd60b42366d19` |
| 1038 / `orders` | `request` | SNAPSHOT | `5c28b0ebae1f2a54f344d52c2c422801c8e82381c7fd8ad95d13cd549e88ddec` |
| 1071 / `on_item` | `macro` | SNAPSHOT | `a5c107b3d98e73b4c73ebd13c21dcf8cb307a0041bc896cc3e6e808fb63cf446` |

### `venues/paradex/src/depth_query.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 141 / `book_query_response` | `fail` | BOOK | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |
| 154 / `book_query_response` | `done` | BOOK | `7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b` |
| 155 / `book_query_response` | `fail` | BOOK | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |

### `venues/paradex/src/history.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 273 / `query` | `done` | HISTORY | `7e6ac08cf11a9f2f3a366e224d2c5df531cef78fd5b215d673548273ddd1fe50` |
| 290 / `query` | `fail` | HISTORY | `8153debcec98fa16c1dfc5c29c116571f0f4f5735aa73f778c9fafc32448211b` |
| 306 / `query` | `fail` | HISTORY | `55fe601b0e4d079ab998430cfbafaa7ea570dd124789ffb38859d78b00696d10` |
| 310 / `query` | `done` | HISTORY | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` |
| 413 / `issue` | `macro` | HISTORY | `156d12f0af1aa73d997695a9ee5ae2f0b10efdaf05fbf3c5a44b49c0b8c67eb3` |
| 431 / `issue` | `macro` | HISTORY | `552d45a1b59d0f22ccc9a74886bba18ab8ed7ed63abbbaf21329272a874b4ace` |
| 450 / `issue` | `request_streamed` | HISTORY | `a876e5300951dc42d8c9828d78f0d32d52e42b439cbaf763cc6cdd6b63174865` |
| 458 / `issue` | `request` | HISTORY | `a81744f9afdaf13c609cee0a6b18b04c80c9fa876308896b9d619fd75c8c7b9c` |
| 686 / `on_response` | `macro` | HISTORY | `156d12f0af1aa73d997695a9ee5ae2f0b10efdaf05fbf3c5a44b49c0b8c67eb3` |
| 727 / `on_response` | `macro` | HISTORY | `60ad9d1f92b48483572276a3a75cf6e7c6579a0c8d0118a9f9f51688b851c9aa` |
| 762 / `on_response` | `macro` | HISTORY | `4f41451eb1e1a039f8b647eaee00abfdc9027246d230dff2fad9129e9919c3e7` |
| 802 / `finish` | `macro` | HISTORY | `156d12f0af1aa73d997695a9ee5ae2f0b10efdaf05fbf3c5a44b49c0b8c67eb3` |
| 803 / `finish` | `macro` | HISTORY | `4f41451eb1e1a039f8b647eaee00abfdc9027246d230dff2fad9129e9919c3e7` |
| 836 / `on_failed` | `macro` | HISTORY | `156d12f0af1aa73d997695a9ee5ae2f0b10efdaf05fbf3c5a44b49c0b8c67eb3` |

### `venues/paradex/src/identity.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 31 / `finish` | `done` | IDENTITY | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` |
| 34 / `finish` | `fail` | IDENTITY | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |
| 53 / `fetch_identity` | `request` | IDENTITY | `d04c9c210338abd029de862338a9937ff19e38f96097cbfba0efb4bbf2728caf` |

### `venues/paradex/src/lib.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 184 / `on_request_failed` | `fail` | BOOK | `56d848e442eb914c45e3641ce58f354c9b36dd108d1f0bd57c97c4a58794d4a5` |
| 219 / `on_command` | `request` | BOOK | `47a71c977b3aa01bebe78866d791cab178725822d90756258925d00d0b297eaa` |
| 266 / `dispatch_failed` | `fail` | BOOK | `56d848e442eb914c45e3641ce58f354c9b36dd108d1f0bd57c97c4a58794d4a5` |

### `venues/paradex/src/public.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 135 / `public_seed` | `request_streamed` | PUBLIC | `daa9160114df450bc32e689954faee7fd2d88713b62477953faf17c6ac8e3ebf` |
| 882 / `public_failed` | `macro` | PUBLIC | `d5350d6b7eb9bb6083de59c2c42f6ec4de997ccda8d28c277c0803969d9f2a77` |
| 902 / `public_failed` | `fail` | CANDLE | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |
| 958 / `fetch_candles` | `request` | CANDLE | `094a464645386463fbc12779495700591cd0fc4570d7257183d9928fc29b60f5` |
| 966 / `fetch_candles` | `request_streamed` | CANDLE | `f4ff4cf72ac7033d5f05a949c3bebdf69840234f87e83ff025e63bdb7d7999b6` |
| 985 / `finish_candles` | `done` | CANDLE | `9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d` |
| 1213 / `public_command` | `fail` | CANDLE | `03068ad7442b394da421e7d1e0e04829b66c8bf11ddefd0dfdf69862ab1fa97f` |
| 1215 / `public_command` | `fail` | CANDLE | `da04770a10b1e89bc8090a23a76b5cb0ac39bd576ec7a35134c4bb8873d55eaf` |

### `venues/paradex/src/trading.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 480 / `command` | `fail` | NOIO | `276fd284f7ecc932e8d4430bf32c778c3de3e7a7f46113994c48402211e45d5e` |
| 493 / `command` | `fail` | TRADE | `4ada6c6855acc366bcebdec0d513cbd75e093c2d0370d90d63518324067264fa` |
| 650 / `command` | `fail` | NOIO | `6a0599e4c3ead752e1fac93edf85bc5bf64ee081d1300d7728344098cd86dd71` |
| 656 / `command` | `done` | NOIO | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` |
| 666 / `command` | `fail` | NOIO | `f80a7c4dd74d78d93f9fcaaf73728e8b73decaf64625a69627120abd7e21e2cf` |
| 671 / `command` | `done` | NOIO | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` |
| 689 / `command` | `fail` | TRADE | `f718cd9697a099abb6d65005f734a76de5469bbca2984f9fbaacd731245a1e56` |
| 714 / `command` | `fail` | TRADE | `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` |
| 725 / `advance` | `request` | TRADE | `7cf89c50bfeffaac54a620d887d14879fca5f73e4ff4f4a65b5b8be20f9c4677` |
| 757 / `advance` | `fail` | TRADE | `875f1a6284bd44b2cb2a0162e6552aa3452fb40a08d59ca048d6ac203ab4bd71` |
| 770 / `advance` | `fail` | TRADE | `875f1a6284bd44b2cb2a0162e6552aa3452fb40a08d59ca048d6ac203ab4bd71` |
| 777 / `advance` | `request` | TRADE | `e4578488db238a03f4d95805575986f0310d866963219d31eacc194020c4eb0e` |
| 800 / `advance` | `done` | TRADE | `6ac1d501912009ef984d7ec4253af4a40cf5f968674394d53b23cf8c1e2a8924` |
| 801 / `advance` | `fail` | TRADE | `1477aae8aee24cd4b02439b7b83caafb3d96bfa9c8e3f36a92a98019cc564283` |
| 805 / `advance` | `fail` | TRADE | `b4ba438587075e63cacec774fb985b089d5333b56a39444d8f95e98da3dd1d92` |
| 924 / `on_response` | `macro` | TRADE | `9742f5187f1c15e238ada18898fb2bbb42d37e2c3081630b27a002ffbc4e42c7` |
| 944 / `on_response` | `done` | LEVERAGE | `4310c56cdb4207e078977e5ecfb03800099cc6b8559b1e4cd4c0ef98cc4de727` |
| 945 / `on_response` | `fail` | LEVERAGE | `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` |
| 998 / `complete` | `macro` | TRADE | `1dbbda3e437f9c8cf16d7a9484f766e3e1e6caee6e87b49d901623b1ba0ca0ee` |
| 1011 / `complete` | `macro` | TRADE | `1dbbda3e437f9c8cf16d7a9484f766e3e1e6caee6e87b49d901623b1ba0ca0ee` |
| 1055 / `complete` | `macro` | TRADE | `5b8e2cf8afd1c07e2519c23ec48a6e71507c064e521e622302925fd78fdc2e59` |

## Independent plan review receipt

Source/adversarial, caller and full-map reviews are clean. The full-map reviewer
independently compared every path/line/function/kind/full fingerprint and
multiplicity to source and manifest, including all conservative matches. Both
request-context checks pass, including13 adversarial controls; whitespace checks
pass. Architecture gate: No for the proposed adapter adoption. This is planning
proof; implementation, CI and merge authorization remain separate gates.


## Independent book implementation checkpoint

Plan commit `f4301a83317`; regression commit `c3b433e5677`. All eight original
depth-query tests passed unchanged before source work. All five new regression
functions then failed at the explicit no-legacy-HTTP/no-send-before-claim
assertion; the migration passes them without changing that assertion. The
ordinary depth fixture adopts shared prepared-actions, verifies its new single
original20s timer, then retains every existing outbound/codec/error assertion.
The exact source/inventory prune matches the plan:62->57, lib3->0, depth3->1;
all other path maps remain identical. No held source/test path is edited.

Full-suite verification exposed two further existing legacy-draining book
fixtures: public live-SBE isolation and private JWT preservation. Their only
command-dispatch adaptation uses shared prepared-actions at the identical clock,
retaining every original request/body/state/error/sequence assertion and receipt
trace. The public fixture explicitly verifies exactly one new20s timer before
its original no-extra-actions assertion; the JWT fixture retains that timer.
This extends fixture scope to `tests/public.rs` and `tests/trading.rs`; neither
is held by an open PR, and held account/trading source remains untouched.

Final focused verification passes486 tests across38 Paradex/Binance suites,
strict all-target Paradex Clippy, scoped formatting, bans, both request-context
checks (13 adversarial controls) and whitespace checks. Source/adversarial and
caller follow-up reviews find the added fixture changes clean. Native codec,
protocol bodies, public routing and all existing classifier behavior stay intact.
CI and the overseer's merge window remain separate gates.

## Next slice: local history completions and validation

Plan checkpoint,2026-10-24, on actual landing
`c473342d48363a057c98c59292abb957462f3590`. This appendix supersedes the
historical holds and counts above for this slice. No production, test or
inventory change is included in this checkpoint.

### Inventory and collision refresh

The fingerprint/destination mapping above remains the family map with the
following recorded changes; the pinned manifest is the exact remaining multiset.
Current path counts are:

| Remaining path under `venues/paradex/src/` | Count | Destination |
| --- | ---: | --- |
| `account.rs` | 10 | CONFIG/AUTH/SNAPSHOT/SENDER |
| `depth_query.rs` | 1 | BOOK initial authority refusal |
| `history.rs` | 13 | HISTORY, private and local branches |
| `identity.rs` | 3 | IDENTITY |
| `public.rs` | 7 | PUBLIC/CANDLE |
| `trading.rs` | 21 | NOIO/TRADE/LEVERAGE |
| Total | 55 | |

The account orders SNAPSHOT fingerprint was rebound, count-neutral, from
`5c28b0ebae1f2a54f344d52c2c422801c8e82381c7fd8ad95d13cd549e88ddec`:1 to
`29a6633fa3f6e05f04f534c0e675690014adde1e8a26e93c3769e8618602c13c`:1.
Previously mapped occurrences already removed are depth numeric success
`7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b`:1
and one of its two `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa`
failures; history public sender
`a876e5300951dc42d8c9828d78f0d32d52e42b439cbaf763cc6cdd6b63174865`:1;
both lib failures `56d848e442eb914c45e3641ce58f354c9b36dd108d1f0bd57c97c4a58794d4a5`:2
and book sender `47a71c977b3aa01bebe78866d791cab178725822d90756258925d00d0b297eaa`:1;
public seed `daa9160114df450bc32e689954faee7fd2d88713b62477953faf17c6ac8e3ebf`:1.
Do not remove their remaining siblings in other paths or the retained depth
issuance refusal as part of this work.

Fresh open-PR file lists show no Paradex source/test collision. #1848 history
bridge and #1877 error-text change have landed. #1905 still holds GRVT
`account.rs`, `lib.rs` and account/history/identity/login/markets/products/trading
fixtures. #1866 holds Lighter trading and Hyperliquid signing/trading; #1852
also holds Hyperliquid signing/trading. Hyperliquid's candle fixture remains
held by #1857/#1914/#1923 despite #1877 landing. Own #1935 holds Lighter
`lib.rs` and its public/candle/depth/ticker fixture drivers. Recheck all file
lists before source or test edits. Shared documents and inventory permit
append-only updates and own-venue pruning under the standing integration rule.

The first plan checkpoint considered the four fixed-mode trading terminals.
Independent source review found that the proposed Mutation Context plus
`done_context` cannot preserve the two no-op `[]` successes: issuance declares
Mutation writes, and an empty CommandWrites journal rejects success. That
proposal is superseded. Shared engine owner was asked for a genuine no-op
Mutation completion capability; no fabricated accepted write, Read relabeling
or venue finalizer may bypass it. All four settings remain unchanged here.

Select instead four synchronous Read terminal occurrences at
`History::query` lines289,306,322,326. They need neither physical requests nor
Plan/history-walk authority. The larger candle family has two native GET
routes with klines-to-TradingView fallback and polling owners, requiring its
own Plan/authority audit. Paradex signing remains untouched while the remote
Stark payload-signer option is being decided.

### Native behavior and shared admission

`lib.rs::on_command` routes Query through `public_command` into
`History::query`. Preserve its ordered decisions before creating a private
legacy History Request:

1. PublicTrades always dispatches to the already-managed public consumer first,
   on either public-only or private-enabled construction. It must not be caught
   by a new local completion path.
2. Public-only non-PublicTrades returns exact `[]`, or exact
   `{"found":false,"definitive":false}` for OrderByClientId. This precedes client
   ID and time-range validation, so even invalid private queries retain their
   existing empty/non-definitive result when admission succeeds.
3. On private-enabled construction, OrderByClientId with empty ID, more than64
   Unicode scalar values, CR or LF fails with exact `invalid Paradex client
   order ID`, no financial outcome and no added error kind.
4. Fills/OrderHistory/Income rejects an explicitly non-finite or negative bound,
   or from greater than to, with exact `invalid Paradex history time range`,
   no financial outcome and no added error kind. Valid zero/equal bounds remain
   valid. This precedes the no-symbol Fills branch.
5. Private-enabled Fills without a symbol returns exact `[]` after bounds
   validation, before credential checks. It also works before authentication.
6. All other queries enter the existing private History Request unchanged;
   do not admit an otherwise unused root ahead of that branch.

Inside each selected local branch, before result/error publication, call
`Cx::admit_http_command(Policy)` with Operation::Read, no credential,20,000ms,
no pacing and RetryPolicy::Never. It borrows the genuine initial callback;
do not pass an ID, manufacture a Context, queue a request, construct a history
owner or create a venue completion wrapper. Entry failure returns immediately:
Rejected already emitted exactly one correlated engine denial; Revoked has no
completion authority. Use `done_context` or `fail_context` with the existing
native string. Handle the Result without unwrap or numeric fallback. Shared
entry and terminal APIs own capacity, custody and retirement.

Caller-visible change: these unconditional local results or validation errors
now require genuine command authority and valid clock policy. At capacity the
single engine-owned failure is `Engine request capacity reached`; invalid clock
admission gives the shared InvalidInput error. Read failures have no financial
outcome. With successful admission preserve the ordered decisions, exact JSON,
messages and metadata above. No Store/row/error-marker/account mutation,
outbound HTTP/sign/WS operation or command-specific timer is introduced.

### Original REDs, implementation and proof gates

1. Commit the reviewed plan before tests. Separately add
   `tests/request_context_local_history.rs` with real Engine command entry and
   existing support fixtures, without editing those fixtures. Both networks,
   public-only/private-enabled and pre-authentication construction are covered.
   Native controls pin all five Query variants and the precedence matrix above;
   compare raw JSON, exact error message/kind/outcome and Store/frame effects.
   Include64/65-character Unicode IDs, CR/LF, NaN/infinite/negative/reversed
   bounds, zero/equal bounds and valid no-symbol Fills.
2. Fill capacity with256 genuine parked managed book Reads at one unchanged
   clock; assert every Prepare exists and no request is claimed. Exercise
   each of the four selected source branches. Each must emit exactly one
   correlated shared capacity failure, no financial outcome, never the old
   result/native error. Record four original assertion REDs on unchanged source.
   Add invalid-clock admission controls using the current engine policy.
   Native controls must pass on the same source; fixture setup failures do not
   count as REDs. Independently review that test-only commit before source work.
3. Retire one real Read and rerun each branch to recover its exact native
   terminal; confirm parked peers remain live, no new outbound/timer effects
   appear and the freed slot remains available. Exercise300 sequential local
   queries at a fixed finite clock and post-completion ID reuse, then refill
   the full256-owner bound to expose leaks without expiry masking them. An
   ID owned by a parked Read must not finalize that Read; permit its existing
   canonical duplicate diagnostic. Preserve the managed PublicTrades tests
   unchanged, plus valid private history/lookup tests to prove fallthrough.
4. Change only the four local terminal branches in `src/history.rs`, using
   existing shared APIs, then run the same assertions unchanged. Prune exactly
   these path fingerprints, one occurrence each:

   | Full fingerprint | Native branch |
   | --- | --- |
   | `7e6ac08cf11a9f2f3a366e224d2c5df531cef78fd5b215d673548273ddd1fe50` | Public-only result |
   | `8153debcec98fa16c1dfc5c29c116571f0f4f5735aa73f778c9fafc32448211b` | Invalid client ID |
   | `55fe601b0e4d079ab998430cfbafaa7ea570dd124789ffb38859d78b00696d10` | Invalid bounds |
   | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | No-symbol Fills |

   Expected exact shrink: **Paradex55->51; history13->9**. Every other manifest
   path and fingerprint remains identical, including both trading no-op siblings
   with the same `2b6c` hash. No inventory edit belongs in plan or RED commits.
5. Scoped checks: `CARGO_BUILD_JOBS=2 cargo test -p safe-cex-venue-paradex -p
   safe-cex-venue-binance`, strict all-target Paradex Clippy, scoped cargo fmt,
   bans, `python3 sdk-rs/scripts/check-request-context.py` and
   `python3 sdk-rs/scripts/check-request-context.test.py`. First execute only the
   new target on original source to establish REDs. Full WASM/replay/timing
   gates run in CI; keep only one background120s watcher per PR.
6. Append landed caller behavior to CALLERS and exact native/RED/ratchet receipts
   to AUDITS/QUALITY and this plan. Independent source/adversarial, caller and
   full-map reviews must all be clean. Architecture gate: **No** proposed local
   cross-cutting mechanism: native Read facts use existing engine command entry
   and terminal APIs. Genuine actual-landing integration, green CI, an authorized
   train slot, fresh separate own-files/own-manifest-keys guard and head-matched
   merge remain mandatory. Combo-4 review duty may interrupt this planning slice.

### Local history original RED and source checkpoint

Original test-only commit `45c743903b8` was executed with production and
inventory exactly equal to actual `c473342d483`: three native/control tests pass
and five tests fail at the intended assertions (four capacity branches, one
clock aggregate). Independent source/adversarial, caller and full-map reviews
of that checkpoint are CLEAN, architecture gate No. The initial missing Fills
fixture `order_id` field was corrected before that commit; compilation failure
was not counted as a RED. The source adoption then passes all eight tests with
the committed RED assertions unchanged. It changes only the four local history
Read terminals and their four exact manifest occurrences. Full verification,
actual-landing integration, implementation reviews and CI remain separate gates.

Local history verification on reviewed integration `56dacc36562` genuinely
includes actual landing `23d0d490009` (#1892 background executions and #1817
credential selection). All513 Paradex/Binance tests pass across42 targets,
none ignored, including the same eight original local-history controls.
Strict all-target/all-feature Paradex Clippy, scoped formatting, bans, the
request-context scanner and all15 ratchet tests pass. Source/adversarial,
caller and full-map reviews are CLEAN on this actual integration; architecture
No. The seven-path diff preserves every unowned incoming blob and all foreign
manifest entries, with only the four planned Paradex history occurrences pruned.
CI and an authorized train slot remain separate gates.
