# TxFlow public decoding

The ticker cache stores only decoded results for byte-identical, previously
validated frames. Every message still applies the row to the Store and startup
overlays. Never skip application: another feed can have changed the same ticker
between two identical messages.

Cache entries are limited to the catalogue size or 128, whichever is smaller;
each cached frame is at most 512 bytes. Successful catalogue replacement resets
the cache. Unknown instruments and malformed frames never populate it.

Numeric parsing, null/empty-string policies and observation handling retain
their existing rules. Public-frame controls are in
`tests/public_numeric_overlay.rs`. Control fast paths must match complete frames
so a ticker or rate-limit payload cannot be mistaken for a pong.

## Margin-mode sizing

Quantity-bearing writes require current metadata with explicit `useNotionalMargin: false`.
Missing/null retains an unknown mode without removing public market data. Limit/market
placements and price-only or size amendments refuse unmeasured modes before signing.
Serial placement batches and queued amendments recheck the mode at dispatch. Native-ID
cancellation remains available because it does not infer quantity units.
