# TxFlow testnet perpetuals RequestContext inventory and plan

Lane `codex-hcmq3f`, overseer `claude-ddi2il`, 2026-10-23. Immutable source base: `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`. Inventory: **4 source paths, 29 path/fingerprint entries, 30 conservative lexical occurrences**. This is a plan, not implementation, CI, live connectivity or cutover proof.

## Scope and first single-request path

Testnet only: preserve explicit mainnet refusal, account/vault identity, exact instrument IDs and native node/API-service routes. POST /info operations are semantic reads; POST /exchange is mutation. Do not infer class or credential route from HTTP method.

Single command candleSnapshot POST /info is a public Read. It goes through lib.rs info() and carries Content-Type:application/json then x-api-version:2026-08-16. Use the overseer-authorized reviewed #1820 stack for the unsigned managed-header API; do not replace either header or invent signing permission.

## Collision and dependency gate

Before editing, open PR file lists were checked with `gh pr list --base feat/rust-rewrite --state open --limit 100 --json number,title,files,headRefName`. Source/test collisions below remain off-limits until actual landing; recheck before every slice. Shared docs/index and exact venue-only inventory prunes are permitted by the overseer.

| Open PR | Occupied source/test files |
| --- | --- |
| #1794 | `sdk-rs/venues/txflow/src/history.rs`; `sdk-rs/venues/txflow/tests/history.rs` |
| #1788 | `sdk-rs/venues/txflow/src/account.rs`; `sdk-rs/venues/txflow/tests/account.rs` |
| #1787 | `sdk-rs/venues/txflow/src/account.rs`; `sdk-rs/venues/txflow/tests/account.rs` |
| #1786 | `sdk-rs/venues/txflow/src/account.rs`; `sdk-rs/venues/txflow/tests/account.rs` |
| #1785 | `sdk-rs/venues/txflow/src/account.rs`; `sdk-rs/venues/txflow/tests/account.rs` |

ACTUAL LANDING PENDING; reviewed #1820 stack authorized: unsigned request_managed_with_headers, owned by codex-5c57jl. Need Content-Type:application/json then x-api-version:2026-08-16. #1794 owns history; #1785-#1788 own account/tests. Signer/public derivation and the HL/TxFlow shared nonce domain coordinate with claude-19pt8s; no process-global venue substitute. Group fan-out is claude-fe7lgm, kind gaps claude-dazzy3.

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

1. FIRST after static-header seam: lib.rs fetch_candles/info command branch and new tests/request_context_candles.rs. Preserve exact serialized coin/interval/startTime/endTime, native asset resolution, range validation and streamed data array. Keep actual public Read intent despite POST.
2. Public metadata/ticker reads and per-asset background snapshots follow through the same actual header API. Keep generation/catalog admission and native asset identity; multi-request fan-out or one owner for several nodes needs shared plan/group admission.
3. Account settings reads in account.rs are occupied by #1785-#1788. Integrate actual incoming row/snapshot repairs before adopting managed read contexts; preserve native prior-state retention and market rails. Do not copy row policy or add a per-venue retry clock.
4. History is occupied by #1794. Preserve its actual indexed-page/income-stage owner changes, then replace legacy terminal paths with context-bearing Completion and shared declared indexed/page plans. Keep strict native completeness, lookup-not-definitive and unsupported public-trades behavior.
5. Private /exchange actions wait for signer/provider nonce-domain work shared with Hyperliquid. Then adopt shared Plan/journal, per-leg native facts and bounded reconciliation without local outcome demotion. Account identity/settings local completions and unsupported-method refusals use reviewed no-I/O preflight; no fake network work or new capability.

Every implementation area gets a committed plan, a separate original-source assertion-RED regression commit, then adoption and assertion-GREEN proof. Keep one venue or venue area per PR. Queue refusals retain the continuation; after authority issuance every failure/result uses that Context, never a numeric fallback. A truly pre-issuance refusal may retain its existing path until the shared local-completion adoption; all remaining inventory still must reach zero.

