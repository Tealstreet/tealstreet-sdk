# Legacy WOO account and market read evidence

Code/gate SHA: `28b5ba612542f09fc3c71d07950eb16d20dcb767`.
Actual parent: `2cdb439ce42f9889abad6252fd6a400f87db8632`.
Fresh branch: `feat/rust-woo-legacy-market-reads`. No earlier unreviewed feature
work was used as an ancestor; the preceding legacy socket-control slice was
already merged.

Python 3.11 tomllib over the three owned legacy manifests independently expects
101 rows. Parent **30 ported / 20 divergent / 51 pending**; code commit
**38 / 20 / 43**. Exactly **8 pending -> ported, 0 -> divergent**: getAccount,
validateAccount, fetchBalance, fetchMarkets, fetchTickers, fetchTickersRaw,
parseTickers and fetchPositionsAndBalance, all in woo.exchange.ts. Neither socket
manifest nor any v3-family source/row is edited.

Native targets are in src/legacy_market_reads.rs. This ports named method bodies
over compatible JS-value/property/coercion/imported-helper/transport Host
primitives, not completed exchange-class/runtime wiring. Native values can retain
undefined, nonfinite numbers, UTF16 and object/array identity. Standard array
filter/map behavior is implemented; overridden methods/species are outside the
measured floor. Numeric-operator coercion must reject BigInt mixed with number
literals, rather than use Number(BigInt). No BigInt fixture is claimed.

## Measured clean gates

