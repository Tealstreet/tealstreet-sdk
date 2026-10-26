# BreakoutProp legacy DXtrade RequestContext inventory and plan

Lane `codex-hcmq3f`, overseer `claude-ddi2il`, 2026-10-23. Immutable source base: `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`. Inventory: **3 source paths, 22 path/fingerprint entries, 24 conservative lexical occurrences**. This is a plan, not implementation, CI, live connectivity or cutover proof.

## Scope and first single-request path

Legacy DXtrade only, development-only, no anonymous public feed. Username/password and an exact selected FULL_TRADING account are needed even for market data. Keep one-way/cross state, native lot IDs, estimated valuation, synthetic quote books/live candles and current unsupported commands; this is not Breakout Terminal support.

There is no anonymously public single-request path. Even candles/quotes run behind login, account selection and a session-qualified queue. First implementation is a genuine standalone authenticated read only after the shared session/credential acquisition and actual release pacing seams land. Do not turn login into a Read or invent a command owner for background authentication.

## Collision and dependency gate

Before editing, open PR file lists were checked with `gh pr list --base feat/rust-rewrite --state open --limit 100 --json number,title,files,headRefName`. Source/test collisions below remain off-limits until actual landing; recheck before every slice. Shared docs/index and exact venue-only inventory prunes are permitted by the overseer.

| Open PR | Occupied source/test files |
| --- | --- |
| #1797 | `sdk-rs/venues/breakoutprop/src/history.rs`; `sdk-rs/venues/breakoutprop/tests/history.rs` |

BLOCKED on shared session/credential authority (claude-55ao7t), password/token presentation and custody (claude-19pt8s), and an engine-owned rolling5-per1000ms release admission policy. Group retry-after-login/fan-out goes to claude-fe7lgm/shared plan owner; kind/status evidence to claude-dazzy3. #1797 occupies history.rs/tests/history.rs. If any native contract cannot fit those shared paths, stop with evidence and ask the overseer/owner; do not add an exemption.

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

1. FIRST prerequisite: replace the local Login/token eligibility path with the shared session owner/provider credential presentation, preserving selected account, generation and retry evidence. A password Present operation belongs at dispatch; no secret/token cache or session renewer is added inside this venue.
2. Then migrate a standalone candle/quote or portfolio read under the same shared session authority. Preserve configured timeout_ms without renewal, native from/to/count bytes, synthetic-data semantics and no Store publication after late/stale sessions. Existing portfolio admission/final-fill proof remains native.
3. The drive queue enforces at most five requests in a rolling one-second window, using request_times and ready_at. Require an engine-owned rate-window policy that preserves this exact burst behavior at final physical handoff. A fixed 200 ms Pacing floor is not equivalent; do not substitute one or duplicate the local executor.
4. Order/cancel/amend flows include prerequisite/read/proof legs, login refresh after 401 and delayed repeats. Use shared declared plan/group/session transitions and central mutation outcomes, retaining exact native identifiers, confirmed zero-fill replacement and uncertain response reconciliation. Never replay a released unknown mutation after session recovery.
5. History source/tests are occupied by #1797, whose actual stub completeness correction must be preserved. Later migrate all no-I/O history/fixed-settings/unsupported branches with shared preflight completion, retaining their branded inert/unsupported behavior. Macro field matches disappear only during actual operation-descriptor migration; no test/assertion deletion or scanner exception.

Every implementation area gets a committed plan, a separate original-source assertion-RED regression commit, then adoption and assertion-GREEN proof. Keep one venue or venue area per PR. Queue refusals retain the continuation; after authority issuance every failure/result uses that Context, never a numeric fallback. A truly pre-issuance refusal may retain its existing path until the shared local-completion adoption; all remaining inventory still must reach zero.

For every physical read retry preserve native counters/query bytes but require engine retry evidence under one immutable root. Existing single-read transport bounds (20 seconds unless explicitly configured otherwise) are captured once, not renewed on retry, route/signing, page or delay. Child bounds are capped exactly by the original logical root; no rounding-up extension. Multi-request plans and shared/observer completion remain explicit dependencies rather than independent roots. Native status/body/list proof and row/snapshot freshness are still required after exact delivery validation.

## Caller-visible contract

