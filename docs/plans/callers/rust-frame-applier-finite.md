# FrameApplier non-finite guards

**Change.** `store/frameApplier.ts` stores a non-finite engine `leverage`,
`unrealizedPnl` (and `extra.unrealizedPnlNative`) or `liquidationPrice` as 0, the
value Rust venues already use for none/unknown. Optional fields read through
`finite()`, which now drops ±Infinity as well as NaN. `contracts`, `notional` and
`entryPrice` stay raw: a NaN `contracts` is what makes nuke refuse a close instead
of skipping a live position. Order quantities and prices are unchanged.

**Why.** Bybit's `float_of` maps an absent or empty position `leverage` to NaN,
which reached the positions table as `NaN`.

**Callers.**
- SDK: store readers no longer see NaN or ±Infinity in those position fields.
- NX: none. It picks the change up through the safe-cex import; no wasm release.

**Obligation.** None.

**Deletion prerequisites.** None added.
