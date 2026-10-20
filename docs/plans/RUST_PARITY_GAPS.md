# Rust venue parity gaps

Branch: `feat/rust-parity-gaps`, based on `origin/feat/rust-rewrite`
(`1f9e9a5206`). Follow `VENUE_PORT_BRIEF.md` and `RUST_REWRITE.md`;
full automatic execution, independent review at each phase, stable commits,
PR into `feat/rust-rewrite`, review findings posted, merge only on green CI.

## Phases and verification

1. **Inventory and plan.** Compare current TS runtime capabilities with the
   merged Rust implementations. Record skips where TS itself lacks the feature.
   The roster-wide limitations audit belongs to `feat/rust-parity-sweep` per
   the user's subsequent scope note.
2. **Paradex delegated signing.** Trace both TS `remoteSigner` and
   `payloadSigner`, including JWT acquisition, exact signing material, headers,
   private-socket policy, failures, disposal and request correlation. Preserve
   the sans-I/O engine: host delegates signatures and returns typed results;
   venues name real URLs and retain existing account transport routing. Verify
   local/delegated equivalence, missing/failed/malformed/late signer replies,
   concurrent auth and writes, and absence of automatic write retries.
3. **Bitrue lookup and capability audit.** Implement the TS cache/open-order
   client-ID lookup with its non-definitive miss contract. Verify existing
   supported fills filters and incomplete-page handling. Pin absent TS
   order/PnL/funding histories and hedge/margin setters with runtime contract
   tests and document the skips. Check native request shapes, aliases and
   cache fallback; failed reads must not clear last-known-good state.
4. **KuCoin spot.** TS has a separate HF spot adapter. Add a typed spot venue
   in the existing crate, reuse signing and host routing, register the exchange
   in WASM/SDK/harness. Cover public markets/tickers/full books/candles, private
   balances/orders/executions and recovery, HF trading/amend/cancel and caller
   IDs, and precisely the queries/settings TS supports. Test spot units and
   market buys, token recovery, snapshot races and page traversal.
5. **Integration and gates.** Update docs for these three venues and add
   deterministic regressions. Run bans, fmt, Clippy,
   native tests, wasm32 Clippy, SDK typecheck/lint/tests, host/harness tests;
   build and replay public/private gates for Paradex, Bitrue, KuCoin futures
   and spot, plus Binance public/private. Record fresh public traffic for the
   touched venues. Standard egress comparison remains enabled. Keep the
   1,258,291-byte budget unchanged; use incoming per-venue build support and
   record exact artifact sizes and provenance.
6. **Landing.** Push, create PR into `feat/rust-rewrite`, post independent
   adversarial correctness reviews, fix/re-review, incorporate landing changes
   without rebase/stash, wait for exact-head green CI, merge, fresh branch.

Each implementation phase follows plan audit, implementation, verification,
independent implementation review and fixes before committing. Venue work may
proceed in disjoint scopes; shared host/engine/SDK/harness changes are coordinated.

## Inventory

- Paradex: merged local signing explicitly rejects delegated modes; compare
  current TS modes before designing the signature boundary.
- Bitrue: TS `BitrueExchange` inherits the branded empty order/PnL/funding
  histories from `BaseExchange`, inherits its throwing hedge-mode setter, and
  explicitly throws in `changeMarginType`. Those capabilities are skipped.
  Its supported fills query is one bounded `myTrades` page; Rust already has
  coverage including explicit incomplete filtered pages. The real gap is
  `BaseExchange.findOrderByClientOrderId`: cache-first, symbol-scoped open-order
  lookup, including the shared native/parent-ID aliases and sentinel checks.
- KuCoin spot: TS supports spot. Its HF adapter does not override order/PnL/
  funding history or public trades; do not invent capabilities TS lacks.
- Other venue limitations and Phemex gaps are assigned to the separate
  `feat/rust-parity-sweep` lane and excluded from this PR.

## Progress

- Created branch from the fetched landing head; working tree was clean.
- Read the current brief and rewrite spec. Parallel bounded discovery and
  implementation owners established for each venue, with shared files owned
  by the integration lane.
