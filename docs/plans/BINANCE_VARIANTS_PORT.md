# Binance variants port

Branch: `feat/rust-binance-variants`, based on `origin/feat/rust-rewrite`.
Follow `VENUE_PORT_BRIEF.md`; all PRs target `feat/rust-rewrite`.

## Phases

1. Public profiles: extend the existing Binance paths table and options for
   Spot and COIN-M. Preserve USD-M/PM. Verify markets, ticker seeds/streams,
   spot versus futures depth continuity, candles and mainnet/testnet routes.
2. Private profiles: typed Spot signed WebSocket API subscriptions and account
   balances; COIN-M contract sizes, coin collateral valuation and private
   reconciliation. Preserve stale response and reconnect fences.
3. Trading and queries: profile-specific order types, identifiers, endpoints,
   split sizes, bracket handling, cancel/amend, unsupported command preflight,
   and history. Keep whole-batch client-ID validation before writes.
4. Integration and evidence: WASM/worker registration, strict independent replay
   tapes, public captures, CI coverage, roster/docs. Reproduce Hyro CPU misses,
   profile any genuine Rust loss and fix its cause without relaxing gates.
5. Final adversarial reviews: post findings on the PR, fix and repeat, rerun all
   required checks including USD-M/PM/Hyro. Merge only with green CI, then cut
   a fresh branch from the landing branch.

Every phase receives a sub-agent audit and commits at stable points. No stash,
rebase, force push or master changes. Official documented payloads and runtime
contracts define regression expectations. TS defects fixed deliberately require
`KNOWN_TS_FIXES`, commit-message and PR disclosure.

## Progress

- Planning: inspected the four TS Binance profiles and current Rust engine.
  Public/account implementation and Hyro investigation have bounded owners;
  trading/query/integration remains with the lane owner.


### Native implementation checkpoint

Public/private/trading phases share option selection and socket callbacks, so
these implementations are committed together after independent phase reviews.
USD-M/PM regression tests remain in the Binance crate. Native checks reached
73 passing cases before the final depth-generation regression; crate Clippy,
WASM-target Clippy, bans, root typecheck (44 tasks), root lint (28 tasks), the
first two actual WASM boundary cases and six independent frame tests passed.
Final rebuilt-artifact checks and replay evidence remain pending.

Phase reviews fixed protective lot splitting, ignored trigger intent, post-only
amendment semantics (including REST seeds), duplicated/reordered executions,
subscription ACK deadlines, Spot collateral valuation, native COIN-M sizing in
both worker/main frame appliers, and old depth snapshots crossing resubscription.
A real live Spot failure was isolated to large compressed subscription messages:
17,392-byte requests received close1008 `Payload too long` with Node's `ws`;
smaller messages worked. Public control messages now have bounded topics/bytes
and 400ms pacing; recapture will verify the correction.

Known TS defects deliberately fixed: retired aggregate Spot feeds; signature-only
Spot subscription liveness; split quantities rounded away; invalid/oversized Spot
protective legs; Spot fill price and replay/cancel identity; lost maker semantics
on amendment; COIN-M caller-ID preflight and duplicate/terminal execution state;
malformed inverse order snapshots. The harness checkpoint lists any further
verified differences. Defensive rejection and Rust-only bridge/depth fixes are
not presented as new TS defects.

Hyro unchanged baseline: three-run public PASS, TS/Rust main50.8/34.4ms,
steady17.6/13.2ms, retained16.1/0.9MB. Profiling found no confirmed production
regression. Machine-wide concurrent work prevents claiming strict isolation;
final fresh-artifact Hyro gates remain required with unchanged thresholds.


### Integration and final review in progress

- Fresh Spot capture after bounded subscription frames: 7,065 frames, 3,729
  markets and two books over 20 seconds; no policy closes. COIN-M live capture:
  203 frames/two books over 20 seconds. Both trimmed fixtures are being gated.
- First strict private replays pass for all four profiles: USD-M14 frames,
  PM6, Spot8, COIN-M6. Helper suite235 passes. These measurements precede the
  final lifecycle fixes and do not replace final-head gates.
