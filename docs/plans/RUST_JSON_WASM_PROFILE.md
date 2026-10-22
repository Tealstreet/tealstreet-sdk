# Shared JSON WASM throughput policy

Owner-directed replacement scope for draft #1744, before new source edits:

Per-family JSON optimization is not the final policy. The same JSON work occurs
across venues; standardize the WASM throughput selection at build.sh's central
build boundary. Preserve and label the d1b2ccae984 experiment as historical.

Phase 1: define one wasm_throughput_args array containing only
--config 'profile.release.package.serde_json.opt-level=3'. Use that same array
in family_args' base build_args and in the combined build_args. Remove the
Bitget-only if clause. Existing WOO/KuCoin Spot/Aster whole-family exceptions,
other explicit venue profiles, binaryen flags, native Cargo.toml, thresholds,
size budgets and replay workload remain unchanged. A family that already built
serde_json at3 gains only an explicit equivalent selection. Every family and
combined now share one declared JSON policy; no per-family fallback is selected.

Commit the updated plan and obtain independent concrete-policy review before
source edits. Capture all 39 alias argument lists and combined/families behavior
without invoking cargo or bind. Run focused checks; all exact-head CI jobs must
pass. Authenticate complete baseline/candidate CI raw archives for all 32 suites,
all indexed modes/runs and every ordered diagnostic; only top-level metrics may
be excluded. Authenticate package input fingerprints/tools/flags/file hashes.
Report per-family optimized wasm byte deltas against each unchanged budget and
public CPU objectives. Ordinary PR replay archives prove family behavior only.
Combined evidence is explicitly limited to equivalent effective serde_json=3
settings plus the ordinary combined build, size and SDK checks. If combined
replay CPU/output equivalence is claimed, dispatch replay_artifact=combined on
baseline and candidate; do not infer it from family archives.
Use actual merged landing, no reuse/fallback to a differing build-input tree.
No local build/replay. Report the full matrix before adopting the global policy.
If any family size/CPU gate fails, keep strict evidence and ask the owner to pick
one shared rule from the per-family data; do not silently add exceptions.

Phase 2, only after Phase1 fits: test removing WOO/KuCoin Spot/Aster whole-family
exceptions under the global JSON throughput policy, preserving native settings.
Use a distinct candidate/run so JSON and broad host/engine/dependency changes
remain attributable. Report every CPU/size/raw result; retain or reject based on
unchanged gates and the owner's resulting shared rule. No promise of removal.
No new per-venue JSON parser, financial/identity change, signing change, allowance,
threshold or budget change belongs in either phase.

Architecture gate: YES. Shared JSON decoding throughput belongs in one central
build policy; the initial Bitget-only rule is explicitly superseded, not landed.

Concrete policy review: CLEAN before implementation, with combined evidence
limited as stated above. Current baseline is actual Kraken Spot merge #1742
`07bb7f04d568a07f17a907e89224e511018100f1`; fresh exact-head CI and
complete evidence precede any adoption.

## Phase 1 landed; Phase 2 concrete probe

Phase 1 merged as #1744 at
`2c5c5f6987f35cafc0e272282e54da37c0a66d4f`. Exact green candidate
`740390bdf946b5fddde89645d0f273c479243b57` passed all 43 applicable checks,
all 28 web/node family budgets, and an independently authenticated 976-output
comparison (488 Rust, 304 stable groups, 110 ordered gates). Full proof and
per-family and 42-public-workload matrices are in #1744. The global policy is
selected; the earlier Bitget-only condition never landed. WOO's already-level 3
JSON path did not establish a margin repair.

Fresh Phase 2 starts from that actual pin. Before source edits, independently
review this plan and the concrete argument change. Replace only the conditional
that excludes WOO/KuCoin Spot/Aster from wasm_profile_args with one unconditional
application for all standalone families. Keep the shared serde_json=3 policy,
all existing explicit venue package overrides and combined/native settings unchanged.
Consequences are explicit: WOO retains its existing explicit venue level z;
KuCoin Spot retains the
existing package-kucoin level 3 override; Aster retains its existing explicit
venue level 3 override. Host/engine/inherited dependency size profiles become
common defaults across families; existing explicit overrides, such as the
Paradex engine profile, still apply. Add no new exception or per-venue parser.

