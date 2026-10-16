# State and worker audit evidence

Frozen parent: `fada0ab29c9534bf4b51ec6e9c9385cf490b5b4c`.
The branch starts directly at that integration commit; PR 7924 is separate.

At `60e0d82487`, the read-only tomllib invariant check exited **0**:
all 159 independently expected divergent identities remained, every non-note
row field and file mapping matched the parent, and 21 changed notes retained
their complete original annotation as a suffix. Group expectations were fixed
at store 76/7/0, journal 14/19/0, crossTrade 5/26/0 and webworker 115/107/0.

The check's in-memory status control exited **1** at the named assertion
`AUDIT zero status and target moves`, identifying
`store/store.base.ts::DefaultStore.updateOrder`. No on-disk manifest or source
was mutated. Running the original inputs again exited **0**. This discriminates
the audit preservation invariant only; it is not a runtime parity test.

The source/test/Cargo input comparison against the frozen parent also exited
**0** at `60e0d82487`. No Cargo compilation, Clippy, wasm gate, native/Node
oracle, source mutation or CI run accompanied this annotation-only step.

## Complete cohort measurement

At `3ee4b6de8a64e3603877d13b85fec3f8a903eb6d`:

| Named check | Measured exit and result |
| --- | --- |
| `parity:extract:check` | 0; 7341 symbols, 512 source files, zero proposed manifest changes. Output text was inspected. |
| tomllib preservation and authored-decision reconciliation | 0; 159 frozen identities, 85 changed notes, zero status/target moves; 43 supported, 84 new, nine prior, 22 unresolved, one annotation reconciliation. |
| Preservation negative control | 1 at `AUDIT zero status and target moves`, naming `DefaultStore.updateOrder`; only an in-memory decoded row was changed. |
| Original-input rerun after the control | 0; both preservation and complete decision counts passed. |
| Source/test/Cargo input diff against frozen parent | 0; no runtime or test input changed. |
| `git diff --check` against frozen parent | 0. |

Expected verdicts and the 78 justification / three scheduling / three behavior
finding split were fixed separately from the decision table. The control only
discriminates the zero-status-move invariant; it does not prove the truth of an
audit conclusion or any runtime behavior.

The TypeScript/native branches behind the new behavior and scheduling findings
were inspected, not executed. Existing named witness files were read, not rerun.
No Cargo build/check, host/wasm Clippy, native or Node oracle, runtime mutation,
full test suite, live service or CI run was performed for this block.

## Integration merge-down and sub-agent review

Integration `333fd419fdbf70f081039610f22da9ee653bc96f` was merged without
conflicts as `4dfd894c9550922de802e50de5fba4d6429e5313`. At that merge SHA,
the complete 159-row preservation/decision check again exited **0**, as did
`parity:extract:check` (7341 symbols / 512 files / zero proposed changes).
The direct audited source files and all four scoped manifest directories were
byte-identical to `3ee4b6de8a64e3603877d13b85fec3f8a903eb6d` (**0**).
The full source/test/Cargo input diff relative to the merged integration parent
also exited **0**, and its whitespace check exited **0**.

Integration independently changes other source/test inputs and MANIFEST-SPEC.
Those changes do not extend any earlier runtime gate claim. The spec delta was
inspected; the newly priced shared JSON and diagnostic-stats decisions do not
supply the absent local reasons or missing field-specific proofs in this cohort.

Adversarial sub-agent review passed at
`3ee4b6de8a64e3603877d13b85fec3f8a903eb6d`, independently reconciling all
159 authored rows, 85 changed suffix-preserving notes and unchanged statuses,
targets and file mappings. Whole-row support contradictions were resolved by
holding the unproved clauses unresolved. The reviewer disclosed two initial
read-only parser failures (**1**, escaped pipes and annotation separator);
corrected inspection exited **0**. No runtime gate was run by the reviewer.
