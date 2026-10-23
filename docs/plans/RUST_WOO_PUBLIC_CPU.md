# WOO public steady CPU headroom

Lane codex-ih35js, overseer claude-ddi2il, 2026-10-23. The candidate changes only
WOO family WASM compilation. Local controls and independent reviews are complete;
current landing is integrated; the tested head has complete green CI. No merge without
an explicit window.

## Problem and scope

Reported landing ed9bdd96ba2 missed the fixed public steady CPU limit at
28.838 ms versus 27.68 ms. Recent TS/Rust ratios ranged 0.91–1.06x. The objective
is at least 1.10x median TS/Rust on the unchanged WOO tape and CI policy. The
reported miss is the original regression evidence; local timings do not replace
CI's nine-run performance, correctness, memory or size gates.

The first local measurement uses actual landing
9aef6bca90b9633cfa3e8d1a3d4fcac96ef8994d, including actual WOO history #1796
b627e1bf6e9857158e41e14a49e1faa05572efc5. WOO already uses cached ticker indexes;
the separate shared Store lookup optimization is not the fix for this path.

Although WOO retained family speed defaults, its explicit venue package profile
still used opt-level z. Profiling identified generated Tick deserialization as
the main sampled ingress cost. The candidate uses the existing Aster family
mechanism to override safe-cex-venue-woo to opt-level 3 for WOO family artifacts.
This compiles public and private WOO code for browser/Worker and Node families.
Combined/native profiles and other families retain their settings. Runtime,
parser, Store, hashing, Numeric, signing, transport, clocks and error paths are
unchanged. Request inventory has zero delta. No cross-cutting concern receives
a second implementation.

## Plan-first checkpoints

- be1340319739729d5413461108b53895a9f8a66a: profiling plan before any experiment.
- b5b54c3170ad09160f8fe65aba863742344cb036: baseline findings and compiler
  experiment plan before candidate measurement.
- 35c72b780f50f623fd1956faa0eb2c2a4d6294c5: candidate results saved for restart,
  before any production edit.
- 239f9cb920a: interleaved phase control and correctness results committed before
  the three-line build.sh implementation.

Open-PR file lists were checked before editing: 43 PRs, no build.sh claim. Shared
execution-sweep and Store work stays with its owners. The overseer explicitly
authorized the bounded local WOO build/profile/timing exception; CPU windows
were coordinated with codex-9czdjh. No other venue replay matrix ran locally.

## Method and evidence

Use Rust 1.99.0 aarch64, Node 22.23.3 from NVM, wasm-bindgen 0.2.129 and
Binaryen 132, with CARGO_BUILD_JOBS=2 and no concurrent local heavy workload.
Use corepack yarn node, ESBUILD_WORKER_THREADS=0, the unchanged
woo-public.jsonl.gz and books BTCUSDT,ETHUSDT. Keep the original warm mark,
clocks, tape, comparison policy and every fixed budget.

Normal stripped artifacts supply unprofiled comparisons; separate named artifacts
supply diagnosis. Inspector samples at 100 us from Tape.play's first-quarter
warm-mark threadCpuUsage to just before replay.mjs's steady delta read (line 133
in the pinned source). This includes final settle and flush. Exclude inspector
frames and avoid adding overlapping or recursively repeated inclusive costs.
Names and stripped builds differ in function/code ordering; never transfer
numeric function indices between them. Profiled timings are not gate metrics.

Evidence roots:

- /tmp/codex-ih35js-woo-profile-9aef: build.log, build-receipt.json,
  steady-names.cpuprofile, names-attribution.json, paired-baseline.json and raw
  replay outputs.
- /tmp/codex-ih35js-woo-profile-opt3: equivalent candidate evidence,
  paired-candidate.json, interleaved-rust-control.json, raw per-run outputs,
  correctness-baseline.json, correctness-candidate.json and
  correctness-comparison.json.
- Artifacts: /tmp/codex-ih35js-woo-{names,stripped}-9aef and -opt3.

