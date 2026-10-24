# Kraken futures and Spot RequestContext inventory and plan

Lane `codex-hcmq3f`, overseer `claude-ddi2il`, 2026-10-23. Immutable source base: `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`. Inventory: **8 source paths, 46 path/fingerprint entries, 53 conservative lexical occurrences**. This is a plan, not implementation, CI, live connectivity or cutover proof.

## Scope and first single-request path

Two distinct products in one crate: Kraken Futures and Kraken Spot. Preserve futures one-way account/contract units, Spot form-auth and v2 WS-token protocol, existing client-ID refusals, row retention, native symbol aliases and independent public/private generations.

Kraken Spot single-response OHLC in src/spot/mod.rs. GET /0/public/OHLC has no auth/header, preserves pair/interval/since bytes and current complete native result. Its transport budget is explicitly 20,000 ms with three zero-delay scheduled transport retries; the new immutable root includes all preparation/retry time. Do not turn its native last cursor into a new pager or invent support for ignored query options.

## Collision and dependency gate

Before editing, open PR file lists were checked with `gh pr list --base feat/rust-rewrite --state open --limit 100 --json number,title,files,headRefName`. Source/test collisions below remain off-limits until actual landing; recheck before every slice. Shared docs/index and exact venue-only inventory prunes are permitted by the overseer.

| Open PR | Occupied source/test files |
| --- | --- |
| #1798 | `sdk-rs/venues/kraken/src/spot/book.rs`; `sdk-rs/venues/kraken/tests/spot_book_crc32.rs` |
| #1797 | `sdk-rs/venues/kraken/src/spot/trading.rs`; `sdk-rs/venues/kraken/tests/spot_trading.rs` |

Kraken Spot OHLC has no signer/header/group prerequisite. Typed context errors require actual #1808 or a separately authorized reviewed stack. Futures paging/multi-leg work waits for shared plan admission; group/observer completion goes to claude-fe7lgm. Kraken HMAC signer coordination routes through claude-19pt8s and the existing codex-5c57jl HMAC owner; WS-token/session ownership is claude-55ao7t.

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

1. FIRST: Spot OHLC only: src/spot/mod.rs plus a new tests/spot_request_context_candles.rs and only necessary shared fixture glue. Keep spot/book.rs and spot/trading.rs untouched while #1798/#1797 own them. Retain unsupported-interval preflight as a separate local-completion tail until the shared seam lands.
2. Public background discovery/ticker reads in futures lib.rs and Spot mod.rs follow as individual managed background reads. Keep native publication/retry/generation facts. Futures candle queries and background candle polls are pageable (2,000 bars/page, MAX_CANDLE_PAGES=10); they need one declared bounded plan/root across pages, not one new command per page.
3. Futures history already has shared CursorHistory/Completion; adopt context-bearing completion and signed/public managed legs without replacing its strict continuation, accepted-page, ledger or closed-order-fold proofs. Spot inert history remains occupied by #1797 until actual landing; keep its branded unsupported/incomplete contract.
4. After signer ownership, migrate futures account signed helpers and Spot private form requests. Spot GetWebSocketsToken is an authenticated background session acquisition, not a Read or fake command; coordinate claude-55ao7t. Preserve explicit primary/secondary identity and separate futures/Spot nonce/signature domains.
5. Trading and settings use shared Plan/preflight and central mutation evidence, including leverage prerequisite reads, batches/amendment legs and known effects. Migrate remaining no-I/O refusals/results through reviewed local completion; convert only lexical fixture bindings after actual helper adoption, preserving all signature/header assertions.

Every implementation area gets a committed plan, a separate original-source assertion-RED regression commit, then adoption and assertion-GREEN proof. Keep one venue or venue area per PR. Queue refusals retain the continuation; after authority issuance every failure/result uses that Context, never a numeric fallback. A truly pre-issuance refusal may retain its existing path until the shared local-completion adoption; all remaining inventory still must reach zero.

For every physical read retry preserve native counters/query bytes but require engine retry evidence under one immutable root. Existing single-read transport bounds (20 seconds unless explicitly configured otherwise) are captured once, not renewed on retry, route/signing, page or delay. Child bounds are capped exactly by the original logical root; no rounding-up extension. Multi-request plans and shared/observer completion remain explicit dependencies rather than independent roots. Native status/body/list proof and row/snapshot freshness are still required after exact delivery validation.

