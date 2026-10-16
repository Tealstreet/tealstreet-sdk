# WOO legacy data reads evidence

Fresh branch `feat/rust-woo-legacy-public-streams` starts at integration
`c749a66602fb72a64210c7f4fad4637c73fde943`; no additional unreviewed feature
ancestor was used. Initial code: `71dbaa7be15d3235eefe83109e3cafbf577be96e`.
Timer callback correction: `2c54fc32a852819ee83381a455e88a05c91a9f70`.
Integration `9c663a908f40c2955e27b1ce58f97fb5c08fb331` was merged down as
`d15c06da978f8c93bacb358fb4a5fa6392a0f103`. This gate sheet is manually authored;
generated logs, mutation controllers, corpus output and module hashes remain in
`/tmp`.

## Rows and ownership

**8 pending → ported, 0 → divergent** in `woo.exchange.ts`:
`startFundingRatesRefresh`, `refreshFundingRates`, `fetchMarketIocTouchPrice`,
`fetchOHLCV`, `listenOHLCV`, `listenOrderBook`, `fetchPositionMetadata`,
`fetchPositions`. The real native bodies are in `legacy_data_reads.rs`, over
compatible JS-value, captured-callee, imported-helper/library, timer and transport
primitives. Full class/runtime wiring remains separately pending. No v3-owned
file was edited by this slice.

Python 3.11 `tomllib` compared the three owned legacy manifests against the
parent, asserted the independent fixed **101** rows and row identities, and
asserted exactly eight status transitions, all pending-to-ported. Parent:
**46 ported / 20 divergent / 35 pending**. Code and merge: **54 / 20 / 27**.
Census stopped at row 100. This is an ownership census, not proof of a complete
venue runtime.

## Measured clean gates

One Cargo invocation at a time, scoped to `safe-cex-venue-woo`, jobs 2. All oracle
runs used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

At `71dbaa7be15d3235eefe83109e3cafbf577be96e`:

| Command | Exit | Result |
| --- | --- | --- |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --test legacy_data_reads --jobs 2` | 0 | 9 passed / 0 failed; 41 read cases and four edge witnesses |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols |

Before that commit, the extractor writer ran after the manifest edits on identical
manifest/source-extraction inputs:
`node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract`,
exit **0**, Updated 0 manifests; 0 added, 0 orphaned symbols. No `rust_files`
array was manually spelled.

At `2c54fc32a852819ee83381a455e88a05c91a9f70`:

| Command | Exit | Result |
| --- | --- | --- |
| Same scoped `legacy_data_reads` Cargo test command | 0 | 9 passed / 0 failed; fifth edge witness proves synchronous timer callback error identity |

The later seven complete method/helper sections were independently compared
byte-for-byte between the two code commits and were equal, stopped section 6.
The timer callback contract now separates a synchronous invocation error from a
returned asynchronous task. Initial mutation exits below remain bound to the
initial code SHA; they are not relabeled as executions at this correction SHA.

At merge `d15c06da978f8c93bacb358fb4a5fa6392a0f103`:

| Command | Exit | Result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2` | 0 | Finished dev profile; zero errors |
| Same scoped `legacy_data_reads` Cargo test command | 0 | 9 passed / 0 failed; 41 primary cases and five edge witnesses |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols |

Development first failed with exit **101**, 6/8 passed, because two independently
written route expectations omitted the actual public orderbook prefix and full
history URL; the expectations were corrected from actual source constants.
The next run failed **101**, 8/9 passed, at the source comparison for null OHLCV
rows: the native error text differed from the actual property-access TypeError.
The production path and independent native error assertion were corrected;
subsequent clean results above passed. Those are development failures, not
credited mutations. A corpus control adapter initially refused a wrong textual
injection pattern with exit 1 before running its target; it was corrected and
both real corpus controls below ran. A timer-edit helper initially refused a
missing textual pattern before writing; the unmodified target passed 9/9 and
is not credited as a timer correction gate.

## Native and source floor

The oracle extracts the eight actual TypeScript AST methods, the actual funding
number parser and positive-price helper, the actual static funding interval,
WOO constants/enums/utils, candle-start/epoch helpers and position-ID helper.
It uses the installed `p-iteration.mapSeries` and lodash flatten/memoize.
No copied method/helper algorithm replaces these source bindings.

Independent native fixtures assert returned values, primitive types, numeric
bits, UTF16 strings, raw OHLCV fields, routing/configuration, sparse slots,
sequence, independent counts, full versus selected error arguments, original
error identity, ticker/callback/options/return references, retained metadata row
references, open-row/array identity, spread precedence and five fake-position
zero overrides. Every native assertion precedes its test's wire comparisons.
The comparisons are not decode/re-encode roundtrips.

Fixed primary corpus: timer 4 (stopped local 3), funding 7 (6), touch 8 (7), OHLCV
6 (5), candle delegation 2 (1), book delegation 2 (1), metadata 6 (5), positions
6 (5): **41**, stopped global row 40. Independent Rust size, family and local
index expectations reject truncation and homogeneous output; independent
fixture input/result expectations verify content. Metadata covers ordinary,
sparse and live-growth results, disposal and synchronous/asynchronous errors.

Final edge corpus: **5**, independently pinned order close / includes / finally /
funding-close / timer-call, stopped witness 4. It checks iterator close before
price getters, indexed includes ignoring a custom iterator, finally clearing the
in-flight flag after an emitter throws, funding iterator close before catch on
row getter errors, and synchronous timer callback invocation errors. Current
floor: **46** cases. Complete JS/prototype/array-species and network coverage are
not claimed.