- Phase 1 audit: inspected current TS implementations rather than interpreting
  the venue API's potential capabilities as the adapter's actual support.
  The user's subsequent scope note assigns the broader roster audit to another
  lane; reverted this lane's unrelated status-row changes accordingly.
- Bitrue phase peer review: KuCoin owner independently compared the complete
  lookup diff and tests against the TS base helper, PASS with no blocker.
  Lookup never commits account snapshots; failures retain cache semantics and
  a miss remains non-definitive because closed orders were not queried.

## Paradex signing design and review

TS `signParadexTypedData` passes the StarkNet typed-data message hash, an
opaque public subkey, `scheme: stark`, and `encoding: felt-pair` to
`payloadSigner`. Auth uses seconds and a 24-hour expiry; Order/ModifyOrder use
millisecond signature timestamps. Signature strings may be stand-ins and are
inserted verbatim with signer headers, matching TS rather than re-parsing keys.
More than one delegated batch entry is rejected before any write.

The engine adds a typed `SignPayload` action and signed/failed result events.
It holds the original typed HTTP action by ID with a bounded pending map and
whole-operation deadline. A valid result resumes exactly that request; failed,
timed-out, disposed and duplicate/late replies cannot send a write. A signing
failure occurs before I/O and can release an unsent amendment guard, whereas
an ambiguous HTTP failure retains that guard. JS and native hosts perform the
callback and return the result event; no venue performs asynchronous I/O.

TS `remoteSigner` is the generic API HTTP adapter, distinct from StarkNet
payload signing. Its request receives account name, real base URL, method,
path, decoded params/body and headers. Returned URL/params/data replace the
old fields, including deliberate deletion, and usable bearer headers retain
the existing fallback contract. It still needs a locally usable key or a
payload signer for JWT acquisition; remoteSigner alone is not a new keyless
StarkNet protocol. JWT auth uses raw direct fetch in TS and bypasses that
remote adapter. Payload signing disables the API factory's HOW adapter while
ordinary provider proxy selection remains intact.

Independent Bitrue-owner review found two P1 cases and one P2 case: auth URL
contains `/auth/{subkey}`, JWT must bypass account HOW/proxy routing, and
returned query arrays require TS's comma serializer. Fixes were verified
with real suffixed paths, host route metadata and array regression cases.
Independent re-review passes for the corrected wrapper, route policy and
inventory scope. The final repository suite also covers
the auth-path, array and callback receiver/arity regressions.

## Implementation and reviews

- Bitrue: `c114e7af84`, positive-query follow-up `3d7f54506a`. Native 68
  tests pass. Cache/open-order lookup matches TS aliases, sentinels, symbol scope,
  non-definitive misses and failure fallback without account-store mutation.
  Duplicate placement-unit bookkeeping is removed only from the returned lookup
  copy; financial extras and the original Store remain intact. Independent review
  passes. Unsupported TS histories and mode setters remain skipped, as above.
- Paradex: delegated host/engine boundary, direct JWT route, generic remote HTTP
  adapter and callback receiver/arity fixes are implemented and reviewed. Native
  venue 115 tests pass. The exact opaque signature/header bridge is verified by
  two actual SDK-to-WASM tests, including CR/LF rejection before HTTP and native
  cancellation/deadline tests. `remoteSigner` alone does not replace TS's local
  key/payloadSigner requirement for JWT acquisition.
- KuCoin Spot: typed public/private/trading/query state machine, feature-gated
  registration, fresh bullet-token recovery and SDK context commands are in place.
  Native 137 tests pass, including 38 Spot. Current/newer malformed financial rows
  are rejected before execution emission/dedup; rejected IDs remain reusable.
  Late distinct executions survive stale/terminal state fences. Missing/null
  required balances preserve good state. Missing/null/empty amend ACK IDs retain
  the original ID, matching TS. Root, Bitrue and independent correctness reviews
  pass after fixes (`84dd4f063e`, `d784821eed`).
- Intentional Spot TS fixes are listed in `KNOWN_TS_FIXES`: deduplicate executions;
  preserve newer private state and good snapshots across stale/failed/malformed
  reads; reject malformed financial rows; encode native caller IDs as URL parts.
  The entry performs no financial output rewriting. Lookup/currency comparison
  symmetrically projects transfer-model fields for both implementations, with
  mutation tests retaining financial and identity differences.