## Caller-visible contract

Preserve Spot OHLC shape/order/units, native pair mapping and unsupported intervals; record required managed transport and the single 20-second deadline. Futures page/history output and current incomplete/strict refusal distinctions remain. Later writes must preserve partial IDs, cancellation effects, final-fill safety and uncertain ACK semantics; do not infer identical product capabilities from the shared crate. Spot OHLC retains ignored limit/to_ms, floor-seconds since, ignored native last cursor, whole-response malformed-row rejection and sorted output. Its three zero-delay retries mean four total attempts; keep engine-owned root authority during the timer gap. Spot FetchLeverage returns null and settings refuse; Futures uses a signed leverage read and fixed OneWay no-op/hedged refusal. Futures history strict continuation refusals remain strict even after admitted pages.

Each code PR appends a CALLERS section describing only its resulting behavior to `RUST_CUTOVER_CALLERS.md`. Record immutable-deadline and managed-transport requirements, typed failure/uncertainty and retained native schemas; distinguish local-operation `notSent` from proof about earlier physical sends or the whole command. Preserve existing native read error fields, including message-only paths where applicable; any classification/outcome normalization requires separate review. Do not add mutation outcomes to pure reads. No default/product/capability change is authorized by this inventory.

Downstream write-error targets remain the audited order send/create/update/cancel and scaled-order flows plus worker/hosted error receivers in the retained caller ledger. Preserve kind/outcome/native/partial metadata and one terminal notification; unknown writes and reconciliation-only IDs never authorize automatic replay. This lane changes no upstream checkout and claims no NX adoption/deployment or live credentials.

## Verification and independent review

Use `CARGO_BUILD_JOBS=2`. Code gates: focused `cargo test -p safe-cex-venue-kraken -p safe-cex-venue-binance`, scoped fmt and strict all-target Clippy, bans, and both `scripts/check-request-context.test.py` / `scripts/check-request-context.py`. Do not run local WASM release builds, replay sweeps, load or timing gates; CI owns those proofs. Yarn runs through existing NVM Node 22 Corepack.

Original-RED controls must exercise real Prepare/bind/final claim (no early HTTP), public/primary/secondary routing and exact bytes, typed pre-send denial versus network failure, original deadline while parked/retrying, duplicate/late/reused command isolation and retained native successful/malformed/empty results. Preserve existing fixture assertions/event clocks and use shared prepared fixture glue. Validate streaming failed-prefix exclusion and row/freshness effects for any changed reducer; preserve known partial/unknown/no-replay behavior for writes. Host disposal belongs to existing shared host controls, with any missing proof requested from its owner rather than a venue-owned executor.

Independent source/adversarial, caller and full-map reviews must answer: **is this a per-venue fix of a cross-cutting concern?** Expected **No**: native semantics plus shared core adoption. Any missing common mechanism stops that area for its core owner. Check every actual fingerprint before pruning; new/changed legacy calls and exemptions are forbidden. CI must remain watched in a background `gh pr checks --watch`; green CI, clean reviews, actual dependency integration, separate revert guard and an overseer merge window are required.

Documentation checkpoint verification compares this exact map with the unchanged lexical scanner/manifest and checks index registration. It proves neither source migration nor native/CI gates. Future landing changes, especially the occupied PRs above, require a fresh inventory; preserve all incoming repairs and reapply only this venue's exact prune.

## Plan review receipt

Independent source/adversarial, caller and full-map reviews checked all seven family maps against the immutable base and unchanged scanner: 36 paths, 221 entries, 247 occurrences. Caller corrections (structured error fields, exact retry counts, settings and routing) and Polymarket background-candle ownership were incorporated before this commit. All three architecture gates: **No**. This receipt covers the plan only; implementation requires new independent reviews.

## Exact frozen inventory

All locations and source SHA256s refer to the immutable base, including inline tests. Each row is one manifest path/fingerprint entry; a repeated fingerprint in another path is distinct. Locations list every occurrence and its native handler/destination; lexical macro rows are not physical sends.

