# Hyperliquid Rust port

Lane: `feat/rust-hyperliquid`; target: `feat/rust-rewrite`.
Spec: `RUST_REWRITE.md`; process: the Process section of the upstream
`VENUE_PORT_BRIEF.md` fetched 2026-10-19. Workspace migration
merged at `fefe9db44c`; crate now lives in `sdk-rs/venues/hyperliquid/`. Full automatic execution, with a
sub-agent implementation review and fixes before every phase commit.

## Phases

1. Public data: crate and workspace registration; typed REST metadata/tickers
   for core and HIP-3, streaming POST lists, allMids and compressed fastAssetCtxs,
   snapshot books with BBO overlay, candles, subscriptions and socket watchdog.
   Verify documented payload tests, bans, fmt and clippy.
2. Private data: EIP-712 phantom signing, identity and abstraction resolution,
   core/per-DEX positions and settings, unified collateral, fills deduplication,
   REST reconciliation with source-time and cancellation guards. Verify signed
   vectors, snapshot races, reconnect and failed-read preservation.
3. Trading: sequenced command jobs; builder approval cache and lazy check;
   caller cloid preflight for all shapes; unique cloids on every wire; brackets,
   full/partial stops, cancel+replace and TP-to-SL reclassification; cancellation,
   leverage, margin and one-way mode. Verify wire/signature and lifecycle tests.
4. Queries: fills, order history, funding/income and public trades in engine JSON
   result shapes, paging/deduplication, filters and fail-closed reads. Verify
   documented responses and paging boundaries.
5. Harness and gates: generic body-sensitive replay, TS registration, synthetic
   private tape and recorded two-book public fixture. Run all brief gates and
   repository checks; preserve Binance. Record correctness, CPU and memory.

## Audit decisions

- The requested lane overrides starting a fresh branch from master. Do not
  rebase or modify master. Merge latest origin/feat/rust-rewrite after gates.
- Hyperliquid info reads are POST requests: preserve the request body when
  streaming arrays, and key harness requests by normalized body as well as URL.
- Wire key order is signed: typed action structs serialize in declaration order;
  cloid is last, after order type. Never serialize an untyped JSON value tree.
- l2Book is an authoritative 20-level snapshot, with no sequence deltas; BBO
  overlays only when at least as new as depth. Reconnect discards depth.
- Caller IDs are validated across the entire batch before any write. A bracket
  uses that ID only on its entry. Replacement always gets a fresh ID.
- Failed or stale snapshots cannot erase newer account state. DEX snapshots
  replace only their own scope. Terminal events suppress overlapping REST rows.
- A builder address and fee are one pair; maxBuilderFee is checked against that
  account's rate. Unknown approval triggers a read, never an assumed approval.
- Keep TS available as the replay oracle until gate verification; do not remove
  code required to execute the mandated TS comparison.

## Progress and evidence

- Planning: architecture, TS contracts, signing helpers and harness inspected.
- Phase 1: implemented and reviewed by `review_public`. Fixed all five findings:
  tuple arity/alignment, finite numbers, bounded metadata read retries, candle
  limit/from semantics, Unicode interval panic. Six documented-payload tests
  pass; bans, formatting and clippy pass. Registration in wasm/harness remains
  in phase 5, so phase 1 does not enable an incomplete venue in user flows.
- Phase 2: implementation checkpoint. Thirteen account tests, one official SDK
  signing-vector test (mainnet/testnet/vault), and six public tests pass.
  `review_private` found and drove fixes for position IDs, core trigger fallback,
  suppression recovery, user event shapes, TIF retention, newer REST source
  times, per-symbol fill clocks and HIP-3 in-flight refresh collapse. Final
  review drove two further fixes: snapshot and optimistic fill clocks are
  separate (two executions in the same millisecond both apply); order reads
  are demand-driven, debounced at 500ms with a 1s minimum cadence. Weighted
  reads bound background traffic; HIP-3 positions reconcile at 120s, with
  debounced fill heals. Bans, fmt, clippy and all twenty native tests pass.
  Deleted unused `crypto/wallet_utils.rs`; private-key address derivation now
  uses a transport-free Ethereum primitive for non-fatal startup warnings.