- Landing integration: Phemex `db10d6bfd3`, Coinbase `916fc8c498`, Polymarket
  `5be8a2c727`, GRVT `168a639673`. Independent merge reviews preserve incoming
  capabilities and this lane's SDK/host/harness changes. Exhaustive test action
  handlers reject unexpected delegated signing; new Spot-only commands receive
  explicit unsupported replies in the strict Polymarket/GRVT dispatchers.
  Other venue capability gaps and roster rows remain outside this lane's scope.

## Size and packaging

The fixed per-venue budget remains **1,258,291 bytes**. Paradex signing was
trimmed through a typed precomputed hash, bounded pending vector, in-place
signature field replacement and validated header-line WASM ingress. Callback
headers remain a Record; native event headers remain typed pairs. Exact bridge
bytes are distinguished from ordinary HTTP header whitespace normalization.

Standalone Paradex omits optional text byte/batch ingress (SBE binary and stable
text control entry remain) and Coinbase's account-ID ABI; combined builds retain
those APIs. Optional prediction storage/frames use the engine `prediction`
feature, enabled by Polymarket. Feature-off/on engine tests (67/69), exact frame
layout/lifecycle tests and Polymarket's 87 native tests pass. Feature-enabled
behavior is unchanged.

Only the standalone Paradex host uses `opt-level=2`; ordinary/default builds
retain their existing profiles. Standard Yarn builds reproduce the measured
artifact exactly: web and Node each **1,257,099 bytes**, SHA-256
`75a8611dbcbbb0ebe6b217bbbe97baf64f85a2113c4dc451c7a5c3700be4765b`.
Its CPU, memory, private replay and SDK tests pass; this is a measured compiler
tradeoff with no budget or gate relaxation.

Bitrue's initial final replay missed main-thread CPU by 0.3 ms (259.623 versus
259.853 ms) despite matching behavior and passing steady CPU/memory. Raw runs
showed narrow margins. The standard standalone build now uses venue-crate
`opt-level=3`: 219.584 / 188.339 ms main-thread CPU and 123.728 / 96.398 ms
steady CPU, at 810,579 bytes. The combined build retains its size profile. The
failed checkpoint is retained, and no failed gate is reported as passed.

## Historical gate checkpoints

Earlier integration and performance checkpoints are preserved in this branch's
commits. The final frozen-source results below supersede their counts and
measurements. All replays retain normal egress comparison, the fixed size
budget and the landing branch's memory-measurement semantics.

## Latest integration checkpoint

BreakoutProp landing `74b0a7dd14` merged in `2741ad7411`. Both routing matrices
are retained, with 62 golden cases in TS declaration order. Its new polling
controls use default-off engine `polling-control`, enabled by BreakoutProp and
the native host; the WASM polling parser is enabled only with BreakoutProp.
Independent review passes: non-polling replay readiness/timing/financial semantics
remain unchanged, and combined/native controls remain available. Full checks
pass on the merged source: fmt, workspace/all-target Clippy, 2,334 native tests,
wasm32 Clippy, bans, 44 typecheck tasks, 28 lint tasks, 425 harness tests and
27 repository test tasks (safe-cex 7,355 tests in each invocation).

Fresh KuCoin Spot traffic exposed a main-thread steady-CPU bottleneck. The
ready public ticker fast path now decodes envelope and quote in one typed pass;
control/private/book/candle messages keep the existing path. Independent review,
137 native tests and all Spot gates pass at 672,731 bytes. Steady CPU falls from
TS 626.704 to Rust 566.178 ms on 26,427 frames; no fixture or gate normalization.

Fresh KuCoin futures initially failed protocol auditing despite identical data:
concurrent identical bullet requests completed in reverse order, so replay
swapped the token-to-shard association. Recorder `6ba4327e4a` now attaches the
request issue ordinal before awaiting I/O. Replay sorts only each matching HTTP
queue; legacy tapes retain their ordering. Mixed/invalid/duplicate ordinals
reject explicitly, and staged/timed reads and single-use writes keep their guards.
Independent review and all 393 pre-Breakout harness tests pass. The failed
capture is checkpointed explicitly; the final fixture requires a new ordinary
20-second recording, with protocol checks still enabled.


