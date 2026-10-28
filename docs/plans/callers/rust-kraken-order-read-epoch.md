
### Kraken order census epoch

Kraken Futures captures `order_read_epoch()` when issuing its single open-orders
request. Kraken Spot captures it in the `OpenOrders` continuation and preserves
it through any existing signature resend. Both publish a clean complete snapshot
before calling `orders_reconciled_since(None, captured_epoch)`. A response cannot
clear uncertainty created after its census began; duplicate stale responses do
not release or re-arm the fence. Backoff re-asks keep joining the running read.
These remain single-request censuses; this adopts the shared explicit epoch API
without claiming a previously reachable multi-page release bug. The production
two-mark controls cover retry/amend refusal, duplicate stale replies, and a fresh
read releasing the fence so the next placement is actually sent.
