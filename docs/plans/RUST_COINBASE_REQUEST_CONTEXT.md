# Coinbase CFM and Spot RequestContext inventory and plan

Lane `codex-hcmq3f`, overseer `claude-ddi2il`, 2026-10-23. Immutable source base: `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`. Inventory: **6 source paths, 31 path/fingerprint entries, 34 conservative lexical occurrences**. This is a plan, not implementation, CI, live connectivity or cutover proof.

## Scope and first single-request path

Preserve the currently supported Advanced Trade CFM/Spot products and saved-account profiles. INTX fixtures are compatibility evidence, not renewed product support. Keep exact product identities, settings/units, primary trading-key checks and secondary account/history credential selection.

Public command candles in src/public.rs are one request, but all REST currently passes through rest.rs and its 334 ms queue pacing. A managed candle may proceed only by declaring the same shared actual-release pacing group used by all relevant REST consumers, or through an owner-provided coordinated seam. Bypassing the queue or pacing only managed candles while legacy siblings race is not equivalent.

## Collision and dependency gate

Before editing, open PR file lists were checked with `gh pr list --base feat/rust-rewrite --state open --limit 100 --json number,title,files,headRefName`. Source/test collisions below remain off-limits until actual landing; recheck before every slice. Shared docs/index and exact venue-only inventory prunes are permitted by the overseer.

| Open PR | Occupied source/test files |
| --- | --- |
| #1799 | `sdk-rs/venues/coinbase/src/lib.rs`; `sdk-rs/venues/coinbase/tests/support/history_family.rs` |

Coherent REST send-group pacing is a shared-core prerequisite: codex-uufub4/claude-fe7lgm, not a second Coinbase queue. #1799 owns lib.rs and tests/support/history_family.rs. JWT/public derivation and remote wire are signer-owned by claude-19pt8s. Error conversion/classification gaps go to claude-dazzy3. Staged products/history and coalesced daily publication require shared plan/group ownership.

No shared mechanism is implemented in this crate. Error-kind/code gaps go to `claude-dazzy3`; signer ownership to `claude-19pt8s`; group fan-out/observer joins to `claude-fe7lgm`; listen keys/session ownership to `claude-55ao7t`. Shared bounded Plan/preflight admission remains coordinated with `codex-uufub4`. An unsigned static-header seam is owned by `codex-5c57jl` and is not yet landed. Never turn signed/session work into public Reads, omit headers, manufacture key refs, or add a legacy fallback.

## Engine-path map

Use actual landed Context/Lifecycle and Cx APIs; prospective Call/Admission internals are not public venue types. Context issuance, attempt identity, final-release admission, retry eligibility, deadline and outcome policy stay in the engine. Named future APIs below are requirements, not claims that they exist.

| Destination | Engine boundary |
| --- | --- |
| READ | One command-owned semantic Read; managed prepare/bind/final claim, exact root/leg delivery validation before result/diagnostic publication, then contextual completion. Keep native parsing facts and configured retry budget under the immutable original bound. |
| BACKGROUND | Explicit background Read contexts and engine scheduler/release admission; retain native row/cache/freshness/generation facts and retire with finish_http_context. A multi-request or shared-observer publication must use its declared shared plan/group, not independent completion owners. |
| HISTORY | Keep existing HistoryRows/CursorHistory/Completion row staging and accepted-only output; add context-bearing completion and one declared root/plan across pages/stages. No venue pager/marker/finalizer or numeric fallback once authority exists. |
| WRITE | Shared preflight/Plan admission, managed signed mutation legs, engine journal/native write facts and bounded reconciliation. Local code may choose native continuation/stop behavior, never aggregate release/outcome or renew the root. |
| DISPATCH | Replace the native helper and underlying legacy sender together after all consumers declare their true Read/Mutation/Session semantics and credential route. Do not simply rename a helper while opaque HTTP remains. Static public headers need the real unsigned-header API. |
| SESSION | Background authentication/credential acquisition belongs to the shared session and provider-custody owners. Never label it Read or manufacture a command; link dependent work through the shared authority. |
| LOCAL | Use reviewed shared preflight/no-I/O completion for cache/fixed-settings/empty/unsupported paths. No dummy Context, timer, accepted fact or numeric completion bridge is a zero-inventory solution. |
| LEXICAL | Conservative macro/field match, not another HTTP call. Preserve native codec meaning; a legitimate descriptor-binding migration may remove this frozen fingerprint. Do not weaken scanner, rehash changed legacy calls or introduce exemptions. |
| TEST | Conservative inline-test macro match. Keep every original oracle/header/nonce assertion. Rename only the fixture binding if needed after real managed helper adoption; never delete the test to reach zero. |