## Binance integration and final signing edge

Landing `e391337054` (Binance variants plus GRVT profiling) merged in
`0d705cee38`. Incoming native-unit conversions, protocol auditing and registrations
are preserved; both financial currency projections and all four SDK Spot names
remain intact. New zero-position normalization stays scoped to Binance so other
venues' unexpected rows remain visible.

Independent merge review found Binance would leave the new Spot symbol-aware
amendment command pending. `db87bfb00d` returns an explicit unsupported reply on
all four Binance variants. The existing no-I/O/snapshot test proved the failure,
then passed with the correction; independent re-review passes.

The merged repository passes 2,404 native tests, fmt, workspace/all-target and
wasm32 Clippy, bans, 443 harness tests, 44 typecheck tasks, 28 lint tasks and all
27 repository test tasks (safe-cex 7,363 tests in each invocation). A later
TS-only signing edge fix `fe1e69da4c` passes 274 focused tests plus typecheck/lint
and independent review. It preserves protocol-relative URL behavior across
browser HTTP, Node HTTP and HOW using the host's already-selected adapter mode;
no routing table or resolver is duplicated.

KuCoin final public/private gates and actual Spot SDK test pass. Exact commands,
artifact/tape/harness hashes, counts and medians are committed in
`venue-ports/KUCOIN_GATES.json`. Four other standard artifacts are being refreshed
before the combined-host and actual SDK suite, PR reviews and CI.


## Shared command compatibility correction

A narrow audit of the newly introduced `AmendOrderWithSymbol` found unsupported
native handlers could panic (KuCoin futures), complete the wrong ID (BloFin), or
leave commands pending. This is compatibility for this lane's new command,
not the separate roster-capability audit. `03f9b37c93` adds a default-false Venue
opt-in and rejects the command centrally before unsupported venue callbacks.
KuCoin Spot alone opts in and receives the entire original command unchanged.
No translation to the legacy amendment drops routing context.

The actual futures regression proves the original panic and then verifies one
error for the caller's command, no I/O and byte-identical state before/after
startup. Engine tests pin default rejection, unchanged legacy forwarding and
opt-in payload forwarding. Engine plus KuCoin: 207 tests pass (KuCoin 138,
including 38 Spot). Workspace Clippy and two independent reviews pass; full
workspace validation and all six final standard artifacts are being refreshed.
Earlier measurements above and in committed evidence retain their measured
source attribution until those refreshes finish.

`2db149c25c` also preserves browser URL resolution in Electron renderers exposing
Node, plus worker location resolution. Six URL mode/runtime cases and the
routing suites pass: 276 focused tests, 44 typecheck tasks, 28 lint tasks and
independent review. This changes host callback handling only.

## Shared decoder and native server integration

Merged landing `0063068241` in `32c1b7fd7a`, retaining the landing structure,
shared host command decoder, native frame server, command tracking and book
deltas. Spot's caller-symbol amendment and cancellation, account alias and
feature-scoped parser now live in the shared decoder. The polling feature gates
only its JSON parser; the public Event and Venue hook remain unconditional.
Independent decoder/Paradex and native/Spot merge reviews both pass.

A native loopback regression pins unsupported symbol-aware amendment with
pending capacity one: one error uses the original caller ID, never enters the
venue, releases capacity and permits reuse of the same ID for a successful
account command. Payload-signer exports, headers and receiver-shutdown
cancellation are preserved alongside FrameServer exports.

The incoming live-smoke census discovers Spot automatically. Its endpoint
profile and monetary sizing remain explicitly unreviewed, so every live mode
refuses before credentials are read. The cutover matrix already discovers the
committed Spot public fixture; its Spot metadata assertion now includes KuCoin.
Full repository checks and six fresh standard artifact gates are in progress;
earlier proofs above remain attributed to their historical sources.
This integration checkpoint passes 44 typecheck tasks, 28 lint tasks and all
540 harness tests. Independent review passes for the live-smoke refusal and
Spot frame assertion; native/full repository tests and artifact refresh remain.
The replay gate now prints its exact output directory before running. Final
proofs bind PASS logs to that directory rather than choosing files by recency;
an independent review confirms this diagnostic changes no gate criteria.

