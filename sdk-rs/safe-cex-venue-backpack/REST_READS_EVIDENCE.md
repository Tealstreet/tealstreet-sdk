# Backpack REST reads and trigger repair

Code measured: `ae9da53213772696025b1308eaa55a63fd0868ef`.
Actual fresh integration parent: `11c26bc6a7bb6240e836df4dcf5cb6ed73ef107f`.
Branch: `feat/rust-backpack-market-data`. No unrelated unreviewed branch work
was inherited; the commits above that integration parent are this slice.

Python 3.11 `tomllib` census of all six Backpack manifests, compared by
(file, TS symbol) against that actual parent, exited 0 at the code SHA:

| State | Ported | Divergent | Pending | Total |
| --- | ---: | ---: | ---: | ---: |
| Parent | 74 | 56 | 17 | 147 |
| Code SHA | 82 | 56 | 9 | 147 |

**8 pending → ported; 0 → divergent.** The eight rows are fetchMarkets,
fetchTickers, fetchOHLCV, fetchOrderBook, fetchHistoricalFills, fetchOrders,
#fixTriggerOrdersWithoutTriggerBy, and fetchPositions. Each points to its
complete body in `src/rest_reads.rs`; no class or HTTP factory row moved.

## Executed gates

These are scoped claims, not workspace compilation or venue completion.

| Gate | Exact code SHA | Measured exit and output |
| --- | --- | --- |
| Cargo check, Backpack, all targets, jobs 2 | ae9da53213772696025b1308eaa55a63fd0868ef | 0; Finished dev profile; zero errors |
| Cargo test, Backpack, only `--test rest_reads`, jobs 2 | ae9da53213772696025b1308eaa55a63fd0868ef | 0; 9 passed, 0 failed |
| Clean `parity:extract:check` | ae9da53213772696025b1308eaa55a63fd0868ef | 0; 7341 symbols / 512 files; would update 0 manifests, 0 added, 0 orphaned |

Commands executed from the worktree root:

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --test rest_reads --jobs 2
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

Writer after the manifest edits exited 0 and reported updated 0 manifests,
0 added, 0 orphaned. Yarn output text was inspected. A separate check overlapped
deliberate body mutations; its exit 0 is excluded from the clean-SHA claim above.
Only one Cargo command ran at a time, including all controls and restorations.

## Native source witness and bounds

The oracle executes the actual exchange module, actual shared TS enums and
venue tables, and actual lodash uniqBy. Construction, HTTP, clock, position-ID,
trigger/reference, mapping and logging seams are configured providers. Date.parse
and new Date run in Node with explicitly pinned UTC; the native fixture date
provider supplies independently fixed values for the enumerated inputs.

Independent constants pin 96 main cases: eight named families × twelve ordered
modes. All options and all nine independently constructed input fixture graphs
are compared in native form before invoking each Rust body. Four separately
pinned live cases cover replacing IMF objects, a changing market symbol with
absent quote, shrinking the repaired-order array during ID lookup, and eager
synchronous not-found position handling. The large-reciprocal IMF case is also
independently pinned in the main fixture. Counts and content do not come from a
Rust-generated corpus or a source-derived expected length.

Native assertions cover fields, booleans, undefined presence, shared enum
variants, finite/infinite/signed-zero f64 bits (NaNs normalized), UTF-16 including
lone surrogates, original market info identity, fresh ticker info identity versus
both endpoint arrays, mapper result identity, posted-order replacement identity,
original/logger/generated failures, requests' eager invocation sequence,
completed calls, logs, repaired arrays and posted native records. Every family
finishes all native assertions before its request-field checks. No Rust native
output is serialized for comparison or decoded and re-encoded.

These enumerations are floors: main family iteration stopped at 11, family
iteration at 7, live witness iteration at 3; native drivers are bounded at twelve
polls and require completion by iteration 11. This does not prove all getters,
array/prototype overrides, arbitrary thenables, provider implementations,
Promise microtask scheduling, native clock/calendar engines, full classes or
end-to-end transport. Bodies use compatible typed options/wire fields and native
built-in record/array views, with captured transport/trigger targets. No shared
type or ExchangeRuntime trait changed.

## Discriminating controls

All controls below were run against the code SHA above. Each compiled, exited
101 with a real panic on the listed native assertion, restored byte-identically,
and the same named test then exited 0. The corpus controls also compiled their
Rust test and failed its independent input pins. Controls stopped at iteration
11. No deliberately run control survived.