### `venues/kraken/src/account.rs`

Source SHA256: `5eb30339c79ce907f8884f30c1a25c074d354852b4bc893d8ee569477209ba24`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 | 375 `signed_request` → DISPATCH | `cx.request(method, url, headers, body, pending)` |
| `fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6` | 1 | 388 `signed_streamed` → DISPATCH | `cx.request_streamed_body(method, url, headers, body, at, pending)` |

### `venues/kraken/src/history.rs`

Source SHA256: `ea0db022f2fe3de734cc7c0e3d771241fba3cd8e6d559bf9dbe7b1f2ba81f875`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `715c868362434911c7a9d556c59d92966e43d911ef11bf5edf375d57b9a0a4f9` | 1 | 357 `history_command` → HISTORY | `cx.fail( *command, "Kraken does not support caller client order IDs or their lookup", )` |
| `25843858bf27d1debf93c3d12a22740d349dd9a6471e1121abc59e58e8bf1d33` | 1 | 425 `history_command` → HISTORY | `cx.fail( *command, "Kraken history range must contain valid milliseconds in ascending order", )` |
| `f02822ef2a046163b957ede1513dbe07f03e1f7740a2d65cca66b85e7130d40a` | 2 | 486 `history_request` → LEXICAL + HISTORY; 493 `history_request` → LEXICAL + HISTORY | `format!("{:.0}", request.from.floor())` |
| `da0f2867dd125348b343e5427fbd8cade0ec456de13cdec4a36a23034df6db71` | 1 | 494 `history_request` → LEXICAL + HISTORY | `format!("{:.0}", request.to.ceil() + 1.0)` |
| `6c7a880f942512575a92c8c8bd1fd04a85df37e7e7bb0a4e0b134b338cdd542e` | 1 | 522 `history_request` → HISTORY | `cx.request_streamed( Method::Get, format!("{}{path}?{form}", self.host()), Vec::new(), at, Pending::History(Box::new(...` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | 537 `history_request` → HISTORY | `cx.fail(command, error)` |
| `865ee8f27182ec723973bb7065183422d5cfeeb51c4438ea22872d4bce0ed862` | 1 | 895 `history_http` → LEXICAL + HISTORY | `format!("{:.0}", request.newest.floor())` |

### `venues/kraken/src/lib.rs`

Source SHA256: `dfb6425469f9580a286cf230ff28e1f0acef2d9016088e8635836b365c5b921d`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `e1fa27a60b5975d6b5d61534567990770bf7acd939fd2bbe1bb6f1d0a69d83f2` | 1 | 190 `markets_request` → BACKGROUND | `cx.request_streamed( Method::Get, format!("{}/derivatives/api/v3/instruments", self.host()), vec![], ArrayAt::Field("...` |
| `6920a8a0e605c8435f4da4f4ba12691439407417c32c29b9a01701a176581561` | 1 | 207 `tickers_request` → BACKGROUND | `cx.request_streamed( Method::Get, format!("{}/derivatives/api/v3/tickers", self.host()), vec![], ArrayAt::Field("tick...` |
| `3086bfd3dddf1389c250505f93b2e3f224f1b31ebbb4c41902976df96e49cadb` | 1 | 365 `candles_request` → LOCAL | `cx.fail(c, "Unknown Kraken market")` |
| `5a5b7fc7c97b592291089a8d86f674baef6620525fef889feccd6133b6e27933` | 1 | 371 `candles_request` → LOCAL | `cx.fail(c, interval.unsupported("Kraken"))` |
| `6bf1b00f37754a5b2c7549f71970a36e362b6efd9c9440d5e400c2cb581ce020` | 1 | 380 `candles_request` → LOCAL | `cx.fail(c, "Invalid Kraken candle range")` |
| `74f351cfb98b55bc2a5dff3f17f95eacc20502e878b8b61b69eda7cc0563b006` | 1 | 426 `candles_page` → READ + HISTORY (paged candles) | `cx.request_streamed( Method::Get, format!( "{}/api/charts/v1/trade/{symbol}/{resolution}?from={:.0}&to={:.0}", self.h...` |
| `f06dce80ebd266653d22bf24f105c411c21c1de8a011cd3d6fe688214267e373` | 2 | 457 `public_failed` → READ + HISTORY (paged candles); 987 `on_response` → READ + HISTORY (paged candles) | `cx.fail(c, error)` |
| `6cb9a1e96c67a077e2d60646f5ef1786c7274adad1b681cbcddadb9317b35947` | 1 | 986 `on_response` → READ + HISTORY (paged candles) | `cx.done(c, data)` |

