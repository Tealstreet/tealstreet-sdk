# Polymarket CLOB V2 RequestContext inventory and plan

Lane `codex-hcmq3f`, overseer `claude-ddi2il`, 2026-10-23. Immutable source base: `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`. Inventory: **4 source paths, 28 path/fingerprint entries, 35 conservative lexical occurrences**. This is a plan, not implementation, CI, live connectivity or cutover proof.

## Scope and first single-request path

Development-only prediction CLOB shares, native local-wallet signature types and existing L2 credentials. Preserve limit/GTC-only restrictions, no caller IDs/chaser support, fixed settings and unsupported generic histories. Public book queries and bounded sampled price history are independent native reads.

Headerless GET /book?token_id=... in history.rs is a single request-owned public read; bounded /prices-history in lib.rs is another candidate. Both require Pending/response dispatch changes in lib.rs, occupied by #1800. Do not work around that by adding an external state map or editing occupied tests/history.rs.

## Collision and dependency gate

Before editing, open PR file lists were checked with `gh pr list --base feat/rust-rewrite --state open --limit 100 --json number,title,files,headRefName`. Source/test collisions below remain off-limits until actual landing; recheck before every slice. Shared docs/index and exact venue-only inventory prunes are permitted by the overseer.

| Open PR | Occupied source/test files |
| --- | --- |
| #1800 | `sdk-rs/venues/polymarket/src/lib.rs`; `sdk-rs/venues/polymarket/tests/history.rs` |
| #1786 | `sdk-rs/venues/polymarket/src/account.rs`; `sdk-rs/venues/polymarket/tests/account.rs` |

#1800 occupies lib.rs/tests/history.rs; #1786 occupies account.rs/tests/account.rs. Public read candidates wait for dispatcher collision clearance. Signer/provider-held L2 import and remote wire are claude-19pt8s; acquisition/session boundary claude-55ao7t, with explicit owner decision if runtime signing-key import cannot fit. Group/plan fan-out goes to claude-fe7lgm; error-kind/status gaps to claude-dazzy3. No exemptions or relabeling credential acquisition as Read.

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

1. FIRST after #1800 actual landing: public book query, then bounded price-history/candle query, with new dedicated tests. Preserve token/asset echo validation, sorted sides, duplicate-price rejection, cumulative totals, sampled time bucket rules and no streaming-book mutation.
2. Public market discovery and POST /midpoints batches require explicit public Read contexts. Preserve pages/offsets and scope; declare a bounded plan or shared group for multi-request discovery/midpoint fan-out. Do not mark POST /midpoints as a mutation. Background candle subscription/polling shares fetch_candles and must preserve its separate generation/freshness/publication owner.
3. Private account/time/positions are occupied by #1786; integrate actual row repairs first. Server-time reads must feed the shared clock boundary. L1 /auth/derive-api-key is credential acquisition, not a plain Read or background mutation shortcut; provider-held L2 import/session ownership must be coordinated.
4. L2 authenticated reads and order/cancel writes wait for signer P0/P6b/P8 decisions and provider-held key import. Preserve selected signer/funder type, wallet/L2 credential separation and exact V2 typed-data bytes. Group cancel-replace and central mutation facts retain final-fill safety and known effects.
5. All cached account/fixed leverage/settings/empty-placement and unsupported query branches use the shared no-I/O completion seam. Preserve actual #1800 capability removals and explicit unsupported query results; generic empty results must not acquire invented history completeness. Lexical pruning follows actual adopted code only.

Every implementation area gets a committed plan, a separate original-source assertion-RED regression commit, then adoption and assertion-GREEN proof. Keep one venue or venue area per PR. Queue refusals retain the continuation; after authority issuance every failure/result uses that Context, never a numeric fallback. A truly pre-issuance refusal may retain its existing path until the shared local-completion adoption; all remaining inventory still must reach zero.

