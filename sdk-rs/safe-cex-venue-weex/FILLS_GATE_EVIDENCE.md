# WEEX fills/retry slice evidence

Code SHA measured: `64e4071a3f36dc0734832d3c8aa550e16db8df43`.
Direct parent actually built on: `b6334d224847aa18aacb3f1da51b2d7bbdcb619d` (requested integration head).
Branch: `feat/rust-venue-weex-fills`. No PR 7404 work is carried as an ancestor;
this is a fresh branch from integration, not from the previous WEEX branch.

Measured with Python 3.12 tomllib over the six WEEX manifests:

| Revision | Ported | Divergent | Pending |
| --- | ---: | ---: | ---: |
| Parent b6334d2248 | 26 | 18 | 112 |
| Code 64e4071a3f | 28 | 20 | 108 |

**Moved 2 rows pending -> ported, 2 pending -> divergent.**

- ported: clampWeexRateLimitDelay, parseRateLimitIntervalMs.
- divergent: normalizeExecution, normalizeHistoricalTrade (CHOSEN).

The clamp covers native f64 inputs, including signed zero, infinities and NaN
(NaN capture alone is canonicalized). The interval parser accepts source UTF16
code units including lone surrogates, so no UTF8 string restriction is introduced.

The two fill rows reuse existing retained-input coercion/timestamp contracts.
Concrete chosen model restrictions: source returns records containing present
null/nonboolean maker/buyer and numeric commissionAsset, whereas the shared Rust
boolean/String fields refuse those records. Four actual-source malformed-field
supplements witness source record acceptance and Rust refusal. Widening shared
models to arbitrary runtime values would change every consumer. Source callable
price coercion and UTF16-symbol supplements additionally witness the retained
input boundary. No income pager, Store guard, notification or live REST operation
was added.

At the code SHA above, with
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`:

| Gate | Measured exit | Output |
| --- | ---: | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 | Scoped crate and dependency check completed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/fills-retry-mutations.py | 0 | Four controls compiled, named assertions failed, restored targets passed |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test fills --test retry (restored run inside script) | 0 | fills: 1 passed; retry: 1 passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0 | WEEX status/invention gate passed: 156 rows, 53 public Rust items |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 | Output read: would update 0 manifests; 0 added, 0 orphaned; 7341 symbols |

Cargo processes ran sequentially. The extractor writer ran after manifest edits,
before committing, and canonicalized two provenance arrays with 0 added/orphaned
symbols. Its check-mode measurement above completed at the code SHA; the earlier
writer invocation is not claimed as executing at that SHA.

One compiling production control per new row, each cargo exit 101:

- price/qty swap: `fills::execution::distinct::route_0::price`.
- time scaled down: `fills::historical::distinct::route_0::time`.
- altered cap: `retry::clamp::below_cap`.
- altered hour multiplier: `retry::interval::hour`.

Restored affected tests exited 0. No control survived these four mutations.
Native fields, enum variants, optional presence and f64 bits are read directly;
no model wire roundtrip is used as the assertion. Counts come from fixed input
and outcome inventories. The finite enumeration stops at 27 fill cases over four
routes for both mappers: 216 outcomes, 120 successful records, 52 null results,
44 errors and 548 numeric comparisons. Retry coverage stops at 12 clamp cases
and 23 interval cases. These are corpus floors, not exhaustive runtime coverage.

Read-only sub-agent review reported no concrete defect in this slice. It measured
only a small Node regex diagnostic (exit 0), confirming that trailing newline/CR/
Unicode line separators do not match the source suffix regex. It ran no cargo,
oracle suites or mutations. Main-agent results are the measured gates above.

Not run: full crate suite, full workspace cargo check, monorepo suites/builds,
host clippy, wasm clippy, live venue/network tests or CI reruns. Earlier drafting
cargo failed with exit 101 on a missing leading zero in a test float literal;
that typo was fixed before the measured code commit and is not test discrimination.

This file is committed as an evidence-only descendant. Gate results cover the
code SHA named above, rather than claiming a build at the evidence commit.


## Integration merge-down for PR 7427

New measured code SHA: `d4f1b3930dbb297efc4ee45426073973d9997cac`.
Actual merge parents: `b3630017f941f04b18a795bf3c853982455ddee1` and
`355683b84a7c62f3f5c9e91910c7e15438c57076`.
Three conflicts were resolved by retaining both CLAUDE sections, both module
export sets and the union of Rust provenance arrays. The canonical extractor
writer normalized the changed array before committing. No mapped row from either
parent was discarded; fills/retry code and seven associated files are unchanged
from b3630017f9. Shared pick_retained appears once.

Fresh exits measured at this new code SHA (earlier results above are not reused):

| Gate | Exit | Result |
| --- | ---: | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 | Scoped crate/dependencies checked |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 | 0 | 15 tests passed across 14 integration-test targets; unit/doc targets have 0 tests |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/fills-retry-mutations.py | 0 | Four compiling named assertion failures, source restored, fills and retry each passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0 | 156 rows, 59 public Rust items, zero WEEX exemptions; all mapped targets resolve |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 | Output read: would update 0 manifests; 0 added, 0 orphaned; 7341 symbols |

All four named controls again exited 101: execution price, historical time,
retry below-cap clamp and retry hour interval, as listed above. Restored affected
targets exited 0. No control survived. Cargo processes ran sequentially with
SAFE_CEX_TYPESCRIPT_PATH set to the compiler path above.

Tomllib census after merge: **28 ported / 26 divergent / 102 pending**. Compared
with integration355683b84a (26 / 24 / 106), the branch still retains **2 pending
-> ported, 2 pending -> divergent**. This merge does not claim four additional
new ports on top of the original slice.

Read-only sub-agent merge audit exited 0: all six manifests retain the mapped
rows and exact row data from both parents, with combined Rust provenance; both
module sets are exported and the projection helper is not duplicated. No cargo,
tests, mutations, edits or CLAUDE/memory reading were performed by that reviewer.
No concrete merge defect was reported.

Not run after this merge: full workspace compilation, monorepo suites/builds,
host clippy, wasm clippy, live venue/network tests or CI reruns. This new evidence
is committed as a documentation-only descendant; measured exits remain bound to
the new code SHA named in this section.