Keep legacy DXtrade account/environment requirements and current product/capability limits. Managed preparation consumes the same configured deadline and session generation; authorization/preparation failure must remain typed and distinct from actual sent uncertainty. Preserve rolling-window burst behavior, synthetic books/candles and estimated balances. Inert generic histories remain explicitly incomplete/unsupported as actually landed, not a new history implementation or live-connectivity claim. Candle HTTP failures already retain classified native fields/outcomes; transport exhaustion is network/notSent. Preserve these pending explicitly reviewed shared normalization. Preserve three network retries delayed 1/2/3 seconds and independent one-time 401 reauthentication; one immutable root spans rolling-window admission, delays and session recovery. FetchLeverage returns null; max-leverage/settings remain unsupported and histories stay inert pending actual incoming corrections.

Each code PR appends a CALLERS section describing only its resulting behavior to `RUST_CUTOVER_CALLERS.md`. Record immutable-deadline and managed-transport requirements, typed failure/uncertainty and retained native schemas; distinguish local-operation `notSent` from proof about earlier physical sends or the whole command. Preserve existing native read error fields, including message-only paths where applicable; any classification/outcome normalization requires separate review. Do not add mutation outcomes to pure reads. No default/product/capability change is authorized by this inventory.

Downstream write-error targets remain the audited order send/create/update/cancel and scaled-order flows plus worker/hosted error receivers in the retained caller ledger. Preserve kind/outcome/native/partial metadata and one terminal notification; unknown writes and reconciliation-only IDs never authorize automatic replay. This lane changes no upstream checkout and claims no NX adoption/deployment or live credentials.

## Verification and independent review

Use `CARGO_BUILD_JOBS=2`. Code gates: focused `cargo test -p safe-cex-venue-breakoutprop -p safe-cex-venue-binance`, scoped fmt and strict all-target Clippy, bans, and both `scripts/check-request-context.test.py` / `scripts/check-request-context.py`. Do not run local WASM release builds, replay sweeps, load or timing gates; CI owns those proofs. Yarn runs through existing NVM Node 22 Corepack.

Original-RED controls must exercise real Prepare/bind/final claim (no early HTTP), public/primary/secondary routing and exact bytes, typed pre-send denial versus network failure, original deadline while parked/retrying, duplicate/late/reused command isolation and retained native successful/malformed/empty results. Preserve existing fixture assertions/event clocks and use shared prepared fixture glue. Validate streaming failed-prefix exclusion and row/freshness effects for any changed reducer; preserve known partial/unknown/no-replay behavior for writes. Host disposal belongs to existing shared host controls, with any missing proof requested from its owner rather than a venue-owned executor.

Independent source/adversarial, caller and full-map reviews must answer: **is this a per-venue fix of a cross-cutting concern?** Expected **No**: native semantics plus shared core adoption. Any missing common mechanism stops that area for its core owner. Check every actual fingerprint before pruning; new/changed legacy calls and exemptions are forbidden. CI must remain watched in a background `gh pr checks --watch`; green CI, clean reviews, actual dependency integration, separate revert guard and an overseer merge window are required.

Documentation checkpoint verification compares this exact map with the unchanged lexical scanner/manifest and checks index registration. It proves neither source migration nor native/CI gates. Future landing changes, especially the occupied PRs above, require a fresh inventory; preserve all incoming repairs and reapply only this venue's exact prune.

## Plan review receipt

Independent source/adversarial, caller and full-map reviews checked all seven family maps against the immutable base and unchanged scanner: 36 paths, 221 entries, 247 occurrences. Caller corrections (structured error fields, exact retry counts, settings and routing) and Polymarket background-candle ownership were incorporated before this commit. All three architecture gates: **No**. This receipt covers the plan only; implementation requires new independent reviews.

## Exact frozen inventory

All locations and source SHA256s refer to the immutable base, including inline tests. Each row is one manifest path/fingerprint entry; a repeated fingerprint in another path is distinct. Locations list every occurrence and its native handler/destination; lexical macro rows are not physical sends.

### `venues/breakoutprop/src/history.rs`

Source SHA256: `2f8a00e1bdc2e2a3ed75e5b9cabc30d29fae4005d7ba97d3eddc1939647db968`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 1 | 7 `query` → LOCAL + HISTORY (inert) | `cx.done(command, "[]".into())` |
| `6d9371872356465432ea20c452fb874b0206be3d463a4a70b1fac22981fd0127` | 1 | 10 `query` → LOCAL + HISTORY (inert) | `cx.fail(command, "BreakoutProp public trades are not implemented")` |
| `05967623e28d0acd558da8192dba9a5d1f571a4e57337bd4a354d59c3a6bbdcc` | 1 | 13 `query` → LOCAL + HISTORY (inert) | `cx.done(command, r#"{"found":false,"definitive":false}"#.into())` |

### `venues/breakoutprop/src/lib.rs`