The first fresh Paradex artifact after the shared decoder merge measured
1,258,633 bytes, failing the unchanged 1,258,291-byte budget by 342 bytes.
Its three signing schemas are fixed, so replacing runtime field-name
formatting/joining with their exact type strings removes allocations without
changing selector bytes, field order, hashes or signatures. Independent plan
and implementation reviews pass; Paradex's 115 native tests (including local
signature and delegated-hash vectors), Clippy, default engine's 74 tests and
standalone Paradex host tests pass. The standard artifact measures 1,258,055
bytes (578 bytes saved; 236 bytes below budget), with identical web/Node SHA256
`d6aef4f270652d49f04d18d6d885db67775b4841ee9e48925164959309cb3cff`.
Performance gates still require the final committed build and CPU release.

## Pre-review replay checkpoint (historical)

Measured source: `41e28f774aba143e524a295b6bc58c1537751e7a`. All six standard
artifacts and all **12 replay gates PASS**, with normal egress comparison.
Each public result is the median of three runs; private results are correctness
gates with one run. Byte budget: **1,258,291**, unchanged.

| Venue | WASM bytes | Public frames | TS/Rust CPU ms | TS/Rust retained bytes | Private frames/commands |
| --- | ---: | ---: | ---: | ---: | ---: |
| paradex | 1,258,055 | 15,085 | 633.185 / 585.095 | 18,818,209 / 2,355,204 | 33 / 15 |
| bitrue | 811,137 | 3,208 | 233.059 / 151.376 | 16,006,025 / 2,637,844 | 23 / 8 |
| kucoinspot | 674,211 | 26,427 | 746.411 / 651.586 | 16,726,649 / 3,299,836 | 22 / 13 |
| kucoin | 775,517 | 8,306 | 454.303 / 246.211 | 18,940,113 / 3,330,628 | 32 / 17 |
| binance | 908,744 | 873 | 270.735 / 147.651 | 16,445,121 / 2,546,196 | 14 / 3 |
| polymarket | 854,277 | 182 | 28.971 / 24.515 | 23,483,121 / 4,914,876 | 20 / 8 |

All public gates also pass strict steady CPU and retained-memory comparisons.
Exact artifact/tape hashes, commands, raw measurements and source attribution:
[four-artifact evidence](venue-ports/PARITY_GATES.json) and
[KuCoin evidence](venue-ports/KUCOIN_GATES.json). Independent review validated
all 12 PASS logs and 48 raw runs, clean request/protocol audits, hashes and
unmodified memory arithmetic. Standalone actual SDK tests pass: Paradex 2,
Bitrue 1 and KuCoin Spot 1.

Repository verification passes: the `32c1b7fd7a` baseline has 2,437 native
workspace tests; fmt, workspace
all-target and wasm32 Clippy, bans; 44 typecheck tasks, 28 lint tasks, 540
harness tests and 27 repository test tasks (safe-cex 7,384 tests in each
test-ci/test-unit invocation). The signing optimization additionally passes
115 fresh Paradex tests, 74 default-engine tests and six standalone Paradex
host tests. Fresh KuCoin native tests pass 138 including 38 Spot, with Clippy.

The combined build, native-frame integration (1), scripted live-smoke
integration (9), JS host tests (74) and combined SDK/WASM tests (385 in
13 files) pass. Final typecheck (44 tasks) and lint (28 tasks) also pass.
All local gates are green; next are the PR correctness reviews and exact-head CI.

Final evidence review caught a manifest key collision between the combined
artifact hash and build-log hash. The fields are now distinct (`sha256` and
`logSha256`), checked against both actual binaries and every recorded log.

## PR landing integration refresh

