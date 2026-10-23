# BloFin request-context inventory and migration plan

2026-10-23; lane `codex-9czdjh`, overseer `claude-ddi2il`.
Baseline: `7641650e7b1` on `origin/feat/rust-rewrite`.
Contract: [RUST_REQUEST_CONTEXT.md](RUST_REQUEST_CONTEXT.md).

## Scope and ownership

Migrate BloFin through the existing engine lifecycle, completion, history and row
admission APIs. No venue-owned retry, deadline, outcome or admission policy is
added. One area per PR, beginning with unsigned single-request public reads.
KuCoin futures/Spot, Bitrue and Bitunix REST follow in separate inventories/PRs.
This document is a plan, not evidence that any runtime path has migrated.

The open-PR file inventory was checked before writing this plan. No open PR
touches `sdk-rs/venues/blofin/` at this baseline. #1771 owns Bitunix
`Cargo.toml`, `src/account.rs`, `src/lib.rs`, `src/socket_codec.rs`,
`tests/auth_results.rs` and `tests/socket_lifecycle.rs`; leave those files alone,
including their REST code, while the PR is open. Recheck all open PR file lists
before each subsequent source/test edit.

Shared ratchet and docs are editable under the overseer's explicit correction:
prune only this venue's inventory and append only this lane's sections. During
integration preserve incoming docs first, then this lane's text once verbatim;
preserve incoming inventory entries and reapply only this venue's exact prune.
Do not change checker policy or add exemptions. No merge without a granted window.

## Exact starting inventory

`request-legacy-sites.json` and the actual lexical scanner agree on 28 occurrences
in five files, represented by 24 path/fingerprint entries. There are 21 direct
legacy HTTP/completion calls and seven conservative macro matches. This is a
lexical count, not a physical request count. Paths below are relative to
`sdk-rs/venues/blofin/src`; line numbers describe the baseline only.

Targets used in the table:

- **Public read:** `Cx::http_context` with `Operation::Read`, explicit public
  routing, no credential, `RetryPolicy::Never`; `request_managed` followed by
  `stream_response(..., ArrayAt::EnvelopeField("data"))`. Retain Context in the
  pending continuation. Background discovery finishes with
  `finish_http_context` on every success/failure terminal path, including immediate
  queue refusal, before a native callback can queue replacement work. A cleanup
  `Revoked` result after expiry does not suppress an otherwise admitted callback.
  Command results use `done_context`/`fail_context`.
- **Private read:** the same managed lifecycle with explicit primary/secondary
  credential reference and `request_signed_managed`. Requires the signing lane's
  dispatch recipe; never pass precomputed signed headers into an unsigned call.
  Preserve existing native snapshot builders, row reports and freshness checks;
  validate delivery authority before changing native caches or the Store.
- **History:** keep `CursorHistory` staged/accepted rows and `Completion`; require
  a shared Context-bearing completion path before conversion. At this baseline
  `engine/src/history.rs::Completion` stores only a numeric command and calls
  legacy `done`/`fail`; there is no `Completion::from_context`. This is an engine
  dependency to request from the owning lane, never a local venue finalizer.
  Queue admitted managed child
  reads under the original owner/deadline, with explicit page retirement and no
  lease renewal from mere queueing. Shared plan admission is a prerequisite for
  more than one physical page or endpoint.
- **Mutation:** retain one root command authority and admitted child leg contexts;
  native codecs report `http_write_result` facts, with late/unproven response
  handling through the existing engine hooks. `done_context`/`fail_context`
  centrally seal outcomes. No local `effective`/first-error outcome aggregation.
- **Preflight:** acquire the relevant root authority before validation and use
  `fail_context` without HTTP. Mutation preflight uses mutation policy even when
  validation fails before a write.