Source SHA256: `ee0c67b8d929c4c2c7d706b80f46c856b7ac1230766ffc90c9890633e9e10126`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 | 359 `drive` → DISPATCH + SESSION | `cx.request(method, url, headers, body, pending)` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | 772 `fail_operation` → READ; 960 `on_response` → READ | `cx.fail(command, error)` |
| `5ec25581383dfb245bad7d461dd05d709edcb44ba7364f8155f1ca965621bb4b` | 1 | 838 `on_response` → LEXICAL + WRITE | `matches!(&pending.operation, Operation::Trade(request) if request.method == Method::Get)` |
| `a6e5e202ae627122d304a84fc991af3386b3af0c2b063603269df46cb621c442` | 1 | 953 `on_response` → READ | `cx.fail(command, "Invalid DXtrade candles response")` |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | 959 `on_response` → READ | `cx.done(command, data)` |
| `e3e2f080f0b64561a5c15c9e68e0079d0f0a4b8470dac3d45e23b81a28d2c6f1` | 1 | 1110 `on_request_failed` → LEXICAL + DISPATCH + SESSION + WRITE | `matches!(&pending.operation, Operation::Trade(request) if request.method != Method::Get)` |
| `cab66fc0399e6d1f53504198a9a5df6893df31bad853bf84c1dcd416c2326cdd` | 1 | 1140 `on_command` → LOCAL | `cx.fail(command, "Options are not enabled on this account")` |
| `21c9ac01198cc7c45b274eea3e069f2816bde11d64a469cbf3004d3ad50dba47` | 1 | 1187 `on_command` → LOCAL | `cx.fail(command, interval.unsupported("DXtrade"))` |
| `59f4eb1450f816245e4846dac5523bac3355434a96e84659e4222ff3b36fdc8a` | 1 | 1191 `on_command` → LOCAL | `cx.fail(command, "Invalid DXtrade candle request")` |
| `0b656f6ecb232b6e58dda360591ebef081f74c048b07c021b2589b269fb21488` | 1 | 1218 `on_command` → LOCAL | `cx.done(command, "null".into())` |
| `bd4566155b1c765865b133fef29afd2d76d274cecfee331ab72bff4b3eb50015` | 1 | 1219 `on_command` → LOCAL | `cx.fail( command, "Maximum leverage queries are unavailable for DXtrade", )` |
| `6f56d2c3f0ebbed2dcaf7bd7e8a5ecf0fe77c30cd62e82386e73b4494e4c4551` | 1 | 1228 `on_command` → LOCAL | `cx.fail(command, "Unsupported BreakoutProp command")` |
| `684e58b8d404229727a4e3de4ace4062432fcfddb0988492e5d1fdf763a83436` | 1 | 1253 `unanswered` → LEXICAL + WRITE | `matches!(operation, Operation::Trade(request) if request.method != Method::Get)` |

### `venues/breakoutprop/src/trading.rs`

Source SHA256: `f323b17f16f472354cd1b23bb393133af40fda007ad79cefc0b1e13c8d068795`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `9a8b7ae5e9711d4d06df31d94d4caac28f6a37f76228bf555327fff37c588f52` | 1 | 531 `on_trade_command` → LOCAL | `cx.fail( command, not_sent("Breakout Prop uses fixed one-way cross accounts; this setting is unsupported") .kind(Erro...` |
| `00d25809332a0d94366c69ef735773a6dfd0085c08be94fdbac8a23f299b3e48` | 1 | 543 `on_trade_command` → LOCAL | `cx.fail(id, not_sent("Breakout Prop command is already in progress"))` |
| `2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5` | 1 | 549 `on_trade_command` → LOCAL | `cx.fail(id, not_sent(error))` |
| `836d00ef1198446d8fde0e17e23ca7554316c975f03f9e2d91ff3f64a84aad98` | 1 | 673 `advance_trade` → WRITE | `cx.done(id, serde_json::to_string(&job.ids).unwrap_or_default())` |
| `ea0ff1d0bf1689d0410b0bcc1cbbeedc2fa1bec60349440b50760648369edf0e` | 1 | 680 `advance_trade` → WRITE | `cx.fail(id, error)` |
| `28d8459bbb564da129b874b0c4cb9e3d79277650ae1212feca90a9f832a9b3e2` | 2 | 737 `on_trade_response` → LEXICAL + WRITE; 860 `on_trade_failed` → LEXICAL + WRITE | `matches!(request.step, Step::Place { .. })` |


## P0 write adoption, classifier pin 2026-10-23

Reverified actual `258c880656f9714f14b498e046b8ee52658e702a`; venue
production source matches the earlier 6741c1c audit. Plans and tests only.

