# GRVT RequestContext inventory and migration plan

Plan,2026-10-23; lane `codex-zs5e83`, overseer `claude-ddi2il`.
Pinned landing: `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`.

## Scope, holds and blockers

GRVT perpetuals on mainnet/testnet only; no future/options product expansion.
Open #1785-#1788 hold `src/account.rs` and `tests/account.rs`. Read them for this
inventory but do not edit them until the open PR ownership check clears.

Every public POST currently carries `Content-Type: application/json`. The
plain managed request API cannot declare headers, while the signed header seam
requires credential-bearing signing. The shared engine owner confirmed the
missing plain static-header extension. Wait for its reviewed pin; never drop
the header, fake credentials or add a venue transport/header escape.

The generic failure handler reissues book/candle/identity/private/history reads
up to five times. `tests/depth_query.rs` and the existing caller ledger explicitly
pin five book network reissues. A migration cannot silently choose Never or copy
that retry loop. Shared bounded read-attempt/Plan orchestration must preserve
those facts under one original deadline, or report a caller-contract decision.
Native/HTTP refusals remain distinct from retryable transport failure.

Catalog discovery is the smallest background HTTP candidate after blockers:
`fetch_markets` has one logical helper call and existing startup Backoff rather
than the immediate five-reissue policy. However `tests/account.rs` bootstraps
by draining legacy all_instruments HTTP directly, so this catalog slice also
waits for that held fixture to clear. No GRVT HTTP slice is currently unblocked.

## Destinations

| Code | Engine path and native facts retained |
| --- | --- |
| CATALOG | Managed background Read, public JSON POST, existing streamed catalog staging/remapping and startup Backoff; retire every terminal path. |
| BOOK | Command Read root and bounded shared retry attempts, exact admitted completion, typed public errors, server row order/cumulative-base totals, unknown-market empty no-I/O result and five network reissues. |
| CANDLE | Command Read root and shared bounded attempts; context completion, nanosecond request bounds, seconds output, sorted results and failed-prefix rejection. |
| AUTH | Shared session-producing auth/refresh ownership and credential recipe, joined waiters and original deadlines; preserve cookie/account-ID and session generation/header rotation. |
| SNAPSHOT | Authenticated managed background Reads plus existing revision/coverage/row admission. Summary discovers subaccount and releases startup; audit those observers first. |
| LEVERAGE | Distinct command/background Read authority, preserved settings revisions, missing-row failures and authenticated native rails. |
| BUILDER | Managed authenticated read producer with shared joined-preflight ownership; preserve existing10s attempt and builder state/retry behavior. |
| IDENTITY | Command Read Plan through login, auth1000 refresh and bounded network reissues; retain main/subaccount identity proof and context-bound diagnostic. |
| HISTORY | Checked Context-bearing CursorHistory/Completion and managed page legs; accepted-only recovery, stage reset, cursor/native limits and strict complete-fill/client-ID proofs remain. |
| TRADE | Shared command Plan with inspection Read and Mutation legs, dispatch EIP-712 recipe, nonce/expiry authority, central journal and bounded reconciliation; context finalizers replace Plan::effective aggregation. |
| NOIO | Issued completion authority for fixed mode, unsupported options/commands and other no-I/O branches; no synthetic transport/mutation evidence. |
| SENDER | Keep generic legacy physical send sites while any caller is legacy; method is POST but operation class derives from originating logical work. |

## Phases and regressions

1. Commit and independently review the complete inventory before source work.
2. Once public headers and held account fixture clear, test catalog RED first:
   exact JSON body/header/public route, preparation/bind/claim, no publication
   before release, typed denial and original-deadline expiry, failed streamed
   prefixes and late replies, recovery through existing startup Backoff, held
   market rows, leverage/signing metadata and ticker remapping. Adapt fixtures
   through shared prepared-actions, preserving every original assertion.
3. Migrate only the logical catalog helper (`lib.rs:337` at this pin), leaving
   generic physical sender sites for remaining jobs. Exact expected prune is
   **56->55**, `lib.rs` **19->18**. Retire on success, malformed/empty result,
   stream/preparation/transport failure and timeout. Do not edit held fixtures
   to make this phase possible before their PRs land.
