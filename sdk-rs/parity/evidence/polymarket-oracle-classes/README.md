# Differential-oracle class-cost experiment

This is the accepted historical pricing snapshot at `511b6d6c43`. Its original
length-65 survivor is retained below. The subsequent
[length-65 closure campaign](length65-closure/README.md) expands the profiles and
requires all twelve selected controls to fail; the old rates do not price that
revised corpus. Reproducing this historical snapshot requires its measured Git
revision; current control commands are documented in the closure campaign.

This experiment prices incremental bounded evidence on **nine existing Rust
rows**, reusing existing Polymarket host adapters. It does not price writing
missing Rust implementations or constructing every venue's first host adapter.
TypeScript remains the maintained reference. Manifest statuses are unchanged.

The assignment began at the first recorded clock, `2026-10-15T16:32:41Z`, on
integration `eadcf36393`. Integration `7bac54167c` was subsequently merged at
`511b6d6c43d78b26ef791705c23d763b6c687cda`. Earlier attempts and control results
are retained under their measured SHAs; post-merge measurements supersede their
rates. SHA-bound command receipts, input digests and full sanitized logs accompany
each run. This branch has no remote push.

## What is measured

Each row has 512 generated case instances, stopping at iteration 511. These are
finite deterministic profiles, **not 512 independent random draws or necessarily
512 distinct semantic inputs**. Three complete post-control positive command observations (files named `warm-*`)
are collected per row. Four first-wave commands recompiled/relinked; these are
observed cache conditions, not 27 guaranteed no-op builds. A command includes Cargo's incremental/no-op work, Node startup,
source extraction/transpilation, generator construction, native execution and
assertions. A separate row-specific mutation is compiled, must fail its named
native assertion at exit 101, and must restore to exit 0. Compile failures are
rejected as mutation detections.

The measured evidence-cycle rate is **one median positive command plus one
compiled fault and its restoration**. It is not a complete harness-construction
price. The gross nine-row workflow checkpoint separately includes reading,
scaffolding, rejected attempts, repairs, review and extra controls. Dividing that
window by nine is explicitly an equal allocation. Raw protocol phase stamps are
chronology: some patch preparation preceded a start marker, and work was
interleaved. They cannot support isolated per-class authoring minutes. Shared
workspace gates are measured separately and must not silently disappear from a
shipping-cost calculation. Gross minutes measure this root lane's elapsed window;
parallel reviewer lane-minutes were not separately instrumented or added. This
is not aggregate fleet effort, and existing adapter/earlier generator authorship
is reused rather than retrospectively charged.

## Samples and observed projection

| Route | Rows | Observation |
| --- | --- | --- |
| Closed scalar | `PolymarketExchange.get name` | Actual selected getter executes on generated receivers; native Rust enum variant is asserted. Full module initialization and constructor dispatch are excluded. |
| Scalar-shaped | `normalizeTickSize`, `precisionFromTickSize` | Selected actual declarations and supported Set; native success/error UTF-16 and binary64 observations. Each command invokes only its selected TS body. |
| Collection-shaped | `parseGammaMarketTokens` | Ordered native token/outcome pairs, mixed and malformed JSON, mismatched lengths, Unicode; positive array lengths end at 32. |
| Record | `PolymarketOrderBookLevel` | Actual declaration shape and actual `getOrderBook` consumer; native decoded `price` and `size`. The erased TS interface is not a runtime validator. |
| HTTP wrapper | `PolymarketAPI.getServerTime` | 448 finite-number and 64 injected-error awaited outcomes, selected request/failure metadata. **512 eager/lazy prefix differences are explicitly excluded from agreement**, witnessed separately. |
| Venue orchestration | `fetchMarkets`, `fetchTickers`, `placeOrder` | Actual method bodies with supplied Base/store/HTTP/signing/socket/clock protocols; direct native result, error, ordered call, state and log observations. Crypto, remote servers and full SDK construction are not executed. |

The revised venue inventory independently requires 864 market records, 5,760
ticker records, and 64 placement successes / 448 rejections. Ticker bulk sizes
0/1/99/100/101/201/257/513 are stratified into profile 0, repeated across four
generation cycles; the remaining profiles use small maps. The earlier repeated
bulk workload is archived and is not used as the revised workload's rate.