- Final adversarial review found negative/tiny inverse quantities turning into
  full-position stops, cancellation acknowledgements accepted without proof,
  and amendments replacing already-filled quantities. Fixes and regression
  proofs are in progress before final gates/PR creation.
- TS retention audit: `registry.ts`, `registry.native.ts`, direct SDK creation
  and non-opt-in workers still instantiate these TS classes; only explicit
  `options.engine === 'rust'` selects the new registered profiles. Their TS is
  not provably unused. Deleting it would change the existing rollout policy and
  break direct SDK/default worker paths, so it remains the executable reference.


Initial public fixture gates (three-run medians; superseded by final-head checks):

| Profile | Frames | TS/Rust main CPU ms | TS/Rust steady CPU ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| Spot | 7045 | 530.4/339.7 | 224.0/89.8 | 26.5/7.5 |
| COIN-M | 202 | 172.5/65.0 | 114.7/35.5 | 15.5/1.5 |
| USD-M | 873 | 189.8/89.6 | 98.8/34.6 | 18.9/2.6 |
| PM | 873 | 187.3/90.9 | 103.0/32.7 | 18.4/2.7 |

Root tests passed all27 tasks (6971 safe-cex cases in each suite). Harness
checkpoint `9a96deeaab` includes240 helper tests, strict raw signing and
subscription audits, truthful input-derived TS reference corrections, and both
live public fixtures. The final lifecycle checkpoint will rerun private gates
against authoritative cancel acknowledgements.

Nonblocking review limitation: native execution/terminal identity fences retain
per-order history for the account lifetime. This avoids forgetting stale native
identities; fixed-tape memory gates do not certify bounded lifetime retention.
No gate threshold or measurement interval was relaxed.


### Adversarial lifecycle checkpoint

Final native and independent review passed:96 native tests, strict crate Clippy,
240 harness cases, four real WASM boundary tests, root typecheck44 tasks,
lint28 tasks and root tests27 tasks (6971 safe-cex cases per run). The ten
original cancellation/amendment regression cases all fail on checkpoint
`23f35ebdf3`; the expanded seventeen-case lifecycle suite passes on the fix.

Cancellation now proves native identity, status and final cumulative quantity;
amendment renders only after that proof and subtracts final cumulative fills.
Filled wins over a late cancel. Unknown/malformed/partial rejection never retries
or advances replacement. Missing original rows, reused client IDs and concurrent
amendments through the replacement POST are fenced. Native terminal records also
protect later replica reads after cancel-all while retaining late executions.
Mixed Spot OCO replies are validated atomically. Raw inverse quantities that
would round into an unintended full stop reject before any batch action.

COIN-M public trades have a documented native regression. Unsupported private
history remains the existing TS empty-result contract; no unverified history API
expansion is claimed. Final replay rebuild is879753 bytes for Binance-only,
below the unchanged1258291-byte limit. Final exact-head gates remain in progress.


### Final integrated replay evidence

Merged landing `373cc09864` (KuCoin) into this branch as `ddd5448fee`;
independent integration audit preserved both venues' registrations, native frame
units and protocol contracts. Merged harness helpers:262/262 pass. Binance native
suite:96/96 pass; fmt, bans, crate Clippy and WASM-target Clippy pass. Root
typecheck44 tasks and lint28 tasks pass. Host/wire45 tests pass.

All eight Binance gates pass on that integrated source. Public gates use three
runs and unchanged thresholds; every public Rust main/steady CPU and retained
memory metric is below TS. Private gates use one run and assert correctness,
including zero unserved requests, missing writes, bad signatures or protocol
errors. Fresh private frame counts: USD-M14, PM6, Spot8, COIN-M6.

| Profile | Frames | TS/Rust main CPU ms | TS/Rust steady CPU ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| USD-M | 873 | 247.223/123.664 | 139.793/56.780 | 18.972/2.679 |
| PM | 873 | 187.656/85.191 | 105.174/28.872 | 18.447/2.663 |
| Spot | 7045 | 860.188/636.820 | 443.165/291.005 | 26.476/7.560 |
| COIN-M | 202 | 211.746/88.434 | 143.736/54.779 | 15.554/1.524 |

