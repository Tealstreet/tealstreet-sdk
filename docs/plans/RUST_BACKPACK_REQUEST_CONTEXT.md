# Backpack perpetuals RequestContext inventory and plan

Lane `codex-hcmq3f`, overseer `claude-ddi2il`, 2026-10-23. Immutable source base: `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`. Inventory: **7 source paths, 30 path/fingerprint entries, 32 conservative lexical occurrences**. This is a plan, not implementation, CI, live connectivity or cutover proof.

## Scope and first single-request path

Perpetuals only; Backpack Spot remains retired. Keep canonical uint32 caller IDs, base/native sizing, global leverage, fixed hedged/cross modes, trigger repairs, execution-ID dedup and failed/stale snapshot retention.

Request-owned public FetchOrderBook (/api/v1/depth) in src/depth_query.rs and lib.rs is a single read, separate from subscription snapshots. It always carries X-Broker-Id:1200 through get(); this header may not be dropped or replaced by an invented credential/provider ref. Use the reviewed unsigned managed static-header seam only under the overseer-authorized exact #1820 stack; integrate its actual landing afterward.

## Collision and dependency gate

Before editing, open PR file lists were checked with `gh pr list --base feat/rust-rewrite --state open --limit 100 --json number,title,files,headRefName`. Source/test collisions below remain off-limits until actual landing; recheck before every slice. Shared docs/index and exact venue-only inventory prunes are permitted by the overseer.

| Open PR | Occupied source/test files |
| --- | --- |
| #1787 | `sdk-rs/venues/backpack/src/account.rs`; `sdk-rs/venues/backpack/src/trading.rs`; `sdk-rs/venues/backpack/tests/account.rs`; `sdk-rs/venues/backpack/tests/trading.rs` |

ACTUAL LANDING PENDING; reviewed #1820 stack authorized: request_managed_with_headers is not landed; codex-5c57jl owns the shared seam. Required public declaration: X-Broker-Id:1200, exact order/bytes, public intent, no fake credentials/provider. Private Ed25519/identity/remote signing belongs to claude-19pt8s. #1787 owns account.rs/trading.rs and both existing test files. Multi-request seed/history/trading/group admission belongs to shared owners; native status/kind policy goes to claude-dazzy3.

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

1. FIRST after header seam: public depth query only, lib.rs/depth_query.rs plus new tests/request_context_depth_query.rs. Preserve native raw price IDs, level order, zero totals and independent Store/books. New tests must first reproduce missing managed preparation, denial/deadline and late-callback behavior while retaining all existing depth validations.
2. Public streamed klines, startup markets/daily/marks and generation-owned subscription snapshots reuse the actual header API with distinct read owners. Keep generation/sequence/recovery bounds and joined seed publication; do not add a local recovery policy or weaken healthy quote/fill behavior.
3. Standalone account identity in identity.rs adopts signed managed Read after signer/API coordination; keep native accountId/userId selection and no live Store mutation. The shared finish helper serves preflight and delivered response: preserve contextual diagnostics once issued and use the reviewed local-completion seam before issuance.
4. Account/trading source and their existing tests are occupied by #1787. After actual landing, adopt managed reads with the incoming shared snapshot tokens/row policy intact. Keep existing CursorHistory output ownership and native offset progression; context-bearing completion/declared page plans replace only provenance.
5. After signer/group prerequisites, migrate signed single writes, batches, amendments and trigger repair with shared Plan/journal and bounded late reconciliation. Preserve final-fill residual sizing, fresh replacement IDs, optional child-leg reports and supported settings. Inline secondary_tests macros are lexical test matches: rename only fixture bindings and retain every exact signature/header assertion.

Every implementation area gets a committed plan, a separate original-source assertion-RED regression commit, then adoption and assertion-GREEN proof. Keep one venue or venue area per PR. Queue refusals retain the continuation; after authority issuance every failure/result uses that Context, never a numeric fallback. A truly pre-issuance refusal may retain its existing path until the shared local-completion adoption; all remaining inventory still must reach zero.

For every physical read retry preserve native counters/query bytes but require engine retry evidence under one immutable root. Existing single-read transport bounds (20 seconds unless explicitly configured otherwise) are captured once, not renewed on retry, route/signing, page or delay. Child bounds are capped exactly by the original logical root; no rounding-up extension. Multi-request plans and shared/observer completion remain explicit dependencies rather than independent roots. Native status/body/list proof and row/snapshot freshness are still required after exact delivery validation.

## Caller-visible contract

Public book queries remain independent snapshots with exact raw level IDs, order and zero totals; candle JSON/limits and live subscription books stay unchanged. Custom transports must preserve X-Broker-Id at Prepare/bind/final claim. Document typed preparation/timeout/network failures and immutable deadline. Keep explicit settings/product refusals and uint32 caller-ID preflight; later unknown/partial write handling must not replay whole batches. Depth queries have zero automatic retries. Preserve existing structured kind, public notSent, native code/message and capped retryAfterMs; credential-looking public refusals remain non-auth. Preserve ignored caller limit, duplicate prices/server order, lexical IDs, zero quantities/totals, no lastUpdateId and SDK startup-readiness caveat. Shared header seam #1820 is reviewed at cb4b508b569; overseer explicitly authorizes stacking exactly on that reviewed head, followed by actual-landing reintegration. It is not yet landed.