### `venues/kraken/src/secondary_tests.rs`

Source SHA256: `8d6716d10a076b1f6a933a5dc208fd0b6b00bb6ae5e69b66230c79d2075ed0a3`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `ac63acd5201ab712e970b2d03882fcfd07b7f0d152240519e8ff9ee7ac69d661` | 1 | 96 `secondary_get_keeps_primary_mutations` → TEST | `assert_eq!( request .headers .iter() .find(\|(k, _)\| *k == "Authent") .unwrap() .1, signer .signature("", nonce, "/der...` |
| `a65fe12dfb71018a2d5101b6b27429e7fdf69d90d8fc256345f85688c8682cee` | 1 | 107 `secondary_get_keeps_primary_mutations` → TEST | `assert!( request .headers .iter() .any(\|(name, value)\| name.eq_ignore_ascii_case("APIKey") && value == expected), "{:...` |
| `e318289eadd3c79f7a0ac84f52289c9d1f1273ee2f4d47532f645310d8f29734` | 1 | 151 `equal_api_keys_share_nonce_clock_but_keep_selected_signing_secrets` → TEST | `assert!(request.headers.contains(&("APIKey", "primary-key".into())))` |
| `1229d1e1d4fe688bc646deff670084de971ab0bd368cd8a671ad7949d207fb24` | 1 | 170 `equal_api_keys_share_nonce_clock_but_keep_selected_signing_secrets` → TEST | `assert_eq!( request .headers .iter() .find(\|(k, _)\| *k == "Authent") .unwrap() .1, signature )` |

### `venues/kraken/src/spot/account.rs`

Source SHA256: `88526fc4b7150e8fd76115b35d27a74e1c2afc3cf710cabdcb22494c7083de36`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `29a69618a4d60ba5b2b3e3872f6ab2e1c20013d981a27fb056e9a0d633b3ceb6` | 1 | 241 `send_private` → DISPATCH + SESSION + WRITE | `cx.request( Method::Post, format!("https://api.kraken.com{}", request.path), vec![ ("API-Key", key), ("API-Sign", sig...` |
| `3ca5b7c3e3f6d4be96de522ae9a17db768465c99cadb29ec2198961b73f6f929` | 1 | 243 `send_private` → LEXICAL + DISPATCH + SESSION + WRITE | `format!("https://api.kraken.com{}", request.path)` |
| `6aa843b32406cb28ef9b16bd489f69dd2529f140fa9642a7f09ccc62943ef76d` | 2 | 664 `private_response` → LEXICAL + DISPATCH + SESSION + WRITE; 785 `private_network_failed` → LEXICAL + DISPATCH + SESSION + WRITE | `matches!(request.kind, Kind::Trade(_))` |

### `venues/kraken/src/spot/mod.rs`

Source SHA256: `9df0e8d357eec7d817295524723491bbf7497952c81cba7b37cf6e8f8115ad4b`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | 133 `public_failed` → READ; 704 `on_response` → READ | `cx.fail(command, error)` |
| `7ee70c01ddfed95c96a161f33976a8fbd5a8d529cda8bd31a364e2d8161df4ce` | 1 | 139 `read` → DISPATCH | `cx.request( Method::Get, format!("https://api.kraken.com/0/public/{path}{params}"), vec![], None, pending, )` |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | 703 `on_response` → READ | `cx.done(command, data)` |
| `bf0c63ade187f2ea61fdb2e369f2baa04ac8745b7d18512e1ea71a3f9955ced9` | 1 | 876 `on_command` → LOCAL | `cx.fail(command, tf.unsupported("Kraken Spot"))` |

### `venues/kraken/src/spot/trading.rs`

