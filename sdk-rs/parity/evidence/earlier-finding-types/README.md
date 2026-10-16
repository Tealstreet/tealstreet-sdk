# Retrospective finding types for this worktree

Input integration: `d4e20845a85cc4e2711c99648ef7547f8aa806df`. This is a historical classification reconciliation, not a new runtime audit or a new finding discovery. No manifest, production or existing native-test edits are made; zero status rows move.

| Historical cohort | Rows | Explanation | Scheduling | Documentation | Previously known | Behavior | Other outcomes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Original OKX/Kraken/WEEX sample | 36 | 16 | 1 | 1 | 1 | 0 | 16 supported, 1 unresolved |
| Full OKX/Kraken/WEEX block | 92 | 28 | 6 | 1 | 4 | 2 | 47 supported, 4 unresolved |
| Polymarket block | 18 | 12 | 5 | 0 | 0 | 0 | 1 supported |
| Latest distinct divergence-audit cohort | 110 | 40 | 11 | 1 | 4 | 2 | 48 supported, 4 unresolved |
| Earlier assigned orphan/Bitget remediation cohort | 15 | 8 | 0 | 1 | 0 | 6 | Historical initial causes; all fifteen separately ruled |

The original36 is wholly included in the full92 and is never added again. The fifteen-row remediation cohort is not fifteen newly open defects. Four of the full92 rows refer back to that work: the three OKX repair rows and the routed diagnostic-stats cause. It would be wrong to sum this table into a fleet-wide defect count. Row identities, prior evidence receipts and lifecycle labels are retained in `classification.json`.

The six historical behavior causes required production repairs: CandleCache numeric ingress, broadcast numeric ingress, instrument-class member identity, OKX category lookup, OKX bill mapping, and OKX client-order lookup continuation/status/spread. At repair input `f02bb77ea7`, their tests and production controls measured those repairs; this reconciliation does not rerun or extend that evidence. The three diagnostic-stat rows and five Bitget rows required explicit representation/diagnostic rationale and pricing. The risk row's finding was stale: calculation and the production postprocessor recovery already existed, so its historical primary type is documentation. The facade test explicitly applies Store clearing/notification; it does not prove automatic full SDK or worker assembly. Its divergent→ported status transition belongs to the earlier repair PR, not this audit.

Primary types describe why an annotation was challenged, not complete behavior parity. A supported explanation or an explanation-type finding does not certify every behavior of the function. Scheduling retains its own class because invocation/poll/drop timing is observable; it is not silently folded into either prose or financial calculation defects. Previously-known rows remain separate from newly discovered causes.

Canonical declarations remain only at [14](../../MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../../MANIFEST-SPEC.md#numeric-ingress-defect), [18](../../MANIFEST-SPEC.md#divergence-taxonomy), and [19](../../MANIFEST-SPEC.md#forced-surrogate-conflict). This artifact applies those declarations and does not redefine them.

## Checker false-positive screen

All61 declared Rust target references across24 physical files of the latest58 finding rows were screened for the reported same-file source shape: local target struct/enum beside `pub use types::*`. No suspect row was found in this floor. The ten Polymarket record destinations are actual local structs in `types.rs`, which has no such adjacent glob; the finding concerns observed source values and the required String surfaces, not a checker import rejection. The shared CandleCache `lib.rs` glob is in a different file from its record destination.

This is a source-shape suspicion inventory, not an execution of parity-check and not proof that the checker has no other false positives. No checker artifact is counted as a new explanation finding. A matching shape would require a minimal checker reproduction before declaring it a proved artifact. The inventory stops at all58 finding rows in the latest110, with every mapped target listed; earlier remediation causes are traced to their existing semantic/native receipts rather than to checker rejection.

## Gates and provenance

`verify.py` checks fixed cohort identities and independent expected type counts, original-sample inclusion, historical receipt references, source-shape inventory, and zero manifest/source changes. Its in-memory type misclassification control must fail the named count assertion, then pass restored. Exact measured SHA/exits/logs are in `gates.json` after measurement. Prior runtime exits remain tied to their original SHAs.

Not run here: Cargo check/test/build, native/source runtime mutation controls, workspace build, host/wasm clippy, live exchange calls, CI or global parity readers. No complete conformance or population-wide severity estimate is claimed. The other lanes' shared-core/five-venue audit documents are outside this worktree's authorship floor and were not silently relabelled as ours.

At `e351e621ec`, metadata integrity0 / deliberate type-misclassification1 at `EARLIER-TYPES` / restored0, extractor-check0 (zero would-update/additions/orphans). Logs and full input SHA are in `gates.json`; receipt descendants do not relabel this input. Both read-only historical/cohort reviews returned MERGE; no reviewer ran a gate.