## Committable order

1. FIRST candidate: public candles, only after coherent shared pacing admission is available. Preserve /api/v3/brokerage/market/products/{product}/candles, exact granularity/start/end/limit, native symbol mapping, streamed list and original result/row failure. lib.rs is occupied by #1799, so any required Pending or callback change there waits for actual landing.
2. Public discovery/product pages and daily seeds follow. Product cursors/stages and a daily seed fanning into several live intervals require the shared plan/group owner where applicable; keep catalog completeness and public-only startup suppression/queue behavior.
3. After signer adoption, convert account and standalone identity reads with exact primary/secondary JWT identities, provider-owned PEM/CDP material, nonce/claims and raw expiry projection. Cached FetchAccount completion in lib.rs remains a shared no-I/O preflight path, not a dummy HTTP Context.
4. Existing CursorHistory accepted-page ownership remains; bind managed Completion and declared CFM/Spot stages/pages to one original root. Preserve strict preparation refusal, native fingerprints, fills/order/public-trade filtering and no publication of failed prefixes.
5. Migrate trading queue, cancellation proof reads and cancel/replace through shared Plan and central journal. Preserve original root budget through pacing, dynamic legs, signed retries and financial freshness. Remove rest.rs legacy sender only when every public/private/trading consumer is migrated; preserve lexical macro facts with no scanner exemption.

Every implementation area gets a committed plan, a separate original-source assertion-RED regression commit, then adoption and assertion-GREEN proof. Keep one venue or venue area per PR. Queue refusals retain the continuation; after authority issuance every failure/result uses that Context, never a numeric fallback. A truly pre-issuance refusal may retain its existing path until the shared local-completion adoption; all remaining inventory still must reach zero.

For every physical read retry preserve native counters/query bytes but require engine retry evidence under one immutable root. Existing single-read transport bounds (20 seconds unless explicitly configured otherwise) are captured once, not renewed on retry, route/signing, page or delay. Child bounds are capped exactly by the original logical root; no rounding-up extension. Multi-request plans and shared/observer completion remain explicit dependencies rather than independent roots. Native status/body/list proof and row/snapshot freshness are still required after exact delivery validation.

## Caller-visible contract

Preserve successful candle JSON, capped native request bounds and row errors. Preparation, pacing and retries now consume one original deadline; request-only custom transports fail typed. Preserve account credential routing and native CFM/Spot unit behavior. Later central write errors/partial reconciliation IDs require caller handling through worker/hosted transports; no Starbase/INTX feature enablement or new history capability. Command candles have zero automatic retries. Retain limit clamping to 1..300, explicit from and current seconds formatting. Preserve REST queue capacity 256 and coherent 334-ms pacing. Primary key-permission checks and trading proof reads remain primary, distinct from secondary account/history routing. Local leverage reads return cached buy/sell rails or reject; leverage/margin/mode mutations remain unsupported.

Each code PR appends a CALLERS section describing only its resulting behavior to `RUST_CUTOVER_CALLERS.md`. Record immutable-deadline and managed-transport requirements, typed failure/uncertainty and retained native schemas; distinguish local-operation `notSent` from proof about earlier physical sends or the whole command. Preserve existing native read error fields, including message-only paths where applicable; any classification/outcome normalization requires separate review. Do not add mutation outcomes to pure reads. No default/product/capability change is authorized by this inventory.

Downstream write-error targets remain the audited order send/create/update/cancel and scaled-order flows plus worker/hosted error receivers in the retained caller ledger. Preserve kind/outcome/native/partial metadata and one terminal notification; unknown writes and reconciliation-only IDs never authorize automatic replay. This lane changes no upstream checkout and claims no NX adoption/deployment or live credentials.

## Verification and independent review

