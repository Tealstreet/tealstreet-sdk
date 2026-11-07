# BingX frame-fed mode

**Change.** BingX is in `FRAME_FED_VENUES`.
- **At start:** a frame-fed BingX engine reads only the public contracts. It skips the daily and premium ticker seed, the public feed and the private start (listen key, socket, keepalive, and the mode, balance, position and order reads and polls). It also skips the post-write reconcile and the order re-reads.
- **VENUE_STATE:** exports version 2: the position-mode-known bit, and each open order's native observation (kind, position side, attached, native status), followed by a length-prefixed typed JSON original (empty when exact restoration is unproven). Readers also accept version 1 from an older source, loading mode and observations with no original or put-back provenance. Unknown versions fail closed.
- **Observations:** they follow the ingested orders and close with them. In frame-fed mode the ingested ORDERS are the reconcile, so amend's "exactly reconciled" check compares against them.
- **Protective price edits:** refuse `reference stale`. The frame cannot map genuine source receipt time to the execution clock; arrival is never freshness evidence. Orders, cancels and ordinary LIMIT edits remain supported. A separate engine-owned mapped-receipt seam is required before admitting protective price edits.
- **Places:** they refuse visibly while the mode is unknown rather than guessing one-way (ddi).

**Callers.**
- SDK: none beyond the registry list.
- NX: none.

**Obligation.** None.

**Deletion prerequisites.** The forwarding write path for BingX may go once the main-thread engine uses frame-fed mode.
