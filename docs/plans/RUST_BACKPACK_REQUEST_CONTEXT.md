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
