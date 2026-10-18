# Logger console observations

Merge `9b83f52a68` (#974), production change `8396a2cb9c`, made the shared native
logger preserve both arguments of maintained `BaseExchange.log`: message and
severity. The source sends `console.log(message, severity)` and separately emits
`log` with the prefixed message and severity. Venue log-call observations must
keep those two fields separate. A console-text observation includes both.

The seven actual venue declarations inherit this logger without overriding it.
Executing the maintained logger and logging enum for all seven names, four
severities and ordinary/lone-surrogate text confirms 56 console/emitter pairs,
with exact argument counts, values and UTF-16 units. Production TS/Rust behavior
is unchanged by this observer repair.

| Venue | Repaired observation |
| --- | --- |
| ApexOmni | Structured log-call message/severity, with exact console pair checks |
| BingX | Completion/read-stream log pairs; existing error-report observations retained |
| MEXC | Order-fetch and metadata log pairs with exact console pair checks |
| AsterDEX | Native strict-Ticker refusal console text includes error severity |
| Bybit | Paging-cap and option-coercion console text includes warning severity |
| Phemex | Snapshot warning text uses both maintained-source log arguments |
| Polymarket | All three no-op warning console texts include warning severity |

Apex/MEXC/BingX structured observers assert the complete two-argument logging
shape and the native severity argument before recording the original message
and severity independently. Existing generic reporting/trace console observations
keep their complete argument rendering. No message suffix is stripped heuristically.

Exact master `7b77f371e9` compiles and fails the Apex balance comparison and real
constructed Phemex snapshot/retry witness at the missing severity expectation.
The other affected CI target inputs and shared logger are byte-identical at that
master and the compiled #1011 failure snapshot. Existing source cases, request,
Store, retry, notification and native numeric assertions remain intact. The
repair changes no production body or accepted-set policy, so no producer or
receiver source pin requires refresh.

## Parity workflow sweep

On master `36e6581b5f`, the Aster account-fetch observer still joined the two
console arguments into its logical message field. The same defect remained in
the repair runtime (listen-key refresh and position repairs) and subscription
runtime (order execution and private-stream comparisons). Five named tests
compiled and failed with the extra severity suffix, including the private-stream
UTF-16 cutoff assertion, before the observer repair.

These three observers now share `tests/support/log_observer.rs`. Each logical
log requires exactly two arguments and checks the native severity against the
recorded channel before rendering only the message. All four source severities
are covered; the exact shared error-report prefixes and trace channel keep their
complete console rendering. No suffix matching or source expectation changes
are involved. All seven consuming test targets pass (36 tests), retaining every
source case, request, timer, Store effect and native-value assertion.

The sweep followed every venue-oracle script used by the parity job in
`.github/workflows/rust.yml`, its registry/store/types/facade/worker steps, and
all 92 `live_*.rs`/`live-*.rs` paths, including support modules. This was a source
review of observers and their consumers, with execution scoped to the changed
targets rather than a local run of the whole workflow.

| Workflow consumers | Observation reviewed |
| --- | --- |
| Aster account, repair and subscription targets | Logical log pairs repaired; seven consuming targets pass |
| Aster clock lifecycle | Direct console messages; unchanged source/native test passes |
| Aster exchange runtime | Full console text, with existing strict-Ticker severity expectation |
| Apex/MEXC | Existing guarded logical log pairs from the first repair |
| Coinbase/Crypto.com/KuCoin | Full console diagnostics; direct method-table log captures keep separate native arguments |
| Binance | Original console arguments retained alongside rendered diagnostics; logical callbacks capture arguments directly |
| Lighter | Direct log-operation argument vectors; console/error sinks keep full rendering |
| Bybit | Console sinks include severity; logical operation logs remain native values |
| WOO | Direct log callbacks; history/page error sinks observe full reporter output |
| Registry/store/types/facade/worker | Forwarded console arguments, direct operation captures or diagnostic error counts; no joined-message logical pair comparison |
| Other live-test paths | No additional joined-console observer used as a logical message/severity pair |

The previously repaired BingX/Phemex/Polymarket sinks and the remaining venue
test-runtime console sinks were also reviewed. Full console text remains full
console text. The sweep found no further instance requiring an observer change.