For every physical read retry preserve native counters/query bytes but require engine retry evidence under one immutable root. Existing single-read transport bounds (20 seconds unless explicitly configured otherwise) are captured once, not renewed on retry, route/signing, page or delay. Child bounds are capped exactly by the original logical root; no rounding-up extension. Multi-request plans and shared/observer completion remain explicit dependencies rather than independent roots. Native status/body/list proof and row/snapshot freshness are still required after exact delivery validation.

## Caller-visible contract

Public book and price-history output stays within current sampled/native limits; managed transport and immutable deadline become required. Preserve prediction/holding metadata, live books and fixed 1x settings. Do not infer new history/chaser/client-ID support; #1800 removes misleading capabilities. Credential acquisition must not leak raw L2 secret bytes to the venue. Later write uncertainty/partial IDs must preserve caller reconciliation and prohibit automatic replay. Command book and price-history have zero automatic retries. Book limit=0 refuses; preserve asset echo, cumulative totals and Store isolation. Preserve price-history upper-only/from-under-1000 refusals, unbounded interval=max and local output limiting. Fixed setting setters succeed with [], leverage reads return 1x, generic histories reject and client lookup remains nondefinitive. fetch_candles also serves background subscription/polling reads; later migration must retain the background owner, freshness and cx.candle publication separately from command completion.

Each code PR appends a CALLERS section describing only its resulting behavior to `RUST_CUTOVER_CALLERS.md`. Record immutable-deadline and managed-transport requirements, typed failure/uncertainty and retained native schemas; distinguish local-operation `notSent` from proof about earlier physical sends or the whole command. Preserve existing native read error fields, including message-only paths where applicable; any classification/outcome normalization requires separate review. Do not add mutation outcomes to pure reads. No default/product/capability change is authorized by this inventory.

Downstream write-error targets remain the audited order send/create/update/cancel and scaled-order flows plus worker/hosted error receivers in the retained caller ledger. Preserve kind/outcome/native/partial metadata and one terminal notification; unknown writes and reconciliation-only IDs never authorize automatic replay. This lane changes no upstream checkout and claims no NX adoption/deployment or live credentials.

## Verification and independent review

Use `CARGO_BUILD_JOBS=2`. Code gates: focused `cargo test -p safe-cex-venue-polymarket -p safe-cex-venue-binance`, scoped fmt and strict all-target Clippy, bans, and both `scripts/check-request-context.test.py` / `scripts/check-request-context.py`. Do not run local WASM release builds, replay sweeps, load or timing gates; CI owns those proofs. Yarn runs through existing NVM Node 22 Corepack.

Original-RED controls must exercise real Prepare/bind/final claim (no early HTTP), public/primary/secondary routing and exact bytes, typed pre-send denial versus network failure, original deadline while parked/retrying, duplicate/late/reused command isolation and retained native successful/malformed/empty results. Preserve existing fixture assertions/event clocks and use shared prepared fixture glue. Validate streaming failed-prefix exclusion and row/freshness effects for any changed reducer; preserve known partial/unknown/no-replay behavior for writes. Host disposal belongs to existing shared host controls, with any missing proof requested from its owner rather than a venue-owned executor.

Independent source/adversarial, caller and full-map reviews must answer: **is this a per-venue fix of a cross-cutting concern?** Expected **No**: native semantics plus shared core adoption. Any missing common mechanism stops that area for its core owner. Check every actual fingerprint before pruning; new/changed legacy calls and exemptions are forbidden. CI must remain watched in a background `gh pr checks --watch`; green CI, clean reviews, actual dependency integration, separate revert guard and an overseer merge window are required.

Documentation checkpoint verification compares this exact map with the unchanged lexical scanner/manifest and checks index registration. It proves neither source migration nor native/CI gates. Future landing changes, especially the occupied PRs above, require a fresh inventory; preserve all incoming repairs and reapply only this venue's exact prune.

## Plan review receipt

