# Backpack batch request and placement bodies

Measured code SHA: `bc4d729154117393d06aa2e588e2c970212c5d10`.
Actual starting parent: `eede8975507c09d346335b376389f0a57e52e144`.
Branch: `feat/rust-backpack-order-updates`, created directly at that integration
commit without inheriting unrelated off-integration feature work. Later
evidence-only commits do not change the measured code. Merge preservation and
any post-merge gate measurements are recorded separately below.

Python 3.14 tomllib compared all six Backpack manifests at immutable revisions:

| Revision | Ported | Divergent | Pending | Rows |
| --- | ---: | ---: | ---: | ---: |
| Actual starting parent | 19 | 51 | 77 | 147 |
| Measured code SHA | 20 | 52 | 75 | 147 |

Two newly implemented rows moved: `BackpackExchange.placeOrders` pending ->
ported and `BackpackExchange#buildBatchLimitOrderRequest` pending -> divergent.
The builder's CHOSEN note names its existing typed options/request, prepared
market, UTF-8/property/error boundaries and the shared request serializer's
different field order. Its rounding behavior matches the source, including
scientific precision notation; that behavior is not silently corrected.
This census is not proof of complete venue parity.

## Measured exits

Cargo ran sequentially with jobs 2. Oracle runs used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Node reported `v24.20.0`. The oracle transpiles and invokes the actual source
bodies; compatible parent helpers, collection access and logging callbacks
expose their effects and errors. Unused constructor dependency paths throw.

| Command at measured code SHA | Exit | Observed output |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2` | 0 | Finished dev profile; zero errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test batch_orders` | 0 | Three tests passed; zero failures |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | 7341 symbols / 512 files; would update 0 manifests, 0 added, 0 orphaned |
| `git diff --check` after restorations | 0 | No output |

The writer ran after the manifest edits before the implementation commit:
exit 0, updated 0 manifests, 0 added, 0 orphaned. Its exit is not attributed to
the later code SHA. Extractor check output was inspected in addition to the
Yarn exit; a shim exit alone is not the result.

## Deliberate native controls

Each named test ran using this exact command:

```text
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test batch_orders <test> -- --exact --nocapture
```

| Test | Compiling production mutation | Named native assertion | Mutation exit | Restored exit |
| --- | --- | --- | ---: | ---: |
| `actual_typescript_batch_request` | Exchange native price/quantity formatting assignments | `BATCH-REQUEST native price 0` | 101 | 0 |
| `actual_typescript_multi_order_placement` | Advance one slot after awaited pair completion instead of two | `PLACE-MANY native IDs normal` | 101 | 0 |

Both compiled and reached the named assertion, before wire/trace comparison.
Each restore was byte-identical to the measured source and each restored named
test passed. No survivors among these two controls; this is a two-control
floor, not an exhaustive mutation search.

## Native assertions and scope

The request test reached input 38/38: 39 independently prepared option,
client-ID and market records, checking every input value including float bits.
It completed 13 native requests and 26 native errors. Requests assert all
emitted fields, enum variants, formatted strings, flags, IDs and absent unused
fields before source key observations. Error name/message and lookup metadata
are native assertions; guard precedence and lookup count have independent
expectations. Cases include uint32 limits, nonfinite/zero values, bounds,
post-only/TIF/reduce-only, formatted zero/overflow and exponent/half rounding.

Placement reached mode 18/18: 19 independently ordered modes, native expected
ID sequences and eager/final effect counts. It covers pair and single sync/
async failures, original/replacement error identities, sparse/live arrays,
append/shrink and member replacement during eligibility. Paired members are
captured before eligibility and retain native identity. UTF-16 ID and log
contents include lone surrogates. The separate normal deferred-pair scenario
completed, proving that the next call waits for the pair response.
Generic ID pointer identity is not tested; the assertion is native UTF-16
content. No decode/re-encode wire equality or length-to-itself expectation is
used as proof. These enumerations are corpus floors, not exhaustive inputs.

Read-only sub-agent review found no concrete defect within the stated views
and supported the builder CHOSEN / placement PORTED classifications. It noted
the untested generic ID pointer identity; that claim is excluded here. The
reviewer made no edits and ran no Cargo, oracle or mutation gates.

Not run: workspace compilation, full crate/monorepo suites, host clippy, wasm
checks/clippy, CI, live order writes or full venue/store/transport wiring.
Prepared typed market/options and compatible live object-array/helper/string
coercion views are used. Hostile collection/iterator prototypes, arbitrary
runtime values beyond those views and full JavaScript scheduling are not
established. Raw local logs and exits remain outside the repository at
`/tmp/backpack-batch-orders-gates`.

## Integration merge and renewed measurements

Fetched integration `19cb1a2b735c3bc6db40bbb0cee849121cfd985b`, merging down at
`4b7200c008a29ee646c813f960779064f33d8e0c`. The documentation conflict retained
both paragraphs. Full crate source/test comparison against the initial code SHA
returned exit 1: integration added the previously delivered cancellation and
pair helpers plus their tests. Comparison restricted to the new batch source
and two batch test/oracle files returned exit 0. The two deliberate controls
remain measured at the initial SHA with those control inputs byte-unchanged.

The following commands were rerun sequentially at the merge SHA, using the same
oracle environment:

| Command at merge SHA | Exit | Observed result |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2` | 0 | Finished dev profile; zero errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test batch_orders --test order_actions` | 0 | Three batch tests + four order-action tests passed |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | 7341 symbols / 512 files; would update 0 manifests, 0 added, 0 orphaned |
| `git diff --check` | 0 | No output |

New tomllib census: the integration parent has 21 ported / 52 divergent /
74 pending; the merge has 22 / 53 / 72. Only the two new batch rows differ from
that parent. The three earlier cancellation rows arrived through integration
and are not credited as new work in this slice. These renewed exits cover the
merge SHA, not a subsequent evidence commit.

## Delivery preservation

Immediately before delivery, fetched integration
`3a4f19c35ab9a8a795bef393a969448fc2cad908` and merged down at
`2e382cc3657dd1ef970451bef3d89a1e5ecc36f9`. The full crate comparison
`git diff --exit-code 4b7200c008a29ee646c813f960779064f33d8e0c HEAD -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests`
returned 0 with no output. The renewed source/test inputs are byte-unchanged;
those gate exits stay attached to `4b7200c008`, not represented as new runs at
the delivery merge. Tomllib again found integration 21 / 52 / 74 versus
delivery 22 / 53 / 72. This following commit changes only the evidence file.
