# Orderly broker family RequestContext inventory and plan

Lane `codex-hcmq3f`, overseer `claude-ddi2il`, 2026-10-23. Immutable source base: `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`. Inventory: **4 source paths, 35 path/fingerprint entries, 39 conservative lexical occurrences**. This is a plan, not implementation, CI, live connectivity or cutover proof.

## Scope and first single-request path

One crate serves the six registered Orderly broker names, with native broker-suffixed symbols, distinct cross/isolated position identities and existing account settings. WOO X is separate. Keep exact native IDs, bracket/caller-ID restrictions and account-wide cancellation scope.

Two alternative headerless public candle request shapes in lib.rs are single-request paths: /v1/tv/kline_history when bounds are present, otherwise streamed /v1/kline. Each invocation selects one endpoint; they are not paired requests. Start with bounded history candles, then unbounded/current candles in one small candle-area PR if both remain isolated.

## Collision and dependency gate

Before editing, open PR file lists were checked with `gh pr list --base feat/rust-rewrite --state open --limit 100 --json number,title,files,headRefName`. Source/test collisions below remain off-limits until actual landing; recheck before every slice. Shared docs/index and exact venue-only inventory prunes are permitted by the overseer.

| Open PR | Occupied source/test files |
| --- | --- |
| #1791 | `sdk-rs/venues/orderly/src/history.rs`; `sdk-rs/venues/orderly/tests/history.rs` |

Headerless public candle reads are unblocked except actual shared typed context-error API. #1791 occupies history.rs/tests/history.rs. Ed25519 and hosted/keyless compatibility are owned by claude-19pt8s; grouped cancellation/history fan-out goes to claude-fe7lgm plus plan owner codex-uufub4. Kind/code gaps go to claude-dazzy3; no new status ladder.

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

1. FIRST: command candle continuations in lib.rs plus new tests/request_context_candles.rs. Keep both endpoint-selection branches, seconds conversion, supported intervals, limits/filtering and result/row validation. Preserve no-I/O invalid/empty branches separately until shared preflight completion; do not mint fake accepted work to remove lexical sites.
2. Public market/ticker seeds adopt background contexts and shared release admission while keeping staged startup publication, broker identities and socket/subscription limits. Serial seed stages or one completion with multiple owners require declared plan/group admission, not another callback queue.
3. Private account reads wait for signer-owned Ed25519/key custody and then adopt managed Read with explicit account identity. Preserve RowAdmission/reporter/freshness and native isolated/cross rails. Cached FetchAccount remains local and needs shared no-I/O completion.
4. History source/tests are occupied by #1791. Integrate its actual owned pages first, then bind the existing HistoryRows/CursorHistory/Completion to one managed command plan across normal/algo/orders/fills/funding alternatives. Preserve strict setup/refusal and accepted-only partial recovery without a local pager/marker owner.
5. Migrate signed writes, account-wide DELETE pairs, bracket protection replacement and reconciliation reads through shared preflight/Plan/journal. Preserve optional child behavior, exact native IDs, native stop/continue facts and known cancellation effects; remove per-venue aggregation only when engine facts replace it. Lexical format/matches entries disappear only with legitimate descriptor migration.

Every implementation area gets a committed plan, a separate original-source assertion-RED regression commit, then adoption and assertion-GREEN proof. Keep one venue or venue area per PR. Queue refusals retain the continuation; after authority issuance every failure/result uses that Context, never a numeric fallback. A truly pre-issuance refusal may retain its existing path until the shared local-completion adoption; all remaining inventory still must reach zero.

For every physical read retry preserve native counters/query bytes but require engine retry evidence under one immutable root. Existing single-read transport bounds (20 seconds unless explicitly configured otherwise) are captured once, not renewed on retry, route/signing, page or delay. Child bounds are capped exactly by the original logical root; no rounding-up extension. Multi-request plans and shared/observer completion remain explicit dependencies rather than independent roots. Native status/body/list proof and row/snapshot freshness are still required after exact delivery validation.

## Caller-visible contract