| File:line / native use | Fingerprint | Count | Engine target and sequence |
| --- | --- | ---: | --- |
| `account.rs:239`, leverage unknown market | `37bc238a40045dda234a3daaf68329976cef33caee8f13734a73f526bfef32cd` | 1 | Preflight for owned private read. |
| `account.rs:243`, leverage mode unavailable | `e2c8da59c7ed525a6d5342feecd9df9ef825904a02af1c97de61f39d1a7abf89` | 1 | Same read authority; no request. |
| `account.rs:343`, streamed private snapshots | `4f2afd7cb212a9914e8dece3dbb3733c167148c003035b2f7628bb8aab8876c8` | 1 | Private read; paired account/order families wait for plan admission. |
| `account.rs:351`, unstreamed modes/leverage | `1a8e09311f93f0d7d9c69519e6d325d16f9366b33946b861ab28ffaa45d927f5` | 1 | Private read; single leverage/mode requests first after signing prerequisite. |
| `account.rs:1222`, leverage result | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Validate root/leg delivery before settings publication; `done_context`. |
| `account.rs:1223`, leverage decode/freshness error | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `fail_context`; preserve superseded-response refusal. |
| `account.rs:1585`, leverage request failure | `569abc443c07aca5127ee71c347600be68bb35c14aeae80c3a14b2f5f2642980` | 1 | `command_error_context` immediately before `fail_context`; no retired diagnostic. |
| `closed.rs:133`, closed-position page | `bb5ed84ac1882574929599a10507277895dbe7521e20ef78b3f641d2ff116b38` | 1 | Private read + History; signing and multi-page admission prerequisites. |
| `history.rs:437`, history/public-trades page | `d7f5d65d34ca3d770c22743829f9f73e57682c52d8b63bb54030b167abb8fa43` | 1 | Public trades are a single unsigned History read; private order history is also single-response. Both need shared completion, private reads need signing, and paged fills/income also need plan admission. |
| `history.rs:424`, `matches!(request.query, PublicTrades)` | `4f41451eb1e1a039f8b647eaee00abfdc9027246d230dff2fad9129e9919c3e7` | 1 | Conservative macro match, no I/O; remove alongside complete history conversion. |
| `history.rs:479,500,524`, `matches!(request.query, Fills)` | `60ad9d1f92b48483572276a3a75cf6e7c6579a0c8d0118a9f9f51688b851c9aa` | 3 | Same conservative matches; native timestamp/filter/dedupe policy remains. |
| `history.rs:457,518`, `matches!(request.query, OrderHistory)` | `f33c5fe4590c54205bbed83f2a6cf21ba3ccfd0cbdab18d6832e43ae0c4d36a0` | 2 | Same conservative matches; no claim that these are HTTP calls. |
| `lib.rs:222`, markets/tickers/candles dispatcher | `be227961a60c241031c77b6f55bfe7a5eb6639414454495d99feecab259c5188` | 1 | Public read, first area. Each discovery request is its own background owner. |
| `lib.rs:819`, sorted candle result | `9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d` | 1 | Retained command read Context, `done_context`. |
| `lib.rs:892`, candle request failure | `497d185372b077e0511e2f21fa2b9b14522eab0cd1c71ac0f7a65741155d0a23` | 1 | `fail_context` from the exact managed callback. |
| `lib.rs:1028`, unsupported candle interval | `5c0db5f18cab95e26663139a6147567f0e1515e939302f034cf5588b2177eb3c` | 1 | Preflight read authority and typed unsupported refusal. |
| `lib.rs:1056`, unknown candle market | `37bc238a40045dda234a3daaf68329976cef33caee8f13734a73f526bfef32cd` | 1 | Preflight read authority and invalid-input refusal. |
| `trading.rs:923,1178`, initial/dequeued preflight rejection | `349d1b39839519801db71f4348af20496c0443cb14880093d11ecbbe7cc68843` | 2 | Preflight mutation root retained across native queueing. |
| `trading.rs:1242`, paired hedged leverage POSTs | `af0b7f916c07c8f53f3b2731f1c70b01fa5b0253ae68ebc878782fdf2ef51817` | 1 | Mutation with two admitted legs; shared plan admission prerequisite. |
| `trading.rs:1303`, job final success | `604f28d91adfdb4cd0caac06047abd9569f9a6c248b698a784032256d7cf8047` | 1 | Central command sealing; native IDs/cleanup preserved. |
| `trading.rs:1360`, sequential POST or verification GET | `0dfe0bf78ed209b6c5b74f9867340478f38851c8e4151adc3396745bf7db467b` | 1 | Mutation root, semantic read leg for verification; preflight/action and cancel-replace wait for shared plan admission. |
| `trading.rs:1391`, `matches!(request.step.effect, Place)` | `afb3c3744f13da9606f5f456bd95797992a932cf227eb74b2fe4405e3c64ee98` | 1 | Conservative macro match; remove only with trading conversion, preserve native invalidation. |
| `trading.rs:1405`, failed job | `368af85f963a94d4984aa5e5d920f2728ce55064b20be7dea5116c8cde1a251e` | 1 | `fail_context`; central evidence replaces local effective/outcome edits. |
| `trading.rs:1630`, failed leverage join | `3c6e80032656d492647546e807c37c73b3593d225419189ac1d8531e2e16fb1b` | 1 | One command, two admitted writes; central outcome aggregation, never first-error policy. |

