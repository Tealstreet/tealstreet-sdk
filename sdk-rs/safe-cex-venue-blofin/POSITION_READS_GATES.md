# Blofin position and history reader slice

Nine pending rows moved: **2 ported, 7 divergent**. The ported rows are
`fetchIncomeHistoryRange` and `fetchHistoricalPositions`: their method bodies match
within the existing shared contracts. Each divergent row names its actual boundary
in its retained manifest note; no blanket venue classification was applied.

The `tomllib` census stopped after all seven Blofin manifests (125 rows):
**14 ported / 59 divergent / 52 pending** at the parent, then
**16 ported / 66 divergent / 43 pending**. This census does not prove all inputs.

## Commit and review provenance

- Actual parent: `3b71021173f5f36b01899046aa2dde4db36f8a15`.
- Implementation: `8997a5042384890bcabd85c94319dfb6f0ffbbe1`.
- First merge-down: integration `671cdadf56e6ec1c1955795f0976a8cfd0d28d09`,
  merge `2b7b80da8842f2f3269b55d1e5f68aa2b338048c`. Changed Cargo.lock and
  package.json inputs were built by the subsequent gates.
- **Cargo/source gate SHA: `457e868bc4c210c155782bc12b68636dbd733a14`.**
- Static sub-agent review: `/root/ticker_review`; reviewed production correction
  `41c370e9cd3b9be5ac8ef76a30d4caed98d98e38`, test boxing at the gate SHA,
  and harness-only changes through `54151f23e196efb0190504a62da238153f2eb473`.
  No reviewer-run Cargo, oracle, mutation, or network gates.
- Final merge-down: integration `11c26bc6a7bb6240e836df4dcf5cb6ed73ef107f`,
  merge `e2438d5d7275661714a46bde030c0d24ca580693`. Static merge-preservation
  review at that SHA found no findings; untracked evidence was not reviewer-audited.

`position-reads-evidence/final-merge-preservation.json` records an actual scoped
`git diff --exit-code` exit **0** for crate production source against the gate SHA,
and exit **0** for the complete local Rust dependency closure and tests excluding
the separately reviewed Python harness. Cargo configuration/lock, source oracles,
and Blofin manifest inputs were checked too. package.json only adds an unrelated
Kucoin test command; the extractor was re-run at the merge SHA. Gate exits remain
at their measured SHA, rather than being relabeled as merge measurements.

Integration ancestors are inherited and were not separately reviewed by this
lane. No private lane branches were merged. Evidence files added after these
measurements contain no production, test, oracle, or manifest changes.

## Measured exits

All Cargo commands were serial and scoped to `safe-cex-venue-blofin`, jobs 2.
Exact commands and SHA bindings are in `position-reads-evidence/cargo-gates.json`.

| Gate at 457e868bc4 | Exit | Measured result |
| --- | ---: | --- |
| Cargo check, all targets | 0 | Scoped crate and dependencies compile |
| Host Clippy, dependencies included, `-D warnings` | 101 | Two inherited `arc_with_non_send_sync` findings in transport `tealstreet.rs:157,177` |
| Host Clippy, `--no-deps`, all targets, `-D warnings` | 0 | Crate lint passes |
| Scoped `position_reads` and prior `order_reads` test binaries | 0 | 10 new + 9 prior tests pass |
| Rustfmt for new source and compiled oracle test | 0 | Formatting passes |
| Extractor check | 0 | Output: would update 0 manifests, 0 added, 0 orphaned |
| Blofin source declaration coverage | 0 | Independent floors 20/3/3/66 declarations, stopped after four source files / 92 declarations |
| Blofin-only Rust parity audit | 1 | Three inherited public targets in `position_adapter.rs`: `BlofinPositionAdapterDeps`, `map_blofin_position_history`, `num` |

The scoped Rust parity audit used the existing binary, not rebuilt this turn,
production symlinks and empty exemptions. Inherited files are byte-unchanged from
the parent. No targets or exemptions were fabricated. The final merge extractor
check measured exit **0** at `e2438d5d7275661714a46bde030c0d24ca580693`, with
the zero-update output checked explicitly (`final-merge-extractor.json/.log`).

## Native assertions and discrimination

Actual TypeScript method bodies and shared mappers run in the source oracle.
The fixture enumeration stopped at **56 cases**, each exercised with six native
adapter schedules/body phases: **336 comparisons**, plus four boundary cases
across six adapters (**24 comparisons**). Native decoded fields, enums, optional
values, signed-zero bits, and changed Store state are asserted before request
traces. There is no decode/re-encode roundtrip gate. The source witnesses also
check live aliases, `Object.is` signed zero, and actual market replacement/removal.

Review found two P2 differences and both were corrected: lodash sums preserve the
first value's signed zero, and page requests keep the initially selected market
while mapping continues to use live Store markets. Their native fixtures and
mutation controls now discriminate. Income/PnL fixtures reach the 100-page cap;
other fixture enumerations are floors, not exhaustive behavioral proofs.

**19 deliberate runtime controls caught / 19 restored**: each named native
assertion failed with exit **101**, then passed after byte restoration with exit
**0**. No runtime survivors. `position-reads-evidence/controls.json` preserves the
individual call sites, assertions, exits, and measured SHAs:

- Controls 1–6: `457e868bc4c210c155782bc12b68636dbd733a14`.
- Controls 7–11: `4683601e4b911baabfd4bb8cc0b3afceb90a103d`.
- Controls 12–19: `54151f23e196efb0190504a62da238153f2eb473`.

The first control-7 attempt was rejected by the compiler (unused argument), so
it was not assertion proof. The second run stopped before control 12 because its
text pattern did not match; no mutation or Cargo ran for that attempt. Both failed
harness attempts and logs are retained. The corrected controls passed the required
broken/restored sequence. Earlier test-only enum lint failure is retained under
`pre-box-41c370/`, with the successful boxed-test gates bound to 457e868bc4.
`mutation-restoration.json` records scoped source/test restoration exit **0**.

Not run: workspace compilation/suites, full crate suites, wasm build/Clippy,
live venue/network tests, or CI reruns. Host crate lint is not wasm lint or
workspace lint, and the inherited dependency/parity failures remain failures.
