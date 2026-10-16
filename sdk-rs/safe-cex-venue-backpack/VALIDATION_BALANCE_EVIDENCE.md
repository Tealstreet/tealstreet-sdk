# Backpack validation, balance and serial cancellation

Measured code SHA: `2e97266f5a7332d70de763484194b88f26b8da4c`.
Branch: `feat/rust-backpack-validation-balance`.
Starting parent: `f22d7b2c94b74e1f29f96d28b6d07c3163bb5409`.
Implementation commit: `425443ccc0711d4a8d4cdb1ea4c6f99c84910530`.
The measured SHA merges integration `55d8addaf74e5ff2c2da80ab19a03109b6fb9476`
into that implementation commit. No unrelated off-integration feature work was
inherited. The subsequent evidence commit changes only this file; gates below
were measured at the code SHA, not at that later evidence commit.

Python 3.14 tomllib census over all six Backpack manifests, reading immutable
revisions with git show:

| Revision | Ported | Divergent | Pending | Rows |
| --- | ---: | ---: | ---: | ---: |
| Starting parent | 14 | 47 | 86 | 147 |
| Merged integration parent | 14 | 47 | 86 | 147 |
| Measured code SHA | 15 | 50 | 82 | 147 |

Four rows moved off pending: `cancelOrders` became ported; `fetchBalance`,
`validateAccount` and `cancelAllOrders` became divergent with CHOSEN buckets.
Their notes name the prepared value/error/UTF-8 boundaries. Balance additionally
uses the shared declaration's serializer key order rather than the source
method literal's order. This census does not establish full venue parity.

## Measured exits

All Cargo invocations ran sequentially with jobs 2. Oracle runs used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`;
Node reported `v24.20.0`. The oracle executes the actual Backpack source and
the installed p-iteration implementation, including live collection length,
hole skipping and awaited elements/callbacks.

| Command | Exit | Observed output |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2` | 0 | Finished dev profile; zero errors |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test validation_balance --test account_reads --test runtime_methods` | 0 | 5 + 4 + 4 tests passed; zero failures |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | 7341 symbols / 512 source files; would update 0 manifests, 0 added, 0 orphaned |
| `git diff --check` after all restores | 0 | No output |

The extractor writer ran after the manifest edits and before the implementation
commit, exit 0, updating 0 manifests with 0 added and 0 orphaned symbols. It is
not presented as a gate measured at the merged code SHA. The merged checker
output above was inspected rather than inferred from the Yarn exit alone.

## Deliberate controls

Each control changed only `src/account_reads.rs` from the measured SHA. For
each named test below the exact command was:

```text
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test validation_balance <test> -- --exact --nocapture
```

| Test | Mutation | Named failing native assertion | Mutation exit | Restored exit |
| --- | --- | --- | ---: | ---: |
| `actual_typescript_fetch_balance` | Exchange total/free in the native Balance construction | `FETCH-BALANCE native total bits case 0` | 101 | 0 |
| `actual_typescript_validate_account` | Replace the default error with `Invalid key` | `VALIDATE native error value 2` | 101 | 0 |
| `actual_typescript_serial_cancellation_waits` | Drop the cancellation future instead of awaiting it | `SERIES-WAIT native after element resolution method0` | 101 | 0 |
| `actual_typescript_cancel_all_orders` | Replace DELETE data.symbol with data.market | `ALL-CANCEL native symbol field 1` | 101 | 0 |

All four compiled and reached the named assertion. No survivor among these four
controls. Every restore was byte-identical to the committed source, SHA-256
`b89d007ac36a1ad88a133e3b80cd97f00e2e7396952f6d4299bda2e3c37796a5`.
This is a four-control floor, not an exhaustive mutation search.

## Assertion scope and limits

The independent Rust input tables and stage/mode expectations pin corpus size
and content. All enumerations reached their following last cases: balance
47/47 (24 inputs times two logging states, 42 native successes and 6 failures),
transport failure 3/3, validation 109/109 (56 returned results and 54 emitter
rejections), and each cancellation method's mode 10/10. A separate validation
case checks the post-emission error reread. Deferred cancellation scenarios
reached method 1/1 and independently pin both method names, unresolved element
behavior, pending cancellation and absence of overlapping callbacks.

Tests compare native balance bits, optional field absence, raw validation
values, balance presence and identity, full DELETE fields, sequencing and error
precedence. Native comparisons precede source shape observations; no
decode/re-encode wire equality is used as proof. A source lone-surrogate balance
case is witnessed as an explicit Rust string-ingress exclusion.

Read-only sub-agent review found the missing independent deferred-case method
identity check. It was fixed before the measured SHA and the reviewer confirmed
the finding closed using committed content; the reviewer ran no Cargo, oracle
or mutation commands.

Not run: workspace compilation, host clippy, wasm checks/clippy, full suites,
CI, live REST requests or full venue/store/socket integration. Full JavaScript
scheduling, executable getters/prototypes, arbitrary thenables and object-valued
error alias semantics are not established by these prepared native views.
Raw local gate logs remain outside the repository at
`/tmp/backpack-validation-gates`; this file records the measured exits.