Independent source/adversarial, caller and full-map reviews checked all seven family maps against the immutable base and unchanged scanner: 36 paths, 221 entries, 247 occurrences. Caller corrections (structured error fields, exact retry counts, settings and routing) and Polymarket background-candle ownership were incorporated before this commit. All three architecture gates: **No**. This receipt covers the plan only; implementation requires new independent reviews.

## Exact frozen inventory

All locations and source SHA256s refer to the immutable base, including inline tests. Each row is one manifest path/fingerprint entry; a repeated fingerprint in another path is distinct. Locations list every occurrence and its native handler/destination; lexical macro rows are not physical sends.

### `venues/polymarket/src/account.rs`

Source SHA256: `eb3c13ec036a646f8fc97192f3bab315bf918966577fcf0a4ccab3a42a138e15`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `56e99f37347afd5a3125760ebe88af363da636585bb23c2106b573894f2450dc` | 1 | 284 `signed_request` → DISPATCH | `cx.request(method, format!("{REST}{path}"), headers, body, pending)` |
| `766f5ee44b7b25802cc85973a468fc872f0d83bbef8320e067ed436d4985244b` | 2 | 394 `start_private` → BACKGROUND; 548 `account_timer` → BACKGROUND | `cx.request( Method::Get, format!("{REST}/time"), vec![], None, Pending::Account(Request::Time), )` |
| `9abef505f5f9a1664cdae8b3c966edfebd4748240d5855c984067457d1b77499` | 1 | 429 `account_issue` → BACKGROUND | `cx.request_streamed( Method::Get, url, vec![], ArrayAt::Root, Pending::Account(Request::Positions(snapshot)), )` |
| `3d7ff71d016e5adcb86b686890425c0fc0777ed3e15fb49d43910e0ef97553b7` | 1 | 791 `account_response` → SESSION | `cx.request( Method::Get, format!("{REST}/auth/derive-api-key"), vec![ ("POLY_ADDRESS", address.into()), ("POLY_SIGNAT...` |

### `venues/polymarket/src/history.rs`

