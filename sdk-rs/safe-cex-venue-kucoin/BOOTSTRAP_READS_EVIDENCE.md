# KuCoin bootstrap and public/account reads

## Parent, source and row movement

Fresh branch: `feat/kucoin-bootstrap-market-reads`.
Actual integration parent: `0ffe13d5ef70c19c137639eafaad6bc118fa6c9a`.
Source/gate SHA: `20a31a6fe36bf0b7d48d01ef5ec9cf1a3fd2b2b5`.
No unreviewed feature ancestor was inherited. The previous order-control source
is already in this integration parent; only its missing oracle-target wiring
was carried as content into this fresh slice.

Independent Python 3.11 `tomllib` comparison with tracked manifests at the parent:
**8 pending -> ported, 0 -> divergent**.
Before: 122 ported / 39 divergent / 30 pending.
After: 130 ported / 39 divergent / 22 pending.
This 12-file venue enumeration stopped at `kucoinspot.ws-public.ts.toml`.
It is a census, not proof of the remaining rows.

| Source row | Primary native function |
| --- | --- |
| KucoinExchange.start | start |
| KucoinExchange.tick | tick |
| KucoinExchange.fetchPositionMetadata | fetch_position_metadata |
| KucoinExchange.fetchPositions | fetch_positions |
| KucoinExchange.fetchOpenPositions | fetch_open_positions |
| KucoinExchange.fetchMarkets | fetch_markets |
| KucoinExchange.fetchTickers | fetch_tickers |
| KucoinExchange.fetchOHLCV | fetch_ohlcv |

The manifest parser/serializer wrote all transitive target arrays and provenance.
An independent function-name enumeration found precisely 27 actual functions in
`bootstrap_reads.rs`, all mapped; it stopped at `map_candle`. Read-only review
confirmed exact transitive mappings and notes. Only `BootstrapOps` and
`BootstrapAwait` receive supporting C3 records, grouped beside existing KuCoin
records. No venue algorithm is exempted. The writer exited 0 with zero updated
manifests, zero additions and zero orphans.

## Body behavior and boundary

Native bodies own the source's bootstrap branches, per-await early returns,
loaded-field spreads, snapshot/private setup, logs and original callback returns.
Tick's inner disposal guards exit the whole method; the four-tick balance cadence,
conditional payload/loaded fields, caught diagnostic and late timer callback/counter
are retained. Scheduled and snapshot callbacks return their original values.

Metadata captures flatten before awaited imported mapSeries, starts both margin
and mode requests before Promise.all, destructures response bindings, checks late
disposal/codes, reads cross leverage or retains the source's default on unsuccessful
leverage code, and constructs one-way/hedged rows. Position selection preserves raw
includes predicate results for custom filters, original open arrays/rows, missing
symbol multiplicity, metadata spread precedence, zero overrides and shallow info
copies. Open positions retain explicit-side precedence, contract conversion,
native/base quantity distinction, parsed values and original raw info identity.

Market/ticker/candle callbacks retain fallback precedence, parsing, limits and
Infinity, XBT alias, quote/expiry filtering, raw-info copy with isQuanto false,
array/Object.values handling, push capture before ticker fields, and timestamp/
OHLCV field order. Candle bounds retain limit fallback/clamp, interval binding,
conditional dayjs calls, live API-symbol lookup and method valueOf calls.

`BootstrapOps` extends unchanged local `StreamOps`; no shared type/trait or core
query ABI changed. Opaque values, errors, callables and promise handles retain
identity. Bound-call capture/member operations own full live member lookup and
receiver binding; get/set/define/spread/index operations own JavaScript property
and object-literal semantics. Boolean filters are used only for Boolean predicates;
raw_filter preserves the original includes callback value until the filter owner
consumes it. Callback wrappers retain native synchronous/asynchronous bodies.

