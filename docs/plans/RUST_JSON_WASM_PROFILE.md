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