Source SHA256: `67ef2ebf0fb7f8eac0b2f8bf69034bea04302fc4c9f524dd5fc7961f02a3c98c`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `12b11a734392e4b5f59ab459518e495d3828d71d50c76f221998150e18f76b64` | 1 | 271 `trade_next` → WRITE | `cx.done( job.command, if job.cancel { "null".into() } else { serde_json::to_string(&job.ids).unwrap() }, )` |
| `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` | 1 | 359 `trade_failed` → WRITE | `cx.fail(job.command, error)` |
| `ae460df304c6f896a1e611c805f9da6a417d4eab82daf7595c0908570f6506f2` | 2 | 367 `trade_command` → LOCAL; 376 `trade_command` → LOCAL | `cx.fail(*command, not_sent(e))` |
| `5a9df80195da64cfb0db15e8bd3898f2f4d0651de85f73b81dac78e4196cb1d3` | 2 | 380 `trade_command` → LOCAL; 394 `trade_command` → LOCAL | `cx.fail(*command, not_sent("Kraken Spot empty cancel identity"))` |
| `ea8c9ac0d82c56d77ecb772d1eb46fc2d9f3154891144bfd2fb3aac247c7c42a` | 1 | 419 `trade_command` → LOCAL | `cx.fail( *command, "Kraken Spot order book snapshots use subscriptions", )` |
| `35058464be07d81db637fa288630993ec85908b4d75d5e863ad46e8e5a1cb92b` | 1 | 423 `trade_command` → LOCAL | `cx.done(*command, "null".into())` |
| `6ab6b41277dbc3bba05c301ce36fbd8f2124f060f0f091ccd3c7053ec7a68520` | 1 | 428 `trade_command` → LOCAL | `cx.fail( *command, not_sent("Kraken Spot does not support this account setting") .kind(ErrorKind::Unsupported), )` |
| `59077428494e9e9a07dce584eecf768f775534c946b0d4421e19b1939a43acdc` | 1 | 435 `trade_command` → LOCAL | `cx.done(*command, r#"{"found":false,"definitive":false}"#.into())` |
| `2b6496f802ed0103ee7ed4474d33d7bc7e686d9888780e81d48d13562d234123` | 1 | 438 `trade_command` → LOCAL | `cx.fail(*command, "Kraken Spot public trades are not implemented")` |
| `09d284b47513158ba424efe6b85ef618c3f2649072f4211be5a3ec043ecc0e88` | 1 | 440 `trade_command` → LOCAL | `cx.done(*command, "[]".into())` |

### `venues/kraken/src/trading.rs`

Source SHA256: `2172a730070eb08548b01eb0fd476fad94c643c5a8870a0c6e4ec18208cbbe06`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `153595b6489266ace2fc389c86c71c07fe76b79d5928210ed88b6ffad5f5127e` | 1 | 184 `trading_command` → LOCAL | `cx.fail( *command, not_sent("Kraken Futures only supports one-way positions") .kind(ErrorKind::Unsupported), )` |
| `35058464be07d81db637fa288630993ec85908b4d75d5e863ad46e8e5a1cb92b` | 1 | 190 `trading_command` → LOCAL | `cx.done(*command, "null".into())` |
| `2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5` | 1 | 197 `trading_command` → LOCAL | `cx.fail(id, not_sent(error))` |
| `5d0b0c93e4647bbcb1a8083fdb3059600f03febcb10aee2a745e56a833e726e2` | 1 | 200 `trading_command` → LOCAL | `cx.fail( id, not_sent("Kraken trading requires API credentials") .kind(ErrorKind::AuthInvalid), )` |
| `604f28d91adfdb4cd0caac06047abd9569f9a6c248b698a784032256d7cf8047` | 1 | 579 `trading_pump` → WRITE | `cx.done(job.command, serde_json::to_string(&job.ids).unwrap())` |
| `9584979d6cce6bf6576093402c85d6195493ed09a64ba6f72927c8f53bf61847` | 2 | 718 `trading_http` → LEXICAL + WRITE; 1003 `trading_failed` → LEXICAL + WRITE | `matches!(request.step, Step::Leverage { .. })` |
| `ca37246cf8052b21b57e7f1400b1026f755a6bdaadfe661eb481c929ddcc2d68` | 1 | 969 `trading_finish` → WRITE | `cx.done(job.command, result)` |
| `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` | 1 | 988 `trading_finish` → WRITE | `cx.fail(job.command, error)` |