Per-file totals: account 7, closed 1, history 7, lib 5, trading 8. No
`sign_http` occurrence exists here, but `account::headers` signs inline and is a
real migration prerequisite. False-positive macro removal must not hide remaining
legacy calls: keep the checker unchanged, remove the real calls, then simplify
continuation naming when that complete area migrates and prune exact hashes.

## Committable sequence and regression proof

1. Commit this inventory and plan before production edits. Independently audit
   source/adversarial behavior, caller effects and the complete inventory map.
2. Unsigned markets/tickers/candles only: commit assertion-failing tests first,
   then migrate `lib.rs` public reads and candle completion. Existing discovery
   retries stay in `PublicRetries`; no managed retry is added. Native fixtures
   adopt the engine's `test_support::prepared_actions`, preserving response,
   row, ordering and error assertions. Verify delayed preparation, pre-release
   response rejection, fixed deadline, timeout/late/duplicate completion, command
   ID reuse, synchronous no-I/O validation, streamed malformed data, and startup
   recovery. Preserve unknown-market validation before unsupported-interval
   validation. Override typed preparation/signing failure callbacks so candle
   completion retains Unsupported/Timeout and NotSent rather than losing them
   through the default string-only failure chain. Parked/pre-release/unsupported
   controls drive raw lifecycle actions; `prepared_actions` auto-readies routes
   and cannot prove those negative boundaries. Exercise more than 256 rapid
   completed/failed discovery attempts to detect logical-handle leaks. Prune
   exactly five lib occurrences (28 to 23). Append landed caller
   behavior to CALLERS in that PR, then clean independent reviews and CI.
3. After the shared Context-bearing history completion prerequisite lands,
   convert the single unsigned public-trades query: retain
   `CursorHistory`/`Completion` and bind its command owner to one public managed
   read. Do not route private
   history through unsigned managed HTTP or duplicate the native decoder. Shared
   dispatcher/macro inventory may remain until the later private area converts.
4. Coordinate BloFin HMAC dispatch signing with `codex-5c57jl` before private
   edits. After that prerequisite, migrate single-request private leverage and
   mode reads and, once shared history completion exists, single-response private
   order history. Preserve credential intent, auth evidence, stale-setting fences,
   native row admission and rejection visibility.
5. After `codex-uufub4`'s shared plan-admission API lands, migrate multi-page
   histories/closed positions and joined account/order refresh families. Retain
   command-bound incomplete-before-result ordering, accepted-page recovery and
   strict preparation/policy refusals. Closed positions currently route signing
   failure through recoverable `history.finish`, unlike general histories'
   strict `reject`: making managed preparation/policy refusal strict is an
   intentional caller change. Test refusal after an accepted full page: reject
   with no partial result or incomplete marker. Ordinary later-page failure
   must still recover accepted rows. Do not create a venue pager/admission core.
