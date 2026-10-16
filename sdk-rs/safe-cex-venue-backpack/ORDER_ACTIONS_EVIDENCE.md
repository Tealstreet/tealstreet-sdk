# Backpack eligibility, pair placement and cancellation

Measured code SHA: `9b4c07b0a7f75e98101feb935e8e54d9588ecca6`.
Actual starting parent: `e5bae834bdc27615e4986fb1f3d71a8fcbb57356`.
Branch: `feat/rust-backpack-order-cancellation`. It starts directly at the
requested integration commit; no unrelated off-integration feature work was
inherited. Evidence-only commits do not change the measured code. Later
merge-down preservation is recorded separately, without claiming gates ran at
a SHA where they were not run.

Python 3.14 tomllib compared all six Backpack manifests at immutable revisions:

| Revision | Ported | Divergent | Pending | Rows |
| --- | ---: | ---: | ---: | ---: |
| Starting parent | 19 | 51 | 77 | 147 |
| Measured code SHA | 21 | 52 | 74 | 147 |

Three rows moved: `BackpackExchange#canUseNativeBatch` and `#placeBatchPair`
pending -> ported, and `BackpackExchange.cancelOrder` pending -> divergent.
The latter's CHOSEN note names its existing typed UTF-8 IDs, shared error ABI,
future/logging and store/property boundaries. No fabricated runtime target or
second error ABI is used. Parent helper bodies and full venue wiring remain
separately tracked; this census is not proof of complete venue parity.

## Measured gates

Cargo ran sequentially with jobs 2. Oracle commands used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Node reported `v24.20.0`. The oracle executes the actual exchange source;
configured parent helper, store and transport dependencies expose calls,
identities and errors. Constructor dependencies trip unused paths.

| Command at measured code SHA | Exit | Observed result |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2` | 0 | Finished dev profile; zero errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test order_actions --test runtime_methods` | 0 | 4 + 4 tests passed, zero failures |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | 7341 symbols / 512 files; would update 0 manifests, 0 added, 0 orphaned |
| `git diff --check` after all restores | 0 | No output |

The extractor writer ran after manifest edits before the code commit: exit 0,
updated 0 manifests, 0 added, 0 orphaned. Its exit is not attributed to the later
code SHA. The checker's output text was inspected in addition to its Yarn exit.

## Deliberate controls

For each named test the exact command was:

```text
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test order_actions <test> -- --exact --nocapture
```

| Test | Compiling production mutation | Named native assertion | Mutation exit | Restored exit |
| --- | --- | --- | ---: | ---: |
| `actual_typescript_batch_eligibility` | Invert builder success from is_ok to is_err | `ELIGIBILITY native result case 2` | 101 | 0 |
| `actual_typescript_place_batch_pair` | Select client_ids[index.saturating_sub(1)], repeating the first ID | `PAIR native clientId row1 case0` | 101 | 0 |
| `actual_typescript_cancel_order` | Replace data.orderId key with clientOrderId | `CANCEL native orderId field 0` | 101 | 0 |

All three compiled and reached their named assertion; every source restore was
byte-identical to the measured commit and every restored named test passed.
No survivors among these three compiling controls; this is a floor, not an
exhaustive mutation search.

The initial pair control selected client_ids[0] directly. It exited 101 during
compilation because index became unused under deny-warnings; no named native
assertion ran, so that attempt is not semantic proof. Its restored named test
passed with exit 0 before the revised compiling control above. The harness
rejected the compile-only result because the required assertion text was absent.

## Native assertions and limits

Eligibility reached record 8/8: nine independent labels, ordinary-order states,
client-ID descriptors and failure stages. Native assertions check booleans,
short-circuit callback counts and input identity, clientId 1, propagation of
predicate/access failures, and builder-only rejection swallowing.

Pair placement reached stage 8/8: nine independently ordered stages covering
ID/build failures, synchronous/awaited POST failures, decoder failure, short
ID arrays and live post-time request mutation. Native assertions check both
input identities, row fields and distinct IDs before any source trace
comparison; both rows precede the only eager write. The same live request array
reaches decoding, which receives the original data identity. The original
decoder result is returned unchanged, including independently asserted UTF-16
contents with a lone surrogate. Original opaque errors retain identity.
The separate deferred-response case completed: decoding stays suspended while
the response is pending, then runs after resolution. Its trace probe uses
floating-point bits, avoiding integer-versus-float JSON representation equality.

Cancellation reached record 9/9: five independent lookup/request stages times
two logging states. Native assertions inspect the entire DELETE config, field
names and distinct symbol/orderId values before source trace comparisons, then
check eager/final callback counts and original/replacement error metadata.
No cancellation source-only surrogate witness was run. These corpus sizes and
contents come from separate native expectations; no length-to-itself or
decode/re-encode wire equality is used as proof.

Read-only sub-agent review found no concrete defect within the stated views,
supported the two ported / one CHOSEN classifications, and identified the
unrun cancellation surrogate witness as a non-claim. The reviewer made no
edits and ran no Cargo, oracle or mutation gates.

Not run: workspace compilation, full crate/monorepo suites, host clippy, wasm
checks/clippy, CI, live REST requests or full venue/store/transport wiring.
Compatible declared readonly pairs and configured property/helper views are
used; hostile collection prototypes/access, arbitrary runtime IDs, opaque
thrown-object formatting and the full JavaScript scheduler are not established.
Raw local logs and exit records remain outside the repository at
`/tmp/backpack-order-actions-gates`.

## Delivery merge-down

Fetched integration `2c04dafa36dd53b183f10b974fcd0a4adf2ada1a` and merged it
down at `f41f2e6ec9f4ebe44e037e822383cf9e9ebca1d2`. The command
`git diff --exit-code 9b4c07b0a7f75e98101feb935e8e54d9588ecca6 HEAD -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests`
returned 0 without output at that merge SHA. Own source/test inputs are
byte-unchanged; the earlier gates remain bound to their measured SHA. No
post-merge Cargo, oracle or extractor rerun is claimed. `git diff --check`
returned 0. A new tomllib census found the integration parent's 19 ported /
51 divergent / 77 pending versus delivery's 21 / 52 / 74, preserving the
three-row movement. The following commit changes only this evidence file.
