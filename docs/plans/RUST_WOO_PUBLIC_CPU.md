# WOO public steady CPU headroom

Lane codex-ih35js, overseer claude-ddi2il,2026-10-23. Work follows the MEXC book
slice #1887. Plan and profile precede production edits; no merge without a window.

## Target and baseline

Restore at least1.10x median TS/Rust public steady CPU on the unchanged WOO tape
and policy. Reported landinged9bdd96ba2 missed at Rust28.838ms versus27.68ms limit;
recent WOO ratios range0.91-1.06x. This report is context, not our measurement.
The FIRST local measurement is pinned to actual landing9aef6bca90b9633cfa3e8d1a3d4fcac96ef8994d,
which includes actual #1796 b627e1bf6e9857158e41e14a49e1faa05572efc5. Do not measure
an older WOO history tree or mix another branch's unlanded runtime into baseline.

CI uses woo-public.jsonl.gz and books BTCUSDT,ETHUSDT, with the normal first-quarter
warm mark and nine performance runs. Keep fixture, books, clock/polling, comparison
policy, budgets and shared transport/Store/engine paths unchanged. The ticker
path already uses cached ticker_indexes/update_ticker_at; the pending shared
Store symbol-index optimization is not assumed to solve this venue's cost.

## Profile method and evidence

Reuse codex-9czdjh's direct named-WASM method: Rust1.99.0 aarch64, Node22.23.3 from
NVM, wasm-bindgen0.2.129 and Binaryen132. WOO retains its actual speed-default
family build plus serde_json opt-level3; no new compiler-profile change is planned.
Build only WOO with CARGO_BUILD_JOBS=2, outside any local native-gate/profiling load.
The overseer expressly authorizes this bounded local profiling exception to the
usual CI-only WASM/replay rule; no other venue matrix or release sweep is run.

Keep an untouched-source normal stripped artifact and a separate names-preserved
diagnostic artifact in /tmp, with source/tool/fixture/hash receipts. Use the
normal artifact for a bounded five paired TS/Rust baseline sample and candidate
comparison. These local medians guide diagnosis; exact-head CI's nine-run gate
remains merge evidence. Do not compare a profiled run's CPU metric to gate limits.

A preload inspector samples at100us, starts at Tape.play's existing warm-mark
threadCpuUsage and stops just before replay.mjs's steady delta read (currently
line133; verify the source each time). Record and exclude inspector.post overhead.
Attribute direct named frames from this exact artifact, aggregate repeated names
across paths and avoid recursive double-counting. Do not map numeric function
indices from a different stripped build. One initial profile is planned; repeat
only if sparse sampling leaves the dominant cost unresolved. No production,
harness, tape, polling or budget edit is used to collect the profile.

Coordinate CPU windows with codex-9czdjh. Its candidate profile is complete and
its local gates/build are idle before this first WOO build/profile begins.

## Fix boundary and review gates

Identify the dominant steady cost before code. If shared, fix or extend the
shared engine/Store/decoder path; no WOO-local bypass or second implementation.
Open-PR source collisions remain binding. In particular execution-sweep #1886,
Store metadata #1747 and the wider engine seams own shared files: coordinate
source ownership and consume a reviewed pin rather than guessing their changes.
Native WOO codec work is permitted only if the profile identifies native codec
cost, preserving every wire/financial/error/recovery semantic and shared path.

Commit reproducible baseline/profile findings and a narrow implementation plan
before production changes. Add meaningful regression/RED controls for the
identified defect or lost work, preserve existing assertions, and run full
WOO+Binance native suites, scoped fmt/strict Clippy and both request-context
checks. Any optimization must retain public/private behavior, fixed size/memory
budgets and exact inventory; no exemptions or ignored timing gate are introduced.
Obtain independent source/adversarial, caller and full-map/architecture reviews.
CI, actual dependency integration and a separate revert guard precede the
head-matched merge in an overseer window. Until evidence exists, no improvement,
1.10x target achievement or complete replay proof is claimed.

## First unchanged-source findings (9aef)

Build receipt and raw evidence are under /tmp/codex-ih35js-woo-profile-9aef.
The first names profile has211 samples and58.5ms sampled wall intervals;25.333ms
of inspector frames are excluded. Named WASM ingress occupies19.252ms, of which
TickerMessage/Vec<Tick> deserialization occupies11.918ms inclusively. Default
string hashing occupies3.834ms; shared numeric deserialization1.333ms. These are
nested sampled wall costs, not additive CPU totals or gate metrics. The empty
execution timer/sweep path is small in this sample, not a supported primary fix.

Five alternating normal stripped-artifact pairs have medians TS31.517ms and
Rust25.085ms (TS/Rust1.2564), with all77 frames served and no protocol errors.
TS ranges16.787-34.770ms; Rust21.229-25.873ms. Local variance and the reported CI
miss prevent claiming restored headroom from this baseline. The normal WOO
artifact is1,142,442 bytes against the unchanged1,343,488-byte venue ceiling;
names-preserved1,403,405 bytes is diagnostic and never a shipped artifact.