Opened [PR #1338](https://github.com/Tealstreet/tealstreet-sdk/pull/1338) into
`feat/rust-rewrite` after all local gates passed. The target then advanced to
`0a0b3453e9` with TxFlow, the native server session adapter and replay-finalize
changes. Resolved conflicts preserving both venue registrations, feature gates,
typed signing, request ordinals, financial projections and all landing behavior.
The native adapter forwards raw commands/IDs to the shared decoder; independent
merge review passes. The Spot registration also updates the nearby census to
45 Rust names (36 opt-in, nine defaults) and four names without a Rust adapter.

Merged formatting and all 562 harness tests pass. The engine, crypto and native
host implementation sources are unchanged, but replay now snapshots before
asynchronous GC and includes reconstruction cost in measurement. Consequently,
the frozen `41e28f774a` proofs above remain historical until all six artifact
replays are refreshed. Full checks, combined host/SDK checks, PR correctness
reviews and exact-head CI are required on the merged source before landing.

## PR correctness review round 1

Three independent reviews of actual PR head `8487855f5a` were posted as PR
comments. Paradex/signing and Bitrue/shared-protocol reviews pass. The Spot
review found two P2 bugs: same-time native instances could mint identical client
IDs, and a pong before welcome could cancel the welcome watchdog permanently.
Missing IDs now use the existing fallible UUID helper; caller IDs bypass entropy,
and whole-batch staging rejects entropy failure before any write. Pongs require
both readiness and an outstanding heartbeat. Four new regressions failed before
the fix, then all 143 KuCoin tests (42 Spot plus the new staging unit test) and
Clippy pass. Independent re-review closes both findings.

The merged baseline passes 2,510 native tests, workspace/all-target and wasm32
Clippy, bans, 562 harness tests, native-frame and server-adapter integrations,
44 typecheck tasks, 28 lint tasks and 27 repository test tasks (safe-cex 7,467
tests in each invocation). These counts precede the five added Spot regressions.

CI found Linux Paradex at 1,258,344 bytes, 53 bytes over the fixed budget despite
the local build passing. Keeping the validated chain ID as a Felt and signing
from a fixed field array removes formatting/parsing and allocation work. All
115 Paradex tests, Clippy, formatting and independent review pass. The standard
local artifact saves another 240 bytes, reaching 1,257,815; Linux must verify it.

CI's scan flagged one old synthetic invalid-auth header in a rejected-request
test. Its author confirmed the fixture was authored without supplied credentials;
its exact derivation is not asserted. The current value is now `placeholder`,
and only the exact historical fingerprint is excluded, following existing scan
policy. All 47 PR commits scan clean locally; independent narrow-exclusion
review and 562 harness tests pass. No scanner rule or broad allowlist changed.

After the fixes, all 2,515 native workspace tests, all-target and wasm32 Clippy,
formatting and bans pass. The target advanced again to `02e9db0e8b` with GRVT
and Lighter follow-ups. Resolved the three conflicts with both registrations,
standalone build overrides and live-smoke refusal lists retained; independent
merge review passes. The nearby census is now 46 Rust names (nine defaults,
37 opt-in) and three unregistered products. Formatting and all 568 harness
tests pass on this merge. Refreshing the six artifacts and final verification
will use this merged source; prior exact-source evidence remains historical.

## Final PR verification

Final runtime source: `e0abc9b0f1ca1cb4116411d00094dc6432ee39ff`, including
landing `02e9db0e8b`. All six standard artifacts and **12 replay gates PASS**
after the landed finalization change: reconstruction and structured cloning are
inside both measured CPU windows; endpoints detach before asynchronous GC.
Normal egress and protocol audits remain enabled. Public values are medians of
three runs; private gates use one run. The fixed byte budget is **1,258,291**.

| Venue | WASM bytes | Public frames | TS/Rust CPU ms | TS/Rust steady ms | TS/Rust retained bytes | Private frames/commands |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| paradex | 1,257,815 | 15,085 | 981.444 / 747.342 | 526.515 / 387.094 | 18,945,929 / 2,238,124 | 33 / 15 |
| bitrue | 811,137 | 3,208 | 249.277 / 236.248 | 162.468 / 150.906 | 16,960,809 / 3,305,548 | 23 / 8 |
| kucoinspot | 681,398 | 26,427 | 594.896 / 495.324 | 388.790 / 327.918 | 18,446,273 / 4,241,500 | 22 / 13 |
| kucoin | 775,517 | 8,306 | 537.768 / 277.517 | 335.724 / 160.907 | 21,327,505 / 3,998,852 | 32 / 17 |
| binance | 908,744 | 873 | 274.791 / 157.181 | 155.951 / 77.491 | 18,027,137 / 4,042,340 | 14 / 3 |
| polymarket | 854,277 | 182 | 28.917 / 23.821 | 10.203 / 5.319 | 23,618,529 / 4,986,836 | 20 / 8 |

Independent evidence audits verify all 12 PASS logs and 48 raw runs, exact
source/artifact/tape hashes and output-directory bindings, zero request/signature/
protocol audit failures, and unchanged retained-memory arithmetic. Every public
gate wins main-thread CPU, steady CPU and retained memory. Process CPU remains
informational. Standalone actual SDK tests pass: Paradex 2, Bitrue 1, Spot 1.
Linux CI also passes Paradex at **1,258,104 bytes**, within the same budget.

Three independent round-2 correctness reviews of the actual PR head pass after
both Spot findings were fixed. The findings and closure are posted on
[PR #1338](https://github.com/Tealstreet/tealstreet-sdk/pull/1338#issuecomment-6285680991).
No outstanding correctness findings remain. These Rust-specific fixes do not
add TS divergences; the earlier documented Spot TS safety corrections remain
listed in `KNOWN_TS_FIXES` and the implementation commits.

Final frozen-source integration verification passes: **2,537 native workspace
tests**, fresh scoped KuCoin **143** (42 Spot integration tests), workspace
all-target and wasm32 Clippy, fmt and bans; **44** typecheck tasks, **28** lint
tasks, **568** harness tests, **27** repository test tasks (safe-cex **7,513**
tests in each CI/unit invocation). The default combined artifact is
**11,395,815 bytes**, with identical web/Node SHA256
`20acd64815c43d188fb08be5a6e0ca64440fec909732c65ec88c82c1de94b665`;
its integration checks pass **415 SDK/WASM tests in 14 files**, **74 JS host
tests**, **9 scripted live-smoke tests**, **1 native-frame test** and
**1 native-server-adapter test**. The standalone byte budget applies to the
per-venue artifacts in the table, not this default integration build.
PR-range secret scanning and both CI scanners pass. All current-head CI checks
passed before this evidence-only update; final-commit CI is required to merge.
Exact commands, source and hashes are recorded in
[PARITY_GATES.json](venue-ports/PARITY_GATES.json) and
[KUCOIN_GATES.json](venue-ports/KUCOIN_GATES.json).

The intentional TS capability skips remain those in the inventory: Bitrue
histories and hedge/margin setters, and Spot histories/income/public trades.
Paradex remote signing alone still requires local signing or a payload signer
for JWT acquisition. Credentialed Spot lifecycle smoke remains unavailable until
its separate endpoint/sizing profile is reviewed; every mode refuses before
credential access. No budget increase or transport-table duplication was needed.

## CI follow-up: Bybit decimal throughput

The evidence-only `2dfeb2668d` CI run hit two timing failures outside the scoped
ports. A single bounded rerun passed BreakoutProp but repeated the HyroTrader
steady-CPU miss: TS/Rust 7.3/7.6 ms initially, then both rounded to 7.4 ms.
Parity, main CPU and memory passed. The identical runtime had passed the prior
head; failures and rerun policy are documented on the PR. No further unchanged
retry is planned. All other 37 checks passed.

Bybit already uses opt-level 3, so a proposed duplicate compiler flag was
rejected during plan review. Its numeric decoder fast-pathed short integers
but sent decimal prices/quantities through the full parser. Extend that path
to at most 15 unsigned ASCII digits and one decimal point: numerator and
power-of-ten divisor are exact f64 integers, and a single division rounds the
same rational as the standard parser. Signed zero, signs, exponents, long,
malformed, non-ASCII and whitespace forms keep the original fallback. No
request, trading, state or transport behavior changes.

Independent mathematical and implementation audits pass. Bitwise differential
coverage includes every decimal-point position at widths 1–16, 128 deterministic
random mantissas per width, boundary values and existing fallback/signed-zero
cases. All **124 Bybit native tests**, Clippy, fmt and bans pass. Preliminary
standard public replay3 improves local Rust steady CPU from **3.693 to 3.205 ms**;
main CPU remains below TS. The standard artifact shrinks from **958,115 to
957,021 bytes** without a profile or budget change. All five Bybit-family public/private gates and refreshed native/combined host
verification pass on the committed source. The six earlier standalone builds exclude
the Bybit crate, so their explicitly attributed evidence remains unchanged.


## Final local checkpoint after CI follow-up

The six earlier standalone proofs retain source `e0abc9b0f1`. Their compiled
inputs and harness are unchanged: the only runtime change is Bybit's numeric
decoder, and these six feature selections exclude the optional Bybit crate.
Independent dependency/source review confirms this boundary. Bybit-family and
combined/native verification instead use exact source `2f5433ff7b`.

All **22 replay gates PASS** (the earlier 12 plus the following 10). All five
Bybit-family public medians win main CPU, steady CPU and retained memory; all
private audits pass. One standard Bybit web/Node artifact serves all five names:
**957,021 bytes**, SHA256
`43734e52829af1a6f286d97919e4ab18036633adcc0aa8a2e73ce6225d91ec98`.
No fixture, egress, threshold, build-profile or byte-budget changes.

| Venue | TS/Rust CPU ms | TS/Rust steady ms | Private frames/commands |
| --- | ---: | ---: | ---: |
| bybit | 252.657 / 174.396 | 138.871 / 73.203 | 18 / 4 |
| bybitinverse | 114.294 / 30.953 | 72.259 / 12.055 | 14 / 3 |
| bybitspot | 159.090 / 111.483 | 80.606 / 48.537 | 11 / 2 |
| bybitdemo | 218.302 / 138.645 | 112.714 / 52.341 | 14 / 3 |
| hyrotrader | 21.744 / 16.216 | 3.993 / 2.948 | 13 / 3 |

An independent evidence audit validates all ten new gate logs, 40 raw runs,
saved binaries, source/tape/output/hash bindings, audit counters and unchanged
memory arithmetic. A deliberate unsafe 16-digit/17-byte parser mutation makes
the bitwise regression fail on `.9999999999999999`; restoring the exact
committed source makes it pass. The financial rounding guard is tested.

Fresh source `2f5433ff7b` passes **2,538 native workspace tests** (124 Bybit,
143 KuCoin including 42 Spot), workspace/all-target and wasm32 Clippy, fmt,
bans, secret scan, **415 combined SDK/WASM tests**, **74 JS host tests** and
**9 scripted live-smoke tests**. The combined artifact is **11,394,719 bytes**,
identical web/Node SHA256
`bd492668238e4c0ce5f6df8fa6528c27d7974b645e631d411e6b51c505c5698c`.
The earlier exact-source TS/harness/server checks remain explicitly attributed
to their unchanged scopes. Detailed evidence:
[Bybit CI follow-up](venue-ports/PARITY_BYBIT_CI_GATES.json) and refreshed
[integration verification](venue-ports/PARITY_GATES.json).

Local implementation and evidence reviews pass. Final-head PR correctness
review and all-green CI are still required before merging; earlier failed
checks remain documented on the PR.

## Landing spot-products integration

The target advanced to `f4ea9e06ab` (Hyperliquid/Kraken Spot, PR #1339) before
landing. Resolved seven conflicts by preserving both sets of registrations,
Spot contracts, exact synthetic-fixture fingerprints, fail-closed live-smoke
lists, TS-fix notes and standalone compiler overrides. The nearby registry
census is now 48 Rust names, nine defaults, 39 opt-in and one unregistered.
Two independent merge reviews pass, including automatic WASM/SDK/harness merges;
no delegated signing, command decoder, request ordinal or financial projection
behavior was lost. Formatting and all 578 harness tests pass.

Because shared WASM and venue-descriptor inputs changed, all prior exact-source
proofs above are historical checkpoints. Refresh all 22 replay gates, full native
and combined/TS verification on this committed merge before final PR review/CI.