Each code PR appends a CALLERS section describing only its resulting behavior to `RUST_CUTOVER_CALLERS.md`. Record immutable-deadline and managed-transport requirements, typed failure/uncertainty and retained native schemas; distinguish local-operation `notSent` from proof about earlier physical sends or the whole command. Preserve existing native read error fields, including message-only paths where applicable; any classification/outcome normalization requires separate review. Do not add mutation outcomes to pure reads. No default/product/capability change is authorized by this inventory.

Downstream write-error targets remain the audited order send/create/update/cancel and scaled-order flows plus worker/hosted error receivers in the retained caller ledger. Preserve kind/outcome/native/partial metadata and one terminal notification; unknown writes and reconciliation-only IDs never authorize automatic replay. This lane changes no upstream checkout and claims no NX adoption/deployment or live credentials.

## Verification and independent review

Use `CARGO_BUILD_JOBS=2`. Code gates: focused `cargo test -p safe-cex-venue-backpack -p safe-cex-venue-binance`, scoped fmt and strict all-target Clippy, bans, and both `scripts/check-request-context.test.py` / `scripts/check-request-context.py`. Do not run local WASM release builds, replay sweeps, load or timing gates; CI owns those proofs. Yarn runs through existing NVM Node 22 Corepack.

Original-RED controls must exercise real Prepare/bind/final claim (no early HTTP), public/primary/secondary routing and exact bytes, typed pre-send denial versus network failure, original deadline while parked/retrying, duplicate/late/reused command isolation and retained native successful/malformed/empty results. Preserve existing fixture assertions/event clocks and use shared prepared fixture glue. Validate streaming failed-prefix exclusion and row/freshness effects for any changed reducer; preserve known partial/unknown/no-replay behavior for writes. Host disposal belongs to existing shared host controls, with any missing proof requested from its owner rather than a venue-owned executor.

Independent source/adversarial, caller and full-map reviews must answer: **is this a per-venue fix of a cross-cutting concern?** Expected **No**: native semantics plus shared core adoption. Any missing common mechanism stops that area for its core owner. Check every actual fingerprint before pruning; new/changed legacy calls and exemptions are forbidden. CI must remain watched in a background `gh pr checks --watch`; green CI, clean reviews, actual dependency integration, separate revert guard and an overseer merge window are required.

Documentation checkpoint verification compares this exact map with the unchanged lexical scanner/manifest and checks index registration. It proves neither source migration nor native/CI gates. Future landing changes, especially the occupied PRs above, require a fresh inventory; preserve all incoming repairs and reapply only this venue's exact prune.

## Plan review receipt

Independent source/adversarial, caller and full-map reviews checked all seven family maps against the immutable base and unchanged scanner: 36 paths, 221 entries, 247 occurrences. Caller corrections (structured error fields, exact retry counts, settings and routing) and Polymarket background-candle ownership were incorporated before this commit. All three architecture gates: **No**. This receipt covers the plan only; implementation requires new independent reviews.

## Exact frozen inventory

All locations and source SHA256s refer to the immutable base, including inline tests. Each row is one manifest path/fingerprint entry; a repeated fingerprint in another path is distinct. Locations list every occurrence and its native handler/destination; lexical macro rows are not physical sends.

### `venues/backpack/src/account.rs`