## Discriminating runtime controls

At initial code `71dbaa7be15d3235eefe83109e3cafbf577be96e`, eight method mutants
were batched into the scoped Cargo target with `-- --skip edge_native`: **101**,
**0 passed / 8 failed**. Every family's first rejection was a named native
assertion at local iteration 0:

| Mutation | Test / assertion |
| --- | --- |
| Change interval delay to 60001 | `timer_native` / `DATA native interval delay` |
| Swap funding rate and time | `funding_native` / `DATA native funding rate` |
| Select bids for buy | `touch_native` / `DATA native touch price` |
| Omit reversing candle rows | `ohlcv_native` / `DATA native reversed timestamp` |
| Delegate candle listener to book | `candle_native` / `DATA native candle eager sequence iteration 0` |
| Delegate book listener to candle | `book_native` / `DATA native book eager sequence iteration 0` |
| Pass one-way metadata mode | `metadata_native` / `DATA native metadata mode` |
| Populate fake-position zeros with one | `positions_native` / `DATA native fake zero` |

Restore, same command: **0**, eight passed, one filtered. Then two filtered Cargo
controls:

- Suppress touch iterator closing: `edge_native -- --exact`, **101** at
  `DATA native close before price`; restore **0**, one passed, eight filtered.
- Shallow-copy every defined flattened metadata row in the native method:
  `metadata_native -- --exact`, **101** at
  `DATA native metadata row alias iteration 0 result 0`; restore **0**, one
  passed, eight filtered. Fields remain equal; native row identity catches it.

Two primary-corpus controls used the restored Cargo-built
`legacy_data_reads-f4f8aa47b42cfeed` binary, filtered to `timer_native --exact`:
return only 40/41 rows → **101**, `DATA corpus SIZE`; restore **0**, one passed,
eight filtered. Return 41 copies of row zero → **101**,
`DATA corpus CONTENT index iteration 1`; restore **0**, one passed, eight
filtered. The second corpus has the expected length and fails for content.

At correction `2c54fc32a852819ee83381a455e88a05c91a9f70`, convert a synchronous
refresh invocation error into a returned rejected task: filtered Cargo
`edge_native -- --exact`, **101**, `DATA native timer callback synchronous error`.
Restore: **0**, one passed, eight filtered. All five edge witnesses passed.

Distinct credited runtime control floor: **13**, stopped control 12. All credited
failures are runtime assertion failures; no compiler-only failure is counted.
All credited controls were rejected and each restore passed byte-identically at
its respective code SHA. Controllers and measured exits remain in
`/tmp/woo-data-controls.json`, `/tmp/woo-data-corpus-controls.json` and
`/tmp/woo-data-timer-control.json`.

## Survivor finding and review

A read-only sub-agent reproduced an initial instrument survivor at
`WOOXExchange.fetchPositionMetadata`'s flattened return: shallow-copying defined
source rows produced byte-identical 41-row output, original and mutant producers
both exit **0**. Neither the source corpus nor initial native observations
retained row identity. This was a finding, not evidence of parity.

The source now observes membership of the actual rows returned by each
per-symbol call, and native tests retain those returned Rc rows, independently
pin 2/1/0 expectations and assert pointer equality before wire comparison.
The native-copy Cargo control above now fails **101** and restores **0**. The
reviewer reran the source-only copy once: producers still exit **0** (producers,
not assertion gates), but output differs at metadata cases 0, 1 and 4 through
false aliases. The reviewer ran no Cargo gate; output inequality is not claimed
as a Rust gate execution.

Read-only review also found/fixed touch synchronous borrow lifetime, iterator
closing, indexed includes, eager metadata catch, funding iterator abrupt close,
and checked the final timer callback distinction. No remaining material finding
was reported in the reviewed finite floor; that is not exhaustive proof.

## Preservation and non-claims

Code correction to merge: own crate `src`/`tests` diff exit **0**. The expanded
reachable-input diff exit was **1**: incoming integration changed
`safe-cex-exchange/src/websocket.rs`. Therefore both required scoped Cargo gates
were rerun at the merge; earlier Cargo measurements were not silently promoted.
All **481** reachable Cargo.lock package records compared equal, stopped record
480. Eight internal packages were candle-cache, crypto, exchange, num, store,
transport, types and venue-woo. Unrelated lock/source inputs are not included in
that preservation claim.

The expanded comparison included the root Cargo manifest, those eight packages,
WOO manifests, actual TS inputs and conservative helper/logging/transport inputs.
Actual-source Node loading observed **51** external module files, stopped file
50; paths and SHA256 contents re-resolved identically after merge. OS, toolchain,
Node engine and complete module universe equality are not claimed.

Restored correction/merge SHA256:

- `src/legacy_data_reads.rs`:
  `4c8878d96abc3d378cb43181d9411e63c268e281412c9151041c7c80bf2002b6`
- `tests/legacy_data_reads.rs`:
  `28ea3ba059e2ebbb3f426901b81949fca1d32e4e4faf24c53aabc3497f94aa4a`
- `tests/legacy_data_reads_oracle.cjs`:
  `02da2498d40f166eec9e438534d8d9c08f80118d5b39b8184f8aa7f60392c237`

Not run: workspace compilation, full crate suite, host or wasm clippy,
credentialed HTTP, live sockets, full adapter integration, v3 runtime tests or
CI reruns. No shared type or runtime trait was edited by this slice.
