# Backpack REST and private WebSocket order mapping

Measured code SHA: `e7823788877bfca3082e3ef4827afff2e44fc974`.
Actual starting parent: `df065d29b58fd12bdc103e2ef156c8de602663a0`.
Branch: `feat/rust-backpack-order-mapping`. It starts directly at the requested
integration commit, with no unrelated off-integration feature work inherited.
Later evidence-only commits do not change the measured code. Merge-down is
verified separately before delivery; no claim is made that a gate was rerun at
a later commit merely because its own source and tests are unchanged.

Python 3.14 tomllib census read all six Backpack manifests from immutable git
revisions and compared each symbol's status, rather than deriving expectations
from the generated oracle corpus:

| Revision | Ported | Divergent | Pending | Rows |
| --- | ---: | ---: | ---: | ---: |
| Actual starting parent | 15 | 50 | 82 | 147 |
| Measured code SHA | 19 | 51 | 77 | 147 |

Five rows moved: four pending -> ported (`BackpackPrivateWebSocket` helpers
`mapOrderType`, `determineTriggerOrderType`, `mapOrderStatus`, `mapTimeInForce`),
and one pending -> divergent (`BackpackExchange#mapOrder`, CHOSEN). Its note
names the existing typed input/output and UTF-8 boundaries, excluded runtime
property behavior, null-present optional ingress and serializer field order.
The source accepts a combined null-present optional input that the existing
native schema rejects; the test witnesses that choice without treating it as
an admitted-domain comparison. It does not prove each null field separately.

## Measured gates

All Cargo commands ran sequentially with jobs 2. Oracle tests used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Node reported `v24.20.0`. The oracle transpiles and invokes actual exchange and
private socket source bodies; constructors use dependency stubs whose unused
paths throw, without stubbing the methods under comparison.

| Command at measured code SHA | Exit | Observed output |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2` | 0 | Finished dev profile; zero errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test order_mapping --test order_helpers --test order_utils` | 0 | 5 + 4 + 1 tests passed; zero failures |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | 7341 symbols / 512 files; would update 0 manifests, 0 added, 0 orphaned |
| `git diff --check` after restorations | 0 | No output |

The extractor writer ran after the manifest edits before the code commit:
exit 0, updated 0 manifests, 0 added, 0 orphaned. That writer exit is not
presented as a measurement at the later code SHA. Checker output was inspected
in addition to recording its Yarn exit.

## Deliberate controls

For each test below the exact command was:

```text
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test order_mapping <test> -- --exact --nocapture
```

| Test | Deliberate production mutation | Named native assertion | Mutation exit | Restored exit |
| --- | --- | --- | ---: | ---: |
| `actual_typescript_private_order_type` | Market mapping returns Limit | `PRIVATE-TYPE native enum case 3` | 101 | 0 |
| `actual_typescript_private_order_status` | Filled mapping returns Open | `PRIVATE-STATUS native enum case 16` | 101 | 0 |
| `actual_typescript_private_time_in_force` | PO mapping returns GoodTillCancel | `PRIVATE-TIF native enum case 24` | 101 | 0 |
| `actual_typescript_private_trigger_type` | Missing trigger defaults to TakeProfit | `PRIVATE-TRIGGER native enum case 36` | 101 | 0 |
| `actual_typescript_map_order` | Exchange native filled and remaining construction | `MAP-ORDER native filled bits 0` | 101 | 0 |

All five compiled and reached their named assertion. Both mutated source files
were restored byte-identically to the measured commit after each control, and
each restored named test passed. No survivors in this five-control floor;
an exhaustive mutation search is not claimed.

## Native assertions and scope

The independent input expectation reached mapping input 25/25 (26 UTF-16
strings) in each of the three enum tests. It includes mixed case, Kelvin-sign
case folding, unknown strings, and a lone surrogate. The native private trigger
test reached case 197/197: 11 independently pinned ticker scenarios times two
sides times nine optional trigger values. It checks every scenario, symbol,
ticker field, side and trigger value before comparing native OrderType values.
The generic symbol view preserves the lone-surrogate symbol and first-match
behavior; last/mark fallback retains signed zero, NaN and infinity semantics.

The native REST mapping test reached case 95/95: 32 independently pinned inputs
times three callback failure modes, producing 82 mapped results and 14 original
callback errors. It checks native input properties, all mapped fields/enums,
floating-point bits, optional field absence, callback counts and sequencing,
and opaque error identity. Native output assertions precede source object-key
observations. No decode/re-encode wire equality is used as proof. A separate
combined null optional input witnesses the explicit typed-schema exclusion.
These enumerations are corpus floors, not full venue coverage claims.

Read-only sub-agent review compared all five source bodies and the independent
input/native assertions, found no concrete defect in the stated domains, and
confirmed the combined boundary supports the CHOSEN classification. The
reviewer ran no Cargo, oracle or mutation gates and made no edits.

Not run: workspace compilation, full crate/monorepo suites, host clippy, wasm
checks/clippy, CI, live REST/socket requests or full venue/store/socket wiring.
Executable property/prototype behavior and arbitrary runtime values outside
the declared prepared inputs remain unclaimed. Raw local logs and control exit
records remain outside the repository at `/tmp/backpack-order-mapping-gates`.

## Delivery merge-down

Fetched integration `ed7ac407c716cea8b8115aea53e466c9ac69d129` and merged it
down at `60e42f6931bd52196cd5f6b7a8ae9aa6b0123933`. The command
`git diff --exit-code e7823788877bfca3082e3ef4827afff2e44fc974 HEAD -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests`
returned 0 with no output at that merge SHA. Own source and test inputs are
byte-unchanged, so the earlier gate exits remain attached to their measured
SHA; no post-merge Cargo/ oracle/ extractor rerun is claimed. `git diff --check`
also returned 0. A new tomllib census found the merged integration parent's
15 ported / 50 divergent / 82 pending versus delivery's 19 / 51 / 77, preserving
the five-row movement. The following commit records only this evidence.
