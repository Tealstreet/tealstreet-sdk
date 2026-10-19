# Facade divergence audit

Assignment: codex-l89ogr's de-duplicated 94-row FIX-audit list. This document
records only the facade rows exercised here; it does not authorize other rows.

| Source row | Verdict | Production evidence |
| --- | --- | --- |
| `index.ts::preloadExchanges` | FIX | `safe-cex/tests/factory_preload.rs` and `factory_preload_source.cjs`: eager loader invocation, first-error sibling continuation, delayed cache insertion, cache hits, discarded waiter, empty input and first-settled rejection ordering. |

The baseline native factory failed all three named regressions: no prefix before
polling, only the first failing loader invoked, and a discarded waiter never
filled the cache. The source probe extracts and executes the maintained factory
declarations with held and rejected loader promises. A separate compiled
regression also rejected an input-order join: the second input rejected first
but the custom join selected the first input. The shared Promise.all combinator
now preserves settlement order.

The direct cutover requires an `ExchangeRuntime` at factory construction. Existing
host loaders and the shared cache remain the only constructor and cache authority.
The canonical typed `start_eager` boundary owns each load; shared Promise.all
owns the aggregate. Awaiting a ready loader still suspends before cache publication.
Rejection stops the aggregate result without cancelling the other loads. No venue wrapper or
second task driver is introduced.

`load_exchange`, `create_exchange` and `create_public_exchange` are unchanged by
this preload-specific repair. Their separate scheduling and typed option claims
are not discharged by these witnesses.
