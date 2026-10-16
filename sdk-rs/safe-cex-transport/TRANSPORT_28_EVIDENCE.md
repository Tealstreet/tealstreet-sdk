# Transport 28-row evidence

Measured code SHA: `32f310c89f0812f08ba7178c4757cce8f77e05f8`. Actual integration parent: `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444`.
Branch: `feat/rust-core-transport-28-v2`. No unrelated feature branch is an ancestor.
The following evidence commit adds only this sheet and its JSON record; it does not move the measured gate inputs.

## Census and implementation

Python 3.11 `tomllib`, transport subtree only, enumeration iteration 1: 11 manifest files and 63 symbol rows.
Before: 28 pending, 29 ported, 6 not-applicable. After: 0 pending, 42 ported, 15 divergent, 6 not-applicable.
Moved **14 pending → ported and 14 pending → CHOSEN divergent**. Separately demoted one prior ported `HowEgress` row:
source declarative `regionConditional` is absent from the native resolved jp/sg enum.

Implemented the executable Tealstreet provider, shared preference resolver, time-source host orchestration,
process relay flag and direct-provider constant. The routing-table mapping names the existing executable
compiled route data and row selector. Every new divergent note names its own actual native targets and boundary.
No query-value conversion or JSON projection was introduced.

## Measured gates

All at the measured SHA above; the exact commands/exits and full census are in `TRANSPORT_28_EVIDENCE.json`.

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-transport --all-targets --jobs 2`: **0**.
- Scoped `cargo test ... -p safe-cex-transport --test tealstreet_transport --jobs 2`: **0**, six native tests passed.
- Extractor `parity:extract:check`: **0**; read output explicitly: `Would update 0 manifests; 0 added, 0 orphaned symbols.`
- Routing source/generation freshness, `scripts/routing.mjs --check`: **0**, 47 exchanges / 1128 source cases / 43 unique routes.
- Actual-source `tests/tealstreet-source.cjs`: **0**.
- `scripts/transport-census.py`: **0**.
- Serial `scripts/tealstreet-controls.py`: **0**; five compiled fail/restore pairs below, byte-exact restoration confirmed.

## Discrimination

| Mutation | Named failing native assertion | Broken | Restored |
| --- | --- | ---: | ---: |
| blofin-exception | `TRANSPORT native REST URL` | 101 | 0 |
| empty-token | `TRANSPORT native authorization headers including empty credential` | 101 | 0 |
| undefined-proxy | `TRANSPORT native optional proxy final operand` | 101 | 0 |
| signed-rtt | `TRANSPORT native negative RTT floors` | 101 | 0 |
| compiled-route | `TRANSPORT native routing-table REST host` | 101 | 0 |

No survivors in these five controls. This is an enumeration floor at iteration 1, not an exhaustive mutation claim.
Native fields, flags, callable adapter identity/results, errors, timing and retained signed-zero values are asserted directly;
there is no Rust decode/re-encode gate. Native expectations come from independently executed TS functions or explicit literals.
Case floors are independent: 22 provider cases, 12 credential/runtime combinations, 162 preference combinations
(3×3×2×3×3), seven URL cases, six endpoint cases and 1128 routing cases (47×2×2×3×2).

## Review and limits

Read-only sub-agent review at `32f310c89f0812f08ba7178c4757cce8f77e05f8` found no remaining body/mapping mismatch within the declared domains.
Review findings fixed: eager time request start, source-shaped sparse metadata inputs, direct constant's actual-source
REST witness and native date-parser refusal. The reviewer ran no Cargo or mutations; those exits were measured by this lane.

Host assembly is deliberately CHOSEN: canonical access-token/date/HTTP/diagnostic/HOW-adapter bindings remain supplied
capabilities. Diagnostic call order is proven; concrete diagnostic state and end-to-end facade/HTTP behavior are not.
Shared mutable JS headers become owned copies; native process state replaces module-realm state; owned options and Rust
future drop/poll behavior differ from JS objects/Promises. Their corresponding manifest notes identify these differences.

Not run: workspace builds, whole monorepo suites, host/wasm clippy, wasm check, venue oracles,
safe-cex-exchange check, CI or live network tests. No claim covers those exits.
Initial prototype/oracle/reader failures and their resolutions are retained in the JSON record.

## Merge-down preservation

Merged integration `1fd422739443705dc4e28907e4ab8e559db5df0b` down, producing `9c56fc30598781f2175fd5696dbbcf6217458491`.
`git diff --exit-code 32f310c89f 9c56fc30598781f2175fd5696dbbcf6217458491 -- <transport src/tests, Cargo inputs, dependencies, oracle source inputs and transport manifests>`
exited **0**. These inputs are byte-unchanged, so the measured Cargo/native/control exits remain attached to their original
SHA and cover the delivered implementation. The only package script change was an unrelated KuCoin oracle command.
The merged-tree extractor check at `9c56fc30598781f2175fd5696dbbcf6217458491` exited **0**, with the same explicit zero-drift output.
Post-merge tomllib census still measures 14 pending → ported, 14 pending → divergent and the one separate demotion.
This follow-up evidence commit changes only the two evidence files.