For every physical read retry preserve native counters/query bytes but require engine retry evidence under one immutable root. Existing single-read transport bounds (20 seconds unless explicitly configured otherwise) are captured once, not renewed on retry, route/signing, page or delay. Child bounds are capped exactly by the original logical root; no rounding-up extension. Multi-request plans and shared/observer completion remain explicit dependencies rather than independent roots. Native status/body/list proof and row/snapshot freshness are still required after exact delivery validation.

## Caller-visible contract

Preserve testnet-only refusal, candle shape/raw millisecond bounds, API version and both node/feed routes. Typed preparation failures do not become ordinary network retry, and all attempts consume one original bound. Query unsupported/empty/incomplete semantics remain as actually landed. Later central mutation outcomes and exact reconciliation IDs must survive hosted/worker replies without replaying uncertain actions. Command candleSnapshot has zero automatic retries and retains RequestRoute::PublicCors, exact header order and node/API feed routes. FetchAccount is unsupported/notSent, not cached successful identity. Local FetchLeverage requires confirmed buy/sell rails and optional marginType, otherwise rejects. Shared header seam #1820 is reviewed at cb4b508b569; overseer explicitly authorizes stacking exactly on that reviewed head, followed by actual-landing reintegration. It is not yet landed.

Each code PR appends a CALLERS section describing only its resulting behavior to `RUST_CUTOVER_CALLERS.md`. Record immutable-deadline and managed-transport requirements, typed failure/uncertainty and retained native schemas; distinguish local-operation `notSent` from proof about earlier physical sends or the whole command. Preserve existing native read error fields, including message-only paths where applicable; any classification/outcome normalization requires separate review. Do not add mutation outcomes to pure reads. No default/product/capability change is authorized by this inventory.

Downstream write-error targets remain the audited order send/create/update/cancel and scaled-order flows plus worker/hosted error receivers in the retained caller ledger. Preserve kind/outcome/native/partial metadata and one terminal notification; unknown writes and reconciliation-only IDs never authorize automatic replay. This lane changes no upstream checkout and claims no NX adoption/deployment or live credentials.

## Verification and independent review

Use `CARGO_BUILD_JOBS=2`. Code gates: focused `cargo test -p safe-cex-venue-txflow -p safe-cex-venue-binance`, scoped fmt and strict all-target Clippy, bans, and both `scripts/check-request-context.test.py` / `scripts/check-request-context.py`. Do not run local WASM release builds, replay sweeps, load or timing gates; CI owns those proofs. Yarn runs through existing NVM Node 22 Corepack.

Original-RED controls must exercise real Prepare/bind/final claim (no early HTTP), public/primary/secondary routing and exact bytes, typed pre-send denial versus network failure, original deadline while parked/retrying, duplicate/late/reused command isolation and retained native successful/malformed/empty results. Preserve existing fixture assertions/event clocks and use shared prepared fixture glue. Validate streaming failed-prefix exclusion and row/freshness effects for any changed reducer; preserve known partial/unknown/no-replay behavior for writes. Host disposal belongs to existing shared host controls, with any missing proof requested from its owner rather than a venue-owned executor.

Independent source/adversarial, caller and full-map reviews must answer: **is this a per-venue fix of a cross-cutting concern?** Expected **No**: native semantics plus shared core adoption. Any missing common mechanism stops that area for its core owner. Check every actual fingerprint before pruning; new/changed legacy calls and exemptions are forbidden. CI must remain watched in a background `gh pr checks --watch`; green CI, clean reviews, actual dependency integration, separate revert guard and an overseer merge window are required.

Documentation checkpoint verification compares this exact map with the unchanged lexical scanner/manifest and checks index registration. It proves neither source migration nor native/CI gates. Future landing changes, especially the occupied PRs above, require a fresh inventory; preserve all incoming repairs and reapply only this venue's exact prune.

## Plan review receipt