Capture before/after all 39 aliases and combined without building: only the
three named standalone families may gain the existing shared-size argument
array. Other family arguments and combined must be identical. Run shell syntax
and appropriate focused checks; CI owns release builds, all 28 family budgets
and all 32 replay suites. Authenticate exact actual-landing baseline and candidate
fingerprints, package hashes, all 976 raw output and 110 ordered diagnostics;
exclude only top-level metrics. Keep combined evidence limited to unchanged
settings plus ordinary build/size/SDK checks. WOO, KuCoin Spot and Aster public
CPU/memory results must be individually reported with raw samples, including
runner variation. No local replay or release build is authorized.

This is a distinct candidate, not a claim that all exceptions can be removed.
If any unchanged budget/objective fails, preserve complete available diagnostics
and report the per-family result for an owner policy decision. Do not invent a
one-off fallback, normalize differences or relax thresholds. Because #1745's
hosted bridge is already receiving actual #1744 integration/fresh proof, coordinate
merge order before landing another build-input change; keep this probe in CI
without forcing an unannounced dependency race. Normal integration/trial rules,
independent evidence review, a fresh separate guard and a separate exact-head
merge remain required for any selected follow-up.

Concrete source/argument review confirmed that WOO already has an explicit
venue-level z override in Cargo.toml. This probe changes its shared host/engine/
inherited dependency defaults from 3 to z; its venue level stays z. All existing
venue overrides remain unchanged.


### Phase 2 initial evidence and actual bridge integration

Initial probe 37115e7eefcf41b40be2c60eadd1ede3bd2c35ac passed all applicable
checks in Rust PR run 39759510533 against actual #1744 push 39758508925.
All 28 family web/node budgets fit. Aster shrank 1,213,871 to 1,080,655 bytes,
KuCoin Spot 844,486 to 739,920 and WOO 1,015,373 to 849,410 per target. Only
these three families' WASMs changed. Their public CPU/memory objectives passed;
Aster's overall raw status retained its existing correctness differences.
Steady Rust CPU was 28.377 to 29.477 ms (Aster), 292.990 to 180.256 (KuCoin
Spot) and 27.597 to 23.827 (WOO); cross-run timing also varies for unchanged
WASMs, so these samples establish no attributable throughput or WOO margin fix.

The complete 976-output report is deliberately unequal: all 488 Rust outputs
match, while two TS correctness outputs differ only in Backpack checkpoint-0
book presence and TxFlow private request multiplicity. Both entire non-metric
variants match authenticated pre-change controls. The owner conditionally
accepted exactly those classes, and independent review verified the live
run/artifact/digest/full-output evidence. The strict false result and changed
Backpack diagnostic text remain; no exclusions or comparator changes. PR #1746
records the four controls, exact archive identities and proof hashes.

Hosted bridge #1745 landed first as actual
5743c485d44e348e3d29039c4ebe83f6a78f0186, with unchanged db25 reviewed head
and identical actual/head tree. A real porcelain merge into the probe preserved
all 37 incoming paths and every unowned blob; the two existing probe blobs
were unchanged before this evidence addition. Fresh CI and a new actual-bridge
baseline proof are required for the integrated profile head. Initial 37115
evidence remains historical and is not relabeled as final. Same budgets,
workloads, thresholds, entropy, comparators and explicit package policies remain.


## Owner decision: retain speed defaults where size costs CPU margin

The all-size integrated candidate 51c5ab1e653 is NOT selected and must not
merge. Despite all green checks, Aster's normalized TS/Rust steady ratio fell
from 45.429/29.004 = 1.5663 to 72.015/62.325 = 1.1555. Its four historical
near-1.0x size-profile misses were the reason #1734 restored whole-family level
3. One passing sample does not justify removing that restoration.

The shared measured rule is: keep a family's speed defaults where the size
profile materially costs CPU margin, using the CI comparison and historical
gate evidence. Keep global JSON level 3 for all families. Under the measured
rule, retain Aster's existing whole-family speed defaults; remove only WOO and
KuCoin Spot from the broad exclusion. Their observed ratios did not degrade:
WOO 28.794/28.318 = 1.0168 to 28.631/27.645 = 1.0357, and KuCoin Spot
323.664/268.070 = 1.2074 to 197.034/159.947 = 1.2319. Absolute timings remain
runner-sensitive; WOO still has a narrow margin and no repair claim.