### Next bounded compiler experiment, before production code

WOO uses family speed defaults, but its venue crate is explicitly opt-level z
in Cargo.toml. The dominant generated native Tick visitor therefore remains
size-optimized even though shared serde_json is already opt-level3. Test ONE
unchanged-source WOO-only build overriding safe-cex-venue-woo to opt-level3,
using the existing family-profile mechanism as with Aster. Native/shared/combined
profiles remain unchanged, and no parser/Store/engine path is bypassed. Reject
an oversize candidate; only then consider a narrower compiler setting. Compare
five alternating unprofiled candidate pairs and the same named profile boundary.
Do not adopt or claim1.10x from names timings or a favorable TS outlier.

Any eventual profile-policy change needs the unchanged fixed size budget,
correctness/replay gates and exact-head CI. If this experiment fails to improve
Rust cost materially or exceeds size, return to the named evidence and shared
owner coordination rather than inventing a WOO-specific shared-path substitute.

## Opt3 experiment results and restart checkpoint

The unchanged9aef source was rebuilt with the single experimental override
profile.release.package.safe-cex-venue-woo.opt-level=3. No production build policy
or runtime source has been edited. Evidence is under
/tmp/codex-ih35js-woo-profile-opt3; baseline evidence remains under
/tmp/codex-ih35js-woo-profile-9aef.

Five alternating candidate TS/Rust pairs produced medians TS17.729ms and
Rust10.031ms (ratio1.7674). Rust values were10.145,10.031,9.774,9.258,12.130ms;
TS values were23.456,19.472,16.613,16.028,17.729ms. All runs served77 frames with
zero unserved requests and no protocol errors. The normal candidate artifact is
1,203,818 bytes, versus baseline1,142,442, within the unchanged1,343,488 ceiling.
Candidate WASM SHA256 is
5c099e62745b95734de474fcf60bf4c5ec78b68206c89dbc4f928d65ee00cbb3.

The candidate named profile has77 samples, with4.332ms sampled WASM ingress,
1.998ms inclusive Tick deserialization,0.542ms hashing and0.291ms Numeric.
These sparse, overlapping samples have changed inlining boundaries and do not
support exact function speed ratios. TS median shifted substantially between
baseline and candidate phases, so neither sequential-phase ratio establishes
restored CI headroom. No performance goal or correctness gate is claimed passed.

Next, before production code: run five interleaved baseline/candidate Rust pairs
with alternating order to control phase drift, then one correctness replay per
artifact and compare full snapshots/checkpoints using snapshot-compare.mjs.
Preserve the tape, books, clocks and budgets. Record literal build/bind/wasm-opt
commands and environment alongside receipts: current hashes authenticate files,
not a complete rebuild provenance chain. Only after these controls consider the
existing family-only build.sh override, refreshing open-PR file ownership first.
Native WOO/Binance gates and independent final reviews follow the timing window.

Independent source review of b5b54c3170a found no blocking interpretation or
architecture issue. It verified baseline raw values, warm/stop boundaries and
that this family-only compiler experiment bypasses no shared path. No production
change, new correctness replay, candidate acceptance or final CI has occurred.
Restart requested by the overseer stops further experiments and builds here.

## Resumed controls and implementation decision

On the same unchanged9aef artifacts, five interleaved Rust baseline/candidate
pairs alternated execution order. Baseline median23.105ms versus candidate
18.957ms is1.2188x, an approximately18% median reduction. Baseline values were
19.531,17.970,23.514,23.105,23.894ms; candidate11.483,21.724,18.957,17.898,20.374ms.
Four pairs favored the candidate; one favored baseline. This confirms a smaller
local improvement under the phase control, without treating the earlier10ms
median as stable or asserting the TS/Rust CI target has passed. Every run served
77 frames, with zero unserved requests and no protocol errors. Raw outputs and
artifact hashes are in interleaved-rust-control.json under the opt3 evidence root.

One correctness replay per normal artifact also served77 frames and captured
five checkpoints each. Existing compareSnapshots and compareCheckpoints report
zero differences in final markets/tickers/books/account and every checkpoint;
protocol/signature/write diagnostics agree. This is a compiler-artifact control,
not a replacement for the exact-head public/private replay gates.

Proceed with only a WOO family override in sdk-rs/harness/build.sh:
profile.release.package.safe-cex-venue-woo.opt-level=3. Keep Cargo.toml, all
runtime/shared paths, combined/native profiles, budgets and inventories unchanged.
The current open-PR file-list snapshot (43 PRs) has no build.sh collision. This
uses the existing Aster family-profile mechanism; the generated venue visitor
identified by profiling is the code whose compiler policy changes. No new parser,
hashing, Numeric or engine implementation is justified. The reported original CI
CPU miss supplies regression evidence; preserve the unchanged performance and
size gates for acceptance instead of adding a test that repeats a compiler flag.