Independent source/adversarial, caller and full-map reviews checked all seven family maps against the immutable base and unchanged scanner: 36 paths, 221 entries, 247 occurrences. Caller corrections (structured error fields, exact retry counts, settings and routing) and Polymarket background-candle ownership were incorporated before this commit. All three architecture gates: **No**. This receipt covers the plan only; implementation requires new independent reviews.

## Exact frozen inventory

All locations and source SHA256s refer to the immutable base, including inline tests. Each row is one manifest path/fingerprint entry; a repeated fingerprint in another path is distinct. Locations list every occurrence and its native handler/destination; lexical macro rows are not physical sends.

### `venues/txflow/src/account.rs`

Source SHA256: `21c1f41fdc65a8d6db25b175d05a708daa7249181eb3d20f8ed48dac30e7ad05`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `433b39542c0b7e8d83e35d3e897cccc64377df2f741272c2b8a3ec51f571b6e1` | 1 | 445 `next_setting` → BACKGROUND | `cx.request( safe_cex_engine::Method::Post, format!("{}/info", self.rest()), vec![ ("Content-Type", "application/json"...` |

### `venues/txflow/src/history.rs`

Source SHA256: `64fc82adb4364c2d6347db12723da960bffb7e66e53a689c4a6237d92bf05062`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `4ce12e59632e0254941c73d324e69a3bef963c397abfc608ebe870235b641dc0` | 1 | 257 `query` → HISTORY + LOCAL | `cx.fail(command, "TxFlow does not document a REST public trades history query; use the trades stream")` |
| `77063f1696715727b1107911eca58c91a8e282f3c1b578a7d9e58d9fa3cea620` | 1 | 261 `query` → HISTORY + LOCAL | `cx.fail(command, "TxFlow indexed lookup cannot prove non-placement")` |
| `068adec2da4da377173d27e066eb8ffa88210dd66f4f2b2edcdfdc958b8ff833` | 1 | 266 `query` → HISTORY + LOCAL | `cx.fail(command, "TxFlow testnet account identity required")` |
| `7e5f7cce4d3dd3910df81bb17391ba5d98da90d41edc9618285356c19fbcfcf2` | 1 | 276 `query` → HISTORY + LOCAL | `cx.fail(command, "Invalid TxFlow history range or limit")` |
| `3b4922943e648f98c7f1a8e506f8b096ba20593bf0a402d2e5f4e9be02add303` | 1 | 283 `query` → HISTORY + LOCAL | `cx.fail(command, "Unknown or ambiguous TxFlow history symbol")` |
| `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | 315 `query` → HISTORY + LOCAL | `cx.done(command, "[]".into())` |
| `cc943120cc2b7c1c28be1abb81a50c06608bf80d01dcd2d198a2b1aa4493a7c4` | 1 | 322 `history_page` → HISTORY + LOCAL | `cx.fail(request.command, "TxFlow account identity required")` |
| `db15e9f8a226656cfdc25258e389cf082535f04165d5e96ecf9cbb0b3f16368b` | 1 | 585 `history_response` → HISTORY + LOCAL | `cx.fail(r.command, e)` |
| `0a896649413e8442d28120953169602e686bd84c3cad5b0d30cd2e5cc3f7530b` | 1 | 600 `history_response` → HISTORY + LOCAL | `cx.fail( r.command, "Incomplete TxFlow history: stalled page or offset cap", )` |
| `50bc93ae81e29acbce7f2fbb2bdcb2303967d0c4c91799d80152db776812e2a4` | 1 | 637 `history_response` → HISTORY + LOCAL | `cx.done( r.command, format!( "[{}]", r.rows .iter() .map(\|r\| r.json.as_str()) .collect::<Vec<_>>() .join(",") ), )` |
| `d5e898e07cc7c2b9f7927c0f9a7f462214e004fdec265ff4f3ed0370a6db1a81` | 1 | 650 `history_failed` → HISTORY + LOCAL | `cx.fail(r.command, error)` |

### `venues/txflow/src/lib.rs`

Source SHA256: `1d714af31eb766436f92c2890d5789cb8bfb1099721761582b4c571d78f5417a`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `17292f7e8ba96b8ca495e3a6d66f3451c5c670f2658c3ccb7117b49878c06dd0` | 1 | 207 `info` → DISPATCH | `cx.request_streamed_body( Method::Post, format!("{}/info", self.rest()), vec![ ("Content-Type", "application/json".in...` |
| `4acf3c28534e3da6c67ffe837e424df6be41024ef99ee653ab4d68994a510f42` | 1 | 235 `ticker_request` → BACKGROUND | `cx.request( Method::Post, format!("{}/info", self.rest()), vec![ ("Content-Type", "application/json".into()), ("x-api...` |
| `4108d1559a4ad73e52be4c21b463683f0720640e6139a19d3dc124fc94a3a924` | 1 | 643 `fetch_candles` → LOCAL | `cx.fail(command, "Market not found")` |
| `f4dd040ec082c0c3b8b9c27264a6cf19fc8219ed5674849ebf0f81bb43e36d90` | 1 | 647 `fetch_candles` → LOCAL | `cx.fail(command, interval.unsupported("TxFlow"))` |
| `ae0c28ce939a28c6614ef199cbb7eae2c3061d25690063d605ed713d389341ae` | 1 | 661 `fetch_candles` → LOCAL | `cx.fail(command, "Invalid candle time range")` |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | 1076 `on_response` → READ | `cx.done(command, data)` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | 1077 `on_response` → READ; 1114 `on_request_failed` → READ | `cx.fail(command, error)` |

### `venues/txflow/src/trading.rs`

Source SHA256: `b4c769a3981e26ec1a469b99a79826f0886d42cf6ab8d99f2f6c92579d4fd4eb`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | 289 `trade_command` → LOCAL | `cx.done(command, data)` |
| `c4612a7b4d1c083729fc59c1c33a5f317d07427b0efadbfa3887a061ea21bbfd` | 1 | 290 `trade_command` → LOCAL | `cx.fail(command, VenueError::new(error))` |
| `e60d4faaec87616724c424c27deebd0b50a9a0a5627364b78cebd550ffc19b6b` | 1 | 295 `trade_command` → LOCAL | `cx.fail(command, not_sent("TxFlow account query completion is not implemented; use live account state").kind(ErrorKin...` |
| `45e81b21770b8faa80ec515808227739b4bc380064e497743d8adc24d6de8515` | 1 | 299 `trade_command` → LOCAL | `cx.fail( command, not_sent( "TxFlow order book queries are not implemented; use the book subscription", ) .kind(Error...` |
| `a67b9a30c64359f4529e3120be03e6bf5370c74a831f321ea8d94a23f8a064dd` | 1 | 309 `trade_command` → LOCAL | `cx.fail( command, not_sent("TxFlow does not document configurable self-trade prevention") .kind(ErrorKind::Unsupporte...` |
| `cd2a9d4a22e59e9867995f9e74cddddbaa44e863c21892754d59db3a4e6e7cc8` | 1 | 325 `trade_command` → LOCAL | `cx.fail( id, not_sent("TxFlow trading requires an authenticated testnet account").kind(kind), )` |
| `2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5` | 1 | 332 `trade_command` → LOCAL | `cx.fail(id, not_sent(error))` |
| `5da66136fae2982361da17d1fe866e3dfd910ad30333a4eeee8a1e3fc8bd4899` | 1 | 740 `pump_trades` → WRITE | `cx.request( Method::Post, format!("{}/exchange", self.rest()), vec![ ("Content-Type", "application/json".into()), ("x...` |
| `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` | 1 | 878 `finish_trade` → WRITE | `cx.fail(job.command, error)` |
| `604f28d91adfdb4cd0caac06047abd9569f9a6c248b698a784032256d7cf8047` | 1 | 880 `finish_trade` → WRITE | `cx.done(job.command, serde_json::to_string(&job.ids).unwrap())` |