Binance-only WASM879753 bytes, limit1258291, SHA-256
`b1d43b765c2d1c7d25fbbbb08f2275ad101e6a3b44dfc1be2e3f173cbf93395c`.

Fresh Bybit-only rebuild also passes Hyro public160 frames (TS/Rust main
21.0/15.7ms, steady4.7/2.6ms, retained16.2/0.8MB), Hyro private13 frames,
Bybit public8743 frames and private18 frames. Hyro private main42.2/30.4ms and
steady21.8/15.1ms are informational and also below TS. The earlier tiny Hyro
misses did not reproduce on either the unchanged baseline or fresh integrated
artifact. Diagnostic profiling identified shared URL canonicalization/scheduling
overhead; it did not establish a production Rust handler regression. No threshold,
measurement window or production Hyro code changed. Concurrent machine activity
precludes claiming strict isolation; live Hyro origins remain unconfigured.

Final combined WASM boundary suite79/79 passes across six files. Root test
scripts both pass6972 safe-cex cases; all27 root test tasks pass. PR review
publication and green CI remain the final landing checks.


### AsterDEX landing compatibility

Landing advanced again to `d9aa597fd3` while the combined tests finished.
The merge preserves optional currency and orphan-ticker wire tails, typed HTTP
routes/deadlines, both venues' registrations, and lazy Aster crypto loading.
Phase reviewers found two semantic integration gaps: new STP intent could be
silently ignored by Binance, and the explicit replay balance view could omit
Binance coins. Both are fixed before rerunning final gates.

All four Binance profiles reject a batch containing STP intent before any
read/write/store mutation; newly shared unsupported commands return errors
instead of hanging. The wrapper rejects non-Aster STP intent before commands.
Six wrapper cases demonstrably fail without the guard; all20 focused wrapper
tests pass with it. Native suite98 and strict crate Clippy pass; peer reviews
of the wrapper/native guards pass. Merged harness272/272 passes, including
lazy-oracle isolation. Fresh artifact and full gates follow this checkpoint.


### Final Aster-integrated checkpoint

The now-used public `wire::PositionRisk` contract is restored for Aster's actual
consumer. Aster99/99 and Binance98/98 native tests pass; both crate Clippy
checks and default WASM-target Clippy pass. Final integrated source is
`16f43063ca`. All twelve final replay gates pass: eight Binance gates, plus
Hyro and baseline Bybit public/private. The following evidence supersedes the
older intermediate replay measurements above.

Final integrated Binance evidence (`16f43063ca`, including KuCoin/AsterDEX): all eight gates PASS. Public three-run medians, unchanged thresholds.

| Profile | Frames | TS/Rust main ms | TS/Rust steady ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| USD-M | 873 | 262.265/139.307 | 151.445/61.177 | 19.097/2.666 |
| PM | 873 | 252.373/126.933 | 147.727/52.878 | 18.572/2.667 |
| Spot | 7045 | 753.982/620.454 | 363.794/284.658 | 26.539/7.533 |
| COIN-M | 202 | 211.416/90.890 | 141.328/54.074 | 15.682/1.515 |

Private frames: USD-M14, PM6, Spot8, COIN-M6; all expected writes, HMAC signatures, subscription identities and account comparisons pass.

Binance-only WASM **910175 bytes**, budget1258291. SHA-256 `e3b8fb107949471929b153c78df651dddddde320d57610d70d708dca1965f58e`.


Hyro's fresh Bybit build passes public160 frames: TS/Rust main42.034/26.761ms,
steady14.809/9.611ms, retained16.3/0.9MB. Private13 frames also passes with
main54.758/34.345ms and steady25.477/16.119ms. No private informational or
public gate metric misses. Base Bybit public8743/private18 frames also passes.
The earlier unchanged-source and rebuilt-artifact results agree; no genuine
Hyro production regression was established. All outcomes and diagnostic limits
remain disclosed; no metric interval or threshold changed.

Final default combined WASM rebuild passes all79 actual WASM cases across six
files. Both root test scripts pass7011 safe-cex cases; all27 test tasks pass.
Root typecheck44/lint28 tasks,51 host/wire tests,272 harness helpers, fmt,
bans, strict crate/default WASM Clippy and the secret scan all pass. Phase
review findings and fixes are ready for PR publication; green PR CI is required
before merging.