Response-pair and interval capabilities own their complete language destructuring
protocols, including binding reads and closing. Iterator/next/close_throw own
for-of protocol with an existing throw completion; body failures, including push
member lookup, close once, while next failures do not close. Other intrinsics are
language arithmetic, comparison, type tests and template expressions, never an
entire venue method. Imported operations and dayjs remain captured callees.
Await handles complete initiated, noncancelling host work; new native futures are
polled immediately for pre-await initiation.

Concrete exchange/provider/language-capability assembly and arbitrary host
thenables, scheduling, callback/custom iterator implementations are not proven
by this fixture adapter. Constructors and complete class assembly stay pending.

## Measured gates at the source SHA

At `20a31a6fe36bf0b7d48d01ef5ec9cf1a3fd2b2b5`, sequential commands measured:

| Command | Exit | Observed output |
| --- | --- | --- |
| `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2` | 0 | Finished dev profile; no errors |
| `/opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --test bootstrap_reads --jobs 2` | 0 | 1 passed, 0 failed |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | 7,341 symbols / 512 source files; 0 updates/additions/orphans |

The focused target used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Extractor text was inspected alongside its measured exit. A development compiler
exit 101 for an unused test import was corrected; it is not a mutation proof.

## Native/source proof floor

The single named target `bootstrap_reads_native_before_source_oracle` first runs
two native protocol witnesses: original push-member error closes once, original
next error closes zero times. Native Error identity/name/message/code/IDs are
asserted in both. This enumeration stops at the next-failure witness.

It then checks 50 independently listed fixtures: start 8, tick 8, metadata 9,
positions 5, open 5, markets 4, tickers 5, candles 6. The fixture enumeration
stopped at `ohlcv/zero-limit`. Every native assertion and both protocol witnesses
run before packing any native value or comparing any source wire result.
Assertions require native number/string/Boolean variants and literal values,
independent counts and complete metadata request route/query ordering, error
metadata/identity, original arrays/rows/payload references, separate shallow info
copies, preserved raw custom-filter returns and original scheduled/snapshot
callback return handles. No decode/re-encode comparison is used as native proof.

The source oracle extracts the maintained eight initializers, three parser helpers,
endpoint/interval tables, enums, safe-math multiply and createPositionId ASTs.
Receiver-bound factories preserve arrow lexical this. It uses installed lodash,
p-iteration and actual dayjs code with a fixed Date for these bounds. Other receiver
methods, XHR, clocks and snapshot construction are finite fixture bindings; this
is not a full class/provider integration test. The raw-filter source witness
independently asserts INCLUDED/empty string values returned by the callback.

## Maintained oracle wiring

Root `venue-oracle:kucoin` now appends `--test order_control --test bootstrap_reads`.
The first repairs the previous slice's explicit-target-list omission; the second
includes this new corpus. A Node JSON comparison against `git show` at the actual
parent exited 0, proving exactly those two targets were appended and every other
package field, dependency and script was preserved. The full venue command was
not run. No old source row was re-hardened or recounted as new movement.

## Discriminating controls

Eight production mutations were measured against source SHA
`20a31a6fe36bf0b7d48d01ef5ec9cf1a3fd2b2b5`. Each compiled, reached the
named native assertion before source comparison, exited 101, restored the exact
source bytes, and passed the focused target at exit 0.

| Injected change | Observed native failure | Broken / restored exits |
| --- | --- | --- |
| Loaded flags false | START native loaded markets | 101 / 0 |
| Invert four-tick balance cadence | SEQUENCE native tick/default | 101 / 0 |
| Invert one-way metadata branch | META native independent output count | 101 / 0 |
| Boolean-coerce includes callback result | POSITIONS native raw includes predicate | 101 / 0 |
| Omit IteratorClose on push member lookup failure | OPEN native push member failure closes once | 101 / 0 |
| Preserve raw isQuanto true | MARKETS native corrected quanto flag | 101 / 0 |
| Swap bid/ask native values | TICKERS native numeric field | 101 / 0 |
| Swap high/low candle input indices | OHLCV native decoded candle field | 101 / 0 |