- Phase 3: implemented and reviewed by `review_trading`; nine trading tests,
  two additional signing tests and previous tests pass (31 total). Fixed review
  findings: leverage must use integer MessagePack, every cancel acknowledgement
  must validate before replacement, and queued settings resolve when executed.
  Builder timeout matches TS's explicit 8s negative-cache fallback; transport
  errors remain unknown for lazy checking. Every wire has a cloid; own bracket
  legs are identified from their records before REST returns.
  Deliberate TS fixes recorded in `gate.mjs`: malformed cancel acknowledgements
  fail closed, queued setting changes compose, and HIP-3 bracket pending rows
  retain each leg's metadata instead of assigning entry metadata to every oid.
- Phase 4: implemented and reviewed by `review_public`; ten history regressions
  pass (41 native tests total). Streamed reads return Binance JSON shapes;
  cloid lookup is store-pure and never reports a definitive miss. Phase 5
  enables its TS wrapper with venue registration. Review fixes include integer
  wire times (live 422/200 control), whitespace numeric strings, inclusive-page
  saturation errors, raw-millisecond fill ordering, and reserving response
  weight before concurrent heavy reads. Queries share the typed read budget.
  Deliberate TS fixes in gate.mjs: execution tid identity and signed rebates,
  vault/range/all-symbol fills, fail-closed history errors and saturation,
  correct newest-fill limits, and exact income-kind filtering.
- Phase 5 checkpoint: wasm/TS venue registration, forwarded wallet/vault/builder
  options, perps-only validation, replacement cloid and lookup wrapper are in
  place. `review_private` fixed promise rejection semantics; 269 targeted TS
  checks pass. `review_public` audited body-sensitive replay, caller-ID
  preservation, same-URL socket fanout and normalized outbound capture; seven
  harness regressions pass. `review_trading` generated and replayed the
  deterministic private tape against TS with zero unmatched requests. All 41
  Rust tests, native/wasm lint, bans and fmt pass; repository lint passes.
  Release wasm build, replay parity/performance and repository checks remain
  in progress.
- Phase 5 replay checkpoint: Hyperliquid private and 20-second/two-book public
  replays pass account, market, ticker, book, command and normalized outbound
  request/subscription comparisons. Binance public/private also pass. The
  recorded fixture is committed. Fixed the recorder to tap streamed responses;
  timed private snapshots preserve live orders between placement and cancel.
  Added shared prehashed EIP-712 hashing and fixed-schema Agent encoding;
  official signatures and dynamic-parser equivalence pass (43 venue tests,
  five crypto EIP-712 tests). Public HIP-3 subscriptions now follow TS's account
  identity requirement. Wasm wrapper 4/4 and Workers host 2/2 checks pass.
  Repository typecheck (44 tasks), lint (28 tasks), tests (27 tasks) pass.
  Combined wasm remains over the 1.2 MB size budget; a size-optimized release
  build and final replay repetition are in progress. These are preliminary
  replay passes, not a claim that every gate is complete.

Standing overseer authorization (2026-10-20): resolve future landing-branch
merge conflicts while keeping its layout; never stash, use WIP commits.

After all gates: merge origin/feat/rust-rewrite into lane, push, open PR into
feat/rust-rewrite, adversarial local sub-agent review with findings posted to
PR, fix and repeat; merge PR and create a fresh follow-up branch. No master
changes or release automation.

Phase 5 size checkpoint: default builds retain all compiled venues; optional
`rust:build-wasm --venue <venue>` applies the browser size budget per venue.
This avoids raising the 1,258,291-byte budget when a new venue is registered.
Hyperliquid is 1,147,832 bytes; Binance is 907,075 bytes. Both single-venue
public/private gates pass (public medians across three runs):

| Venue | TS CPU ms | Rust CPU ms | TS steady ms | Rust steady ms | TS retained MB | Rust retained MB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Hyperliquid | 317.4 | 62.2 | 23.6 | 21.4 | 4.1 | 1.7 |
| Binance | 213.0 | 88.2 | 120.6 | 35.7 | 2.3 | 2.0 |

