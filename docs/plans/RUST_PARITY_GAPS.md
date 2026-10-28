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

At the initial packaging checkpoint, only the standalone Paradex host used
`opt-level=2`; ordinary/default builds retained their existing profiles. The
standard Yarn builds then reproduced web and Node artifacts of **1,257,099
bytes**, SHA-256
`75a8611dbcbbb0ebe6b217bbbe97baf64f85a2113c4dc451c7a5c3700be4765b`.
Its CPU, memory, private replay and SDK tests passed. This checkpoint is
historical; the product-semantics integration below adds the standalone engine
opt-level 2 override. The fixed budget and gate criteria are unchanged.

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

## Final merged-source verification

All **22 replay gates PASS** at exact runtime source
`d06809837aadc771c5c19b9f06bfd61445e0581f`, including landing `f4ea9e06ab`.
These supersede every earlier checkpoint: all eleven public three-run gates and
eleven private one-run gates were refreshed on this merge, with normal egress,
unchanged CPU/memory criteria and the fixed **1,258,291-byte** budget.

| Venue | WASM bytes | TS/Rust CPU ms | TS/Rust steady ms | TS/Rust retained bytes | Private frames/commands |
| --- | ---: | ---: | ---: | ---: | ---: |
| paradex | 1,257,815 | 445.261 / 417.729 | 200.901 / 150.586 | 18,926,697 / 2,230,172 | 33 / 15 |
| bitrue | 811,137 | 118.912 / 97.478 | 69.120 / 49.597 | 16,988,425 / 3,292,612 | 23 / 8 |
| kucoinspot | 681,398 | 510.892 / 453.728 | 329.012 / 295.746 | 18,434,921 / 4,235,900 | 22 / 13 |
| kucoin | 775,517 | 394.581 / 204.665 | 240.213 / 111.375 | 21,344,617 / 4,035,076 | 32 / 17 |
| binance | 908,744 | 274.997 / 140.337 | 163.177 / 68.249 | 18,021,497 / 4,042,004 | 14 / 3 |
| polymarket | 854,277 | 27.650 / 22.739 | 8.974 / 4.619 | 23,621,913 / 4,987,852 | 20 / 8 |
| bybit | 957,021 | 242.031 / 163.105 | 133.748 / 64.014 | 22,266,361 / 4,470,012 | 18 / 4 |
| bybitinverse | 957,021 | 114.121 / 29.972 | 73.711 / 11.861 | 14,235,969 / 1,667,308 | 14 / 3 |
| bybitspot | 957,021 | 138.374 / 97.978 | 74.625 / 41.766 | 16,275,321 / 2,732,564 | 11 / 2 |
| bybitdemo | 957,021 | 202.783 / 138.967 | 99.964 / 51.789 | 21,811,345 / 4,478,316 | 14 / 3 |
| hyrotrader | 957,021 | 21.732 / 16.406 | 4.155 / 2.944 | 13,384,673 / 837,260 | 13 / 3 |

The five Bybit names share one standard artifact. Independent evidence review
verifies all 22 logs, 88 raw runs and all seven standard artifacts. Request,
write, signature and protocol audits are clean. Retained-memory arithmetic stays
unclamped; reconstruction and cloning stay inside the measured CPU window for
both implementations. Standalone SDK tests pass: Paradex 2, Bitrue 1, Spot 1.

Fresh merged-source checks pass: **2,605 native workspace tests** (124 Bybit,
143 KuCoin including 42 Spot), fmt, workspace/all-target and wasm32 Clippy,
bans and PR-range secret scan; **44** typecheck tasks, **28** lint tasks,
**578** harness tests, **27** repository test tasks with safe-cex **7,538**
tests per CI/unit invocation. The tested default combined web/Node artifact is
**11,615,221 bytes**, SHA256
`9f726691cae1e9317bb8ab0b043ca772ae1006cb4b8f537d5f8cdc3ad91466a8`.
Its **427 SDK/WASM tests in 14 files**, **74 JS host tests**, **9 scripted
live-smoke tests**, **1 native-frame test** and **1 server-adapter test** pass.
The default package was restored after the standalone gates and both rebuilt
hashes match the tested artifact exactly.