Source SHA256: `7f3621902fec9c2e1be2616f635ade2c4cc865a38098197b82e216678acd1f6b`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `90455717d42b4ec702a3daffd7a277887a30622046104a208dc55b3e8202992c` | 1 | 348 `signed` → DISPATCH | `cx.request_streamed(method, url, headers, ArrayAt::Root, pending)` |
| `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 | 350 `signed` → DISPATCH | `cx.request(method, url, headers, body, pending)` |

### `venues/backpack/src/depth_query.rs`

Source SHA256: `3f14fe42431762b0d0ced632a4e6a7c7a10736afd0a588a18217772f3aab855f`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `f6b3252151a3765eb2f388fe2cea072c5f4cca93182d86cd2e3d1f143ec9b8d1` | 1 | 98 `book_query_response` → READ | `cx.fail( command, failure("Invalid Backpack order-book envelope", ErrorKind::Internal), )` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | 150 `book_query_response` → READ; 171 `book_query_response` → READ | `cx.fail(command, error)` |
| `7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b` | 1 | 170 `book_query_response` → READ | `cx.done(command, json)` |

### `venues/backpack/src/history.rs`

Source SHA256: `e3c2f598339e3a5ecd5999b110e07ed679251ae377b047908173b3a8eab7e0b1`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `dfd77b4e5b63c20a2980a0daf2a87b52d71b54e7389a0b8ab4b83cc09f4e9c44` | 1 | 149 `history_command` → HISTORY + LOCAL | `cx.fail(command, "Backpack client-order-ID lookup is unsupported")` |
| `9b3adb3b1766d2dc6999e2d6032c71c0af3cd853931092aa839972828fe32016` | 1 | 189 `history_command` → HISTORY + LOCAL | `cx.fail( command, "Backpack income query supports funding payments only", )` |
| `3305fa21e56e82fd162910828b2249d986e2b5557f6c0e7b8475dccd31d4e12f` | 1 | 219 `history_command` → HISTORY + LOCAL | `cx.fail(command, "Backpack invalid history time range")` |
| `465c0d77039125ad56cc26f76f44ea3cdc2befb62f5d1badbb8062f48908811c` | 1 | 226 `history_command` → HISTORY + LOCAL | `cx.fail(command, "Backpack history market not found")` |
| `0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb` | 1 | 302 `history_request` → HISTORY + LOCAL | `cx.fail(command, e)` |

### `venues/backpack/src/identity.rs`

Source SHA256: `addc4833ae6bc3aff3f03a1cbad9eb2dc83ec077c5d039172a38658fbf10e850`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | 66 `finish` → READ + LOCAL (identity) | `cx.done(command, data)` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | 69 `finish` → READ + LOCAL (identity) | `cx.fail(command, error)` |

### `venues/backpack/src/lib.rs`

Source SHA256: `cc5b004d2e5d658f05a4a1164e2313584ead9da739644a0853d7a218fe26c74a`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `9cce7db7c9b199779663966a17b3855d01e71c0b43228a87d1e38cc70b74cd6a` | 1 | 252 `get` → DISPATCH | `cx.request_streamed(Method::Get, url, headers, ArrayAt::Root, pending)` |
| `9d039db0b83b2ae54446aa0bc9e3aff9127670a816554104c007a106d268becd` | 1 | 254 `get` → DISPATCH | `cx.request(Method::Get, url, headers, None, pending)` |
| `9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d` | 1 | 988 `on_response` → READ | `cx.done(command, serde_json::to_string(&rows).unwrap())` |
| `d0f067540be6f5b7aa8c5b2d02f4a4ace669920eb11cabb79b1d5bb8b18db63e` | 1 | 989 `on_response` → READ | `cx.fail(command, "Backpack invalid candles")` |
| `56d848e442eb914c45e3641ce58f354c9b36dd108d1f0bd57c97c4a58794d4a5` | 1 | 1003 `on_request_failed` → READ | `cx.fail( command, depth_query::failure(error, safe_cex_engine::ErrorKind::Network), )` |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | 1023 `on_request_failed` → READ | `cx.fail(command, error)` |
| `22c49186907c8c49d8df31f258aba554f02baed35fa741626b157584432ea540` | 1 | 1134 `on_command` → LOCAL | `cx.fail(command, interval.unsupported("Backpack"))` |

### `venues/backpack/src/secondary_tests.rs`

Source SHA256: `78732a3b57949821f5ede20fafc8e682c15a4aa694d50cda7ac1ef7484e7ceb8`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `b248430824b5c2fbbc2f727d0c081665b367a7847b5d1ef04aa51186b26ee862` | 1 | 71 `secondary_get_keeps_primary_mutations` → TEST | `assert_eq!( request.headers, credentials.headers(instruction, &[Default::default()], 1700000000000.0) )` |
| `765fe706f004e024e6e68fce224723e35453a1d36fd475e7ce2590fd3e5e72ca` | 1 | 75 `secondary_get_keeps_primary_mutations` → TEST | `assert!( request .headers .iter() .any(\|(name, value)\| name.eq_ignore_ascii_case("X-API-Key") && value == expected), ...` |

### `venues/backpack/src/trading.rs`

Source SHA256: `c8b68c5ee9554b9de0dc867aed79a159f4c1350e4e512d13315d054bfcca65a5`.

| Fingerprint (SHA256) | Count | Base locations / native handler → engine destination | Frozen native expression |
| --- | --- | --- | --- |
| `bcd45e2e77893c71604dfc34d2a327ce254b90adc4e9178be8643e022a6f4219` | 2 | 503 `trade_command` → LOCAL; 508 `trade_command` → LOCAL | `cx.fail(command, error.kind(ErrorKind::Unsupported))` |
| `b70e9c2e2d2a7b5bf31d71625ed006497f17176e855c618b929752eb3c7d6ab8` | 1 | 515 `trade_command` → LOCAL | `cx.fail(id, error.kind(ErrorKind::AuthInvalid))` |
| `c4fdc5b119a2342e4494f8ba9691545abcda5a6eb945131bf4ee4639cfa3257c` | 1 | 519 `trade_command` → LOCAL | `cx.fail(id, not_sent("Backpack command is already active"))` |
| `349d1b39839519801db71f4348af20496c0443cb14880093d11ecbbe7cc68843` | 1 | 546 `trade_command` → LOCAL | `cx.fail(id, not_sent(e).kind(ErrorKind::InvalidInput))` |
| `a9da85dc9c6ad70f0b610a5315cdae2d3668c519171a26feee437a0ae17ca3ba` | 1 | 558 `run_trade` → WRITE | `cx.done( command as CommandId, if job.collect_ids { serde_json::to_string(&job.ids).unwrap() } else { "[]".into() }, )` |
| `19212dbca6ffa52bf2e412ea152d5a6dfda29048f1fb441f7a9ec4c1ac133e2b` | 1 | 595 `run_trade` → WRITE | `cx.request( Method::Post, format!("{REST}/api/v1/orders"), headers, Some(serde_json::to_string(rows).unwrap()), Pendi...` |
| `a1c7fc500594612cd32082bb8ee3d5a96a6c5843791fcdd8818e1b60bcf53b7a` | 1 | 698 `run_trade` → WRITE | `cx.fail(command as CommandId, error)` |
| `9ecfe514716e00b34e5c7703e920e8dc9024caab0837bbf2949a3a78d4eb842b` | 1 | 937 `trade_failed` → LEXICAL + WRITE | `matches!( request.step, Step::Place { optional: true, .. } \| Step::Cancel { optional: true, .. } )` |
| `2364edc99329ee0ea1944828c497c7adcec199804350a5db2811f2906f0e4d92` | 1 | 954 `trade_failed` → WRITE | `cx.fail(request.command as CommandId, error)` |


## Phase one: single command public depth on actual header admission

Base: actual `2b1e35e50b2095c6651344fc5ffe0553ad456f35`, including command entry #1830 and static headers #1820.
Scope is standalone FetchOrderBook only: lib.rs BookQuery entry/pending/response/
failure dispatch, depth_query.rs completion authority, a dedicated managed test
and the two narrow native query fixture drivers. Keep account/trading files and
tests occupied by #1787 untouched; keep generic get(), discovery, history and
subscription snapshot/recovery helpers unchanged.

Admit a genuine public Read before request allocation, with one 20-second root,
no credential, no pacing group and zero retries. Declare the exact static
X-Broker-Id:1200 header through request_managed_with_headers; retain GET, native
symbol encoding, no body and route None. Carry Context in BookQuery, validate
exact delivery before native decoding and settle through contextual completion.
Preserve the existing public response/status codec, native code/message and
capped Retry-After. Contextual preparation, transport and native failures keep
the approved public NotSent presentation without changing their typed kind.
Initial engine admission rejection is the explicit new exception: it is a Read
failure with no financial outcome; revoked entries grant no completion rights.

Regression controls first must assert original-source RED at early opaque HTTP,
then prove headers through Prepare/Bind/reprepare, exact parked/released expiry,
no retries, typed refusal/integrity failures, duplicate/reused IDs and initial
clock/capacity rejection. Preserve every native book oracle: all limits ignored,
lexical IDs/server order/duplicate prices, zero sizes/totals, empty sides,
whole-query failure, reverse concurrent completion and independence from
subscription generations, Store caches and account auth. Keep fixture event
clocks and all existing business assertions.

Expected ratchet delta is only the BookQuery failure in lib.rs and four direct
completions in depth_query.rs. The generic legacy sender remains for other
consumers. The registry capability probe hits this new managed path; until
#1818 lands, carry only its explicitly authorized identical generic fixture
commit d654816753b3881b8ea0704d5758c65f7687a30f, without importing Crypto.com source.
Re-integrate its actual landing before final proof when available.

Append CALLERS including the initial-admission exception and the existing SDK
startup-readiness caveat. Run Backpack/Binance full native tests, scoped strict
Clippy/fmt, bans and both inventory checks. Obtain source/adversarial, caller and
full-map reviews, a fresh exact integration CI proof, and a separately granted
venue merge window. No venue classification, scheduler, signer or authority shim
is introduced; TxFlow routing remains separately blocked on actual #1833.


Phase-one checkpoint: `4bf299b52af` compiles all nine new lifecycle controls and
all nine assert RED on early opaque HTTP before source adoption. The unchanged
controls pass after adoption, including header re-preparation, typed refusal,
parked/released expiry and initial rejection. Existing native query drivers use
the shared prepared fixture helper at their original command clocks; business
oracles and response clocks remain. Exact generic registry carry is `e83c805ae02`
from authorized `d654816753b3`; no other Crypto.com source is imported.

The scanner prune removes three depth_query.rs entries/four occurrences and one
lib.rs BookQuery failure entry/occurrence. Backpack becomes six paths, 26 entries
and 27 occurrences (from seven/30/32); all other inventory maps remain intact.
Full native/static gates and independent implementation reviews are recorded on
the PR before readiness. Source/caller plan reviews confirmed the initial
admission outcome exception and architecture gate No.


## P0 write adoption, classifier pin 2026-10-23

Audited actual `6741c1c6609437e32090c76745a56ca6c2262445`. This appendix
supersedes earlier draft WRITE API names. It adds plans/regressions only.

| Audit row | Reverified native path | Required caller contract |
| --- | --- | --- |
| #2 | `trading.rs::trade_command` marks every explicit CancelOrders step optional; `trade_failed` logs and advances; `run_trade` completes Done([]) even when all cancels failed. | Each requested leg contributes its own fact; all-failed cancellation is a terminal failure, never successful empty output. |
| #20 | Pair response validation exits on any invalid/failed row before copying accepted IDs from the response into the job. | Preserve each independently validated native ID as reconciliation evidence on the failure; missing or contradictory rows stay uncertain. |

Current `http_failure` distinguishes AuthInvalid/Permission/RateLimited by
status only; Pair has `operation`, `clientId`, `id`, symbol/side/type identity
facts. Do not manufacture a native code table by promoting arbitrary statuses
or exception strings. Start one CodeTable with explicit unlisted Unknown and
only existing finite native code distinctions whose refusal evidence is
verified; empty rows/statuses is safer than guessing. Move native error `code`
fields as typed codes when present; `operation: Ok` alone is not a complete
success. The Pair codec must validate correlation/uniqueness before claiming
caller attribution, while retaining independently valid IDs even when row
count is wrong. Do not infer succeeded/total/failed_index for incomplete ACKs.

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

### Phases, file reservations and RED controls

Single ordinary placement/cancel comes before paired placements, optional
bracket children, cancel-many and amend cancel/replace plans. Managed signing
needs claude-19pt8s's Backpack recipe plus actual Ed25519 provider #1844; its
recipe branch is based on the landed depth slice and does not own lib/account/
trading conversion. No eager Ed25519 through unsigned static headers. Declare
native X-Broker-Id/body/credential identity exactly through managed signed
preparation. Multi-leg commands wait for genuine shared Plan admission.
Preserve native receipt timing, UINT32 client IDs, minimum sizing, generated
replacement identities, bracket relationships and cancel/fill races.

At this audit #1787 holds account/trading source and their test files; #1870
holds lib.rs and candle/public tests. This slice touches none of them. Add a
new `tests/write_adoption_red.rs` using the unchanged existing support fixture:
(1) stored order 123, explicit CancelOrders, native HTTP refusal -> one failure
and no Done; (2) Pair returns a valid first row plus failed second row ->
Unknown with first ID preserved; (3) Pair returns only the valid first row ->
Unknown with evidence-only Partial, no invented total or failed_index.
Read actual generated clientId/side values from the request, never hardcode
random identities. Run `cargo test -p safe-cex-venue-backpack --test
write_adoption_red p0_write_ -- --ignored --nocapture`. All three must compile
and fail their original behavior assertion; the ignore markers explicitly
await adoption, not a passing production claim.

Future controls include all cancel legs failing transport, mixed required and
optional child failures, reversed ACK order, duplicate/unknown client IDs,
contradictory symbols, 408/5xx refusal-looking bodies, prior accepted effects
followed by preparation NotSent, and duplicate/late/wrong-attempt delivery.
Known partial definitive effects need structured IDs without forcing Unknown;
unresolved legs require Unknown. Shared CommandWrites, not optional-step
logging or `effective` demotion, owns the whole-command verdict.
Recheck the frozen inventory above on adoption, prune only actual vanished
Backpack entries and add CALLERS then. No manifest change in this slice.

Architecture gate: No for native Engine regressions and shared API planning.
Future implementation gates: Backpack + Binance, scoped strict Clippy/fmt,
bans, both ratchets, three clean independent reviews and exact-head CI.

Local plan/test evidence: 3 original-source controls compile and fail
by the intended behavior assertions; normal venue suites pass
(137 tests, 3 explicit ignored RED controls).
Scoped strict all-target Clippy, fmt, bans and both request-context ratchets
pass. Binance's same-base 297 tests passed with the ApeX plan slice. These
controls expose defects; they are not GREEN adoption or P0 closure evidence.

## Foreground write adoption implementation plan (2026-10-24)

Selected from #1880 on actual landing `43775f60d0f`: independent foreground
jobs have no FIFO activation dependency on #1896. Merge the reviewed plan/tests
first; retain all three original behavioral assertions. Source reservations:
`trading.rs`, the Options/sign-module and trade callback regions of `lib.rs`,
`Cargo.toml`, native trading fixtures and dedicated write controls. Our #1870
candle regions remain separate and unchanged. The signer owner supplies or
reviews the dispatch recipe in `sign.rs`; provider Ed25519 is #1844. No venue
or shared source belonging to another open PR is replaced.

1. Begin genuine mutation Preflight before native preflight terminals. Admit
   one immutable serial Plan from the final native steps, Primary credentials,
   no retry and an explicit 20-second managed attempt policy. Legacy opaque
   HTTP declares `timeout_ms: None`; this adds a bounded managed lifetime, not
   a claim that native commands already had a 20-second root. The checked Plan
   root is the declared serial attempt span, never a venue timer. Empty work uses the engine's
   explicit empty-plan completion. Every pending foreground call retains its
   exact Plan/Slot/Context; Plan-ready drives only that job. No queued owner is
   activated through another job's delivery.
2. Replace eager credential headers for foreground writes with the shared
   Backpack dispatch recipe, preserving instruction, caller-ordered batch rows,
   canonical parameters, exact JSON body, X-Broker-Id and receive window. Pass
   only an opaque identity through Options. Never populate it from raw secrets
   inside the venue. Test setup supplies the provider and identity explicitly;
   registry/SDK admission and account/WS migration remain separate obligations.
3. Use one conservative CodeTable (existing codes only, unlisted Unknown,
   no inferred status refusal). Validate exact Plan delivery before native
   effects. Record accepted/failure/batch facts through the shared classifier
   and journal before call settlement. A paired ACK correlates clientId,
   symbol, side, type and unique native IDs; individually validated rows retain
   their native IDs even if another row fails or is absent. Only a complete
   proof publishes the existing paired success reducer.
4. Every requested explicit cancellation contributes to the command verdict.
   Continue native optional child work for callback-delivered native failures, while its
   failure remains in the journal: a final Done cannot erase it. Native optional
   child failures can therefore produce a failed command with prior effects,
   rather than a misleading successful empty result. Remove local effective
   demotion for foreground jobs; the journal owns partial IDs and uncertainty.
5. Amend fill exhaustion uses the landed immutable conditional-terminal seam.
   Keep cancellation/fill revision fences and receipt order. Cleanup retires
   only the exact foreground generation and releases its cancellation latches;
   no stale callback completes a reused ID. Internal automatic trigger repair
   remains explicitly legacy background work, with no fabricated command or
   Context; background Plan issuance is a separately assigned shared follow-up.
   Its legacy transport sites remain inventoried, with no manifest exemption.
6. Add managed-release, asynchronous delegated Ed25519, malformed/short/reversed
   batch, failed-cancel, accepted-ID/later-failure and generation controls. Turn
   #1880's original REDs green without weakening assertions. Prune only vanished
   foreground fingerprints after scanning actual source; preserve incoming and
   background entries. Run Backpack/Binance, scoped strict Clippy/fmt, bans,
   both request-context checks, three independent audits and exact-head CI.

Architecture gate: native request/ACK correlation and Store freshness stay here;
command ownership, clock, preparation, signing, retry, classification, aggregate
outcome and cleanup authority remain engine/provider choke points. This is a
foreground trading slice, not whole-family signer or background readiness.


Plan-audit rulings: the overseer explicitly approved today's shared core rule:
a delivered 408/5xx immediately stops the Plan as Unknown, retains prior IDs and
retires unsent legs. Callers must reconcile before resending. Adopt the upcoming
shared PlanPolicy independence flag when it lands to restore native optional/
continue-on-error continuation; this remains an open obligation, with no venue
fallback. The explicit new 20-second managed attempt policy is also approved.

Before first managed mutation release, capture bounded native correlation data
through capture_http_reconciliation and define the conservative late policy.
Never publish late rows without the independent native account revision fences.
Add an original optional-child RED: accepted entry, failed TP, accepted SL still
emits CHILD_LEG_FAILED and sends SL, then one failed terminal with both accepted
IDs rather than Done. Preserve native wire/sequence oracles. Classified DATA
settles via finish_plan_call; callback-delivered preparation/transport failures
use finish_plan_failed_call if continuing, never fabricate current DATA.

### Foreground write implementation checkpoint

PlaceOrders, CancelOrders, CancelSymbolOrders, AmendOrder and SetLeverage now
retain a genuine Preflight and admit the final native serial steps as one Plan.
Each attempt has a new explicit 20,000 ms bound, one attempt and no resend; the
engine derives the immutable root from those declarations. Independent jobs do
not borrow FIFO readiness. Empty native work completes through done_empty_plan.

The shared journal owns all verdicts and the terminal. One conservative CodeTable
has no undocumented refusal rows: unlisted native codes remain Unknown. Partial
Pair acknowledgements retain only independently correlated native IDs, without
inventing a total or failed index when the reply is incomplete. Every requested
cancel contributes its failure, so all-failed cancellation cannot return success.
An optional child answered with a native 400 can continue to its native sibling,
but the final failure retains both earlier and later accepted IDs. A racing fill
that exhausts an amendment uses the declared conditional terminal after the
exact validated cancellation DATA.

The current shared core stops on delivered 408/5xx, retires unsent suffix slots
and returns Unknown with prior IDs. Pure native capture preserves code/message
and exactly correlated IDs without success, Store, retry or authentication
rights. Reconcile before resending. Adoption of PlanPolicy independent_calls to
restore native continue_on_error/optional-child continuation is an OPEN follow-up
when that shared policy lands; there is no venue fallback.

The dispatch recipe owns the canonical instruction/body and headers. Native
GET, single, batch and cancel proofs are byte-equal to legacy signing; a real
CancelOrders command also carries a fixed independent Node Ed25519 vector and
remote-only public-key sentinel through asynchronous Sign/Signed dispatch. No
HTTP is emitted before Signed, and a duplicate receipt is inert. This proves the
native delegated-provider path, not the SDK/browser remote adapter or admission.
The five-second recipe freshness is the queue refresh bound within X-Window.

Only a trusted opaque Options.identity can authorize managed foreground writes.
The registry's mechanical identity=None literal does not provide admission: raw
legacy credentials cannot substitute for a managed identity and refusal occurs
before write I/O. Registry/SDK setup, account/startup/session presentation and
background repair signing remain OPEN separate work. Internal automatic repair
retains its legacy sender and response contract. Local unsupported-setting tails
remain local; no command owner is fabricated for internal repair.

Typed pre-send cancellation refusal restores optimistic state only when its
native stream stamp is unchanged. Each job releases only its own cancel-latch
contribution, including retirement and internal preparation failure. Late-response
capture retains bounded operation correlation; cancelAll records the symbol,
not an unbounded snapshot of Store IDs. Late delivery does not publish Store
changes or another command terminal. Original four RED assertions are active;
new controls cover missing identity, amend refusal versus a newer fill, native
metadata/prior IDs on 408/5xx, and the real asynchronous cancel.

Exact legacy inventory change: trading.rs ten occurrences become three (the
internal repair sender and two local unsupported-setting refusals). Other
Backpack paths remain unchanged. No exemption or inventory addition is used.

Signer review tightened the implementation before readiness: managed admission
and receive-window selection now depend only on the opaque identity and Options,
not on legacy account credentials. The actual asynchronous cancel proof uses a
separate venue constructed with key=None and secret=None; only non-secret market
and order rows are copied from the native fixture. That proof originally failed
before removing the account-credential dependency and passes after the fix.

Automatic trigger repair is explicitly the legacy account path, not migrated by
this foreground slice. Without legacy account credentials it stops before job
creation or I/O, with an account-signing-pending notice. Existing legacy account
fixtures retain their repair behavior. The owned ignored RED
legacy_account_repair_must_adopt_dispatch_signing_before_account_cutover fails on
the actual legacy sender and belongs to codex-hcmq3f's subsequent account/background
Plan and signer adoption; it must be un-ignored in that adoption. No private
account, automatic repair or whole-family signer readiness is claimed here.

Duplicate native body fields are refused by the canonicalizer, rather than
letting a map silently select one value. The adversarial duplicate-symbol control
originally released a request and now refuses it before I/O.

Public-data-only mode remains unable to mutate even with a trusted identity.
Its original regression issued managed work instead of a terminal; the explicit
mode refusal now returns NotSent before any write preparation. Identity-only
local signing also pins the declared 5,000 ms receive window, and the repair
cutover RED matches the exact native cancel preparation rather than unrelated
managed polling.

## Transitional SDK admission custody (owner ruling, 2026-10-24)

Backpack admission uses option B, explicitly authorized by claude-ddi2il. The
trusted registry setup supplies the managed trading identity and provider while
retaining raw credentials ONLY for the existing unadopted account reads/private
WS and their legacy account-repair scope. This temporarily retains venue-held
material alongside a provider copy; it is not whole-family signer migration.
Managed foreground writes still require the opaque identity and sign exclusively
through the shared provider at send time. There is no raw/eager trading fallback.

construction-rules dispatchSigning stays off and TS construction is unchanged.
SDK/browser delegated signing readiness is not implied by local admission. The
registry must register the SAME identities it gives the venue; externally supplied
providers cannot silently fall back to a local signer. #1946 stacks on m26ps4's
reviewed admission PR, with the scoped recorded prepareHttp/PrototypeTransport
fixture carry. This child now integrates reviewed #1964 at 9aa317fc3ae. Its actual landing
remains pending; this stack alone claims no SDK/browser remote readiness.

REMOVE BEFORE MASTER: codex-hcmq3f owns account/WS/background repair adoption,
which removes the retained raw fields and ends dual custody. That cutover uses
ref-only trusted setup and enables the construction declaration only with actual
SDK/native acceptance. No new raw-material consumer or permanent fallback is
allowed under this transition. The remaining inventory and owned repair RED
continue to track that obligation.


## Phase two plan: unsigned command candle stream

Base: actual `8d57199e884`, including landed public depth #1860 and command-entry follow-ups. Fresh PR file lists leave lib.rs and tests/public.rs clear; private account/trading source/tests remain occupied by #1787. The signer lane owns a future sign.rs recipe; this unsigned /api/v1/klines path does not use it or edit sign.rs.

For supported FetchCandles, admit genuine Read/no credential/20 seconds/no pacing/Never retry. Carry Context in Pending::Candles, declare the same GET /api/v1/klines native sign::query and exact static X-Broker-Id:1200 through request_managed_with_headers, then register ArrayAt::Root through existing stream_response. No engine API, fake identity or streamed-header workaround is needed. Exact read delivery must precede both item parsing/malformed diagnostics and response/status handling. Finish through done_context/fail_context. Typed preparation errors preserve the engine kind and local NotSent fact; released HTTP failures retain kind with no outcome; native non-2xx/string/malformed-row messages retain their existing fields. Book-query NotSent compatibility is untouched.

Preserve native symbol/interval encoding and lexical parameter order, floor startTime/endTime to seconds, ignored limit, no local range validation, server row order/duplicates, existing millisecond ISO/numeric response timestamps (no extra conversion), finite JSON numbers, negative/zero values and empty-list success. Preserve native whole-list failure after a malformed row and its STREAM_MALFORMED diagnostic; no valid prefix may complete after failure. Unsupported interval stays before admission. Do not change public WS candles, startup get(), depth recovery or private/account/history behavior.

Commit original-source regression tests before production edits. Thirteen controls cover exact header/query through prepare/bind/reprepare, typed refusal, parked/released root expiry, zero read retries on network/integrity failure, duplicate/concurrent/reused owners, initial invalid-clock/capacity rejection, unchanged native rows/order/no local bound filtering and ignored limit, malformed-row prefix exclusion, late malformed stream exclusion and non-2xx prefix failure. Keep assertions unchanged after RED. Update only the existing public candle fixture to shared prepared_actions at its original 0ms declaration clock; keep response clocks and every business assertion.

Five planned paths: this appendix, CALLERS, manifest, lib.rs and tests/public.rs, plus new tests/request_context_candles.rs (six total). Remove only lib.rs fingerprints 9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d, d0f067540be6f5b7aa8c5b2d02f4a4ace669920eb11cabb79b1d5bb8b18db63e and c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa, each count one. Generic get senders and unsupported preflight stay. Family six paths/26 entries/27 occurrences becomes six/23/24; no other map changes.

Run focused lifecycle tests, full Backpack/Binance native suites, strict all-target Clippy, scoped fmt, bans and both request checks. Three independent plan/implementation reviews must verify architecture gate No (shared admission/header/stream/lifecycle APIs, native codec facts only). Fresh CI, separate guard and overseer parallel window are required. This slice claims no write-classification, signer, session, group, background or local-tail closure.


Phase-two blocked checkpoint: plan a26b317e68f precedes separate test commit ea55c2e03b8; all thirteen controls compile and assert RED on the original opaque candle path. Plan audit clarification 8b7c29b344b changes one test name but no assertion, preserving millisecond response timestamps versus floor-second request bounds. Actual MEXC #1858 (`0d3f1dfa9bf`) was genuinely integrated before source adoption.

The narrow candidate passes ten controls. Three controls remain RED because shared preparation failures unconditionally add NotSent to Reads (`http_lifecycle::http_preparation_failed`, affecting parked expiry and typed route refusal), and an unfinished non-UTF-8 ArrayAt::Root stream takes the untyped on_stream_malformed callback before the typed envelope check. Exact evidence is reported to the error-core owner and overseer. No venue outcome clearing, error-string classifier, engine edit, assertion weakening or readiness claim is added. The candidate needs a shared contract decision/fix; the final CALLERS contract follows that decision before any merge.

Strict all-target Backpack/Binance Clippy, scoped formatting, bans and both request checks pass. Scanner matches the exact three-entry/three-occurrence prune. Caller and full-map audits found no additional issue; source audit is recorded separately. No full native-suite or CI pass is claimed for this blocked candidate. Original BookQuery presentation, private/account/trading/history/signing, startup/recovery and all fixture business assertions remain intact.


Core adjudication supersedes the first checkpoint's preparation interpretation: pre-release NotSent is a true local attempt fact for Reads too, not a classification of a released Read or a whole-command money claim. The error-core owner explicitly requires preserving it. Two assertions are corrected to that pinned engine contract; no adapter outcome clearing is introduced. The unfinished non-UTF-8 stream is a real shared gap, fixed on the owner's reviewed/frozen #1857 head 1947e6b2064 through the default on_stream_malformed→typed on_http_failed path. Exact reviewed stacking is authorized by the owner; actual #1857 remains mandatory before any final venue proof/merge after the current venue phase. No engine source will be edited by this lane.


Corrected-contract proof at 52077fd6268: the revised assertions were rerun against original actual 0d3f1dfa9bf source and all thirteen still assert RED. The restored candidate passes twelve with only the genuine streamed-integrity gap failing. The reviewed exact #1857 1947e6b2064 head is then genuinely merged, touching only its four shared files and leaving the entire owned Backpack tree byte-identical. The owner later announced 289f1c02887 as a comment-only successor and explicitly confirmed 1947 runtime equivalence. This does not claim actual #1857 availability; final integration/CI and a six-own-path guard remain required after the seam lands.


Reviewed-seam verification checkpoint: all thirteen corrected lifecycle/native controls and the full Backpack/Binance suites pass (447 tests, zero failed/ignored), together with strict all-target Clippy and scoped formatting. New train inventory prunes were genuinely integrated; own Backpack, engine seam, crypto and Binance trees stayed byte-identical to the tested ff8d6c11a70 head. The first inventory retry passed its scanner controls but stopped correctly at a newer incoming Lighter prune; the initial pass wording was premature. A final ratchet receipt follows genuine integration of that incoming prune. Source/adversarial and caller audits are clean; the pinned full-map audit confirms six own plus four inherited seam files, unchanged original assertions except the explicit contract correction, and architecture gate No. This is reviewed-stack evidence only, not an actual-#1857 or final CI/merge claim.


Final reviewed-stack ratchet receipt: actual Lighter #1807 (`8e0726f7675`) is genuinely integrated at 11bf92f61ef. Bans and both request checks exit zero there, including all fourteen scanner controls; only the intended three Backpack fingerprints are removed. The entire owned Backpack, reviewed engine, crypto and Binance trees remain byte-identical to the 447-test/strict-Clippy/fmt verified source. The earlier premature gate statement was corrected before this receipt. Actual #1857 and fresh final CI/guard remain mandatory; no merge readiness is claimed.


## Candle slice adoption and actual-seam integration, 2026-10-24

The overseer transferred #1870 from hcmq3f to mxb4mq; hcmq3f confirmed the
handoff and stopped edits. `4a82e0e1ee0` plain-merges actual `be710b3f2d7` into
`0399ba95a3b`. Actual #1857 `867896f3e25` is an ancestor of that landing, so
the earlier reviewed-stack blocker is historical and is now satisfied. All
incoming engine/registry/private trading changes are retained. The only source
conflict kept the incoming Trade failure arm first and the original Candles
Context failure arm verbatim, as source-cleared by the previous owner. Shared
plan/CALLERS conflicts retain incoming first plus the exact candle suffix once.

The six own paths and exact three one-count lib.rs removals remain the scope;
all other manifest maps and unowned paths equal the incoming landing. Corrected
regression controls are byte-identical to `52077fd6268` (SHA-256
`ebd8db948d29c1256f3160a931ef8527f47e86dbf9b1686a9f3e46d82d179505`).
The preserved original-source receipt records all 13 assertion REDs on
`0d3f1dfa9bf`; the current integrated source passes those same 13 controls.
No assertion or native candle behavior is changed during adoption.

Local verification is scoped to Backpack with CARGO_BUILD_JOBS=4 under the
overloaded-machine instruction, superseding the earlier broad two-crate local
suite. The Backpack suite passes 173 tests with zero failures; one pre-existing
trading repair control remains ignored under its separate Plan migration gate.
Formatting and bans (including inventory scanner/ratchet controls) pass. Fresh
full CI and exact committed preservation review remain readiness gates; no
merge window or source-equivalent CI claim is inferred from old evidence.