Private gates compare correctness only: Hyperliquid 26 events/seven commands;
Binance 14 events. Both host integration suites pass. Native/wasm Clippy passes
for default and each single-venue feature combination. The final harness review
fixed availability to advance with delivered events and limits Binance fixture
integration tests to the Binance CI matrix entry. Default combined-build replay
revalidation is running. Origin has since landed Bybit; merge it next and repeat
affected checks before opening the PR.

Phase 5 complete after merging Bybit at `1e3dee5f5c`: all local gates pass.
Final single-venue bundle sizes are Hyperliquid 1,148,178 bytes, Binance 907,396
bytes and Bybit 886,508 bytes, each below 1,258,291 bytes. Default combined
release build also passes, with all three constructors retained; six real-wasm
TS bridge checks and two Workers host checks pass. The CI matrix runs those
integration checks on the combined build before replacing it with the selected
venue bundle for size/replay checks.

| Venue | TS CPU ms | Rust CPU ms | TS steady ms | Rust steady ms | TS retained MB | Rust retained MB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Hyperliquid | 398.0 | 69.8 | 33.0 | 29.1 | 4.1 | 1.9 |
| Binance | 149.6 | 62.8 | 81.8 | 20.8 | 2.3 | 2.3 (1.02x less) |
| Bybit | 393.7 | 291.9 | 272.6 | 169.7 | 3.6 | 2.7 |

Public numbers are three-run medians. All three private correctness gates pass.
Bans, workspace fmt, venue Clippy, default wasm Clippy and 115 venue native tests
pass. Repository typecheck, lint and test pass; the first post-merge test run
hit three unrelated Hyro configuration import timeouts, and the complete retry
passed without code changes. Twelve harness tests pass. Bybit's optional
API-key/VIP UI metadata request has an explicit scoped normalization, documented
in its port plan; this is an intentional omission, not a TS bug. Authentication,
trading requests and every other read/subscription remain checked.

Next: publish PR into feat/rust-rewrite, adversarial local reviews with findings
posted on the PR, fixes/review repetition, then wait for green CI before merging
and creating the follow-up branch.

PR #1306 is open into feat/rust-rewrite. Adversarial pass 1 found stale HIP-3
scope replacement, reconnect order-read invalidation, cancel/read resurrection,
settings acknowledgement/uncertainty races, failed-signature cancellation
rollback, and a replay gate that hid duplicate writes. Findings are posted on
the PR; fixes and second reviews are in progress. Pass 2 additionally found
queued amend duplication and a placement-ack/immediate-cancel window.

Merged incoming OKX layout/registration at 693ad4f139, preserving four venue
features and all OKX aliases, auth/subscription/heartbeat validation, staged
private requests and TS oracle fixes. Combined builds retain all constructors;
per-venue builds keep the original 1,258,291-byte browser budget. The replay
now consumes each economic-write response once and compares write counts.
Seventeen harness tests and 269 TS bridge/guard checks pass after resolution;
OKX-only host compilation passes. Re-run all gates after review fixes.

PR review checkpoint: the Hyperliquid wrapper now preserves identity-based
private reads for watch-only applicationId/vault accounts and legacy key-role
resolution. Explicit public-only mode still wins; local signing remains in the
native trading path. Five regressions added; 278 targeted wrapper/client-ID/OKX
oracle checks pass. First full test run overlapped the wrapper edit and observed
its pre-fix module; rerunning the settled source along with typecheck/lint.

Private-data review fixes: retry invalidated order snapshots, fence staged rows
against successful cancels, reject stale HIP-3 scope replacement, and keep fill
watermarks monotonic. Settings use their own revision/time barriers so position
updates cannot reverse acknowledged writes. Ambiguous settings writes recover
through fresh, budgeted per-symbol activeAssetData reads, including flat/HIP-3
markets; failed reads retain state and retry. Sixteen account tests pass.
Independent review accepted the final revision/time fences and wrapper startup.

