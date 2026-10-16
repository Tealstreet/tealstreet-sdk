Moved **1 row pending→ported and 3 pending→divergent**.

Gate exits cover code SHA `07deb4acbccb2560838cb92d9ead9d1cdaaa831d`. Exact commands, substitutions, failure
assertions, restoration hashes and census are in
[RUNTIME_METHODS_GATE_EXITS.json](RUNTIME_METHODS_GATE_EXITS.json).
The evidence-only commit containing this file changes no tested code/manifests.

Fresh branch `feat/rust-backpack-runtime-methods`, built directly on requested
integration parent `45ff4e3dd2941ae338dd783a6177e94d00eccce0`. No private/unreviewed ancestor outside that
parent was inherited. Initial status had only the two untracked brief files;
no unfinished production/test mutation existed. No CLAUDE.md/agent-memory
reread, history rewrite, PR creation or integration merge was performed.

Python 3.14 tomllib parsed all six Backpack manifests at both immutable SHAs:

| Reference | Ported | Divergent | Pending | Total |
| --- | ---: | ---: | ---: | ---: |
| Parent `45ff4e3dd2` | 13 | 44 | 90 | 147 |
| Code `07deb4acbc` | 14 | 47 | 86 | 147 |

`dispose` is ported: native state is set before public/private cleanup, failure
short-circuits, and generic errors preserve arbitrary opaque native identity.
The complete bodies of decodeBatchOrderResponse, setLeverage and
cancelSymbolOrders are CHOSEN divergent, with exact boundaries in their manifest
notes. Decoder uses typed requests/own prepared properties and shared UTF-8;
cancellation uses typed market IDs/shared errors/UTF-8; leverage uses shared
Error/emitted-error/future representation rather than an arbitrary executable
throwable ABI. Its actual-source custom-toString throw is captured without
inventing a native admission gate. The leverage method keeps fetched P generic
and unchanged inside PositionsUpdate, and both writes build complete native
PATCH/DELETE configurations. Full venue/socket/transport/store wiring is not
claimed. Census is not proof of the whole venue.

Measured gates at the code SHA:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2`: exit 0.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: exit 0. Inspected text: 7,341 symbols across 512 source files; zero changed manifests, added symbols or orphaned symbols. The writer ran after manifest edits and before commit: exit 0, zero updates/additions/orphans.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --jobs 2 --test runtime_methods --test account_reads --test order_helpers`: exit 0 after all restorations; 12 tests passed (four per scoped target).

One production mutation per newly mapped row, stopped after these four iterations:

| Mutation | Observed native assertion | Mutated exit | Restored exit |
| --- | --- | ---: | ---: |
| decoder-order | `BATCH-DECODE native ordered IDs 0` | 101 | 0 |
| disposal-state | `DISPOSE native disposed state 0` | 101 | 0 |
| leverage-state | `LEVERAGE-WRITE native committed state 0` | 101 | 0 |
| cancel-payload | `CANCEL-SYMBOL native data field 1` | 101 | 0 |

All mutations compiled and produced real test panics. Every file was restored
byte-identically to the tested code commit and its selected test reran passing.
No survivors among the four controls run; no exhaustive mutation coverage claim.

The actual source exchange class, enums and constants execute through explicit
constructor/transport/socket/store seams; unused dependencies trip on access.
Independent native fixture inputs and request projections pin all 47 decoder
cases before calling Rust. Known ID sequences/lengths and source/native error
fields are checked directly; native order cannot hide behind serde roundtrips.
Disposal pins six initial-state/failure cases and an opaque error identity case.
Leverage pins five IEEE inputs times nine failure stages, pre-await state,
eager/final effect counts, all request fields, original fetched Rc/value identity,
committed state and original/replacement error metadata. Cancellation pins six
lookup/transport/logger stages, complete nested native configuration fields and
synchronous-vs-awaited catch timing. No returned-value wire comparisons replace
these native assertions. NaN alone is canonicalized in observations; signed zero
and infinities retain bits.

Enumeration reached decoder case 46 (7 success/40 rejection), disposal case 5
plus opaque identity, leverage case 44 (5 success/40 rejection) plus the source-only
executable throw, and cancellation case 5 plus the surrogate boundary. Decoder
also witnesses accepted Unicode and an accepted source/refused native surrogate
request. These are fixture floors, not every possible host value. A preliminary
precommit independent input assertion caught JS default-parameter substitution
of the intended undefined response with valid rows; the oracle was corrected.
That fixture failure is not counted as a production mutation or SHA-bound gate.
Oracle runtime: Node v24.20.0, arm64, darwin.

Sub-agent source/diff review and follow-up found no behavior bug in the admitted
native domains; boundary notes explicitly include its requested admission limits.
It ran the Node oracle once to confirm fixture indices but no Cargo, Rust tests,
mutations or extractor. Those review observations are not added to the gate exits.

Not run: workspace compilation, full monorepo suites, host/wasm Clippy, other
Backpack test targets, Rust parity invention checker/layer audit, live REST,
browser/wasm execution, arbitrary executable-host admission, or full runtime
wiring. Rust futures do not prove JS scheduler behavior. No CI rerun was requested.
