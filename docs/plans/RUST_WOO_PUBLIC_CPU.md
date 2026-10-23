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