| Iteration | Deliberate mutation | Test filter | Named failing assertion | Broken / restored |
| ---: | --- | --- | --- | --- |
| 0 | Swap amount/price precision sources | markets | MARKETS precision amount native bits | 101 / 0 |
| 1 | Swap ticker mark/index sources | tickers | TICKERS mark native bits | 101 / 0 |
| 2 | Swap candle high/low | ohlcv | OHLCV high native bits | 101 / 0 |
| 3 | Swap bid/ask source sides | depth | DEPTH id native UTF16 | 101 / 0 |
| 4 | Swap native execution price/qty | fills | FILLS price native bits | 101 / 0 |
| 5 | Drop repair future before mapping orders | orders | ORDERS native mapped record | 101 / 0 |
| 6 | Recreate with reduceOnly false | repair | REST native created replies | 101 / 0 |
| 7 | Add one to native contracts | positions | POSITIONS contracts native bits | 101 / 0 |
| 8 | Delete final source corpus case | markets | REST independent corpus size | 101 / 0 |
| 9 | Homogeneous corpus, still length 96 | markets | REST independent corpus iteration | 101 / 0 |
| 10 | Replace Math.round with add-half/floor | markets | MARKETS limit max native bits | 101 / 0 |
| 11 | Cache first IMF object instead of rereading | live_getter_witnesses | MARKETS live IMF native leverage native bits | 101 / 0 |

The filtered Cargo command appended `<test filter> -- --exact` to the test
command above. Original/restored SHA-256:

- src/rest_reads.rs: `1c19aa0844b4ee5680988e340233bff944293b6383d86cb17a97942cdb0d1ab5`
- tests/rest_reads_oracle.cjs: `1a223d9da790fa6143c1cecaac2e93d3f0af79ee21367258df551a88d37bd3ea`

Post-control `git diff --exit-code HEAD --` the crate src/tests exited 0.

## Findings corrected and review

Uncommitted development runs exited 101: first check found an unused helper;
first eight-test run had 5 pass / 3 fail (two genuine eager-error sequencing
mismatches plus a fixture Date.parse local-time mismatch). After sequencing fixes,
7 passed / 1 failed; explicitly pinning UTC corrected the fixture and eight then
passed. These development observations are not gates bound to the final SHA.

The first committed capture-target revision `f68779aa4afb04b64b4eaac6cec730fc5b73bd32`
had test compilation exit 101 because the oracle host lacked the new trigger
argument. `a1af43a873f84def015281d486da7cfe402f8075` fixed it: scoped check 0,
nine-test target 0. An extractor exit 0 overlapped a later test-only edit and is
not used as a clean-SHA claim. Review then identified a missing posted
identity assertion. It was added in the measured code SHA; no old pass is claimed
as proving that new assertion.

Read-only sub-agent review identified and closed exact rounding, IMF reread,
absent quote, eager synchronous position/repair sequencing, array-extension,
undefined mark-row error delegation, callee capture, and identity-proof gaps.
The final reviewer confirmed the posted identity gap closed at the code SHA;
it did not run Cargo. No human review was requested.

Not run: workspace compilation, other Backpack test targets, host clippy, wasm
clippy/build/test, full monorepo suites/builds, live exchange/transport, CI or a
CI rerun. No new crate dependency, shared field or shared trait method was added.

## Integration merge and preservation

Merged integration `95b56a79bc646156e1812b17428e0fb6295413a5` with
`git merge --no-edit --no-stat FETCH_HEAD`, exit 0, producing
`0ae2402d7c2a63787ec6731e9b613f218c0d8d5b`.

At that merge SHA, each separately measured preservation command exited 0:

- `git diff --exit-code ae9da53213772696025b1308eaa55a63fd0868ef HEAD -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests`
- Actual oracle source inputs: exchange module, venue tables, shared TS types,
  LogSeverity, package manifest and yarn.lock.
- Workspace and Backpack Cargo manifests, Cargo.lock, and directly used shared
  Rust crates' source/manifests.
- Independent tomllib traversal reached 481 lock records and eight internal
  packages: Backpack, candle-cache, crypto, exchange, num, store, transport and
  types. A scoped diff of every reachable internal package's src and Cargo.toml
  also exited 0. No changed transitive Rust dependency was omitted.

Thus the measured code-SHA Cargo/test exits stand unchanged; they were not
relabelled as new merge-SHA Cargo runs. The clean extractor was rerun at merge
SHA `0ae2402d7c2a63787ec6731e9b613f218c0d8d5b`: exit 0, inspected output
7341 symbols / 512 files; would update 0 manifests, 0 added, 0 orphaned.

Final tomllib census at that merge SHA exited 0 against both the actual fresh
parent and the just-merged integration head: exactly eight pending → ported,
zero → divergent; 82 ported / 56 divergent / 9 pending of 147. The nine remaining
rows are createAPI, the three full classes, placeOrder, updateOrder,
sha512ForEd25519, public listenOHLCV and public listenOrderBook. Clearing these
is not claimed by this slice. The following delivery commit changes evidence
only; production, tests, source inputs and dependencies remain byte-identical.
