# Shared-core finding types

This is a presentation reconciliation of the historical audit, not new runtime
proof or retrospective resolution. The historical 285 new finding rows were
266 justification gaps, seven scheduling differences and twelve behavior
differences. Calling all 285 code defects would misstate that measurement.

The reference tables are SHARED_CORE_DIVERGENCE_AUDIT.md at
2253c82d18d2a5a500982e39d66170b5085f57a2,
STATE_WORKER_DIVERGENCE_AUDIT.md at
702fc59dd864719b85af90c2dde5c7bd5003a241 and
HOW_TRANSPORT_DIVERGENCE_AUDIT.md at
0e2cde6a073f254ce0140ce4d064fc0deebd52c6. Those commits preserve the original
per-row decisions and evidence limits. The latter two PRs were separate open
branches when this resolution branch began at integration
333fd419fdbf70f081039610f22da9ee653bc96f; neither is an ancestor of that parent.

| Group | Explanation | Scheduling | Behavior | Documentation | Already known | Last iteration |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Exchange-level | 117 | 2 | 2 | 5 | 4 | 151 |
| Direct utilities | 36 | 2 | 5 | 0 | 1 | 53 |
| Store | 6 | 0 | 0 | 0 | 0 | 7 |
| Journal | 15 | 0 | 1 | 0 | 0 | 19 |
| CrossTrade | 26 | 0 | 0 | 0 | 0 | 26 |
| Webworker | 29 | 3 | 2 | 2 | 9 | 107 |
| Transport | 14 | 0 | 0 | 0 | 0 | 15 |
| HTTP-over-WebSocket | 13 | 0 | 2 | 3 | 1 | 34 |
| Total | 256 | 7 | 12 | 10 | 15 | 412 identities |

The first four columns sum to 285 new finding rows. Already known rows are
separate; the worker annotation reconciliation at iteration 86 is one more
documentation item outside that numerator. The other audited rows are 87
supported explanations and 24 unresolved evidence gaps. Together these
categories partition the frozen 412 identities. Behavior counts rows, not
independent root causes: the two worker dispatch rows share one missing route.
Existing execution and inspection-only limits remain attached to the original
decisions. Explanation means a missing rationale for a plausible choice, not
proof that the choice is correct or that code repair will never be needed.

The ten documentation-only presentations are exchange iterations 9, 34, 38,
71 and 98; worker 89 and 96; HOW 14, 18 and 28. They concern unsupported
classification, a missing existing consumer citation or a false source
initialization description without an independent missing choice rationale.
Rows with both an incorrect bucket and a missing rationale remain explanation
findings, including worker 57/75, utilities 6/43 and HOW 24. Original table
kind labels are retained so this presentation does not overwrite history.

Classification references remain [MANIFEST-SPEC §18](MANIFEST-SPEC.md#divergence-taxonomy).
Representation references remain [§14](MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](MANIFEST-SPEC.md#numeric-ingress-defect) and
[§19](MANIFEST-SPEC.md#forced-surrogate-conflict).

## CrossTrade checker contamination check

Read-only reconciliation of all 26 authored decisions at immutable
702fc59dd864719b85af90c2dde5c7bd5003a241 found zero verdicts relying on
checker output, an imported-target rejection, an absent Rust target or an
unresolved mapping. All 26 are explanation findings. Class iteration 5
explicitly recognizes executable composition. Both that commit and archived
333fd419fdbf70f081039610f22da9ee653bc96f declare CrossTradeLoop beside the
types glob re-export. The separately reported conservative checker false
positive therefore supplied no evidence to this audit and overturns none of
its 26 annotation verdicts. This investigation did not execute the checker.
An independent sub-agent reviewed all 26 identities and reasons, stopping at
iteration 26. This is not a claim that 26 runtime defects existed.

Later numeric-ingress, literal-info-object and stock-prototype order-branch
counterexamples belong to the subsequent resolution work. They are not
retroactively credited as behavior evidence in this historical table.

The reconciliation and taxonomy review ran as read-only source/table
inspection with exit 0. No runtime, extractor, Cargo, mutation or checker gate
is claimed for the taxonomy-only change. No manifest status or target changes.