No mutation survived. This enumeration stopped at the high/low input-index swap;
it is not exhaustive. No compiler error was accepted as a negative proof.
The raw-filter mutation leaves ordinary selection behavior equivalent, but its
original string value fails the independent native observer before any packing.
Raw JSON and logs are under `/tmp/kucoin-bootstrap-reads-*`, not tracked artifacts.
This manually written receipt binds each measured exit to its actual source SHA.
A scoped git diff against that SHA exited 0 after all restorations.

## Review and unrun scopes

Read-only sub-agent review confirmed raw filter return preservation, complete
push-member close handling, native protocol ordering, exact 27-function mapping,
two C3 declarations and both maintained oracle targets. No remaining concrete
blocker was found under the stated host contracts. The reviewer ran no gates.

Not run: workspace compilation, full KuCoin suite/maintained venue command,
other venues' tests, full monorepo suites, host clippy, wasm clippy, full Rust index,
CI or live exchange/network tests. No claim of complete remaining venue coverage,
class/provider assembly or arbitrary host timing/thenable equivalence.

## Integration merge and refreshed gates

First merge: incoming `d8bc6fc533787b072aa5aaad1ee3b0ee09f8681e` into
`28dfa7eab7d046878d44b5ac81f072cef275c8a4`. Venue source, tests, scripts,
crate manifest, KuCoin parity manifests and root package were byte-unchanged
against source SHA `20a31a6fe36bf0b7d48d01ef5ec9cf1a3fd2b2b5` (scoped
`git diff --exit-code` exit 0). The shared-input comparison found an incoming
Cargo.lock change, so all three gates were measured afresh at the merge SHA:

| Gate at `28dfa7eab7d046878d44b5ac81f072cef275c8a4` | Exit | Observed output |
| --- | --- | --- |
| Scoped KuCoin all-targets cargo check, same command above | 0 | Finished dev profile, no errors |
| Focused bootstrap_reads target, same command and TypeScript path above | 0 | Named native-before-source test passed; 1 passed, 0 failed |
| Source parity extractor check, same command above | 0 | 7,341 symbols / 512 files; 0 updates/additions/orphans |

While these gates ran, integration advanced. Second merge: incoming
`039d308cd99cfd09e22205fb1083c3df5a306548` into
`6f7a7e25029406489afc6cc82e269d49d136a6b6`. Scoped comparison against
the measured first merge exited 0 for venue source/tests/scripts/Cargo.toml,
KuCoin manifests, SDK Cargo.toml/Cargo.lock, the six dependency crates
(types, num, crypto, store, exchange, transport), maintained KuCoin TypeScript,
types.ts, utils, root package and yarn.lock. Cargo gates were not rerun after
this second merge; their input preservation is the basis for carrying them.
The extractor was refreshed at this second merge: exit 0, observed 7,341
symbols / 512 files and zero updates/additions/orphans.

Fresh independent tomllib census against the second incoming parent again
measured exactly 8 pending -> ported, 0 -> divergent: 122/39/30 -> 130/39/22.
It stopped at kucoinspot.ws-public.ts.toml after 12 manifests. Parsed exemption
union preserves all 1,163 incoming records, their complete values and relative
order, plus exactly the two source-identical BootstrapOps/BootstrapAwait records
(1,165 total). Independent package JSON comparison confirms only the exact two
oracle targets were appended. Sole merge base is the second incoming SHA.
The branch diff contains eight files, stopping at tests/bootstrap_reads.rs.

Read-only sub-agent review verified the first merge's source/input preservation,
eight moves, incoming exemption union and package target append; it ran no gates.
Mutation controls remain measured at the source SHA, not relabeled as merge runs.
Raw refreshed records/logs are under /tmp/kucoin-bootstrap-reads-merge-* and
/tmp/kucoin-bootstrap-reads-delivery-census.json; this tracked receipt records
their measured exits and exact SHAs. The unrun scopes and host limits above stand.
