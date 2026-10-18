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
