Moved **3 rows pending→ported and 1 pending→divergent**.

Every gate exit and mutation/restoration pair recorded here covers code commit
`b92fb8834e69af67a9be5d9f95e23e966b572dbf`. The evidence-only commit containing
this file does not change tested code or manifests. Exact commands, replacements,
named assertions, exits and restoration hash are retained in
[ORDER_HELPERS_GATE_EXITS.json](ORDER_HELPERS_GATE_EXITS.json).

Fresh branch: `feat/rust-backpack-order-helpers`. Actual implementation parent:
`128daa572bff854c64297a24e95ac320b7f78f60`, the requested integration head.
There are no inherited private/unreviewed commits outside that parent in this
fresh branch. Initial status had only the two untracked brief/priority files;
no production/test mutation was inherited. No CLAUDE.md or agent-memory reread,
history rewrite, PR creation or integration merge was performed.

Python 3.14 tomllib parsed all six Backpack manifests at both immutable SHAs:

| SHA | Ported | Divergent | Pending | Total |
| --- | ---: | ---: | ---: | ---: |
| Parent `128daa572b` | 10 | 43 | 94 | 147 |
| Tested code `b92fb8834e` | 13 | 44 | 90 | 147 |

Three rows are ported: nextBatchClientId, createBatchClientIds and getTriggerBy.
They preserve their complete numeric/enum method bodies through configured
compatible entropy/generator/metadata dependencies. This does not claim an
ambient native RNG, the pending shared metadata table, or full venue wiring.
The fourth row, determineTriggerOrderType, is CHOSEN divergent at the existing
shared UTF-8 boundary: actual source accepts a matching lone-surrogate symbol
and returns TakeProfit while native string ingress refuses the escaped string.
Numeric price behavior and first matching ticker selection otherwise match the
source. Counts are a manifest census, not proof of the entire venue.

Measured gates at the code SHA:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2`: exit 0.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: exit 0. Inspected output: 7,341 symbols across 512 source files; zero changed manifests, added symbols or orphaned symbols. The writer was run after the manifest edits, before the code commit: exit 0, zero updates/additions/orphans.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test order_helpers --test order_utils`: exit 0 after all restorations; four new helper tests and the existing enum-mapping test passed.

One production mutation per newly mapped row, stopped after these four controls:

| Control | Observed assertion | Mutated exit | Restored exit |
| --- | --- | ---: | ---: |
| rng-scale | `NEXT-ID native bits case 8` | 101 | 0 |
| attempt-ceiling | `UNIQUE-ID independent native attempts 4` | 101 | 0 |
| last-price-trigger | `TRIGGER-BY native enum case 2` | 101 | 0 |
| last-matching-ticker | `TRIGGER-ORDER native enum case 158` | 101 | 0 |

All failures were real test panics after successful compilation. Each restoration
was byte-identical to the code commit and reran the selected test successfully.
No survivors in the four controls executed; no exhaustive mutation coverage is
claimed. The attempt-ceiling harness initially predicted case 8, but the actual
63-attempt mutation failed earlier at Infinity case 4. The harness stopped after
restoring and passing, the observed assertion was verified in its retained log,
and the remaining two controls continued without discarding or rerunning that
successful discrimination pair. This was a harness prediction mismatch, not a
surviving mutation.

The actual TypeScript exchange module and source enums/constants execute in each
oracle call. Constructor transport/socket objects are inert, unused dependencies
trip on access, and deterministic entropy, metadata and ticker snapshots are
explicit seams. Native comparisons do not re-encode returned values: they inspect
f64 bits, request buffer/call details, ordered numeric IDs, shared error metadata,
and native request-trigger/OrderType variants. No wire comparison substitutes
for a native assertion. NaN is canonicalized only in observations; signed zero
and infinities retain exact bits.

Independent expectations pin all RNG inputs and route counts; every unique-ID
case label, requested count bits, candidate sequence, throw position, attempt
count, known output sequence/length; every trigger kind/default/explicit enum
combination; and every ticker projection, side, trigger price and string boundary.
Sub-agent review initially identified missing input pins for unique-ID and
classification cases. Both were added and re-reviewed with no remaining concrete
gap found. The review was read-only and ran no Cargo or mutations.

Fixture enumeration reached RNG case 15 plus entropy-failure case 1, unique-ID
case 13, trigger-selection case 127, classification case 167 and string-boundary
case 1. These are fixture floors, not all possible host values. The 64-attempt
ceiling is exercised with invalid and duplicate candidates, a success exactly
at attempt 64, a candidate at 65 that is never consumed, and 64/65 requested IDs.
Oracle runtime observed: Node v24.20.0, arm64, darwin. Preliminary precommit tests
had one incorrect TS enum spelling in the classification witness; correcting it
to the actual stop_market/take_profit_market spellings passed. That preliminary
failure is not counted as a production mutation or a SHA-bound gate here.

Not run: workspace compilation, full monorepo suites, host or wasm Clippy, other
Backpack test targets, the Rust parity invention checker/layer audit, live REST,
browser/wasm execution, ambient native entropy, full venue/store composition,
native batch placement or WebSocket integration. No CI rerun was requested.