Use `CARGO_BUILD_JOBS=2`. Code gates: focused `cargo test -p safe-cex-venue-coinbase -p safe-cex-venue-binance`, scoped fmt and strict all-target Clippy, bans, and both `scripts/check-request-context.test.py` / `scripts/check-request-context.py`. Do not run local WASM release builds, replay sweeps, load or timing gates; CI owns those proofs. Yarn runs through existing NVM Node 22 Corepack.

Original-RED controls must exercise real Prepare/bind/final claim (no early HTTP), public/primary/secondary routing and exact bytes, typed pre-send denial versus network failure, original deadline while parked/retrying, duplicate/late/reused command isolation and retained native successful/malformed/empty results. Preserve existing fixture assertions/event clocks and use shared prepared fixture glue. Validate streaming failed-prefix exclusion and row/freshness effects for any changed reducer; preserve known partial/unknown/no-replay behavior for writes. Host disposal belongs to existing shared host controls, with any missing proof requested from its owner rather than a venue-owned executor.

Independent source/adversarial, caller and full-map reviews must answer: **is this a per-venue fix of a cross-cutting concern?** Expected **No**: native semantics plus shared core adoption. Any missing common mechanism stops that area for its core owner. Check every actual fingerprint before pruning; new/changed legacy calls and exemptions are forbidden. CI must remain watched in a background `gh pr checks --watch`; green CI, clean reviews, actual dependency integration, separate revert guard and an overseer merge window are required.

Documentation checkpoint verification compares this exact map with the unchanged lexical scanner/manifest and checks index registration. It proves neither source migration nor native/CI gates. Future landing changes, especially the occupied PRs above, require a fresh inventory; preserve all incoming repairs and reapply only this venue's exact prune.

## Plan review receipt

Independent source/adversarial, caller and full-map reviews checked all seven family maps against the immutable base and unchanged scanner: 36 paths, 221 entries, 247 occurrences. Caller corrections (structured error fields, exact retry counts, settings and routing) and Polymarket background-candle ownership were incorporated before this commit. All three architecture gates: **No**. This receipt covers the plan only; implementation requires new independent reviews.

## Exact frozen inventory

All locations and source SHA256s refer to the immutable base, including inline tests. Each row is one manifest path/fingerprint entry; a repeated fingerprint in another path is distinct. Locations list every occurrence and its native handler/destination; lexical macro rows are not physical sends.

### `venues/coinbase/src/account.rs`

Source SHA256: `83f04a77fb5b624952e1ab873f16393dd9a49df45d64f126736b5c4cb135b1e6`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `339bef5311673edab54f33f82bfdbbdf053c7abffec9e341f0c0449783eb9235` | 1 | 932 `account_read` → BACKGROUND | `self.request( cx, Method::Get, path, None, at, Pending::Account(req, false), )` |

### `venues/coinbase/src/history.rs`