Trading review fixes complete: reserve each amendment source across queued and
active jobs, revalidate it before canceling, and abort replacement on an observed
fill or an ambiguous terminal-miss cancel acknowledgement. Ordinary cancels
remain idempotent. Core resting acknowledgements retain Open order metadata
before returning IDs and advance the order-read revision. Definite venue settings
rejections retain certainty; only ambiguous writes require reconciliation. Failed
signing restores optimistic cancellation status. Regressions also cover flat
settings recovery and prewrite REST revisions with future source timestamps.
All 56 Hyperliquid tests (19 trading), Clippy with denied warnings and formatting
pass on the settled source. The independent final lifecycle review is pending.

Final lifecycle correction: after a replacement has already been dispatched, a
late source-fill event cannot hide its acknowledged replacement IDs/cloid behind
an error. Completed jobs return their known result; fills before dispatch still
abort. Independent final reviewer accepts this correction and reports no remaining
P1/P2 findings in scope, with diminishing returns. All 57 Hyperliquid tests
(20 trading), Clippy, formatting and diff checks pass.

Post-review four-venue checkpoint (before Crypto.com integration): all public and
private replay gates pass, including all four OKX aliases. Hyperliquid public
three-run medians: TS/Rust main-thread CPU 273.1/70.4 ms, steady 31.7/27.9 ms,
retained 4.4/1.8 MB; bundle 954,721 bytes. Binance 174.0/80.7 ms, retained
2.3/2.3 MB (1.03x less), bundle 763,031 bytes. Bybit 482.5/421.0 ms,
retained 3.6/2.8 MB, bundle 742,675 bytes. OKX 1049.0/865.8 ms,
retained 3.9/3.8 MB (1.03x less), bundle 707,514 bytes. All fit 1,258,291.
All 241 workspace tests, native/wasm Clippy, bans, fmt, 17 harness tests,
278 targeted TS checks, 16 combined real-wasm checks and two Workers checks pass.
Repository typecheck/lint pass. All 6,511 safe-cex CI tests pass with capped workers;
full test-unit retries still encounter existing Hyro import timeouts under load.
No timeout or unrelated production-code change has been made.

Origin now includes Crypto.com at e6e410146e. Merge its venue, harness memory and
protocol improvements, and provably unused crypto deletions while retaining the
Hyperliquid helpers used by production signing. Re-run affected gates before
pushing the final PR head; adversarial findings/resolutions are posted on #1306.

Crypto.com merge resolution: retain all five default constructors and optional
single-venue features at the unchanged budget; union TS registrations and CI
matrix. Adopt incoming unused crypto deletions, retaining only the production
Hyperliquid prehashed EIP-712 digest, vault-address validation and private-key address helpers plus the
prehashed/dynamic oracle equivalence test. Crypto.com explicitly rejects the
Hyperliquid-only client-ID query variant. Harness resolution combines incoming
memory/protocol audits with strict write identity, timing and multiplicity.

Five-venue merge review: independent harness audit found no P1/P2 blockers and
accepted HTTP write consumption/count/order, timed reads, socket fanout, exact
identities, protocol validation and retained-memory accounting. Independent host/
crypto/CI audit accepted the five-feature union; it found the incoming Crypto.com
wrapper's missing client-ID rule. Added its existing TS rule and common guard
regressions. Full native workspace tests and native/wasm Clippy pass; Crypto.com-
only host compilation passes. Isolated unchanged Hyro configuration tests pass
10/10, confirming the full-suite import timeouts are resource-sensitive.

Settled five-venue repository checks pass: typecheck, lint and all 27 root test
tasks. Capping Vitest to one worker and serializing package jobs avoids the Hyro
import timeout without changing assertions or timeout thresholds. The new
Crypto.com common-rule regressions pass alongside Hyperliquid/OKX checks;
31 harness regressions pass. Final five-venue replay/host builds are running.