Three PR correctness-review rounds and both merge reviews pass; all findings
were fixed and independently closed. Final-head review/CI remain the last
landing checks. This PR's requested capability scope remains unchanged by
the landing merge. Intentional TS capability skips and the unreviewed credentialed
Spot live-smoke profile remain as documented above. Source, commands, logs,
artifact/tape hashes and raw metrics are in the three adjacent proof files.

## Product-semantics landing integration

Head `06ba804dd3` passed all **40 CI checks**, but the target advanced to
`5e92f93a8a` (product semantics, PR #1341), producing six shared engine/host
conflicts. Preserve both command capability guards, option-query parser features,
Spot context commands, cold subscription dispatch and delegated-signing lifecycle.
Prediction and product-semantic Store features remain independent; Greek cleanup
runs for all three position mutation paths even when prediction is disabled.
A new regression covers each close/reopen path and exact native-ID retention.
Removing the cleanup deliberately fails the regression; restoring it passes.
Two independent merge reviews pass, including Store, both dispatch guards,
Cargo features and the standalone-host test feature boundaries.

Engine tests pass with no features and product-semantics (81 each), prediction
and both features (83 each); KuCoin 143, standalone Paradex host 7, harness 583
and typecheck 44 tasks pass. The mixed vanilla/IV Spot batch rejects centrally
before HTTP. The incoming host test follows the shared decoder's compiled
features; native targets avoid constructing JS-only errors, while WASM checks
the actual rejection path.

The unchanged standard Paradex profile initially grew to **1,259,060 bytes**,
769 over the fixed budget. Its standalone engine now uses opt-level 2 alongside
the existing host opt-level 2; combined and every other venue profile stay
unchanged. The standard build is **1,246,685 bytes**, 11,606 below budget.
A preliminary quiet public replay3 passes main CPU TS/Rust **512.8/494.8 ms**,
steady **221.1/174.2 ms**, and retained memory **19.0/2.2 MB**; private33/15
and actual SDK2 pass. An initial CLI-only packaging probe used the root Cargo
working directory and was discarded; all accepted artifacts use the standard
script and its 128 KiB stack configuration. No criterion or budget increase.

Refresh full integration checks and all 22 replay gates on this committed merge;
all preceding exact-source proofs remain historical checkpoints until refreshed.

### Four-artifact verification on ee4a319061

Fresh standard builds at `ee4a31906146a02b1370ece46b5e0d113b30d602` pass
these **eight replay gates** (four public three-run gates and four private
one-run gates, 32 raw TS/Rust runs). Normal egress and the fixed
**1,258,291-byte** budget remain unchanged. Paradex uses the standard standalone
host and engine opt-level 2 overrides; Bitrue retains its standalone venue
opt-level 3 override. Web and Node binaries match for each venue, and immutable
copies, exact commands, source markers, log/output bindings and raw metrics are
recorded in [PARITY_GATES.json](venue-ports/PARITY_GATES.json).

| Venue | WASM bytes | TS/Rust main CPU ms | TS/Rust steady ms | TS/Rust retained bytes | Private frames/commands/writes |
| --- | ---: | ---: | ---: | ---: | ---: |
| paradex | 1,246,685 | 415.929 / 396.732 | 191.797 / 138.690 | 18,955,737 / 2,242,868 | 33 / 15 / 9 |
| binance | 910,026 | 184.801 / 88.716 | 100.640 / 35.826 | 18,062,145 / 4,033,444 | 14 / 3 / 4 |
| bitrue | 812,550 | 120.625 / 93.369 | 66.392 / 47.550 | 17,025,465 / 3,299,516 | 23 / 8 / 5 |
| polymarket | 855,936 | 27.780 / 20.928 | 8.960 / 3.938 | 23,657,809 / 4,969,468 | 20 / 8 / 5 |

Actual SDK/WASM tests pass: Paradex **2**, Bitrue **1**. Process CPU remains
informational; Bitrue's TS/Rust process medians are **238.261/244.184 ms**.
Measured CPU includes reconstruction and cloning for both implementations, and
retained memory uses unchanged, unclamped arithmetic after endpoint detachment
and settled async GC.

Paradex's initial unchanged public comparison failed steady CPU by **0.314 ms**
(TS/Rust medians **243.841/244.155 ms**), while parity, main CPU and memory
passed. Its three TS main/steady rows were **949.925/558.294**,
**563.653/205.260**, **509.680/243.841 ms**; Rust rows were
**813.845/428.556**, **545.554/244.155**, **550.566/201.675 ms**.
After reviewing the crossed-run variation, one controlled, explicitly authorized
three-run repeat passed with the medians above. The canonical rebuild was
byte-identical (`c80c8d1547d65a5253fe07376530e14732758dbed0084ab0acc7391780d22604`).
This repeat introduced no semantic fix, fixture, egress, criterion, budget or
run-count change. Both comparisons, raw result hashes and before/after host
samples remain in the proof; no further Paradex retry was run. Other owners'
remaining merged-source gates and final integration evidence are recorded
separately.

## Final product-semantics merged-source verification

All **22 replay gates PASS** at exact runtime source
`ee4a31906146a02b1370ece46b5e0d113b30d602`, including landing `5e92f93a8a`.
This refresh supersedes prior source checkpoints: eleven public three-run gates
and eleven private one-run gates, **88 raw TS/Rust runs**, normal egress and
unchanged strict CPU/memory criteria. All seven standard standalone artifacts
fit the fixed **1,258,291-byte** budget.

| Venue | WASM bytes | TS/Rust CPU ms | TS/Rust steady ms | TS/Rust retained bytes | Private frames/commands |
| --- | ---: | ---: | ---: | ---: | ---: |
| paradex | 1,246,685 | 415.929 / 396.732 | 191.797 / 138.690 | 18,955,737 / 2,242,868 | 33 / 15 |
| bitrue | 812,550 | 120.625 / 93.369 | 66.392 / 47.550 | 17,025,465 / 3,299,516 | 23 / 8 |
| kucoinspot | 682,723 | 540.381 / 488.503 | 340.056 / 322.642 | 18,482,537 / 4,241,092 | 22 / 13 |
| kucoin | 776,846 | 401.933 / 201.076 | 253.549 / 109.002 | 21,377,425 / 4,042,492 | 32 / 17 |
| binance | 910,026 | 184.801 / 88.716 | 100.640 / 35.826 | 18,062,145 / 4,033,444 | 14 / 3 |
| polymarket | 855,936 | 27.780 / 20.928 | 8.960 / 3.938 | 23,657,809 / 4,969,468 | 20 / 8 |
| bybit | 1,113,641 | 185.405 / 139.950 | 85.896 / 48.037 | 22,188,993 / 5,017,820 | 18 / 4 |
| bybitinverse | 1,113,641 | 120.495 / 39.924 | 77.027 / 17.628 | 14,268,449 / 1,622,772 | 14 / 3 |
| bybitspot | 1,113,641 | 160.532 / 118.139 | 89.353 / 51.450 | 16,297,433 / 3,114,020 | 11 / 2 |
| bybitdemo | 1,113,641 | 211.658 / 149.807 | 104.425 / 55.248 | 21,881,257 / 5,047,396 | 14 / 3 |
| hyrotrader | 1,113,641 | 24.234 / 19.677 | 5.758 / 3.511 | 13,419,449 / 845,844 | 13 / 3 |

The Bybit family shares one artifact. Independent audits verify all three proof files: all 22 logs, 88 passing raw
runs, seven standard artifacts and the separately retained historical failures.
Request, write, signature and protocol audits are clean; process CPU remains
informational, with no clamping of retained memory. The initial Paradex steady
CPU failure and single byte-identical passing repeat are retained above and in
the proof. The pre-commit size failure and its engine opt-level 2 resolution
remain explicitly historical. No budget, fixture, egress or criterion changes.

Fresh integration checks pass: **2,655 native workspace tests**, scoped KuCoin
**143** including **42 Spot**, feature-off/product-semantics/prediction/both
engine **81/81/83/83**, standalone Paradex host **7**, fmt, workspace/all-target
and wasm32 Clippy, bans and PR-range secret scan. Repository verification passes
**44 typecheck tasks**, **28 lint tasks**, **27 test tasks** and safe-cex
**7,552 tests per CI/unit invocation**; harness **583** tests pass.

The tested combined web/Node build is **11,825,315 bytes**, SHA256
`0a4235a8a41966bc0b8e74babce133f985e11e1f0eb1ec48bc5362f4fd79d1fc`.
Its **427 SDK/WASM tests in 14 files**, **74 JS host tests**,
**16 product-semantics tests**, **9 scripted live-smoke tests**, **1 native-frame
test** and **1 server-adapter test** pass. Standalone actual SDK tests pass:
Paradex **2**, Bitrue **1**, Spot **1**. The default combined package was restored
after all standalone gates, and both rebuilt artifacts are byte-identical to
the saved tested binaries. An independent integration evidence audit passes.

Capability scope and intentional TS skips remain unchanged. The actual credentialed
Spot lifecycle profile remains unreviewed and refuses before credential access;
the scripted lifecycle coverage above passes. All phase reviews and source merge
reviews pass. Final published-head adversarial correctness review and exact-head
green CI are the remaining landing requirements. Source, tape/artifact/log hashes,
raw metrics, separate historical failures and fresh integration checks are recorded
in the three adjacent proof files.

## Parity-sweep landing integration

Published `a423107682` passed all **40 CI checks**, including Linux Paradex
**1,246,974 bytes** with public main/steady CPU TS/Rust **499.1/413.8 ms**
and **186.9/135.3 ms**. Before merge, target advanced to `df2d6a4a03`
(parity-sweep PR #1340), creating two conflicts. Merge the twelve-venue
client-ID lookup registration union, retaining Bitrue/Spot and the incoming
Phemex/Binance/OKX registrations; preserve the unsupported TS fallback contracts.
The native routing golden matrix contains **66** merged examples, including
Spot HOW and the incoming secondary-read policies; its assertion follows the
actual merged matrix. Preserve host-side primary/direct/public-CORS routing,
Paradex direct JWT and remote signing, and all incoming secondary-read identity
contracts without venue URL rewriting.

Independent merge reviews pass after explicitly supplying the new optional
KuCoin read-credential field in the Spot constructor. Review also established
that TS Spot supports secondary GET credentials; Rust previously rejected this
mode. The next phase closes that newly exposed Spot parity gap using the landed
shared read-credential/routing support before final verification. Because shared host
routing/decoding and several touched venue read paths changed, refresh full
integration and all 22 replay gates on the committed merge before publishing.
The complete ee4a proofs and green a423 CI remain historical checkpoints.

### Spot secondary-read phase

Current TS `KuCoinSpotExchange` constructs `createXhrWithSecondaryKey`, so
all dynamic-client GETs use secondary credentials/egress; public paths remain
unsigned. Bullet-token POSTs, placement/amend and cancellation use the primary
identity. Close the previous Rust rejection by reusing the landed shared
`ReadCredentials`, KuCoin signer and host routing. Validate configured read keys
and the optional-passphrase fallback before any startup request. Preserve
primary-only private socket credentials, order-ID correlation, cache-hit no-I/O
and failed-read snapshot contracts. No venue URLs name a proxy.

Native phase tests cover exact key/signature/passphrase/broker identity, default
and inherited passphrases, public-only unsigned behavior, all mutation verbs,
bullet tokens, invalid startup and non-mutating lookup. Host tests exercise
forced private HOW, public CORS and direct providers; the merged native golden
matrix expands from 66 to **69** cases. The actual SDK/WASM fixture runs both
primary and secondary modes and checks every signed request independently.
New Spot routing cases fail against the old rejection and pass after enabling
the shared policy. The incoming generic KuCoin routing test also needed its
request recorder to observe the factory's forced HOW adapter even when the
account flag is false; this changes the test oracle, not production routing.

Verify native/Clippy, host routing, SDK/WASM, typecheck/lint and independent
phase review, then commit before the full frozen-source gate refresh.

Spot secondary phase review passes with no blockers. Native **149** tests pass,
including **46 Spot**, plus scoped Clippy. Host routing **306** tests, the
**69-case** native golden assertion, actual SDK/WASM **2** modes, typecheck
**44** tasks, lint **28** and fmt pass. The merged native test fixtures needed
`signer_headers: []` on two incoming synthetic requests; production requests
and the delegated-signing bridge remain unchanged. Full integration and all
22 replay gates follow on the next frozen runtime commit.

### Four-artifact verification on f920bcf5e5

Fresh standard builds at `f920bcf5e5ab51a163525df5b0b1cb170aa06b0a`, including
the parity-sweep landing and Spot secondary-key phase, pass these **eight replay
gates** on their first attempt (four public three-run and four private one-run
gates, **32 raw TS/Rust runs**). All four public comparisons win strict median
main-thread CPU, steady CPU and retained memory. Normal egress, financial and
protocol audits, and the fixed **1,258,291-byte** budget remain unchanged.

| Venue | WASM bytes | TS/Rust main CPU ms | TS/Rust steady ms | TS/Rust retained bytes | Private frames/commands/writes |
| --- | ---: | ---: | ---: | ---: | ---: |
| paradex | 1,250,863 | 408.845 / 400.800 | 187.394 / 140.023 | 18,968,289 / 2,245,492 | 33 / 15 / 9 |
| binance | 935,485 | 177.954 / 85.400 | 97.251 / 34.240 | 18,075,665 / 4,034,540 | 14 / 3 / 4 |
| bitrue | 817,358 | 116.985 / 88.717 | 66.433 / 44.504 | 17,048,041 / 3,302,020 | 23 / 8 / 5 |
| polymarket | 859,221 | 26.024 / 20.588 | 8.154 / 3.842 | 23,677,617 / 4,969,620 | 20 / 8 / 5 |

Actual standalone SDK tests pass: Paradex **2**, Bitrue **1**. Process CPU stays
informational; Bitrue TS/Rust process medians are **231.570/236.008 ms**. The
standard Paradex build retains host and engine opt-level 2; Bitrue retains its
standalone venue opt-level 3. No compiler profile was changed during this refresh.
Each web/Node pair is byte-identical, with immutable saved copies and exact
source, command, log/output, artifact, tape and raw-result hash bindings in
[PARITY_GATES.json](venue-ports/PARITY_GATES.json). That file preserves the
full earlier `ee4a319061` four-artifact checkpoint, including its initial Paradex
failure and single unchanged passing repeat, with their original source
attribution. Other owners' remaining gates and final integration checks are
recorded separately.

## Final parity-sweep merged-source verification

All **22 replay gates PASS** at exact runtime source
`f920bcf5e5ab51a163525df5b0b1cb170aa06b0a`, including landing `df2d6a4a03`
and Spot secondary GET identity support. Every gate passed on its first attempt:
eleven public three-run comparisons and eleven private one-run correctness
comparisons, **88 raw TS/Rust runs**, normal egress, unchanged strict public
CPU/memory criteria and the fixed **1,258,291-byte** standalone budget.

| Venue | WASM bytes | TS/Rust CPU ms | TS/Rust steady ms | TS/Rust retained bytes | Private frames/commands |
| --- | ---: | ---: | ---: | ---: | ---: |
| paradex | 1,250,863 | 408.845 / 400.800 | 187.394 / 140.023 | 18,968,289 / 2,245,492 | 33 / 15 |
| bitrue | 817,358 | 116.985 / 88.717 | 66.433 / 44.504 | 17,048,041 / 3,302,020 | 23 / 8 |
| kucoinspot | 687,701 | 472.816 / 420.256 | 302.067 / 270.743 | 18,509,121 / 4,249,156 | 22 / 13 |
| kucoin | 782,377 | 381.541 / 193.144 | 231.219 / 108.682 | 21,400,489 / 4,043,004 | 32 / 17 |
| binance | 935,485 | 177.954 / 85.400 | 97.251 / 34.240 | 18,075,665 / 4,034,540 | 14 / 3 |
| polymarket | 859,221 | 26.024 / 20.588 | 8.154 / 3.842 | 23,677,617 / 4,969,620 | 20 / 8 |
| bybit | 1,118,611 | 185.189 / 138.330 | 82.477 / 45.469 | 22,223,033 / 5,021,956 | 18 / 4 |
| bybitinverse | 1,118,611 | 116.153 / 33.217 | 74.514 / 12.216 | 14,288,449 / 1,623,268 | 14 / 3 |
| bybitspot | 1,118,611 | 149.283 / 106.753 | 80.909 / 45.838 | 16,338,425 / 3,129,612 | 11 / 2 |
| bybitdemo | 1,118,611 | 196.226 / 145.059 | 101.717 / 55.073 | 21,902,873 / 5,051,596 | 14 / 3 |
| hyrotrader | 1,118,611 | 21.147 / 19.349 | 4.309 / 3.181 | 13,441,489 / 845,932 | 13 / 3 |

The Bybit family shares one standard artifact. Independent audits pass for all
three proof files, all 22 logs and 88 raw runs, seven saved standard artifacts,
strict public medians, unclamped retained-memory arithmetic and source/tape/log
hashes. Request, write, signature and protocol audits are clean. Private CPU
and memory remain informational. Every earlier failed size/timing checkpoint
and the ee4a passing repeat retain historical source attribution; no retry or
semantic fix is claimed for this F920 refresh.

Fresh source verification passes **2,765 native workspace tests**, scoped KuCoin
**149**, including **46 Spot**, engine feature matrix **83/83/85/85** and
standalone Paradex host **8**. Fmt, workspace/all-target and wasm32 Clippy, bans
and PR-range secret scanning pass. Repository checks pass **44 typecheck tasks**,
**28 lint tasks**, **27 test tasks** with safe-cex **7,747 tests per CI/unit
invocation**, and **586 harness tests**.

The tested combined web/Node artifact is **11,948,887 bytes**, SHA256
`546ccdcc3939fe5f22efb42420dfcb437aa0eea2fc083d7bba8a2aa2f381b9c4`.
Its **431 SDK/WASM tests in 14 files**, **74 JS host**, **16 product-semantics**,
**9 scripted live-smoke**, native frame **1** and server adapter **1** tests pass.
Standalone SDK tests pass: Paradex **2**, Bitrue **1**, Spot **2**, with both Spot
read-identity modes independently checking every signed request. The default
combined package was restored after standalone gates and both binaries match
the saved tested artifacts exactly. The host routing oracle has **69** cases.

All phase, merge and evidence reviews pass. Final published-head adversarial
review and exact-head green CI remain required before landing; the old a423
40/40 green checkpoint does not satisfy those requirements for this source.
TS capability skips remain documented above. The credentialed Spot lifecycle
profile remains unreviewed and fails before credential access; scripted lifecycle
coverage passes. Current complete measurements and historical checkpoints are
in the three adjacent proof files.

## Round 6 public-only credential correction

At published `722c976271`, the KuCoin reviewer found a new inherited P2:
futures `signed` and `signed_streamed` could restore secondary authentication
without a primary authenticated context. A public-only account's public-trades
query could therefore carry API/signature/passphrase/broker headers, unlike TS.
Both selectors now require an authenticated context, as Spot already does.
Unsigned public GETs keep ordinary secondary read routing; no Direct/Primary
hint is added. Private queries still reject with their original command IDs.

Two new regressions deliberately fail on the old guards, then pass after the
fix: both signing helpers and actual `Query::PublicTrades` cover unsigned
headers, original-ID completion and no private I/O. Existing authenticated read
and primary mutation HMAC controls remain and now exercise streamed GET too.
Scoped native **151** (including **46 Spot**) and Clippy pass. Independent
closure review passes. The production patch changes only two predicates in
KuCoin futures; tests are confined to its secondary-read module.

Both independent plan audits approve bounded reverification: refresh committed
futures public/private gates, native checks and the rebuilt combined SDK/frame
consumers. Canonically rebuild all six other standard artifacts and prove web/Node
byte identity to the F920 tested binaries, with TS/harness/tapes/build flags
unchanged, before retaining their **20** old gates under their original source
attribution. Spot shares the crate, so rebuild it and rerun SDK2. Any changed
artifact requires fresh gates for every profile it serves. Do not relabel F920
measurements as the new source.

Separately, the 722 Bybit CI job missed Hyro steady CPU: rounded TS/Rust
**7.2/7.6 ms**, with parity/main CPU/memory passing. Preserve that failed job
and run a fresh local Hyro comparison after byte identity checks. The required
security-fix head will run all CI again; no unchanged manual retry is requested
and no Bybit performance improvement is attributed to this credential fix.
If the strict timing failure recurs, investigate and measure a real performance
change before continuing. Exact new-head green CI and final review remain required.

## Public-only correction: bounded final verification

Runtime source `6c7cc518fa` fixes both KuCoin futures authentication selectors.
Native workspace **2,767**, scoped KuCoin **151** (Spot **46**), workspace and
wasm32 Clippy, fmt and bans pass. The rebuilt combined artifact is **11,948,833
bytes**, SHA-256 `b440a9e996bc45e3f8b4428742e4ffa70e8da8a991d9f2b9b663ae40bdacc039`;
its SDK **431/14 files**, host **74**, products **16**, scripted lifecycle **9**,
frame **1** and server adapter **1** checks pass.

Fresh KuCoin futures public3/private1 gates pass on their first attempts:
**782,323 bytes**, main CPU TS/Rust **385.528/197.154 ms**, steady CPU
**231.398/106.412 ms**, retained memory **21,417,945/4,043,340 bytes**;
private **32 frames/17 commands**, with all protocol/signature audits clean.
Canonical web and Node builds of Paradex, Bitrue, Spot, Binance, Polymarket and
Bybit exactly match their saved F920 artifacts. The TS, harness, tapes and build
inputs are unchanged. Their **20** passing gates remain attributed to F920;
the fresh futures pair supplies the other **2**, without relabeling measurements.
Actual Spot SDK **2** tests pass again. See
[bounded follow-up evidence](venue-ports/PARITY_PUBLIC_ONLY_FIX_GATES.json).

Latest landing merge `c2874607b2` adds only `RUST_AUDITS.md`; runtime trees match
6c7. An additional first-attempt Hyro public3/private1 comparison at that merge
passes with the identical Bybit binary: main **20.981/19.208 ms**, steady
**4.022/3.543 ms**, retained **13,438,833/845,932 bytes**. This is fresh local
measurement, not a claimed performance fix for the preserved 722 CI failure.
The published correction requires new exact-head CI and correctness review;
old green checkpoints do not authorize landing. Credentialed private production
captures and the Spot live sizing profile remain unavailable; synthetic replay
and scripted lifecycle results do not establish live parity.

Gate PASS above means the existing replay oracle passed. The landing audit notes
partial final-state comparisons and Binance/Bybit margin-size normalization;
this PR does not turn those checks into full financial or live-account evidence.
Those unchanged limitations remain in [RUST_AUDITS.md](RUST_AUDITS.md#gate-honesty).

## Differential parity: accepted differences and the parity report

Owner: claude-tbz5hr (2026-10-28). The differential harness is the existing
replay gate. `sdk-rs/harness/gate.mjs` replays one recorded tape through the
TS adapter and the Rust/wasm engine, then compares markets, tickers, books,
account orders, positions, balance, fills, settings, flat-position PnL and
command results, including errors. This change adds a third classification
next to the known failures.

- **Accepted differences** (`sdk-rs/harness/accepted-differences.json`) are
  TS/Rust output differences kept on purpose: `rust-better` (Rust is more
  correct or fresher, such as a later freshness epoch) or `ts-unsupported`
  (TS cannot express the native value). Each entry names the exact venue,
  tape SHA and diagnostic paths, and carries a rationale and evidence. The
  auditor's exact-tree GO on the PR that adds an entry is its sign-off.
- **Known failures** (`known-gate-failures.json`) stay debt: they never
  certify parity.
- **New** diagnostics fail verification, as before.

`known-failure-policy.mjs` classifies each diagnostic key. An accepted key
must be an output comparison (account, command results, books, markets or
tickers). Performance, coverage, transport and harness-integrity diagnostics
can never be accepted. A path cannot be both known and accepted, and an
accepted key that a complete replay no longer reproduces fails verification
as stale. `verify-gates.mjs` labels a gate whose only differences are
accepted `PARITY WITH ACCEPTED DIFFERENCES` and counts those gates separately
from known failures.

`sdk-rs/harness/parity-report.mjs` aggregates gate reports, either `gate.json`
files or a gate ledger directory (`--ledger`). It prints a per-venue,
per-entity table of accepted, known and new counts, with samples in `--json`.
It reports and never gates.

Work order: Bitget end to end first, then the cross-venue client-ID and
time-in-force classes, then the beta venue families. Every remaining
divergence becomes a fix or an accepted entry. Hyperliquid freshness is out of
scope; Hyperliquid divergences go to the Hyperliquid lane.