Rust comparisons inspect native values before any native wire projection;
the recursive venue comparator never re-encodes Rust output. Source UTF-16 units,
property order, array slots and raw f64 bits are captured explicitly. Two NaNs
are compared as a separately counted **NaN class**, not payload/sign-bit equality.
Structural tree comparison does not establish pointer aliases or all scheduling
histories. Record and utility profiles exclude materialized lone surrogates;
the HTTP profile excludes non-number response shapes.

Canonical declarations are linked, not duplicated here: [14](../../MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../../MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../../MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../../MANIFEST-SPEC.md#divergence-taxonomy), and
[19](../../MANIFEST-SPEC.md#forced-surrogate-conflict).

## Census and extrapolation

`inventory.json` freezes all **7,341** manifest row identities. `census.json`
contains one disjoint syntactic route for every row and the classifier's reasons.
This census stops at index 7,340 of that frozen manifest inventory; it does not
establish that the manifests enumerate every possible TypeScript symbol.
It reads manifests and source; it never writes or regenerates manifests.
Resolution failures, class/constructor composition, enums and non-simple type
declarations remain unpriced. Unknown calls/constructors are excluded from the
synchronous scalar/collection routes. The async HTTP/orchestration routes still
include arbitrary matching receiver protocols and transformations: for example,
Backpack `fetchMarkets` is routed HTTP despite map/string transforms, and candle
cache/store-I/O methods are routed orchestration. Those broad buckets are
especially coarse scenario proxies; their similarity to these Polymarket samples
has not been established. The coarse first classifier's pricing counts were withdrawn after it
was found to route arbitrary external calls as scalar/collection.

These are **scenario routing proxies**, not empirically validated homogeneous
cost classes. A whitelisted method spelling does not prove intrinsic semantics
or purity. Nine purposive rows from one venue cannot estimate class means,
variance or a population error probability. Some census rows still lack native
implementations; that porting cost is outside this experiment.

`analysis.json` calculates the measured command rates, class fractions over
7,341, the conditional priced-share sum, and visibly hypothetical imputations
for the unpriced remainder. Those imputations are **not bounds, a confidence
interval or a whole-port forecast**. No unpriced row is assigned hidden zero
cost. Engineering allocation and the full workspace gate cost must be shown
separately from these marginal command rates.

## Controls and findings

The nine row faults change the name enum, tick membership, precision fallback,
token identity, decoded record identity, HTTP endpoint, collateral quote,
100-token batching and signing side. An extra length-17 production fault tests
the expanded collection profile. All must fail a named native assertion and
restore to a pass. A source-encoder control reinstates the old cross-realm Map
erasure and must fail native state comparison.

**Coverage finding:** a production fault returning empty only at token-array
length 65 survives. The corpus ends at 32. This is an actual surviving control at
`utilities::parse_gamma_market_tokens`, not an existing production defect, and
demonstrates why passing generated cases are not a completeness argument.

**Harness finding, fixed:** the reused encoder tested a VM-created Map with
outer-realm `instanceof Map`, silently encoding its entries as empty object
fields. The new native-state assertion failed at
`markets/0/state/marketMetadata`. Brand recognition now captures the actual
entries, and the source-map-erasure control reproduces the original blind spot.
This does not establish that every other existing oracle encoder is correct.

Exploratory compile failures and the pre-merge campaign are retained rather than
discarded from workflow cost. They are not counted as discriminating detections
or fresh assurances after the integration merge.

## What these observations cannot prove

They do not prove complete-function parity, quantify “most” parity confidence,
establish all accepted/rejected inputs, validate external child implementations,
or cover unbounded collections, arbitrary object coercion/proxies, all aliases,
all time/interleaving/cancellation histories, live services, browser execution or
cross-platform numeric behavior. Known differences are not counted as agreements.
The earlier audited 242-row cohort remains **0/242 complete-function parity**;
these nine rows do not change that claim.

Reproduction starts with `python3.14
sdk-rs/safe-cex-venue-polymarket/tests/class_cost.py census`, then `baseline`,
`controls`, and `runtime`, with an available TypeScript installation selected by
`SAFE_CEX_TYPESCRIPT_PATH` or the shared checkout's installed dependency.
The controls mutate and restore source sequentially: do not run other Cargo or
source-reader processes in this worktree during them. Exact test selectors avoid
executing the older tests brought in through the shared fixture includes.

The latest delivery bar requires three fresh SHA-bound commands: workspace build,
host workspace Clippy, and workspace `cargo fmt --check`. Canonical wasm Clippy,
wiring, layers and routing are centrally owned. The in-flight local wasm command
was stopped immediately on that instruction; its exit -15 is not a gate pass. Standalone wasm
build, browser/FFI execution and the full CI suite are separate unrun claims.

## Results at the native checkpoint

| Route | Sample rows | Minutes per row, command cycle | Syntactic rows / 7,341 | Conditional command hours |
| --- | ---: | ---: | ---: | ---: |
| closed | 1 | 0.731 | 163 (2.220%) | 1.986 |
| scalar | 2 | 0.957 | 373 (5.081%) | 5.952 |
| collection | 1 | 0.755 | 29 (0.395%) | 0.365 |
| record | 1 | 1.057 | 101 (1.376%) | 1.779 |
| http | 1 | 0.882 | 161 (2.193%) | 2.367 |
| orchestration | 3 | 1.456 | 938 (12.778%) | 22.756 |

At `2026-10-15T17:38:00.136040+00:00`, the gross workflow window was **65.319 minutes** for nine rows (**7.258 minutes/row by equal allocation**), before the workspace gates and later publication. This includes 39.814 minutes inside recorded command intervals and 25.505 outside them. The latter is not a pure authoring stopwatch. Isolated construction minutes per class were **not measured**.

The six routed buckets total **1,765 / 7,341 (24.043%)**; **5,576 (75.957%)** remain unpriced. Multiplying the observed class command rates by those syntactic counts gives **35.204 hours for the priced share only**, conditional on unsupported class-cost transfer assumptions. A whole-port total is **unidentified**. If the remainder were explicitly imputed at the fastest versus slowest observed class command rates, the totals would be **103.140** versus **170.476 hours**; these are illustrative arithmetic scenarios, neither bounds nor shipping forecasts, and both omit new harness/implementation work and shared gates.

All nine baseline rows and all 27 positive timing commands passed at `511b6d6c43`. Nine row-specific faults and the extra length-17 fault compiled and failed named native assertions (exit 101); length-65 survived (exit 0); the source Map-erasure meta-control failed (exit 101). All twelve restorations passed (exit 0). Analyzer controls independently reject a wrong SHA, missing input dependency and wrong selector (exit 1), then accept restored evidence (exit 0). See `analyzer-controls.json` and the SHA-bound command receipts.

For comparison, uniform multiplication of the earlier 42.41-minute single-getter proof gives **5,188.864 lane-hours** across 7,341 rows. That is a conditional extrapolation from one sample, not an established floor. The historical 42.41-minute endpoint excluded later host/wasm Clippy measurements; comparisons must keep those differing endpoints visible. These cheaper oracle cycles buy bounded regression detection on chosen observations, a different confidence product. The experiment does not measure a confidence percentage or establish that oracles buy “most” completeness confidence.

## Delivery policy and fixed costs

At `2ad30b7d63`, formatting, workspace build and host workspace Clippy all
passed (exit 0). Their combined measured command time was
**48.331 minutes for the batch**. The old required wasm
run was stopped by instruction after 11.793 minutes (SIGTERM / -15);
no wasm result is asserted. These historical receipts remain under their exact
SHA, including the interrupted attempt. The root-lane window through that policy
change was 127.450 minutes (14.161 per row by equal allocation), including duplicated
wasm work and later reporting. This is separate from the frozen native checkpoint
and is not isolated authoring time or aggregate reviewer effort.

The evidence-only handoff commit is rechecked with the three current gates; its
fresh local receipts name that exact SHA. Full workspace/package test suites,
standalone wasm build, browser/FFI execution, other venue oracles and the rest of
CI were not run here. No manifest statuses moved (0 rows).