Five-venue replay checkpoint: Hyperliquid and Binance public/private pass. Updated
public medians with CPU isolated from memory collection: Hyperliquid 432.5/56.1 ms
TS/Rust CPU and 5.1/1.9 MB retained (7.72x/2.75x); Binance 243.8/103.2 ms and
4.3/2.7 MB (2.36x/1.60x). Bundles 956,050 and 766,451 bytes. Crypto.com public
outputs/performance pass, but exact subscription parity exposes an incoming
startup bug: two early books consume automatic ticker capacity, dropping two
seed ticker channels. Fix its automatic/lazy budget distinction with regression.

WEEX has landed at origin/feat/rust-rewrite (6cd219ce4c), including host lifecycle
and OKX allocation fixes. Integrate that landing tree while the bounded Crypto.com
fix runs, then verify the settled tree. No gate threshold or parity exclusion is
being relaxed.

WEEX integration review: six constructors/features, WS upgrade headers, active
symbol notifications and host lifecycle changes are retained. WEEX-only testnet
field is cfg-gated; native WEEX-only/Hyperliquid-only builds pass. Added the
incoming missing WEEX client-ID rule with common regressions. Hyperliquid wallet
semantics, all OKX aliases and the per-venue 1,258,291-byte budget remain intact.
Independent reviewers accepted the union and Crypto.com automatic budget fix.
The harness review found missing WEEX auth timestamp validation; both REST and
upgrade now require safe millisecond timestamps within 30 seconds. Fourteen
malicious signed cases reproduced the gap before the fix; 54 harness tests pass.

First six-venue CI replay exposed a harness startup race in Bybit: accelerated
playback could finish before its paced subscription outbox reached the requested
books, so complete output/performance passed while topic parity failed. Public
Rust playback now waits for all requested Bybit book topics (queued after initial
tickers), matching the TS readiness barrier. Private auth readiness and strict
post-run topic/write comparisons are unchanged. Fifty-five harness tests pass,
including a regression that refuses early ticker-only/wrong-depth/private topics.
CI venue jobs no longer cancel other venues on one failure, preserving diagnostics.

Follow-up Bybit rerun exposed the symmetric TS pacing race: its handler readiness
also precedes send completion. Both public realms now require all seed-derived
912 ticker topics plus requested books before playback; 57 harness tests pass.
BingX has landed at 022ab79de5 with the same seed-derived startup barrier and a
100ms elapsed backing-buffer quiescence fix. Integrate its central pending-topic
counter using our canonical keys, strict comparisons and Hyperliquid config.
Six-venue checks so far: combined 17 bridge/seven host tests pass; Crypto.com,
Hyperliquid, Binance and WEEX public/private gates pass. Native364, targeted296,
root typecheck/lint/all27tests pass. Remaining Bybit/OKX replays and CI stay gated.

BingX integration checkpoint: retain all seven default constructors/features,
existing single-venue budget and incoming workspace structure. The new query
variant is explicitly unsupported in BingX; its existing client-ID shape policy
is retained in the wrapper. Native workspace tests, native/wasm Clippy, bans,
formatting, root typecheck/lint and 296 targeted TS tests pass. The full root
suite again hit one existing Hyro dynamic-import timeout (6,528 other safe-cex
tests passed); repeat in isolation after builds. Harness union passes 61 tests,
including strict identity/timing checks and symmetric Bybit startup readiness.
The host/CI/query union passed independent review; final harness audit and
settled-tree Wasm replay gates remain pending.

MEXC landing integration at 63d521a860: keep eight optional venue features and
constructors, default all eight, and add its per-venue CI matrix entry at the
unchanged byte budget. Preserve native placement-unit frame metadata and
independent leverage rails; Hyperliquid explicitly leaves those hedge-only rails
absent. MEXC rejects the Hyperliquid-only lookup query. The incoming leverage
metadata bridge was missing from the first resolution; independent review caught
it and the implicit public-mode guard. Restored its Deserialize/conversion arms,
added a bridge regression, and use isPublicDataOnly. Reviewer verified every
incoming host change against the merge base and accepted the correction.