## Phase one: single command Spot OHLC on actual seams

This slice is based solely on actual landing `bd52632f2c8`, including #1785/#1808/#1816/#1838. Eight original controls are carried as separate assertion-RED commit `da1749d8d1f`: they compile and fail on opaque early HTTP at this actual base. The adoption validates exact delivery, uses contextual completion and typed preparation/transport failures, and preserves native pair/interval/since bytes, ignored limit/to_ms and last cursor, whole-response rejection, sorted/empty output and four total transport attempts.

Three zero-delay native retry timers are replaced with immediate managed retries under the same original 20-second root. This keeps engine expiry armed while routing is parked. The native exhausted-read warning is retained; late native errors cannot emit that diagnostic. Ten controls include the eight original regressions plus released expiry and invalid-UTF8 retry compatibility against actual #1838.

Command-entry adoption is deferred to a fresh follow-up after #1830 lands, per the overseer's throughput guidance. Initial Context issuance can still use its existing numeric failure before authority exists; issued work has no numeric fallback. Only `spot/mod.rs` is pruned (5 to 3 occurrences, family 53 to 51), keeping that issuance refusal, unsupported interval and discovery/ticker sender. No engine/shared source changes or unlanded seams are carried. Existing fixtures use shared preparation without changing native query/result assertions; retry scheduling changes are explicitly reviewed.


## Phase two plan: genuine Spot OHLC command entry

Base: actual landing `e0322577e35`, including command-entry #1830, typed HTTP failures #1816/#1838, static headers #1820 and classifier #1837. A fresh open-PR file check reserves `spot/mod.rs` and `tests/spot_request_context_candles.rs` for this slice; occupied signing #1852, Spot book #1798 and Spot trading #1797 remain untouched.

Replace only the initial OHLC `http_context(Scope::Command(command), policy)` plus numeric `cx.fail(command, error.venue_error())` with genuine `cx.admit_http_command(policy)`. Keep the existing Read / no credential / 20-second / ReadFailure policy. The engine alone emits the typed admission rejection with no financial outcome; rejected or revoked entry returns without numeric completion. Existing issued-context retry, preparation, exact-delivery, native result and contextual terminal paths remain unchanged. Unsupported interval stays before admission as its separate native preflight tail.

Before production edits, add two regression controls in a separate commit. Non-finite initial event clocks must emit exactly one InvalidInput Read failure with no outcome or HTTP/preparation/timer and permit later reuse of that command ID. At the existing 256-owner bound, a new caller must receive exactly one Internal Read failure with no outcome or new HTTP/preparation/timer; an existing parked owner must still finish, and the rejected command must succeed after that slot is freed. Original source must fail the outcome assertions; keep the assertions unchanged for the adoption. Preserve the ten existing lifecycle/native controls and all fixture clocks and business assertions.

Only five paths are planned: this appendix, a CALLERS appendix, the exact manifest prune, `spot/mod.rs`, and `tests/spot_request_context_candles.rs`. Remove only Spot mod.rs fingerprint `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` count one. Spot mod.rs becomes two entries/two occurrences, and the family loses exactly one entry/occurrence. No write/classifier row, signer, session, background, native codec or unsupported/cache/local path is claimed migrated.

Verification: all twelve focused candle controls, full Kraken plus Binance native suites, strict all-target Clippy, scoped fmt, bans and both request-context scripts. Independent source/adversarial, caller and full-map audits cover the plan and implementation; architecture gate expected No because the shared engine owns admission, rejection and generations. Fresh exact-head CI and a separate current-landing guard plus overseer venue window remain required.


Phase-two execution receipt: plan `a0a116c1f80` precedes separate test commit `b800831381a`. Both new tests compiled against unchanged production source and failed specifically at `Some(NotSent)` versus `None`, proving the initial Read admission regression. The production move only adopts `admit_http_command` with the identical policy and returns on either entry error; it adds no numeric fallback and leaves every issued-context/native path unchanged. The exact prune is one fingerprint/occurrence in Spot mod.rs (three to two); family inventory is eight paths, 45 to 44 entries and 51 to 50 occurrences at this slice's base. Full gates and three independent implementation audits are required before readiness.