Bounded and current candle outputs/filters remain native; requests require managed transport, one original deadline and typed preparation/network/timeout errors. Preserve six broker routes and native settings/local account identity behavior. Later central errors must retain partial IDs and uncertainty through caller/worker/hosted replies. Histories and capability flags must reflect actual #1791/other landed behavior, not planned support. Both command candle branches have zero automatic retries. Position mode alone is fixed OneWay: OneWay succeeds as a no-op and hedged refuses. FetchAccount is cached identity; FetchLeverage is signed GET. Margin-type and leverage mutations exist and remain native writes. Preserve invalid and future-empty no-I/O branches separately.

Each code PR appends a CALLERS section describing only its resulting behavior to `RUST_CUTOVER_CALLERS.md`. Record immutable-deadline and managed-transport requirements, typed failure/uncertainty and retained native schemas; distinguish local-operation `notSent` from proof about earlier physical sends or the whole command. Preserve existing native read error fields, including message-only paths where applicable; any classification/outcome normalization requires separate review. Do not add mutation outcomes to pure reads. No default/product/capability change is authorized by this inventory.

Downstream write-error targets remain the audited order send/create/update/cancel and scaled-order flows plus worker/hosted error receivers in the retained caller ledger. Preserve kind/outcome/native/partial metadata and one terminal notification; unknown writes and reconciliation-only IDs never authorize automatic replay. This lane changes no upstream checkout and claims no NX adoption/deployment or live credentials.

## Verification and independent review

Use `CARGO_BUILD_JOBS=2`. Code gates: focused `cargo test -p safe-cex-venue-orderly -p safe-cex-venue-binance`, scoped fmt and strict all-target Clippy, bans, and both `scripts/check-request-context.test.py` / `scripts/check-request-context.py`. Do not run local WASM release builds, replay sweeps, load or timing gates; CI owns those proofs. Yarn runs through existing NVM Node 22 Corepack.

Original-RED controls must exercise real Prepare/bind/final claim (no early HTTP), public/primary/secondary routing and exact bytes, typed pre-send denial versus network failure, original deadline while parked/retrying, duplicate/late/reused command isolation and retained native successful/malformed/empty results. Preserve existing fixture assertions/event clocks and use shared prepared fixture glue. Validate streaming failed-prefix exclusion and row/freshness effects for any changed reducer; preserve known partial/unknown/no-replay behavior for writes. Host disposal belongs to existing shared host controls, with any missing proof requested from its owner rather than a venue-owned executor.

Independent source/adversarial, caller and full-map reviews must answer: **is this a per-venue fix of a cross-cutting concern?** Expected **No**: native semantics plus shared core adoption. Any missing common mechanism stops that area for its core owner. Check every actual fingerprint before pruning; new/changed legacy calls and exemptions are forbidden. CI must remain watched in a background `gh pr checks --watch`; green CI, clean reviews, actual dependency integration, separate revert guard and an overseer merge window are required.

Documentation checkpoint verification compares this exact map with the unchanged lexical scanner/manifest and checks index registration. It proves neither source migration nor native/CI gates. Future landing changes, especially the occupied PRs above, require a fresh inventory; preserve all incoming repairs and reapply only this venue's exact prune.

## Plan review receipt

Independent source/adversarial, caller and full-map reviews checked all seven family maps against the immutable base and unchanged scanner: 36 paths, 221 entries, 247 occurrences. Caller corrections (structured error fields, exact retry counts, settings and routing) and Polymarket background-candle ownership were incorporated before this commit. All three architecture gates: **No**. This receipt covers the plan only; implementation requires new independent reviews.

## Exact frozen inventory

All locations and source SHA256s refer to the immutable base, including inline tests. Each row is one manifest path/fingerprint entry; a repeated fingerprint in another path is distinct. Locations list every occurrence and its native handler/destination; lexical macro rows are not physical sends.

### `venues/orderly/src/account.rs`

