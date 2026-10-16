# HOW and transport audit evidence

The measurements below belong only to the original audit commit explicitly
named below. They do not cover the later conflict-resolution merge. See the
[integration reconciliation](HOW_TRANSPORT_DIVERGENCE_AUDIT.md#integration-reconciliation)
for its annotation-preservation scope. Fresh workspace build and host Clippy
exits must be reported against the completed merge SHA separately.


Frozen integration parent: `333fd419fdbf70f081039610f22da9ee653bc96f`.
The branch starts directly there, without the open state/worker PR 7937 as an
ancestor. No Cargo build is claimed for this annotation-only branch.

## Complete cohort measurement

Measured at `15f9edc9e040a09b1367f5401433c7f3354c4287`:

| Named check | Measured exit and result |
| --- | --- |
| `parity:extract:check` | 0; 7341 symbols, 512 files, zero proposed changes. Output text inspected. |
| tomllib preservation and authored-decision reconciliation | 0; 49 divergent identities, 33 changed notes, zero status/target/file-mapping moves; 16 supported, 32 new and one prior finding. |
| Preservation negative control | 1 at `AUDIT zero status and target moves`, naming `HOWWebSocket.once`; the changed status existed only in an in-memory decoded row. |
| Original-input rerun after control | 0; complete preservation and decision counts passed. |
| Source/test/Cargo input diff against integration parent | 0; no runtime or test input changed. |
| `git diff --check` against integration parent | 0. |

The two independent populations are fixed at HOW 68/34/0 and transport
42/15/0 plus six n-a rows. Expected verdict counts are HOW 15 supported /
18 new / one prior, transport one supported / 14 new. New row findings are
30 justification and two behavior. The factory's extra annotation subcause is
not counted twice; 33 changed notes all retain their original annotation suffix.
The negative control proves the preservation check discriminates a status move,
not that the audit's runtime conclusions have executed witnesses.

The behavior findings were independently cross-reviewed against source and
native branches. The form contrast excludes bigint/custom-parser modes, and
the post-disposal contrast is limited to the directly callable public entry
with a plain message event. It does not establish a late-network race through
the separately guarded receive/monitor paths. Existing witness files were read,
not rerun; no serialized comparison is claimed to prove a native value.

No Cargo build/check, host or wasm Clippy, native/Node runtime oracle, source
mutation, full suite, live service or CI run accompanied this audit. The remote
integration head was still the frozen parent when checked after the gates.