Phase-two local verification: all twelve candle controls and 541 Kraken/Binance native tests pass; strict all-target Clippy, scoped formatting, bans and both request-context scripts pass. Independent source/adversarial, caller and full-map plan reviews are clean; implementation source and caller reviews are clean, with the full-map audit recorded separately before readiness. No standalone replay/package proof is claimed; CI owns replay gates.


## P0 write adoption, classifier pin 2026-10-23

Audited actual `6741c1c6609437e32090c76745a56ca6c2262445`; this appendix
supersedes draft WRITE API references. Futures and Spot remain separate native
signature, credential, nonce and endpoint domains within this venue plan.

| Audit row | Reverified native path | Required adoption |
| --- | --- | --- |
| #18 Spot | `spot/trading.rs::trade_response` accumulates returned txids; `trade_failed` discards them. | Record each accepted txid as a journal fact and preserve it in structured Partial on a later failed leg. |
| #21 Futures | `trading.rs::trading_finish` appends accepted IDs only to the message and locally clears outcome. | Keep native text, plus exact `partial.order_ids`; shared aggregation replaces `effective`/`amend_canceled` demotion. |
| #37 Futures | `trading_http` parses the body but its non-2xx branch bypasses `refusal` and drops native-code metadata. | Decode native evidence for every delivered status; engine precedence still makes 408/5xx uncertain. |
| #30 / P2 Spot | `spot/account.rs::private_response` retries a substring signature error and deactivates on any 401/403. | Shared classification + opaque resend grant; credential role and native auth evidence control auth handling. Status alone never proves key invalidity. |

Futures already distinguishes `authenticationError` (AuthInvalid),
`apiLimitExceeded` (RateLimited), explicit refusal codes/statuses such as
`insufficientFunds`/`invalidArgument`, and uncertain `Server Error`,
`Unavailable`, `unknownError`. Move that existing finite list into const rows;
retain uncertain/unlisted values. Native `placed`/`filled`/`cancelled`/`edited`
requires the existing complete identity codec, not success-code acceptance.
On edits `postWouldExecute`/`iocWouldNotExecute` may cancel the original: this
is not an effects-free refusal and must remain explicitly unproven/partial.
Spot has native `EAPI:*`, `EOrder:*`, `EGeneral:*` strings. Preserve distinct
finite codes; any documented suffix grammar is decoded before lookup. Do not
convert existing broad prefix/contains branches into newly invented refusal
rows. Audit ambiguous multiple-error envelopes without claiming every error
proves the whole command refused. No new bare-status table entries without
protocol evidence. One family table must not mix code-less Message rows and
native Text rows; Futures/Spot keep their distinct native evidence codecs.

### Landed classifier and write-journal boundary

Use actual #1837 (`e0322577e354739dde6d7e5011e74ab660145885`), not the
older draft signatures. Declare one family `classify::CodeTable` with `rows`,
`success`, explicit `unlisted: Verdict::Unknown(None)` and evidence-backed
`status` entries only. `Key::Message` is exact and code-less; never mix message
and code rows or introduce substring/prefix inference. Decode integer codes
strictly via the native codec / `classify::decode_int`, never through f64.
Engine precedence owns transport, 408/5xx, native evidence, status and malformed
success. A native code or 2xx alone never proves successful execution.

A genuine command Context uses the actual command entry and real credential;
managed signed HTTP carries it through Pending. At the exact delivered attempt,
`classify_write(&ctx, &TABLE, native, message)` creates the failure Fact.
`http_write_fact(&ctx, WriteFact::Failed { fact, known_effects }, reports)`
records it; `http_write_failure` is the whole-failure shortcut. Complete native
success reports `WriteFact::Accepted { order_ids }`; malformed success uses
`http_write_unproven`. A real batch acknowledgement uses `http_write_batch`
with caller-ordered items and the native expected count. The codec proves
identity/uniqueness and correlation; preserve individually known IDs as
reconciliation evidence without invented caller counts on an incomplete reply.