6. Migrate genuinely single-write trading paths after signing, then batches,
   hedged leverage and cancel/verify/replace plans after shared plan admission.
   Preserve caller IDs, broker attribution, native lot counts and partial IDs.
   Tests must distinguish zero release, definitive refusal, accepted plus refused,
   and uncertain released writes, including callback-order permutations and late
   ACKs. Replace native outcome aggregation with engine evidence.
7. Enumerate any one-request/multiple-owner coalescing separately and leave it
   until shared execution-group authority exists. The present hedged leverage
   join is two writes owned by one command, not proof of that coalescing shape.
   Every remaining inventory entry must reach zero; hard API blocks go to Sam
   through the overseer with source evidence, never an exemption.

Each implementation area uses a fresh branch/PR into `feat/rust-rewrite`.
Keep test-first and implementation commits separate. Re-evaluate the remaining
per-file map after every area; earlier numbers are this baseline's inventory.

## Caller-visible contract to document with implementation

Public query success JSON, candle sorting/units and discovery row semantics
remain native. Managed custom transports must implement prepare/bind/final
synchronous claim; legacy-only request adapters fail explicitly. Preparation
does not release HTTP, and late/duplicate responses cannot publish Store effects
or complete a reused command. A deadline covers preparation and response work.
Typed preflight kinds become explicit where legacy calls used bare strings.

For private follow-ups, late accepted writes can reject as `unknown`; keep the
original identities and reconcile before any replacement. Mixed effects retain
partial receipts and uncertainty. History callers must keep owned incompleteness
and exclude failed staged prefixes. Preserve private general histories' keyless
`[]` result before symbol/range validation and closed positions' unknown-symbol
`[]`, while general histories retain their typed unknown-symbol refusal. Test
these no-I/O paths with managed preparation unsupported. Document the stricter
closed-position preparation/policy refusal separately from ordinary recoverable
later-page failures. CALLERS entries describe only the behavior
actually implemented in their PR, with affected NX order/error/history and
custom-transport consumers; this plan does not claim NX migration or deployment.

## Verification and architecture gate

Use nvm Node 22 on PATH for `wt-setup` and all `corepack yarn` commands. Use
`CARGO_BUILD_JOBS=2`, the repository's pinned Rust toolchain and scoped native
tests/fmt/strict all-target Clippy for BloFin and Binance. Run
`python3 sdk-rs/scripts/check-request-context.test.py` and
`python3 sdk-rs/scripts/check-request-context.py`, plus bans; prove every other
venue's inventory map unchanged. Full WASM builds, replay sweeps, timing/size and
package gates run in CI. Keep a background `gh pr checks <n> --watch` while CI
runs; no readiness claim without green proof and independent reviews.

Review question: **is this a per-venue fix of a cross-cutting concern?** This is
venue adoption of existing engine choke points. No new venue lifecycle,
admission, retry or outcome algorithm is allowed. If adoption requires a missing
shared primitive, stop that area, report the exact gap to its owning lane and
the overseer, and continue independent single-request work. Include each review's
explicit answer and any residual limitations in the PR body.

## Command-entry prerequisite found after the inventory checkpoint

At `7641650e7b1`, `Cx::http_context` can refuse capacity/policy/time admission
before any Context exists (`engine/src/request_context_registry.rs:577-615`).
Only numeric legacy failure can then reject that unissued command; the Binance
reference retains that fallback too. `codex-uufub4` independently confirmed the
gap, and the overseer assigned the shared typed entry/rejection seam as its first
priority. Do not forge Context, hang the command, or hide numeric failure in a
new helper. Pending the shared API, retain the exact existing `lib.rs:892`
failure expression/site solely for unissued candle entry failure. Already-issued
reads use Context completion. This first conversion can therefore prune four
lib occurrences (28 to 24), with the fifth removed after the shared seam lands.
The remaining site is a tracked zero-legacy blocker, not an exemption.