PR: [#1326](https://github.com/Tealstreet/tealstreet-sdk/pull/1326), targeting
`feat/rust-rewrite`. All local gates pass; phase findings and final adversarial
reviews are being published. Merge remains conditional on green PR CI.


### PR review round one and Kraken landing

Published phase findings and Hyro evidence on [#1326](https://github.com/Tealstreet/tealstreet-sdk/pull/1326).
Kraken `7da56dc8a5` landed while the PR opened; resolve its roster/replay conflicts,
retaining all Binance coin/currency/native-unit flags and Kraken's scoped hooks.
The independent account/trading review passes. Two other reviewers found:

- P1: Spot omitted native per-asset currency slices after Aster introduced the
  optional frame tail. A native test fails with actual `None`, and the strengthened
  private gate fails with Rust currencies `{}` versus TS BTC free0.01/used0.001
  and ETH/USDT rows. Sorted native currency rows now derive from the same cache;
  zeros, full replacement/removal, partial updates and stale read fences are
  regression-tested. Native99 and Clippy pass; the independent peer review passes.
  Both worker/main real WASM assertions are prepared for the fresh build.
- P2: generic generated-ID normalization could hide foreign HTTP query aliases.
  The raw Binance validator now rejects unsupported Spot/COIN-M aliases before
  matching while retaining genuine USD-M/PM native ID controls. A missing-throw
  red proof and286 passing merged helper tests cover the correction.

The Spot oracle compares every numeric currency field independently; it removes
only TS's opaque per-currency `info`. These are Rust/integration findings, not
new historical TS bug claims. Rebuilt gates and current-head CI remain required.


### Final strengthened replay evidence

Final integrated source `adc42a35d9` includes KuCoin, AsterDEX and Kraken. All twelve replay gates PASS; no thresholds or measurement intervals changed. Public values are three-run medians.

| Profile | Frames | TS/Rust main ms | TS/Rust steady ms | TS/Rust retained MB |
| --- | ---: | ---: | ---: | ---: |
| USD-M | 873 | 170.903/75.524 | 88.525/24.004 | 19.043/2.639 |
| PM | 873 | 162.738/76.628 | 88.490/24.766 | 18.525/2.639 |
| Spot | 7045 | 916.850/716.338 | 499.611/340.458 | 26.590/7.535 |
| COIN-M | 202 | 162.036/59.818 | 107.091/32.302 | 15.676/1.490 |

Private frames: USD-M 14, PM 6, Spot 8, COIN-M 6. Native currency slices are now compared for Spot as well as coin amounts and aggregate USD balances. Every replay has zero unserved requests, missing writes, bad signatures or protocol errors.

Binance-only WASM **915,481 bytes**, budget **1,258,291**. SHA-256 `7c67cb6398deddebd899f4c675200d47cb5e8e275495ea5f07516f97997fdf23`.

Other checks: 99 Binance and 99 Aster native tests; fmt/bans and strict crate/default WASM Clippy; 286 harness helpers; root typecheck 44/lint 28 tasks; 27 focused tests after Kraken integration. Earlier full root checks passed all 27 tasks (7,011 safe-cex cases per script). Final combined WASM suite: PENDING.

Fresh Hyro public (160 frames): TS/Rust main 21.012/15.227ms, steady 3.732/2.561ms, retained 16.281/0.819MB. Private (13 frames): main 35.890/28.608ms, steady 16.918/14.476ms. All pass; no Hyro metric misses. Baseline Bybit public (8,743) and private (18) frames also pass. The Bybit artifact hash matches the earlier fresh passing artifact exactly. Earlier tiny misses did not reproduce across unchanged-baseline and fresh-build runs. Diagnostic sampling identified shared harness URL/scheduling overhead without establishing a genuine production regression. No Hyro production optimization or relaxed gate; machine isolation and live Hyro connectivity are not claimed.

Final combined WASM suite passes81/81 across six files, including all five
Binance boundary cases and worker/main native currency assertions. Host/wire
51/51 passes. Both independent correctness and harness review closures pass;
current-head PR CI is the remaining landing requirement.