Only the engine's opaque resend grant may feed `http_resend_context`, retaining
the original root, deadline and one-shot budget. Use `http_write_leg_intent`
only for permitted intent narrowing before release. Finish once through
`done_context`/`fail_context`; CommandWrites owns aggregation, partial IDs and
whole-command demotion. Do not use private engine-test-only `http_write_result`,
transitional unmanaged `classify::write`, local status/outcome ladders or a
numeric completion fallback. A preparation refusal's local NotSent fact is
valid, but prior released siblings still determine the whole-command outcome.

No runtime conversion is authorized in this plan/tests slice. Missing reusable
signing, session, Plan/CallWindow or reconciliation ownership remains a shared
prerequisite; no per-venue replacement. On adoption, reverify actual seam pins,
remove RED ignore markers while retaining behavior assertions, prune only the
venue's genuinely removed legacy fingerprints, and add a landed CALLERS
appendix. Plan/tests preparation neither prunes inventory nor closes P0 rows.

### Phases, blockers and original RED controls

First migrate genuinely single-write Futures placement/edit/cancel with the
HMAC recipe supplied by claude-4yjlw0, retaining stable_id nonce custody in the
shared signer. Spot nonce/signature custody is distinct. Native batches,
place-many, cancel/replace and settings preflight require shared Plan admission;
not an improvised vector of independently admitted owners. GetWebSocketsToken
is a session mutation owned by claude-55ao7t. Background reconcile delivery
needs #1856; history completion needs #1848. Check actual availability first.

At this audit #1797 owns Spot trading source/tests, #1798 owns Spot book,
#1852/#1866 own Kraken sign.rs. These source/test paths remain untouched.
Append ignored public-Engine regressions only to the unoccupied Futures
`tests/trading.rs`: first native placement succeeds with ID `accepted`, later
placement refuses or loses transport. Require exactly one failed terminal,
`partial.order_ids == ["accepted"]`, no replay, and Unknown for transport.
Run `cargo test -p safe-cex-venue-kraken --test trading p0_write_ -- --ignored
--nocapture`; both assertions must fail on original production source, not
compilation. No new test key material is needed.

Spot acceptance controls use the separate unreserved target described below:
txid preservation after a later refusal; 401/403 with non-auth native evidence
does not deactivate; 503 with a signature string grants no resend. Futures
non-2xx native auth/rate-limit/refusal fields must survive, with 408/5xx still
Unknown. Rejected/NotSent may describe a later local leg, never erase earlier
accepted effects; definitive mixed effects need Partial, not forced Unknown.
Retain cancellation/fill races, replacement IDs and caller order. Recheck the
full legacy inventory above at implementation; this slice removes no entries.

Architecture gate: No for plans and native Engine regressions. Runtime
classification, aggregation, retry, nonce and admission stay shared concerns.
Future implementation gates: venue + Binance, scoped strict Clippy/fmt, bans,
both ratchets, clean source/caller/full-map reviews and exact-head CI.

Local plan/test evidence: 2 original-source controls compile and fail
by the intended behavior assertions; normal venue suites pass
(244 tests, 2 explicit ignored RED controls).
Scoped strict all-target Clippy, fmt, bans and both request-context ratchets
pass. Binance's same-base 297 tests passed with the ApeX plan slice. These
controls expose defects; they are not GREEN adoption or P0 closure evidence.


Additional original-source controls avoid the #1797 reservation entirely:
`tests/spot_write_adoption_red.rs` imports the existing Spot support module
without editing Spot source or the held spot_trading target. It pins accepted
txid preservation, non-auth native evidence on401 without deactivation, and a
503 signature body without write replay. Futures trading adds a non-2xx native
code preservation control. The combined six controls run with `cargo test -p
safe-cex-venue-kraken --test trading --test spot_write_adoption_red
--no-fail-fast p0_write_ -- --ignored --nocapture`. The earlier two-control
receipt remains historical; the updated six-control proof supersedes it.

Updated local evidence: all six original-source controls compile and fail the
intended behavior assertions; the normal suite passes 244 tests with six
explicit ignored controls. Scoped strict all-target Clippy, fmt, bans and both
ratchets pass. This remains preparation, not a runtime fix or P0 closure.