Concrete adjusted source, to review BEFORE implementation: apply the existing
wasm_profile_args unless family is asterdex. Keep its explicit venue-level 3
selection and the global JSON policy, every other explicit package override,
combined/native settings and family driver unchanged. This restores exactly
the actual-bridge baseline arguments for Aster. Against that same actual
baseline, only WOO and KuCoin Spot arguments may change; capture all 39 aliases
and combined without invoking a build. This central rule is not a venue parser
or another per-family JSON override.

Commit this adjusted plan first, obtain independent concrete-policy review,
then update source. Re-run all exact-head CI, the full authenticated 32-suite /
976-output comparison and all 28 family sizes / 42 public CPU/memory workloads.
Verify Aster's package bytes/hash and arguments match the speed baseline,
report every changed-family ratio, and preserve every strict unequal result.
Any TS-reference variation needs exact pre-change full-output control receipts
and owner disposition; no new exclusions. Old 51c5 data/body/guard remain
historical, never relabeled. Fresh independent source/data reviews, fixed-head
separate guard and separate merge are required for the adjusted candidate.


Adjusted implementation checkpoint: the central conditional now excludes only
Aster from the common size defaults. Capture of all 39 aliases confirms only
WOO and KuCoin Spot gain the eight existing shared arguments; Aster and every
other family, combined settings, family driver and native Cargo profiles match
the actual bridge baseline. Shell syntax passes without invoking a build.
Independent concrete source/argument review is CLEAN. Fresh adjusted CI and
artifact evidence remain required; no old all-size guard is valid for this head.

## KuCoin Spot landing trigger: restore speed defaults

Owner trigger on actual #1746 `3f0e3e450b2`: landing Rust run
39769642794 reports KuCoin Spot public steady CPU TS 342.9 ms versus Rust
311.4 ms, approximately 1.101x. The unchanged 10% CPU-saving objective
requires Rust <= 0.9 * TS (ratio >= 1.111...), so the gate missed. The earlier two passing samples do not override this observed loss
of safe margin. This follow-up implements the existing measured exception
rule; no gate, budget, threshold, comparator or JSON parsing change.

Concrete source: in central `sdk-rs/harness/build.sh`, apply the common
`wasm_profile_args` only when family is neither `asterdex` nor `kucoinspot`.
Preserve Aster's explicit level-3 venue override, global serde_json level 3,
all explicit package overrides, and WOO's selected common size defaults.
Only the KCS family stops receiving the eight shared size arguments.
Its complete arguments must match actual pre-#1746 hosted-bridge landing
`5743c485d44e348e3d29039c4ebe83f6a78f0186`; every other alias, combined
driver and native Cargo profile must remain byte-identical to actual #1746.

Commit this plan before source, independently review the one shared policy
condition and all 39 alias argument captures without invoking local builds.
Then push a fresh small PR. Use exact-head green CI and authenticated KCS
public/private raw comparison, all package hashes and per-target size budget
evidence. Require web/node KCS WASM byte hashes to equal that actual
pre-#1746 baseline; other families retain identical build arguments.
No local WASM/replay/size run. Architecture gate: shared build-profile policy
implementing the measured speed exception rule, no per-venue parser fix.

Queue: actual Bybit #1728 first, this restoration next, BingX #1749 then
real-integrates the new build fingerprint and refreshes its final proof.
Any overlapping landing dependency change requires genuine integration and
fresh CI; guard and merge remain separate calls, followed by actual merge
parents/tree/preservation verification and overseer notification.

Owner also authorizes pre-integrating frozen reviewed Bybit
`31035f3f09a71e5fb29d1083c217512972e5f9ec` for parallel CI; actual
unchanged Bybit landing remains mandatory before the fresh separate guard.
If those new covered source inputs move KCS bytes away from the pre-#1746
baseline, report that explicitly and block for evidence/disposition; never
substitute a different baseline or claim byte identity from arguments alone.