Harness integration audit caught a lost Crypto.com inbound audit callback after
the MEXC socket routing change. Central delivery now observes each actual frame
before its callback, covered by an auth/subscription/heartbeat regression. MEXC
socket ordinals/epoch/public timer clock and HTTP trim anchors coexist with HL
legacy shared-URL fanout and explicit private classification. Actual replay keeps
strict write queues/count/order, caller IDs and economic number/string types;
MEXC compatibility is limited to ID fields and generated externalOid. Active-only
rolling windows retain exact width; historical windows stay absolute. All 78
harness tests and syntax checks pass. Workspace523 native tests, native/wasm
Clippy, bans, fmt, root typecheck/lint pass; new host regression, combined bridge,
root tests and all settled-tree replay gates are running. Independent final
harness correction review remains pending before the gate completion claim.

Eight-venue gate checkpoint on 0192036dd5: CI passes native fmt/Clippy/tests,
wasm32, bans, secret scan, and public/private gates for Binance, Bybit, BingX,
Crypto.com, WEEX and all four OKX aliases. Hyperliquid public output/performance
pass but CI rejects one TS backing-buffer baseline (-64 bytes); do not clamp it
or relax any assertion. Fresh local Hyperliquid public/private gates both pass:
298.1/49.6 ms TS/Rust CPU, 18.6/16.4 steady, 5.5/1.9 MB retained, bundle985134.
Binance public passes 244.8/113.2 ms CPU,139.5/47.0 steady,4.3/2.4 MB retained,
bundle795305. Bybit public/private pass locally after symmetric startup fix,
498.1/350.1 ms CPU,306.6/192.4 steady,5.9/2.2 MB retained,bundle774985.
All three sizes stay below1258291. Default combined19 real-Wasm/13 host tests
pass. A new CI head is required for the MEXC correction below and will supply
fresh Hyperliquid samples; add no sampling/retry policy for one invalid baseline.

Strict MEXC private read parity exposed its protective placement refresh using
account-wide reads where TS debounces three per-symbol order surfaces. Implement
scoped, serialized reconciliation retaining other symbols and all revision,
terminal/placed, and failed-batch fences. Add two-symbol, newer-WS and failed-read
regressions; independently review before commit. This fixes the actual lifecycle
rather than weakening request parity or appending unused dummy reads.

MEXC correction complete: per-symbol1500ms debounce queues serialized three-surface
snapshots through existing staging and revision fences, retaining unrelated rows
and committing no subset after failure. Reviewer caught scoped regular-row
preservation in the TS contract; retain absent same-symbol regular Open rows only
on scoped refreshes, leaving full-account cleanup unchanged. Reconciliation also
schedules when WS wins the placement-ack race. Four regressions cover scope,
newer WS/terminal rows, failed subsets, timer serialization and WS-before-HTTP.
All55 MEXC native tests, strict Clippy, fmt and independent re-review pass.
Wasm private replay and a fresh Node22 Hyperliquid sample are running; new CI
must pass before merge. Both general PR CI jobs on0192036dd5 are now green.

Real MEXC replay found its conservative, native-tested denied-batch fallback
paces the first regular write by1500ms. The later plan ACK lands near2210ms,
putting its scoped refresh at3710ms, after the old3300ms fixture end. Extend the
final observation to4300ms; reviewer accepted the trace and unchanged pacing.
Exact requests then pass. The newly observed4s poll exposed stale returned regular
rows being filtered out when their current store row was already terminal.
Reconcile only returned regular terminal rows through monotonic fills/status,
retaining absent-row/tombstone and stop/plan protections. Review caught an
Open/equal-fill row reviving Canceled; explicit terminal status preservation now
covers lower/equal/higher-fill cases across regular/full snapshots. All57 MEXC
native tests and strict Clippy pass; final review accepted, real replay pending.

Final repository checks pass: typecheck44tasks, lint28tasks, all27root test tasks,
including6597safe-cex tests in each CI/unit task. Run the worker cap through a clean
minimal environment: Turbo strict mode filters VITEST_MAX_WORKERS; do not widen
timeout thresholds. Final workspace528 native tests, native/wasm Clippy, fmt and
bans passed before the last two MEXC regressions. The Node22 Hyperliquid rerun
reproduced an invalid cold buffer baseline (-88) while warm runs retained+32;
five seconds of idle stability still yielded-24 on a fresh tsx disk cache.
Investigate loader allocation lifetime; longer quiescence alone is disproven.

