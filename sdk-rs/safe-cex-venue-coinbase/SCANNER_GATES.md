# Coinbase duplicate-key scanner

Moved **10 pending -> ported, 0 -> divergent**, measured with Python 3.11
`tomllib` against authoring parent `9c151409d125381f67d08d2b644afa89c77a79c8`.
The authoring census is **300/17/109 -> 310/17/99**, enumerated through all
16 Coinbase files and 426 rows in that snapshot. Exact moved rows and real Rust
targets are in the final integration comparison in `SCANNER_CENSUS.json`. This fresh branch did not use the unmerged
auth-session branch as an ancestor. Census alone does not prove behavior.

`json_key_scanner.rs` maps the class, constructor and eight source methods.
Raw strings and decoded keys use UTF-16 units, retaining literal and escaped
lone surrogates. Number parsing follows the exact lexical prefix grammar,
including valid overflow spellings; no numeric value conversion occurs. Invalid
grammar takes precedence over duplicate detection, and repeated scans preserve
the source index and duplicate state. Internal exceptions become the invalid
terminal result through the same public scan boundary. The frame classifier
and its classification union remain pending.

## Measured gates

Measurements at **`c6c3bedc6f44c41034e1d86071869176a11a97ef`**, with commands and
log hashes, are tracked in `SCANNER_GATE_RESULTS.json`.

| Gate | Exit | Measured output |
| --- | ---: | --- |
| Coinbase Cargo check, all targets, jobs 2 | 0 | Finished dev profile; zero errors |
| Coinbase tests, all targets, jobs 2, include ignored | 0 | 184 passed, 0 failed, 0 ignored; 37 completed test binaries |
| Extractor check, explicit Node/Yarn path | 0 | 7341 symbols / 512 files; zero updates/additions/orphans, output inspected |
| Scoped native mapping checker | 0 | 426 rows / 493 public native items; real targets and status gates passed |
| Sequential mutation driver | 0 | All three compiling mutants failed; all three restored tests passed |

Oracle environment:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Cargo operations ran sequentially with jobs 2. An exploratory uncommitted mapping
run exited 1 for the missing new file declaration; metadata was corrected via
serialized target-derived file inventory and extractor writer before this gate
SHA. The final gates above do not claim to cover that earlier dirty worktree.

## Discrimination and native values

All mutants were restored in `finally`, then rerun. Production mutation diffs
are hashed against the recorded baseline SHA; no mutation was committed.

| Mutation | Named assertion | Mutated / restored exit |
| --- | --- | --- |
| Clear duplicate state | `decoded native key scan result` | 101 / 0 |
| Lower depth guard from 128 to 127 | `decoded native key scan result` | 101 / 0 |
| Alter decoded Unicode key unit | `decoded native JSON key code units` | 101 / 0 |

Enumeration stopped after these three controls; no observed survivor. Source
and tests were byte-restored to the gate SHA (`git diff --exit-code`, 0).

The native unit test asserts three independently specified decoded key vectors
directly, including lone-surrogate and surrogate-pair units. The live TypeScript
oracle comparison asserts native result enums for first and repeated scans,
stopping at input 75 (150 native result observations). No output serialization
or roundtrip comparison is used.

Independent input plan: 8 values, 12 valid numbers, 12 invalid numbers, 8
whitespace cases, 12 key cases, 10 bad strings, 8 malformed structures, 5 depth
boundaries = 75. The fixture content is pinned to
`d4c6435065966acf99adb2adea012e1841a5850d5a6e5cec22057ae04cf833e1`.
These are coverage floors, not exhaustive grammar or mutation claims. A read-only
subagent review found no blocker and confirmed the ten mappings; that reviewer
ran no Cargo, native tests or controls.

Not run: workspace builds/tests, monorepo suites, host clippy, wasm clippy, wasm
builds, CI, or live Coinbase network operations.

## Integration merge-down

Merged `ebcb51ffa55881e2e5a745c0d419028b0632da7d`, producing code SHA
`23802d5cc3e051687d37e2e6453ae8fcd83f3ec4`. Integration now includes the auth
session slice. The prescribed whole-crate source/test diff against `c6c3bedc6f`
exited **1**, so the affected gates were rerun. The scanner source/test/fixture
subset diff exited **0**, preserving the three controls.

Kept both CLAUDE paragraphs and both decoder-inventory additions. At the merge
SHA: scoped Cargo check **0**, affected scanner/auth/WS/RPC/decoder/library tests
**0** (**29 passed**, 0 failed/ignored, six completed binaries), extractor **0**
(zero-update text inspected), scoped native mapping **0** (426 rows / 526 native
items). Exact commands and hashes are in `SCANNER_MERGE_GATE_RESULTS.json`.
The earlier 184-test all-target suite is not claimed as a full test run at this
merge SHA; only the named affected set was rerun here.

Final tomllib enumeration stopped at 16 files and 426 rows against this merged
integration head: **319/17/90 -> 329/17/80**, a true gain of **10 pending ->
ported, 0 -> divergent**. `SCANNER_CENSUS.json` records that comparison; the
incoming auth slice's nineteen rows are not counted as this branch's gain.

The final fetch merged integration `cc3d25150b320656935009943f72d17b27e380a0`
into code SHA `9c9f3fa36d5b3cd1eb9bb498eb197204f62d5cd9`. The scoped whole-crate
source/test diff against the post-merge gate SHA `23802d5cc3` exited **0**.
Coinbase scripts/metadata, scoped dependency crate paths and Cargo configuration
also showed no additional changes; the measured check, affected tests and native
mapping carry. At `9c9f3fa36d`, the final extractor independently exited **0**
with zero-update output inspected. `SCANNER_FINAL_GATE_RESULTS.json` records
those exits and the exact baseline SHA. Final tomllib census against `cc3d25150b`
is still **329/17/80**, with **10 pending -> ported, 0 -> divergent**.