Source SHA256: `ceb4d6169fb34f3eafa861c3996cc1b9a4a407ed14a0d8c8a3f7d8319c9bb82a`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb` | 1 | 318 `history_start` → HISTORY | `cx.fail(command, e)` |
| `8479aab65a3ff3e83282a3c83d7480252f2d58a32b4a1b3c5434c1d3866caa11` | 1 | 405 `history_read` → HISTORY | `self.request(cx, Method::Get, path, None, Some(at), Pending::History(req))` |
| `76998fe42e6c09630ff50ba44385ed309566a7c45e0b90d584ccbf7534cc0d6e` | 1 | 408 `history_read` → HISTORY | `cx.fail( command, VenueError::new(e) .kind(ErrorKind::Internal) .outcome(Outcome::NotSent), )` |

### `venues/coinbase/src/lib.rs`

Source SHA256: `da97d687e782ea9aa416c218e02f729b1cc8f7dab487e2a09e07d906211ec43f`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | 187 `on_command` → LOCAL | `cx.done(command, data)` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | 188 `on_command` → LOCAL | `cx.fail(command, error)` |

### `venues/coinbase/src/public.rs`

Source SHA256: `0357a27e6382c3b519d11a5591569519ab1e844ff779674bf11100259bebbe74`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `3d0b652732f625e52ffcca8e7d59f829243e0666aa727667ad5588e05ec8b6e5` | 1 | 239 `instruments` → BACKGROUND | `self.request( cx, Method::Get, INTX.into(), None, Some(ArrayAt::Root), Pending::Public(pending), )` |
| `b3b3162818184bb70a2236f515aab53d96a7409de7ec05abe9966cae3914dca7` | 1 | 297 `products_page` → BACKGROUND | `self.request( cx, Method::Get, path, None, Some(ArrayAt::Field("products")), Pending::Public(pending), )` |
| `a7a039a5f67c9622fe5c45051ebd95a0b70818fb43f77e586184a00a8b5420f1` | 1 | 472 `daily_seed` → BACKGROUND | `self.request( cx, Method::Get, path, None, Some(ArrayAt::Field("candles")), Pending::Public(pending), )` |
| `c9775ae4ce7dc43d5bd630b290b9d28de894516e3c32821afefe5b5724785cc4` | 1 | 1054 `public_response` → READ | `cx.fail(command, "Malformed Coinbase candles")` |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | 1058 `public_response` → READ | `cx.done(command, data)` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 3 | 1059 `public_response` → READ; 1072 `public_failed` → READ; 1564 `public_command` → LOCAL | `cx.fail(command, error)` |
| `60b349f5e304a70150b0df808e057d528d77e7f2b4e016c37416a7626a3a83e1` | 1 | 1404 `public_command` → LOCAL | `cx.fail(command, "Coinbase public startup command queue is full")` |
| `e347074f96f429e946a8dd06ad52ec6468d87d13ecf9b0de549424317be7a975` | 1 | 1530 `public_command` → LOCAL | `cx.fail(command, timeframe.unsupported("Coinbase"))` |
| `2ff357e755f38d4c8de909f3b5b32d2c683cb185942daa383330a3f4cc0d5532` | 1 | 1534 `public_command` → LOCAL | `cx.fail(command, "Unknown Coinbase market")` |
| `e7eb9e6b80ae49514fa1e48353268acf5121932b10de4f05cdbd6ced0cc8f58c` | 1 | 1542 `public_command` → LOCAL | `cx.fail(command, "Invalid Coinbase candle range")` |
| `5b61837541fd16a65da9a352d4a7548ff62ba987d3c80a5b9837d4b138e43ed8` | 1 | 1552 `public_command` → READ | `self.request( cx, Method::Get, path, None, Some(ArrayAt::Field("candles")), Pending::Public(Request::Candles { comman...` |

### `venues/coinbase/src/rest.rs`

Source SHA256: `bdf37a19d78d7ec90b1b4e655ccc398a1c0472646dbf7de065795d4b3495d003`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `3fa81ac71523a7b891447ad724997c5c20f6a631052c8a00f4d91013552d35dd` | 1 | 68 `dispatch` → LEXICAL + DISPATCH | `matches!( request.pending, Pending::Account( crate::account::Request::KeyPermissions { trading_portfolio: None }, _ ) )` |
| `a77bee10a9c0761ddbd5fe3a58636cabc28caded9abae529ea4861b8b8634cd8` | 1 | 81 `dispatch` → LEXICAL + DISPATCH | `matches!(request.pending, Pending::Account(..) \| Pending::History(_))` |
| `dcf83acae473b7e34bc8c4f078d94f2ca5fbf999114c7f8e5af43f956237699b` | 1 | 124 `dispatch` → LEXICAL + DISPATCH | `format!("https://api.coinbase.com{}", request.path)` |
| `388c9f66c40d488788f2f4a9808088e22a0425615dfd2818c9d63d58675392d1` | 1 | 126 `dispatch` → DISPATCH | `cx.request(request.method, url, headers, request.body, request.pending)` |

### `venues/coinbase/src/trading.rs`

Source SHA256: `6e2e13fab751a623e3364777378e3342a14bcbe4a5cb7d0b908630f9e6ae1432`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `bd1bdc3f1623f5957810ab1aeb3584ede2872387055a238ea00e3d22cd37f5fc` | 1 | 1113 `trading_command` → LOCAL | `cx.fail(id, not_sent("Coinbase does not support account leverage, margin or position-mode mutations").kind(ErrorKind:...` |
| `263d0619925b412640098c8ec05ba6f5b8211f6e45026f36e4e3a1fd07934a15` | 1 | 1124 `trading_command` → LOCAL | `cx.done( id, encode(&Leverage { buy_leverage: setting.leverage, sell_leverage: setting.leverage, }), )` |
| `479442329d5a2562b05c949ca383d819acffc77d8541e1296588b558d4200b00` | 1 | 1132 `trading_command` → LOCAL | `cx.fail( id, VenueError::new("Coinbase market settings are unavailable"), )` |
| `414b9feff64a0a0bcbcc856631dcf20976a5797d25a40a434f3b710206300bee` | 1 | 1147 `trading_command` → LOCAL | `cx.fail(id, not_sent("Coinbase account is not ready for trading"))` |
| `d6aab93d8c0593f58c17fbc2fba81bb939f0d498ba488ca2bac1d11287e68324` | 1 | 1152 `trading_command` → LOCAL | `cx.fail(id, not_sent("Coinbase trading queue is full"))` |
| `2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5` | 1 | 1160 `trading_command` → LOCAL | `cx.fail(id, not_sent(error))` |
| `ca541fc3d8e6a395306b4e3a9502878ed34272567357cdc4ea87ff10063e54bb` | 1 | 1273 `trading_next` → WRITE | `cx.done( job.command, if job.collect { encode(&job.ids) } else { "null".into() }, )` |
| `e81908cb0fae07346d55748a9e80db822d8af6006cc38fbc4aaaef11041c0283` | 1 | 1367 `trading_next` → WRITE | `self.request(cx, method, path, body, None, Pending::Trading(request))` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | 1394 `trading_finish_error` → WRITE | `cx.fail(command, error)` |
| `c6af5062b90ca20c0fe8e6184bfb2a01dae165494623878deef5314fbced6e8e` | 2 | 1471 `trading_response` → LEXICAL + WRITE; 1487 `trading_response` → LEXICAL + WRITE | `matches!(request.step, Step::Confirm { .. })` |


## Catalogue failure containment before managed adoption

Execution-group owner `claude-fe7lgm` reported a production bug (execution-group inventory section B, #1821): `public.deferred` keeps FetchCandles until catalogue success, while `public_failed` retries discovery without settling those callers. Verified against the immutable source: only `public_pending` drains the deferred queue, and it runs on success.

Separate focused fix branch `fix/rust-coinbase-deferred-candles`: retain subscription commands for discovery retry, remove each deferred FetchCandles on catalogue failure and emit one typed Internal dependency-unavailable error with the original failure as native evidence. Use the existing public failure handler/completion site; add no sender, deadline, retry scheduler, status ladder or managed-pacing bypass. The existing frozen completion fingerprint remains unchanged; full managed command custody still requires coherent shared pacing and collision clearance.

First commit regressions for catalogue transport failure, non-2xx and malformed/empty successful catalogue, multiple waiters and no duplicate terminal on later catalogue recovery. Then implement only `src/public.rs`, a dedicated `tests/deferred_candles.rs`, this appendix and caller notes. `lib.rs`/history capability work stays off-limits under #1799. Run Coinbase/Binance native suites, scoped fmt/strict Clippy, bans and unchanged ratchet controls; obtain source/adversarial, caller and full-map reviews. Architecture question: this repairs native deferred-queue failure handling and adopts existing error/completion types; shared custody/pacing remains with its owners.


Failure-containment proof: `f1210ee4cd5` contains four original-source controls that compile and fail with zero terminals for queued callers. `fc6ba222247` adds supported Spot failure/empty recovery and a real native `private_ready` generation transition; the empty and stale cases reproduced the incremental gaps before correction. The test-only generation wrapper imports no credentials and exposes no production API. All six integration cases plus the stale-generation control pass after the fix. Source/caller/full-map reviews required empty aggregate validation and current-generation fencing; both corrections are incorporated. Coinbase inventory remains exactly 6 paths / 31 entries / 34 occurrences; no manifest or REST/lib source changes.


Final local containment gates pass: full Coinbase/Binance native suites, strict all-target Clippy, formatting, bans and both request-context ratchet checks. Final source/adversarial, caller and full-map reviews are clean, architecture gate **No**. No relevant finding exists in the current audit/quality ledgers to strike; execution-group owner will link this fix from its section B. CI, actual landing integration/revert guard and overseer merge window remain required.