Source SHA256: `c947f839b07b452ced0f2fcac76bc572be1e2767392402e2d282556531d893d2`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `52fcbacda7a2458c4b46b68b74a5abaa4c98e0ed70483d61e3355d902185b5be` | 1 | 351 `request_signed` → DISPATCH | `cx.request_streamed_body( method, url, headers, body, ArrayAt::Path(&["data", "rows"]), pending, )` |
| `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 | 360 `request_signed` → DISPATCH | `cx.request(method, url, headers, body, pending)` |
| `bc5f847684d65a7bfaed0fc4a8e698698d9211c65a7e41a191f62947a5f63cfb` | 2 | 524 `account_read` → LEXICAL + BACKGROUND; 1095 `account_response` → LEXICAL + BACKGROUND | `matches!(request, Request::Info)` |

### `venues/orderly/src/history.rs`

Source SHA256: `d7af3821c100724751b40eff7ab79561a22f4fbafe2377623907d671d98882fb`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `65c654c353026ef4787be20df6b914177817af3566d377c05104b64fc7912dde` | 1 | 312 `history_command` → HISTORY | `cx.fail(command, "Orderly client-order lookup is not supported")` |
| `69026daad85572eae41ba25e7948705848dddccead9bfbd83fab988bd4e34640` | 1 | 352 `history_command` → HISTORY | `cx.fail( command, "Orderly income supports settled funding payments only", )` |
| `0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb` | 1 | 402 `history_command` → HISTORY | `cx.fail(command, e)` |
| `b07e6724c898034bb4084ef20fac4722593134810001c58e1b5bd211d86b3eb0` | 1 | 466 `history_send` → LEXICAL + HISTORY | `format!("/v1/order/{}/trades", request.order_id.as_ref().unwrap())` |
| `7bd36708a9596838f653589ae2971e4fe046d9107525c6704cfb150dd2b7aee5` | 1 | 469 `history_send` → LEXICAL + HISTORY | `format!("/v1/{}orders", if request.algo { "algo/" } else { "" })` |
| `bad9cf77c482cf6477032904a15a598d91f505233d85763ff12bfe2c52612098` | 1 | 485 `history_send` → LEXICAL + HISTORY | `format!("limit={}", request.limit)` |
| `b32729dd8c0c41f6dd0d8557bdb782e763de5ca285d586d8b1ca03194460f52c` | 1 | 487 `history_send` → LEXICAL + HISTORY | `format!("page={}", request.page)` |
| `0f623534a32ada3423ea1fe1ccd852cf3bd5237f515be8a1f08018fa15a4de90` | 1 | 504 `history_send` → HISTORY | `cx.request_streamed( Method::Get, format!("{}{path}", self.rest()), headers, ArrayAt::Path(&["data", "rows"]), Pendin...` |
| `815e133de27609df4a97575da4b1ac77003822d6553c0eb07a3c87d82dc0a9a3` | 1 | 512 `history_send` → HISTORY | `cx.fail(request.command, e)` |
| `87480f92c8dda27618b06cb4ee557eb45f297fdeff2f340b0450c00c20da9cea` | 1 | 790 `history_order` → LEXICAL + HISTORY | `format!("{}:{id}", request.algo)` |
| `aa71c1d92e34443bf9b99a67bc669e086e750105b16ed6b6d1835d2e7f2c2c0f` | 1 | 886 `history_response` → HISTORY | `cx.done(request.command, data)` |
| `5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2` | 1 | 887 `history_response` → HISTORY | `cx.fail(request.command, error)` |
| `a80dbd814bc16ea96974401773a1fe09aa94d2f65961a71a85e19daa1ea4cddf` | 1 | 892 `history_failed` → HISTORY | `cx.fail( request.command, format!("Orderly history read failed: {error}"), )` |

### `venues/orderly/src/lib.rs`

Source SHA256: `c9450fe63ee143ac32c534354f523bb5213efe2807852eecf0c19ad58348f356`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `dee025771565e025cbca449b90666e5c20dc3c0af9834094852eae869ee83f0a` | 1 | 260 `seed` → BACKGROUND | `cx.request_streamed( Method::Get, format!("{}{path}", self.rest()), vec![], ArrayAt::Path(&["data", "rows"]), pending, )` |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 2 | 873 `on_response` → READ; 1194 `on_command` → LOCAL | `cx.done(command, data)` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 3 | 874 `on_response` → READ; 960 `on_request_failed` → READ; 1195 `on_command` → LOCAL | `cx.fail(command, error)` |
| `9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d` | 1 | 935 `on_response` → READ | `cx.done(command, serde_json::to_string(&rows).unwrap())` |
| `b60a1032f32712ccd2c6d2715586aae49efe1eb2f61b7e91d1dff22c70f5d68b` | 1 | 1123 `on_command` → LOCAL | `cx.fail(command, "Orderly unknown symbol")` |
| `7d4810ffde4e87d757f48c273d4d175893ae758fa982469a69177d646b766026` | 1 | 1127 `on_command` → LOCAL | `cx.fail(command, interval.unsupported("Orderly"))` |
| `136ab46ef947f091d2262b51e52d3978c5e7448b2f00eed5f01433ad4bc2ac3b` | 1 | 1137 `on_command` → LOCAL | `cx.fail(command, "Orderly invalid candle parameters")` |
| `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | 1145 `on_command` → LOCAL | `cx.done(command, "[]".into())` |
| `0a74fcd7421b5f4666da4aae1e5039e544680fffbe7803448b60bb08e2b5e5e4` | 1 | 1157 `on_command` → READ | `cx.request( Method::Get, format!("{}{path}", self.rest()), vec![], None, Pending::CandleHistory { command, from: from...` |
| `830509ba0b9af286af5ca42c51217a3d3e2ef05dc3c450978f1e4938fc5a6642` | 1 | 1175 `on_command` → READ | `cx.request_streamed( Method::Get, format!("{}{path}", self.rest()), vec![], ArrayAt::Path(&["data", "rows"]), Pending...` |

### `venues/orderly/src/trading.rs`

Source SHA256: `e1155549cf03c5a86f7a33a467313ad176e04dfed5c632d5f20651242df49dc6`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `a8f3774dc23d4fd21635850184d55eaea951ae853ee80d2c14aafe0810264aa7` | 1 | 1003 `trade_command` → LOCAL | `cx.fail( id, not_sent("Bybit BBO and TickSize controls are not supported by Orderly") .kind(ErrorKind::Unsupported), )` |
| `505a64895d07f255d0827176872b89808869365f6b9e1004484f22c62752ad88` | 1 | 1015 `trade_command` → LOCAL | `cx.fail(id, not_sent("Orderly private account is not ready"))` |
| `a4080fcf77441d554adfe640afefa67ae734e7f711cae540a63f2bd9a61ef7f1` | 1 | 1046 `trade_command` → LOCAL | `cx.fail( trade_id(&queued).unwrap(), not_sent( "Orderly queued order mutation was superseded by bulk cancellation", ), )` |
| `e1ac291cbe12bb207c78b70677767139e269bb2f08850d05218f3ce2173ba9c7` | 1 | 1082 `trade_command` → LOCAL | `cx.fail(id, not_sent("Orderly trading queue is full"))` |
| `3c98818bcbb2b5da43180ebf643ff2a24bef6f432d6af53d5f601eef34c34547` | 1 | 1096 `trade_command` → LOCAL | `cx.fail(id, not_sent(e))` |
| `5677bd68152a39408ecf48c65bd3a9320f0fcb682ce53ae547ac20a6aef13e25` | 1 | 1118 `trade_next` → WRITE | `cx.fail( job.command, error.partial(&job.ids, job.total, job.ids.len()), )` |
| `554f23ddc8f2c0f97ba05d9d331b8f2985e7bcdbcd6b21fe1a64c371dcc4885b` | 1 | 1123 `trade_next` → WRITE | `cx.done( job.command, if job.collect { json(&job.ids) } else { "[]".into() }, )` |
| `780f8c3c54323f6b25b10a33fbbee35f994712f66e19966fab6d4abff969b353` | 1 | 1185 `trade_next` → WRITE | `cx.request( step.method, format!("{}{}", self.rest(), step.path), headers, step.body.clone(), Pending::Trade(Request ...` |
| `b2b41114e06422d97e3616db989959185a6d388c99eac2646a846e3ce3bb39f6` | 1 | 1491 `trade_response` → WRITE | `cx.done( job.command, json(&Rails { buy_leverage: value, sell_leverage: value, }), )` |


## Phase one: command candles on actual seams

This slice is based on actual `bd52632f2c8` (#1785/#1808/#1816/#1838), with no unlanded seam or engine/shared source changes. The separate original-source RED commit `0851eeb7dd5` compiles eight controls and fails them at opaque early HTTP. Both mutually exclusive command endpoints then adopt managed prepare/bind/release, exact delivery validation before stream parsing/diagnostics, contextual completion and typed transport/preparation failures. The original 20-second root is captured once; both branches keep zero automatic retries.

Native parameter validation, future-empty behavior, endpoint/query bytes, filtering, limits, sorting and row/envelope codecs remain. Existing public candle fixtures use shared preparation at each original declaration clock, retaining response clocks and every business oracle. History collision #1791 is untouched.

Command-entry adoption is deferred until actual #1830 in a fresh follow-up, per throughput guidance. Both initial Context issuance failures keep their numeric pre-authority completion; issued work has no numeric fallback. Only `lib.rs` is pruned: 13 to 9 occurrences, family 39 to 35. Two HTTP sites and two contextualized successful completions disappear; the existing shared fail fingerprint remains at its original count through initial issuance failures and cached account failure. The remaining inventory tails still must reach zero before cutover.


Phase-one verification: all eight managed controls and the complete Orderly/Binance native suites pass (470 tests, zero failed/ignored). Strict all-target Clippy, scoped formatting, bans, exact inventory checker and all 13 checker controls pass. Independent source/adversarial, caller and full-map reviews are clean, architecture gate **No**. Source scope remains the candle area plus dedicated/native fixture updates; actual dependencies only. Fresh CI and a parallel window with separate current-landing revert guard remain required.


## Phase two plan: genuine command entry for both candle endpoints

Base: actual `e0322577e35`, including landed command entry #1830, typed HTTP failures #1816/#1838, static headers #1820 and classifier #1837. Fresh open-PR inspection leaves lib.rs and tests/request_context_candles.rs clear; occupied history source/tests #1791 remain untouched.

Replace each mutually exclusive initial candle `http_context(Scope::Command(command), policy)` and numeric failure with `admit_http_command(policy)`. Preserve Read, no credential, 20-second deadline, no pacing and Never retry policy. Rejected entry already emits one engine terminal with no financial outcome; rejected or revoked entry returns without numeric completion. Native symbol/interval/range validation and future-empty preflight stay before admission. Native URLs, both envelope/stream codecs, filtering, limits, ordering, exact delivery and contextual completion stay unchanged.

Commit four regression controls before source edits: invalid-clock and 256-owner capacity rejection separately exercise bounded history and current candles. Each must compile and fail the original financial outcome assertion, then pass unchanged after adoption. Check one typed Read terminal, no new HTTP/preparation/timer, command-ID reuse, and existing parked owner survival followed by freed-capacity reuse; retain the eight original controls, native fixture clocks and business assertions. Use explicit from/to bounds for bounded non-finite-clock controls so the separate future-empty path is not selected.

Only five paths: this plan appendix, CALLERS appendix, manifest, lib.rs, tests/request_context_candles.rs. Reduce only lib.rs fingerprint c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa from count three to one; the cached-account failure remains. lib.rs nine to seven occurrences and family 35 to 33 occurrences, with every other inventory entry unchanged. No new error classification, write/signer/session/background/cache/local-tail closure is claimed.

Run all twelve lifecycle controls, full Orderly/Binance suites, strict all-target Clippy, scoped fmt, bans and both inventory checks. Three independent plan and implementation audits must verify architecture gate No (engine admission and generation ownership), caller precision and exact prune. Fresh exact-head CI, current-landing guard and overseer parallel window are required before merging.


Phase-two execution receipt: plan `8c6a924b170` precedes separate original-source RED commit `dcd263e2226`. All four new controls compiled and failed specifically at `Some(NotSent)` versus `None`, independently covering bounded/current invalid-clock and capacity refusal. Production changes only initial admission for each endpoint, preserving identical policy and native/issued paths. The exact count reduction is lib.rs c873 three to one; the family becomes four paths, 32 entries and 33 occurrences. Full verification and three independent implementation audits remain required before readiness.


Pre-integration verification: all twelve candle controls, full Orderly/Binance native suites, strict all-target Clippy and scoped formatting pass. The bans wrapper passed its thirteen scanner controls but correctly stopped at newer incoming Bybit inventory prunes; current actual landing must be genuinely integrated and the incoming prunes retained before both inventory gates can pass. All three independent implementation audits are clean, architecture gate No. Fresh integrated native/static gates and exact-head CI remain required.