Source SHA256: `d626fa3b91ba82ffc662ce4c5eec6ac7c3347f03280ff4506c0c72971a78d65f`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `945f9a041721fe973e19a8c7736f8128940587e73a19d29ba2a84a9a8065c02e` | 1 | 70 `read_command` → LOCAL | `cx.fail(command, "Invalid Polymarket order-book limit")` |
| `bd42ab82df9fed6e5ae5dba11b55151c1a9c284d315f49f96a038d5097197544` | 1 | 74 `read_command` → LOCAL | `cx.fail(command, "Unknown Polymarket symbol")` |
| `d3d58e81772917b4f53ec556d28f5ea1cee4f22f903db842b60888fe48bff001` | 1 | 78 `read_command` → READ | `cx.request( Method::Get, format!("{}/book?token_id={}", crate::REST, crate::encode(&token)), vec![], None, Pending::B...` |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 3 | 104 `read_command` → LOCAL; 139 `read_command` → LOCAL; 179 `book_response` → READ | `cx.done(command, data)` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 3 | 105 `read_command` → LOCAL; 140 `read_command` → LOCAL; 180 `book_response` → READ | `cx.fail(command, error)` |
| `61edeb21b19d7d3397781984067b45fd9091f1e927cd1942f2eea9b42f4dd0ad` | 1 | 110 `read_command` → LOCAL | `cx.fail( command, "Polymarket self-trade prevention settings are unsupported", )` |
| `f834c044840718f11259ba57d11fb7e57a30239e1585cf37c0571b64d6f3d484` | 1 | 116 `read_command` → LOCAL | `cx.done(command, r#"{"buyLeverage":1,"sellLeverage":1}"#.into())` |
| `d8877910dd4935bc942e96557431f8cb07f3f22f072bc567edd25ba564f4dcf9` | 1 | 118 `read_command` → LOCAL | `cx.done(command, "1".into())` |
| `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | 121 `read_command` → LOCAL | `cx.done(command, "[]".into())` |

### `venues/polymarket/src/lib.rs`

Source SHA256: `0aa79f4df24290df446a603ed034388fe23c6a8bc7f91017dcaa745301595caf`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `3c2518cd3efa9ec044158c6dd5008cc36722ff550877663df294c8c8c2d55621` | 1 | 237 `markets` → BACKGROUND | `cx.request_streamed( Method::Get, url, vec![], ArrayAt::Field("markets"), Pending::Markets { count: 0, rows, metadata...` |
| `2f8cf761894919a113431881453a07440eca0a5c424ff04fe48e592b62a33a8b` | 1 | 321 `midpoints` → BACKGROUND | `cx.request( Method::Post, format!("{REST}/midpoints"), vec![("Content-Type", "application/json".into())], Some(body),...` |
| `7257e88bed7f0d58ba127a75cfb161582d721862e09978f16249884f06a9f49e` | 1 | 640 `fetch_candles` → READ + BACKGROUND | `cx.request_streamed( Method::Get, path, vec![], ArrayAt::Field("history"), Pending::Candles { command, symbol, interv...` |
| `0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb` | 1 | 659 `fetch_candles` → READ | `cx.fail(command, e)` |
| `982f8f0f4b02233b21556691e476824408f6ea831a0ebd301bff2d1c9fdd76da` | 1 | 953 `on_response` → READ | `cx.fail(c, "Invalid Polymarket price history")` |
| `6cb9a1e96c67a077e2d60646f5ef1786c7274adad1b681cbcddadb9317b35947` | 1 | 978 `on_response` → READ | `cx.done(c, data)` |
| `f06dce80ebd266653d22bf24f105c411c21c1de8a011cd3d6fe688214267e373` | 2 | 979 `on_response` → READ; 1132 `on_request_failed` → READ | `cx.fail(c, error)` |
| `cab66fc0399e6d1f53504198a9a5df6893df31bad853bf84c1dcd416c2326cdd` | 1 | 1232 `on_command` → LOCAL | `cx.fail(command, "Options are not enabled on this account")` |
| `40062be4474042e41b637a57fd5647a9f5db174340233694f3e8de781b16c177` | 1 | 1312 `on_command` → LOCAL | `cx.fail(command, "Symbol-scoped amendment is unsupported")` |

### `venues/polymarket/src/trading.rs`

Source SHA256: `50b40c457be9e31962a274a1a4ff5900122d1ec566e5a7438ffc887e7db11d4f`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `df3c1d81952c8a455f966081ae19fcca364d48093c5893e69a82108cac16906d` | 1 | 330 `trading_finish` → WRITE + LOCAL | `cx.done(command, result)` |
| `69079e4a19d7256b68a25a10c7f12140a2b4229ab7411ecf4894bd5a511774fb` | 1 | 338 `trading_finish` → WRITE + LOCAL | `cx.fail(command, error.partial(&job.ids, job.total, job.ids.len()))` |
| `5a10d25b95cc023a50c9ad506fe37e87a89e8d0e1f0e12f669daead116de0099` | 1 | 431 `trading_command` → WRITE + LOCAL | `cx.fail(command_id, "Polymarket does not support clientOrderId")` |
| `81a211396ad294f6b186e78e613ecfa112d1b4452823ff28eadb89ec37be690f` | 1 | 435 `trading_command` → WRITE + LOCAL | `cx.fail(command_id, "Polymarket command is already pending")` |
| `6c356bc987bef30ec8929663a2849ca69e0f04b910a2e4838a3d947c8a96b3ec` | 2 | 439 `trading_command` → WRITE + LOCAL; 443 `trading_command` → WRITE + LOCAL | `cx.done(command_id, "[]".into())` |
| `5b02bb983a5e994687c385c6cbe7676982daf9cf00a067f01bf5aa35a2a29067` | 1 | 632 `trading_command` → WRITE + LOCAL | `cx.fail(command_id, not_sent(error))` |
