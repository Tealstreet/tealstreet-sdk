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
source edits. Capture all39 alias argument lists and combined/families behavior
without invoking cargo or bind. Run focused checks; all exact-head CI jobs must
pass. Authenticate complete baseline/candidate CI raw archives for all32 suites,
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