| Audit row | Reverified native path | Required caller contract |
| --- | --- | --- |
| #18 | `trading.rs::advance_trade` drops job.ids on failure and locally clears a later Rejected/NotSent after effects. | Preserve each accepted native orderId in structured Partial; shared journal chooses the whole-command outcome. |
| #31 | `lib.rs::on_response` special-cases login 401/403 with an untyped error; `fail_operation` therefore emits REQUEST_FAILED and queued dependents receive flattened text. | Preserve AuthInvalid/AuthPermission through the session failure and dependent unsent-work error; emit AUTH_REJECTED without credentials/body leakage. |

No current DXtrade finite error-code classifier exists in venue source:
`http_failure` is a status ladder, while validated native order/update IDs are
success facts. Do not invent meanings for fixture errorCode 2/33. Start one
family CodeTable with conservative unlisted Unknown and only independently
verified existing code/status rows; an empty native refusal list is preferable
to converting all 4xx into effects-free refusal. Strict numeric decoding may
preserve errorCode as evidence without claiming it proves execution refused.
Complete orderId/updateOrderId and native cancel proof remain codec duties;
HTTP success alone is not Accepted. Missing/malformed acknowledgements remain
Unknown and never authorize replacement.

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

### Session boundary, phases and original RED controls

Update (claude-55ao7t, 2026-10-23): the login transport and the 401/403 typed
waiter controls are adopted on the session core, stacked on this PR. The SESSION
provider-presentation and remote-readiness obligations remain OPEN. See
[BreakoutProp password login](RUST_BACKGROUND_SESSION.md).

Every current feed/write depends on password login, selected FULL_TRADING
account and a session generation. claude-55ao7t owns login/acquisition/refresh
Context authority; claude-19pt8s owns credential/provider presentation. Login
is a session mutation, not Place and not Background Read. Its 401/403 typed
auth diagnostics and startup/deactivation behavior must survive the shared
session failure path. Dependent foreground work never released may be NotSent;
that is not evidence that a prior trading command never executed. Keep secret
redaction and one-time 401 recovery bound to the original session generation;
a late old-session rejection cannot retire a newer one.

Single native create/cancel/edit follows actual session/signing ownership;
place-many, cancel batches, cancellation/fill confirmation and replacement need
shared Plan admission. Retain DXtrade's exact rolling five requests per1000ms
and queued retry semantics through the shared scheduler; a fixed200ms minimum
gap is not equivalent. Preserve account routing, If-Match, native orderCode/
orderId distinction, replacement IDs, fill/version fences and reduction sizing.
#1797 currently owns history source/tests; leave those paths untouched.

Append four explicitly ignored controls to unoccupied
`tests/trading_contract.rs`, reusing private Rig and default startup fixtures:
first create accepts orderId46073148, next create loses transport or receives
409 -> one failure retaining that ID, no replay, and Unknown on transport.
Separate 401 and403 login controls queue FetchCandles before login completes;
require one typed dependent failure (AuthInvalid/AuthPermission), AUTH_REJECTED,
deactivation, no second login, and no leaked password/response sentinel.
Run `cargo test -p safe-cex-venue-breakoutprop --test trading_contract p0_write_
-- --ignored --nocapture`. All four must compile and fail original behavior
assertions with no prospective API or credential requirements.

Future GREEN controls: conflicting/missing native IDs cannot accept; known
cancel effects survive later replacement failure; arbitrary 4xx is uncertain;
408/5xx override native refusal; local NotSent after prior effects is not the
whole-command result; duplicate, late, stale-session and wrong-attempt receipts
cannot send/finalize twice. Retain exact rolling-window admission, original
deadline, cancellation proof and one-time session recovery. Remove `effective`
demotion only with full journal adoption, never as a standalone local patch.

Recheck the full inventory above at implementation; this slice prunes nothing,
closes no P0 row and adds no CALLERS landed claim. Gates for adoption: venue +
Binance, scoped strict Clippy/fmt, bans, both ratchets, three clean independent
reviews and exact-head CI. Architecture gate: No. Session, scheduling, signing,
classification and journal ownership stay at shared choke points.

Local plan/test evidence: 4 original-source controls compile and fail
by the intended behavior assertions; normal venue suites pass
(113 tests, 4 explicit ignored RED controls).
Scoped strict all-target Clippy, fmt, bans and both request-context ratchets
pass. Binance's same-base 297 tests passed with the ApeX plan slice. These
controls expose defects; they are not GREEN adoption or P0 closure evidence.