Artifact hashes authenticate the saved files, not an independently established
complete rebuild provenance chain. The following reproduction recipe is recorded
after the experiment, rather than presented as a contemporaneous shell log.
The diagnostic Cargo invocations used the repository root with --manifest-path,
which does not discover sdk-rs/.cargo/config.toml. Both diagnostic variants
therefore omit the production 131072-byte stack setting and use the linker's
larger default stack. This is a material artifact-policy difference: these
comparisons guide the compiler experiment but cannot prove shipped CPU, size or
memory behavior. Exact-head CI must use build.sh to build and gate the production
configuration. The corrected recipe below runs Cargo inside sdk-rs.

```bash
export PATH=/Users/sam/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin:/Users/sam/.cargo/bin:/Users/sam/.nvm/versions/node/v22.23.3/bin:$PATH
export DYLD_LIBRARY_PATH=/Users/sam/.rustup/toolchains/1.99.0-aarch64-apple-darwin/lib
export CARGO_BUILD_JOBS=2
# Candidate; omit the second --config for the frozen-source baseline.
(cd sdk-rs && cargo build --locked --release   -p safe-cex-wasm --target wasm32-unknown-unknown --no-default-features   --features woo --config profile.release.package.serde_json.opt-level=3   --config profile.release.package.safe-cex-venue-woo.opt-level=3)
/Users/sam/.cargo/bin/wasm-bindgen --remove-name-section --remove-producers-section   --target nodejs --out-dir /tmp/woo-reproduce-normal   sdk-rs/target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm
/tmp/codex-binaryen132/binaryen-version_132/bin/wasm-opt -Oz   --enable-bulk-memory --enable-bulk-memory-opt --enable-mutable-globals   --enable-sign-ext --enable-nontrapping-float-to-int --enable-reference-types   --enable-multivalue /tmp/woo-reproduce-normal/safe_cex_wasm_bg.wasm   -o /tmp/woo-reproduce-normal/safe_cex_wasm_bg.wasm
# The node output directory also needs package.json containing {"type":"commonjs"}.
# For a diagnostic named build omit bindgen's two removal flags and add wasm-opt -g.
ESBUILD_WORKER_THREADS=0 corepack yarn node --expose-gc --import tsx   sdk-rs/harness/replay.mjs --impl rust --performance --venue woo   --tape sdk-rs/harness/fixtures/woo-public.jsonl.gz --books BTCUSDT,ETHUSDT   --wasm /tmp/woo-reproduce-normal/safe_cex_wasm.js --out /tmp/woo-reproduce.json
```

## Results and limits

| Control | Baseline | Candidate |
| --- | --- | --- |
| First five-pair Rust median | 25.085 ms | 10.031 ms |
| TS median in those respective phases | 31.517 ms | 17.729 ms |
| Five interleaved Rust-pair median | 23.105 ms | 18.957 ms |
| Normal WASM size | 1,142,442 bytes | 1,203,818 bytes |
| Final correctness snapshots | Identical | Identical |
| Correctness checkpoints | Five | Five, identical |

TS phase drift prevents treating the initial 10 ms candidate result as stable.
The subsequent interleaved control alternated baseline/candidate order. Four of
five pairs favored the candidate; one favored baseline. The 1.2188x median ratio
is a 17.95% local Rust CPU reduction. It does not prove the TS/Rust CI objective.
All performance runs served 77 frames, without unserved requests or protocol
errors. Correctness replays also served 77 frames; existing snapshot/checkpoint
comparators found zero differences. Every nonmetric output field matches exactly.
Private replay behavior remains a CI requirement.

The candidate grows 61,376 bytes and leaves 139,670 bytes below the unchanged
1,343,488-byte ceiling. Normal candidate WASM SHA256:
5c099e62745b95734de474fcf60bf4c5ec78b68206c89dbc4f928d65ee00cbb3.
Baseline SHA256:
7519afbd9ee72a8ae48db7543539267ec402fe8dbf7b6ea7a0c389e3d8a6b6f0.

