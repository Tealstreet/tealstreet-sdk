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