At the code SHA above:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2`: **0**, finished dev profile, zero errors.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --test legacy_market_reads --jobs 2`: **0**, **8 passed / 0 failed**.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols`.

The writer `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js
parity:extract` ran before commit on the same manifest inputs: **0**, output
`Updated 0 manifests; 0 added, 0 orphaned symbols`. No rust_files array was
hand-written. A development run failed **101** because its independent expected
used balance was -0.01, while the actual floating subtraction rounds to signed
zero; the expectation was corrected and the development target passed **0**.
That failed expectation is not a mutation proof.

## Native assertions and finite oracle floor

The oracle transpiles the exact eight members from woo.exchange.ts, actual enums
and endpoints, actual get-key/normalizeSymbol source and installed lodash sumBy.
Fixture providers supply account info, transports, store and sibling method calls.
No credentialed HTTP, full adapter, socket or production exchange is exercised.

Independent Rust fixtures pin **34 rows**, family/iteration, actual seed contents,
getter descriptors, output fields and identities. The floor is account 3 (stopped
at 2), validate 6 (5), balance 6 (5), markets 4 (3), raw 3 (2), parse 6 (5),
tickers wrapper 2 (1), combined 4 (3). Global enumeration stopped at **33**;
this is a finite floor, not exhaustive read/coercion/property/error coverage.

Native assertions precede output projections and inspect JS-kind values, f64 bits,
UTF16, account/user fields, falsy affiliate null, validation transport/order and
balance identity, promise-rejected versus synchronously thrown balance behavior,
cent rounding/signed zero, mode assignment, repeated collateral getter reads,
market boolean active, raw string/numeric precision and leverage fields, original
info-row identity, cached array identity, ticker values/percentage infinity,
live appended-row iteration, raw/wrapper error identity and ordered combined reads.

The combined wrapper mutates the ORIGINAL balance object: upnl then total-minus-
upnl, while retaining the original positions. String-valued sumBy inputs produce
JS concatenation before numeric rounding ("1" + "2" becomes 12); empty arrays
produce zero. No native assertion uses re-encoded or decoded-back bytes. The
native test representation retains aliases with Rc<RefCell<Js>>; only later
projections encode numbers/string units for source comparison.

## Review finding and correction

Read-only sub-agent review found the missing `?` on a prepared combined-read task
and confirmed ordinary source ordering after correction. Eager synchronous
account/HTTP catches run before the returned future is first polled; temporary
RefCell borrows are released before catching/performing secondary requests.

**Confirmed survivor:** replacing successful validation balance with a shallow
copy originally produced byte-identical 34-row source output (reviewer original
and mutated Node exits **0**). The source fixture always reported alias=false,
and native validation lacked a pointer assertion. Source now observes returned
balance identity; Rust independently requires original identity on cases 0/1.
The native copy mutation and source-copy control below reject that regression.
Review ran no Cargo and made no edits; no remaining production mismatch was
reported in these eight bodies.

## Discrimination at the code SHA

A compiling eight-family batch returned **101**, **0 passed / 8 failed**. Each
mutation reached a different test's named native assertion before wire comparison:

| Mutation | Named native assertion |
| --- | --- |
| Swap userId/affiliateId native fields | `READ native numeric field userId` |
| Copy successful validated balance | `READ native validated balance alias` |
| Used collateral result +1 | `READ native number used` |
| Market active true -> false | `READ native active bool` |
| Raw failure null -> undefined | `READ native raw failure null` |
| Parsed mark +1 | `READ native number mark` |
| Skip ticker parser invocation | `READ native ticker wrapper sequence` |
| Combined total subtraction -> addition | `READ native number total` |

Restored source/oracle byte-for-byte; same scoped Cargo target returned **0**,
**8/8 passed**. No compiler-only failure was credited.

Corpus-only controls invoked the restored Cargo-built binary
`sdk-rs/target/debug/deps/legacy_market_reads-1a4574a0f36a4238`, with the same
TypeScript environment and a named test filter; no Rust code/binary changed:

- Source successful balance -> shallow copy: **101**, source/native row comparison at tests/legacy_market_reads.rs:606 in validation_native_routes_best_effort_and_eager_failures; same field values, alias false versus independently required true. Restored **0**, 1/1, 7 filtered.
- Output shortened to 33: **101**, `READ corpus SIZE`; restored **0**, 1/1, 7 filtered.
- Homogeneous 34 copies of row 0: **101**, `READ corpus CONTENT iteration 1`; restored **0**, 1/1, 7 filtered.

Distinct control floor: **8 native + 3 corpus = 11**, stopped at control **10**
(zero-based). All credited controls failed at runtime; no remaining survivor was
observed in this finite floor. Byte-identical restoration SHA256:

- src/legacy_market_reads.rs: `8b72cbc70442d992f105bcb215091dfe4415c1a962d8c65b923fcaa0e4c7ee9f`
- tests/legacy_market_reads_oracle.cjs: `de2a3bb266009a0c930504bb9f8f4c07baf21df260e748fde83fa6c61b9e700a`

No workspace compilation, full crate test suite, host/wasm clippy, credentialed
HTTP, production exchange, complete adapter wiring, v3 test suite or CI rerun was
performed. The scoped exits must not be reported as those claims.

## Integration merge and new measured gates

Merged integration `d32e17c1e710a9609d7d8fff8329f39eb3aea5fe` down, producing
merge SHA `0de456d541507a4195aed6fe28b4fad02c619d0e`. No conflict occurred.
The own-module/test/oracle diff against the original code SHA returned **0**.
The expanded reachable-input diff returned **1**: integration changed shared
exchange/transport inputs and WOO's lib/v3 regular-write source/tests. Those are
incoming integration work, not lane edits. **481 reachable Cargo.lock package
records** remained equal. The 137 observed TypeScript/lodash module files were
re-resolved and contents compared equal; enumeration stopped at 136, with no
complete Node/platform dependency claim.

Because Cargo inputs changed, both required scoped Cargo gates were rerun at
`0de456d541507a4195aed6fe28b4fad02c619d0e`:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2`: **0**, finished dev profile, zero errors.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --test legacy_market_reads --jobs 2`: **0**, **8/8 passed**.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols`.

Mutation exits above remain explicitly attached to the original code SHA; the
mutations were not rerun at the merge SHA. The original method/test bodies are
byte-identical and the merged clean target independently passes. Tomllib
remeasurement asserted 101 owned rows and exactly eight pending->ported
transitions: **38 ported / 20 divergent / 43 pending**. This evidence-only
follow-up changes no gate input.