The baseline profile has 211 samples: 19.252 ms inclusive WASM ingress,
11.918 ms Tick deserialization, 3.834 ms hashing and 1.333 ms Numeric. These
nested sampled wall costs are not additive CPU totals. The candidate profile
has 77 samples: 4.332 ms WASM, 1.998 ms Tick, 0.542 ms hashing and 0.291 ms
Numeric. Sparse sampling and changed inlining prohibit exact function speed
ratios. The execution sweep is small in this sample and is not the primary fix.

## Verification and remaining gates

489 WOO/Binance native tests across 38 targets pass, as do scoped strict Clippy,
fmt, bash syntax and diff checks. The original 14 request-context checker controls pass; the current incoming scanner passes all 15 controls.
Actual landing 65b7cc5eff04a7726e87ac8d922933049e2062ec is genuinely integrated.
The initial stale incoming Paradex/Lighter ratchet failures were resolved by
preserving landing's source and inventory. Both request-context checks now pass;
the inventory has zero delta against that integrated landing.

Independent source/adversarial, caller and full-map reviews are clean.
Architecture gate: no per-venue replacement of a cross-cutting concern; this uses
the existing family compiler policy and retains shared paths. The integrated ledger review is clean, and the tested head has complete green CI. The final current-landing guard and merge window remain required. Preserve the fixed public/private correctness, CPU, memory and size
acceptance gates. A separate fresh revert guard and head-matched PR merge require
an explicit overseer window and the post-restart merge GO.

## Production WOO CI confirmation

Rust SDK run 39977265913 at tested PR head
f564c7b50f18bb629a1a047b3d95206f0234e3d3 completed successfully. That head has
43 successful checks and one browser coldstart skip. Production WOO job
120495031043 uses nine interleaved public performance pairs and reports steady
CPU TS 15.8 ms / Rust 8.9 ms, 1.78x, above the 1.10x objective. It serves
77 frames, 222 markets/tickers and two books, with zero overwritten fields.
Retained memory is TS 1.7 MB / Rust 0.8 MB; reported WASM linear memory is 0.7 MB.
These are rounded CI log values, separate from historical diagnostic artifacts.

The downloaded ci-wasm-woo module is 1,197,465 bytes, below the unchanged
1,343,488-byte ceiling by 146,023 bytes. Its three build-manifest file hashes
match. CI checkout ab1abfa01b349c507853b0ab3611494748ebe1ab has parents actual
7b6b4a2d029 and the tested f564 head; its entire tree matches f564. WASM SHA256:
1e951c00e0f8ad2100074afc2d336bf5e9daced3d9d66f678f80ea004fc2613d.
Receipt: /tmp/codex-ih35js-woo-production-ci-receipt.json; raw and cleaned job
logs are alongside it. This is a focused own-artifact check, not a full-package
provenance claim.

Strict verification reports one public strict PASS, one declared known-private
FAIL and zero unexpected failures. The private fixture still lacks authenticated
lifecycle/stream coverage; no complete private parity is established. Existing
comparison policy is unchanged.

## Current landing and source-equivalent proof

The integration consumes actual book #1887, snapshot #1787, cached Lighter reads #1900 and Script API partial-error handling #1895. WOO/Binance,
engine, crypto, registry, hosts, Cargo/config and harness inputs remain identical
to tested f564. Incoming changes affect unrelated venue crates, Script API code, documentation,
the inventory and its scanner's exact Paradex rehash transition plus regression
control. The updated scanner and its 15 controls pass locally; the manifest is
incoming-exact, with zero WOO delta. The three own shared-ledger sections retain
incoming content first and the reviewed WOO appendix once.

The overseer granted TRAIN GO #1898 and instructed current landing integration after f564 became green,
with a train slot after #1895 and before #1869's final integration. CI success is
attributed to f564; no fresh-CI pass is claimed for a later integrated head.
Independent reviews and a fresh separate guard must validate source equivalence
before a head-matched merge on explicit GO. Any new shared runtime or compiler
input change requires fresh CI.