4. Book/candle/identity slices follow shared bounded retry and original-plan
   authority. Preflight empty/invalid results use shared command admission;
   constructor refusal remains inventoried until the engine entry seam exists.
5. Session credentials and observer completion wait on their shared cores.
   Histories also await `codex-cxxyqr`'s checked Context-bearing owner. Native
   EIP-712 signing goes to `claude-19pt8s`, nonce/expiry to `claude-m26ps4`, and
   Plan/entry/public headers to `codex-uufub4` and the header owner. Signing is
   currently performed in planning, before transport readiness. Do not reuse
   pre-signed expiring bodies or add venue retry/outcome machinery.

Update (claude-55ao7t, 2026-10-23): the AUTH login moves onto the session
core, stacked on #1897. Inventory prune: **56->55**, `account.rs` **12->11**.
The joined waiters stay venue-side until their reads are managed. See
[GRVT API-key login](RUST_BACKGROUND_SESSION.md#grvt-api-key-login-stacked-on-1897-cleared-by-codex-zs5e83).

## Caller-visible contract to document with implementation

Catalog adoption requires prepare/bind/claim support in custom worker/hosted
transports, and one original bound including preparation/body streaming. Preserve
perpetual-only catalog scope, usable-market readiness, five-minute refresh,
private startup sequencing, settings/signing metadata and shared ticker indexes.
Book snapshots retain SDK method/shape, native ID request, depth50 and cumulative
base totals; unknown markets return an owned empty book and network failures
keep the existing five-reissue policy until an explicit shared replacement.
Candles retain interval/native timestamp bounds and sorted seconds output.
Future session/write migrations must keep builder authorization errors actionable,
partial identities and Unknown reconciliation, without claiming NX deployment.

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

**56 occurrences / 53 distinct path fingerprints.**
Every row is one occurrence (count1); repeated fingerprints retain their
full multiplicity. All paths are relative to `sdk-rs`. No `sign_http` sites
exist at this pin; that does not prove dispatch-time signing adoption.

| Path | Occurrences | Distinct |
| --- | ---: | ---: |
| `venues/grvt/src/account.rs` | 12 | 12 |
| `venues/grvt/src/depth_query.rs` | 3 | 2 |
| `venues/grvt/src/history.rs` | 11 | 11 |
| `venues/grvt/src/identity.rs` | 3 | 3 |
| `venues/grvt/src/lib.rs` | 19 | 19 |
| `venues/grvt/src/trading.rs` | 8 | 6 |

### `venues/grvt/src/account.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 831 / `login` | `request` | AUTH | `3bb318de066bd3fa3f0411954d9ea99e36f90139ed03470beabf24628b1f2b28` |
| 1045 / `summary` | `request` | SNAPSHOT | `b2484206c084e77bdc1752247eb37ee88a35a6844eee77dc2d8d8076872cde71` |
| 1057 / `refresh_orders` | `request` | SNAPSHOT | `4af7a34a26fa068b8b111070a3d7acd3609e74ff418c63314de028697c099f13` |
| 1079 / `refresh_positions` | `request` | SNAPSHOT | `b038db835bf8ea1ec66ac21973347c06e0589312a876cb038495794bc4271cd0` |
| 1101 / `refresh_leverage` | `request` | LEVERAGE | `d96e9e03d819f6c10890e79fbfe0b40b14b08b64f9e311649fd8da33c4111cc5` |
| 1120 / `fetch_leverage_command` | `fail` | LEVERAGE | `5dcc3ef2d5d2fc61a53a8e99f5df87d9a5678caece10fe1966cb422429db3b54` |
| 1124 / `fetch_leverage_command` | `fail` | LEVERAGE | `1a500310d6ed84a8168e03b0b0e5ad5c7c62303e2788fa0bdbc19c57d193e16e` |
| 1129 / `fetch_leverage_command` | `request` | LEVERAGE | `190dc4fe24cd734ffc175ff98e58994db3f141d0443164b8dd1a5d9200125834` |
| 1708 / `account_response` | `done` | LEVERAGE | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` |
| 1709 / `account_response` | `fail` | LEVERAGE | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |
| 1794 / `send_builder_check` | `request` | BUILDER | `6aacde9b4fe06f78fb74e0ffb2c12f8facb0863949c2daf6afff48c82298ce59` |
| 1887 / `account_failed` | `fail` | LEVERAGE | `2b1d9411d14819314659a0f77a4c9f2787fd17e551adb57d5ca62d67877a7d37` |

### `venues/grvt/src/depth_query.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 154 / `book_query_response` | `fail` | BOOK | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |
| 178 / `book_query_response` | `done` | BOOK | `7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b` |
| 179 / `book_query_response` | `fail` | BOOK | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |

### `venues/grvt/src/history.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 370 / `start_query` | `fail` | HISTORY | `d714610c15915805a059c4d8796cf1954a2908722189ac570a2c127a55963ee7` |
| 403 / `start_query` | `fail` | HISTORY | `625ef3cfd881af1a997b7e309af2712fabf1013668ca5ca403d4a59913d01d4d` |
| 408 / `start_query` | `fail` | HISTORY | `9d3eea4ddeb346d10f8ebebfbc4dbbcde5939983ebcb9e2530b7c7e718be6e17` |
| 416 / `start_query` | `fail` | HISTORY | `0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb` |
| 421 / `start_query` | `fail` | HISTORY | `83b8e68cd8445af7475b790f03a56ef5d72018274c5416a79fcab14f7f58cd7d` |
| 429 / `start_query` | `fail` | HISTORY | `43e76c2e596d56215f4ecd2f1cd300980b527f24c83ec67fbc6a19d6e78c81b9` |
| 434 / `start_query` | `fail` | HISTORY | `0bc1914737ba4f26b1f4a43a7db513f0c98aafaa24bd482b6cb5b2e21ef9d704` |
| 522 / `history_send` | `request` | HISTORY | `5806755b2e4365a0cce4c02f6f5a15ec7fab6beaa6b0218ae5ccfa2fb93a8892` |
| 831 / `history_response` | `done` | HISTORY | `b075739674bf080c2089eea7c5fb0d89b2aea115d27b532da2ff55f1c32280c1` |
| 832 / `history_response` | `fail` | HISTORY | `5694172a506b48e37a66cc80e38560361ec75b97447d4a687ae99811e66fd516` |
| 926 / `history_rejected` | `fail` | HISTORY | `706e12027f633d2c581b43201426e9539f4b9ecd03159693d03873420a41cd61` |

### `venues/grvt/src/identity.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 25 / `finish` | `done` | IDENTITY | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` |
| 28 / `finish` | `fail` | IDENTITY | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |
| 61 / `fetch_identity` | `request` | IDENTITY | `4f71a50a9bb95a943c3dcc1007169dbeb5a0ae6dfadd7a6d5844792e1df7214a` |

### `venues/grvt/src/lib.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 304 / `send_request` | `request_streamed_body` | SENDER | `f3480e96608be14fe36ce5e8a4f738b47d6488334a3d7f62295a6f0379cfe305` |
| 306 / `send_request` | `request` | SENDER | `37f10b45c2b2138eda25c5042d0405e3e994a042185fdeffbd4896279c57c341` |
| 315 / `fail_pending` | `fail` | BOOK | `56d848e442eb914c45e3641ce58f354c9b36dd108d1f0bd57c97c4a58794d4a5` |
| 333 / `fail_pending` | `fail` | CANDLE | `bbffa0d44e45c58fed0b39052085ed7a7ecf1397f07e2a0f0c74ec6f46c799e0` |
| 337 / `fetch_markets` | `request` | CATALOG | `7f179c914a5bbc8e6e8e7df1c90d04c0c10713495d542a0b5c719caf776b76c1` |
| 922 / `on_response` | `macro` | HISTORY | `e07ada86cc9703007c75cb7854613a6f0e06595d48f73b548db979e1357746f3` |
| 952 / `on_response` | `macro` | TRADE | `a7249d615c64308b35ec03f88b7ecb8075fdbd1bd83eade5bdf461c97141c8f7` |
| 1057 / `on_response` | `fail` | CANDLE | `404800ae5ea295a0b28c142788234954e28c718092cb4ee05447d1e73637cb47` |
| 1061 / `on_response` | `done` | CANDLE | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` |
| 1062 / `on_response` | `fail` | CANDLE | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` |
| 1201 / `on_command` | `request` | BOOK | `082e65c085e6390bc5eb9c332a6bc6dd468d96f4f136e3003644d6dcd5e0a84a` |
| 1203 / `on_command` | `done` | BOOK / NOIO | `e94d7911021caad57d2f3de37251a010ea404564da6e9354762ca01da4c1e7cf` |
| 1238 / `on_command` | `fail` | NOIO | `cab66fc0399e6d1f53504198a9a5df6893df31bad853bf84c1dcd416c2326cdd` |
| 1322 / `on_command` | `fail` | CANDLE | `8de1f3b8296eca111f95dc5251fd9805c744427986bfda443db1acba0afa3a57` |
| 1326 / `on_command` | `fail` | CANDLE | `5747f4f0f812ad946e05c62b4962875b86150f15195e2646df84bdd14daa8254` |
| 1346 / `on_command` | `fail` | CANDLE | `33b6337863c22b02eb7b7f5b01c23e4945054300ace4a5dc3733f38870fa5aa5` |
| 1358 / `on_command` | `request` | CANDLE | `fcb6fd9f79e3c04dc198cdce23f06880483b61e0f38fc7d7ba5615c93df602e5` |
| 1377 / `on_command` | `fail` | NOIO | `bfcdb38d1528a6090a59eb123fc288423fd2ce7a45cf8179d786b7dafb8a5b2c` |
| 1385 / `on_command` | `fail` | NOIO | `d378cf6b03a38047ea106b9344672d153498eb2f0f1aa48f79efcdfeb037c3f9` |

### `venues/grvt/src/trading.rs`

| Line / function | Kind | Destination | Full fingerprint |
| --- | --- | --- | --- |
| 1240 / `trade_command` | `fail` | NOIO | `405d7ebc1c7223ef83cbd56fd92d92ad62d607670337bb8d2251e2db36573d85` |
| 1246 / `trade_command` | `fail` | NOIO | `405d7ebc1c7223ef83cbd56fd92d92ad62d607670337bb8d2251e2db36573d85` |
| 1249 / `trade_command` | `done` | NOIO | `a24b9b2de4710bfdf73ca18d6eb8acc8b639384210b9e939355717d398d2288b` |
| 1258 / `trade_command` | `fail` | TRADE | `fbafcc48aaedbe70a22d054c16acc0858b261af3d5f711681c39d0e866da7e30` |
| 1301 / `trade_next` | `fail` | TRADE | `fbafcc48aaedbe70a22d054c16acc0858b261af3d5f711681c39d0e866da7e30` |
| 1367 / `trade_step` | `request` | TRADE | `fbede0a47e0e356cdd283dca1fff0b6dc1c5762e047963add336e0869ee07b1b` |
| 1412 / `trade_finish` | `fail` | TRADE | `b780b823ba70712169e1be98fecae4d2a2655f7ca35f27bea4b99f09bd684696` |
| 1414 / `trade_finish` | `done` | TRADE | `d6abf86165880819d2e37443be1918451a6c2b07fb046edbc4503019bdc79eff` |

## Independent plan review receipt

Source/adversarial, caller and full-map reviews are clean. The full-map reviewer
independently compared every path/line/function/kind/full fingerprint and
multiplicity to source and manifest, including all conservative matches. Both
request-context checks pass, including13 adversarial controls; whitespace checks
pass. Architecture gate: No for the proposed adapter adoption. This is planning
proof; implementation, CI and merge authorization remain separate gates.