Memory audit correction: the prior claim that a negative component necessarily
invalidates this baseline was wrong. The estimator is NET retained growth:
signed heap/buffer differences may shrink while the aggregate remains positive.
Longer waits, no-cache, setup-factory and deferred-entry experiments did not
reliably remove those legitimate counter changes; none are adopted. Keep the
original sampler/formula and raw signed deltas. Validate finite/nonnegative
absolute before/after counters and wasm/aggregate, finite signed differences,
and exact difference/sum consistency. Independent review accepts this correction;
no clamp, retry, sample exclusion or comparative-threshold change is introduced.
All83 harness tests pass, including deterministic negative-component arithmetic
regressions. Original-sampler Node22 Hyperliquid public gate passes all3pairs:
TS/Rust CPU526.5/62.4ms,steady41.6/25.9ms,retained5.4/1.8MB. MEXC real Node22
private gate now also passes exact requests/writes and account state. All530
native workspace tests and native/wasm Clippy pass on the settled Rust source.

Origin has advanced to d9db55921c with BloFin and MEXC parity fixtures. Merge
that landing tree before final CI; preserve all nine constructors, the scoped
reconciliation/terminal fixes, strict request identity, and validated net-memory
arithmetic. Re-run affected gates and independent integration reviews.

Nine-venue merge resolution: all nine optional/default constructors retained,
incoming BloFin leverage/margin/fill-unit helpers and compact package profiles
adopted, per-venue CI budget remains1258291. BloFin-only/default builds pass;
its query surface explicitly rejects the Hyperliquid-only cloid lookup. Independent
host/CI/wrapper review accepts the union. Reviewing the incoming range conversion
exposed Hyperliquid's missing seconds-or-ms wrapper normalization: apply the same
TS floor/units contract before native millisecond queries. Three command-forwarding
regressions pass;295 targeted wrapper/common-ID tests pass. Root typecheck/lint,
all605 native workspace tests, native/wasm Clippy, bans and fmt pass.

Harness union passes102tests and all28module/build syntax checks. Preserve strict
writes/caller/economic/u64 identity, all topic checks, HL fanout/classification,
Bybit startup readiness, validated signed-memory arithmetic, and incoming elapsed
quiescence with real timer pauses. BloFin authentication is audited before nonce
normalization, including credentials, signature, timestamp, nonce and auth-ack
ordering. MEXC parity fixtures add unsupported-shape probes and post-cancel empty
snapshots; terminal3300 plus4000/6000pongs now observes delayed scoped reconciliation
and subsequent polls. Final integration audit and real nine-venue CI remain gated.

Final nine-venue integration audit accepted after one correction: incoming BloFin
oracle copied Rust margin into the TS expectation. BloFin declares position margin
unsupported and native rows use None; normalize only the TS zero placeholder to
null independently, retaining explicit values for comparison. Regressions prove
arbitrary123456 and plausible8 Rust margins cannot establish their own expectation.
Existing Binance behavior remains untouched. All104 harness tests, syntax and diff
checks pass, and localized independent re-review reports no remaining findings.

Settled nine-venue local checkpoint: all605 native tests; native/wasm Clippy, bans,
fmt; typecheck44,lint28,all27root test tasks (6602safe-cex tests in each task);
20combined real-Wasm bridge tests and15host tests pass. Hyperliquid public/private
pass on Node22: CPU374.3/72.6ms,steady47.4/28.0ms,retained5.5/1.8MB (TS/Rust),
bundle951036bytes. Binance public passes259.1/117.4ms,steady145.7/47.2ms,
retained4.2/2.2MB. Final remaining local private/integration replays and CI on the
pushed nine-venue head remain required before merge. Findings/resolutions are
posted on PR1306; no performance threshold or byte budget was raised.

