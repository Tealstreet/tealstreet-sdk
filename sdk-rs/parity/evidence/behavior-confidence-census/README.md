# Historical behavior-evidence census

**208 of 242 frozen divergent rows (86.0%) have located, SHA-bound historical
paired Rust/TypeScript execution attestations with explicit successful exits.**
This is a lower bound on bounded fixture execution, **not a percentage of behavior
parity, correctness, or production readiness**. Some passing cases deliberately
assert different outcomes. No row in this inspection has established complete
behavior conformance, and this census reran no native or TypeScript code.

| Frozen venue cohort | Rows | Historical paired execution, explicit exit | Not established at that strength |
| --- | ---: | ---: | ---: |
| OKX | 26 | 16 | 10 |
| Kraken | 31 | 17 | 14 |
| WEEX | 35 | 35 | 0 |
| Polymarket | 18 | 18 | 0 |
| MEXC | 40 | 31 | 9 |
| AsterDEX | 92 | 91 | 1 |
| **Total** | **242** | **208** | **34** |

Of the remaining 34, **three** OKX rows have weaker contextual prose saying the
venue suite passed, **22** have paired test code without a located SHA-bound
successful execution receipt, **one** MEXC row has shared-callee/source evidence
without mapped-wrapper execution, and **eight** MEXC rows lack established
mapped-row execution. Including the three weaker OKX attestations would give
211/242 (87.2%); they are excluded from the headline because the exact selection
of ignored tests and numeric exit are not explicitly bound to that run.
Missing evidence here means **not established**, not never tested.

The explicit-exit numerator includes **146 rows with retained runtime output**
and **62 with SHA-bound author measurement attestations in Markdown**. The latter
are historical reports, not independently replayed tests or retained raw logs.
Multiple rows can share one suite receipt; these are not 208 independent test
designs or mutation controls.

## Native assertions and comparison limits

Within those 208 rows, **189** have some direct native scalar, decoded-field,
identity or state assertions; **ten** have partial/mixed native assertions; **nine**
have serialized-value comparisons without independently established native field
identity. The 189/242 (78.1%) figure therefore describes the presence of a native
assertion subset, not verification of every output, branch or known divergence.

Four Polymarket auth rows have direct native assertions but perform wire checks
first. Ten other Polymarket rows use per-fixture native-before-wire ordering,
rather than a full-corpus native phase. MEXC's `fetchAndSetOrders` requires a
separate executed wrapper bridge test; testing its common reconciliation callee
alone does not establish `fetchAndSetRegularOrders`. AsterDEX's legacy
`generateV3Signature` primitive fixture does not establish the full venue wrapper
or a located executed paired gate, and is excluded.

Class rows cover the stated factory/constructor state and binding protocols with
injected hosts. They do not verify every supplied method body or fixed production
SDK composition. Type rows compare real Rust decoding with the stated source
schema/compiler/shared-parser machinery; a TypeScript interface is not itself a
runtime validator. The detailed records identify these comparator kinds, bounded
cases, known-difference assertions and control attribution limits. Source-only
mutation probes from the recent annotation audits are not native behavior passes.

No complete-function or end-to-end parity proof was established for any row in
this inspection. That is a limit of the located evidence, not a finding that all
242 implementations are defective. No current-head runtime assurance is
transferred from historical SHAs.

## Frozen scope and provenance

The six full-venue audit cohorts are frozen at:

- OKX/Kraken/WEEX: `d74a1c01f77439f9df61fa330063be194aab7576`.
- Polymarket: `fada0ab29c9534bf4b51ec6e9c9385cf490b5b4c`.
- MEXC/AsterDEX: `d4e20845a85cc4e2711c99648ef7547f8aa806df`.

The earlier 36-row sample is wholly inside the 92-row OKX/Kraken/WEEX block and
is counted once. The separate historical 15-row orphan/Bitget repair assignment
was a fixed FINDING-remediation cohort, not a whole-venue divergent audit. Its
three OKX rows overlap this census; its five Bitget and seven shared rows are
outside this denominator. Its earlier native/control receipts remain in
[orphan-bitget-findings](../orphan-bitget-findings/README.md); they are not added
as another venue or treated as evidence for all Bitget divergent rows.

Enumeration stops at the independently fixed floors 26/31/35/18/40/92 and the
last symbols recorded in [selection.json](selection.json). These are all frozen
divergent rows for those six cohorts, not all historical/current venue symbols
or all fleet rows. No manifest annotations or statuses are changed.

[rows.json](rows.json) contains every frozen identity, mapping, execution level,
measured input SHA, receipt and assertion axis. [summary.json](summary.json)
contains the totals. `details/` retains each venue's reviewed scopes and evidence
limitations. Historical test/oracle bytes and receipt snapshots are pinned by
SHA-256. Receipt snapshots are read at the census authoring parent; their
contained measurement SHA, not the snapshot commit, owns the old test exit.
Historical command executable/compiler paths are normalized for publication;
these are not claimed as exact argv reproductions. Some supplementary Polymarket
control hashes describe inspected ignored local outputs; those paths are not
published retained artifacts and are not needed for the paired-run numerator.

Canonical declarations remain in MANIFEST-SPEC:
[14](../../MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../../MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../../MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../../MANIFEST-SPEC.md#divergence-taxonomy),
[19](../../MANIFEST-SPEC.md#forced-surrogate-conflict).
This census does not create new representation declarations or change rulings.

## Checks performed for this census

[verify.py](verify.py) checks frozen manifest identities/mappings using `tomllib`,
independent literal venue floors, overlap exclusion, historical code/receipt
hashes, selector declarations, arithmetic, privacy and the evidence-only diff.
Exact mapped-body invocation and comparator semantics were inspected manually
and reviewed by sub-agents; declaration/hash checks do not establish execution
or prove that semantic mapping. A metadata mutation promotes an unexecuted
Kraken row without adding execution evidence and must fail the named
`paired_runtime_receipt` assertion before restoration passes.

Measured commands and exits, including rejected preliminary preparation, are in
the [tracked gate sheet](gates/gates.json) added after measurement. They cover their named input commit,
not an evidence-only descendant. No Cargo build/check/test, live source oracle,
host/wasm clippy, workspace/monorepo suite, live exchange or CI run was performed
for this census. The earlier historical receipts retain their original scope.

The branch starts directly at `d4e20845a85cc4e2711c99648ef7547f8aa806df` on
integration. The earlier unmerged audit branches are not ancestors of this work.
