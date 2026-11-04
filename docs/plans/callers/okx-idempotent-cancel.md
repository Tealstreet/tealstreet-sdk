# OKX already-gone cancels

SDK, NX and server callers receive normal cancel completion for exact OKX
`51400` on a standalone cancel, including batch and algo-cancel rows. The native
code confirms the order is filled, canceled or absent; localized text is not
used. A supplied response ID must match the corresponding requested ID, and
the response must retain the expected row count and successful HTTP status.

The engine records `Satisfied(Cancel)` before completing the call. The existing
account reconciliation read still runs: neither the ACK nor the completion
invents a Canceled Store state for an order that could have filled. Consumers
continue to use the order stream or clean census for the final order state.

Real sibling errors remain visible. HTTP failures, malformed acknowledgements,
other codes, placements, amendments and cancel-then-replace amendments retain
their existing failure semantics. No retry, caller migration or inventory
removal is added.

Reference: [OKX API error codes](https://www.okx.com/docs-v5/en/#error-code).