Backpack landed at b265a0bab0 while the nine-venue checkpoint finished. Integrate
its crate, PATCH transport, owned sorted-book load, disposal/UTF-8 host guards,
and shared actual Tape tests under the standing merge authorization. Retain all
ten default constructors and single-venue feature builds with the unchanged
1258291-byte limit. Bounded independent wrapper/host/CI review accepts the union:
Backpack receive window, corrected clock and bound prototype transport methods
are scoped to Backpack; Hyperliquid watch-only, cloids and history units remain
intact. Backpack-only and default host checks and workspace formatting pass.
The shared Tape extraction must preserve strict financial identity, ordered
single-use writes, all authentication/topic checks and signed net-memory math.
Native/TypeScript/full host verification and end-of-phase harness review are
running before the integration commit; real replays and final CI remain gated.

Ten-venue harness audit found a persistent-error gap in incoming Backpack
verification: a host-caught invalid periodic signature could leave loaded state
and no reported audit failure. Shared signature verification now persists both
false results and thrown verifier errors exactly once; both actual replay realms
reject them before returning metrics/results. Caught REST/WS and parser-throw
regressions pass. Localized independent re-review accepts the fix and finds no
remaining bounded integration issues. All137 harness tests pass, including the
existing104 plus shared transport/Backpack tests;29module/build syntax checks
pass. All655 native workspace tests and the combined Wasm bridge pass. Final
Clippy/root tests and real ten-venue replays/CI are still required before merge.

Ten-venue settled head d8a71bef5e passed Rust CI39308564809: all ten public and
private replays, four OKX aliases, native/wasm checks,137harness tests, and
original1258291-byte per-venue size budget. CI Hyperliquid: CPU253.5/57.2ms,
steady30.0/16.6ms,retained4.9/1.8MB (TS/Rust),951261bytes;26private events.
CI Binance: CPU278.7/132.3ms,steady156.0/50.9ms,retained4.2/2.2MB,689741bytes.
Local public/private replays pass too. One initial local TS public baseline did
not settle before measurement; the same unchanged sampler then passed all three
pairs. No sample selection within a gate or threshold adjustment. All27root
test tasks passed (6602safe-cex tests in each CI/unit task), typecheck44,lint28,
655native tests, native/wasm Clippy,21real-Wasm bridge tests and25host tests.

Bitget PR1308 landed at4e468ed2e5 before general CI finished and created a new
conflict. Integrate its crate,USDT/USDC constructor aliases, streamed envelope
errors and JSON path handling, and incoming O3 dispatch/Bitget profiles. Keep
all eleven optional/default features and unchanged per-venue size budget. The
new venue explicitly rejects HL-only cloid queries. Shared harness union and
independent reviews are required before the next checkpoint; native and host
checks plus affected replay/CI gates are running. The ten-venue full root suite
already passed; rerun affected typecheck/lint and real-Wasm integration tests.

Eleven-venue root review found two incoming integration omissions: the build
selector rejected Bitget and the wrapper lacked client-order-ID rules for both
Bitget aliases. Add selector/help/error support and the exact TS charset/length
rule for bitget/bitgetusdc. Common fail-closed coverage plus placement/mint/live
lookup regressions pass315tests; final typecheck/lint pass. Localized independent
re-review accepts both fixes. All734native tests, native/wasm Clippy, bans/fmt,
25combined real-Wasm bridge tests and25host tests pass. Bitget-only/default
constructors build; incoming O3 dispatch leaves HL single-venue986799bytes.
All147harness tests (137baseline plus Bitget contracts/exact Backpack cancellation)
and30module/build syntax checks pass. Independent harness audit and required real
HL/Binance replays are running before the integration checkpoint and final CI.

Independent eleven-venue harness audit accepts the union with no actionable
findings: Bitget IDs are validated before consuming replies and alias only
matched creations; the taped40009retry preserves ACK accounting; routing keeps
exact product/topic/instrument. TS Classic margin spelling is an exact fixture
body exception. Backpack cancellation IDs remain exact. Wasm bytes are sampled
synchronously after settled JS memory, preserving consistent signed arithmetic.
No broad fallback, threshold or budget relaxation was adopted.
